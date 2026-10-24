import { useQueryClient } from "@tanstack/react-query";
import { Copy, KeyRound, Plus, TriangleAlert } from "lucide-react";
import { type FormEvent, useState } from "react";
import {
  getListApiTokensQueryKey,
  useCreateApiToken,
  useListApiTokens,
  useRevokeApiToken,
} from "@/api/account/account";
import type {
  ApiToken,
  CreatedApiToken,
  Role,
  TokenScope,
} from "@/api/schemas";
import { Button } from "@/components/ui/button";
import { ConfirmDialog } from "@/components/ui/confirm-dialog";
import { Dialog, DialogContent } from "@/components/ui/dialog";
import { EmptyState } from "@/components/ui/empty-state";
import { ErrorState } from "@/components/ui/error-state";
import { Input } from "@/components/ui/input";
import { Select, type SelectOption } from "@/components/ui/select";
import { Skeleton } from "@/components/ui/skeleton";
import { toast } from "@/components/ui/toast";
import { formatInstant } from "@/features/calendar/dates";
import { reportFailure } from "@/features/titles/use-tracking";
import { Meta, SettingsSection } from "./section";

const TOKEN_PAGE = 200;

const SCOPE_LABELS: Record<TokenScope, string> = {
  read: "Read: list and export your data",
  write: "Write: change your library, lists and settings",
  admin: "Admin: manage accounts and jobs",
};

type Expiry = "never" | "7" | "30" | "90" | "365";

const EXPIRIES: readonly SelectOption<Expiry>[] = [
  { value: "never", label: "Never" },
  { value: "7", label: "In 7 days" },
  { value: "30", label: "In 30 days" },
  { value: "90", label: "In 90 days" },
  { value: "365", label: "In a year" },
];

/** The instant `expiry` days from `now`, or `null` for never. */
export function expiryInstant(expiry: Expiry, now = new Date()): string | null {
  if (expiry === "never") return null;
  return new Date(now.getTime() + Number(expiry) * 86_400_000).toISOString();
}

/** Tokens for scripts and other apps: created, listed by prefix, revoked. */
export function ApiTokensSection({ role }: { role: Role }) {
  const tokens = useListApiTokens({ limit: TOKEN_PAGE });
  const [creating, setCreating] = useState(false);
  let body: React.ReactNode;
  if (tokens.isPending) body = <Skeleton className="h-20" />;
  else if (tokens.isError)
    body = (
      <ErrorState error={tokens.error} onRetry={() => void tokens.refetch()} />
    );
  else if (tokens.data.items.length === 0)
    body = (
      <EmptyState
        className="py-6"
        icon={<KeyRound />}
        title="No API tokens"
        description="Create one to reach the API from a script or another app."
      />
    );
  else
    body = (
      <ul className="flex flex-col divide-y divide-line">
        {tokens.data.items.map((token) => (
          <TokenRow key={token.id} token={token} />
        ))}
      </ul>
    );
  return (
    <SettingsSection
      title="API tokens"
      description="Send a token as `Authorization: Bearer <token>`. A token is shown once, when it is created."
      actions={
        <Button variant="secondary" size="sm" onClick={() => setCreating(true)}>
          <Plus aria-hidden className="size-4" />
          New token
        </Button>
      }
    >
      {body}
      <CreateTokenDialog
        open={creating}
        onOpenChange={setCreating}
        role={role}
      />
    </SettingsSection>
  );
}

function TokenRow({ token }: { token: ApiToken }) {
  const queryClient = useQueryClient();
  const [confirming, setConfirming] = useState(false);
  const revoke = useRevokeApiToken({
    mutation: {
      onSuccess: () => {
        setConfirming(false);
        toast({ tone: "success", title: `Revoked ${token.name}` });
      },
      onError: (error) => {
        setConfirming(false);
        reportFailure(error);
      },
      onSettled: () =>
        queryClient.invalidateQueries({
          queryKey: getListApiTokensQueryKey(),
        }),
    },
  });
  return (
    <li className="flex flex-wrap items-center gap-3 py-3">
      <div className="flex min-w-0 flex-1 flex-col">
        <span className="truncate font-medium text-fg">{token.name}</span>
        <Meta>
          <code>{token.prefix}…</code> · {token.scopes.join(", ")} ·{" "}
          {token.last_used_at
            ? `last used ${formatInstant(token.last_used_at)}`
            : "never used"}{" "}
          ·{" "}
          {token.expires_at
            ? `expires ${formatInstant(token.expires_at)}`
            : "never expires"}
        </Meta>
      </div>
      <Button
        variant="secondary"
        size="sm"
        aria-label={`Revoke ${token.name}`}
        onClick={() => setConfirming(true)}
      >
        Revoke
      </Button>
      <ConfirmDialog
        open={confirming}
        onOpenChange={setConfirming}
        title={`Revoke ${token.name}?`}
        description="Anything using this token stops working at once. This cannot be undone."
        confirmLabel="Revoke token"
        pending={revoke.isPending}
        onConfirm={() => revoke.mutate({ id: token.id })}
      />
    </li>
  );
}

/**
 * Creating a token, then showing its secret once. The secret lives only in
 * this dialog's state: closing the dialog drops it, and nothing can read it
 * back afterwards.
 */
function CreateTokenDialog({
  open,
  onOpenChange,
  role,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  role: Role;
}) {
  const [created, setCreated] = useState<CreatedApiToken | null>(null);
  function change(next: boolean) {
    if (!next) setCreated(null);
    onOpenChange(next);
  }
  return (
    <Dialog open={open} onOpenChange={change}>
      <DialogContent
        title={created ? "Copy your token" : "New API token"}
        description={
          created
            ? "This is the only time the token is shown."
            : "Name the token after what will use it."
        }
      >
        {open &&
          (created ? (
            <ShowSecret created={created} onDone={() => change(false)} />
          ) : (
            <NewTokenForm role={role} onCreated={setCreated} />
          ))}
      </DialogContent>
    </Dialog>
  );
}

function NewTokenForm({
  role,
  onCreated,
}: {
  role: Role;
  onCreated: (created: CreatedApiToken) => void;
}) {
  const queryClient = useQueryClient();
  const [name, setName] = useState("");
  const [scopes, setScopes] = useState<ReadonlySet<TokenScope>>(
    new Set(["read"]),
  );
  const [expiry, setExpiry] = useState<Expiry>("90");
  const [touched, setTouched] = useState(false);
  const create = useCreateApiToken({
    mutation: {
      onSuccess: onCreated,
      onError: reportFailure,
      onSettled: () =>
        queryClient.invalidateQueries({
          queryKey: getListApiTokensQueryKey(),
        }),
    },
  });
  const offered: TokenScope[] =
    role === "admin" ? ["read", "write", "admin"] : ["read", "write"];
  const trimmed = name.trim();
  const nameError = touched && !trimmed ? "Give the token a name." : undefined;
  const scopeError =
    scopes.size === 0 ? "Choose at least one scope." : undefined;

  function toggle(scope: TokenScope, on: boolean) {
    setScopes((current) => {
      const next = new Set(current);
      if (on) next.add(scope);
      else next.delete(scope);
      return next;
    });
  }

  function submit(event: FormEvent) {
    event.preventDefault();
    setTouched(true);
    if (!trimmed || scopes.size === 0) return;
    create.mutate({
      data: {
        name: trimmed,
        scopes: offered.filter((scope) => scopes.has(scope)),
        expires_at: expiryInstant(expiry),
      },
    });
  }

  return (
    <form onSubmit={submit} className="flex flex-col gap-4">
      <Input
        label="Name"
        placeholder="e.g. Home server sync"
        value={name}
        maxLength={100}
        error={nameError}
        onChange={(event) => setName(event.target.value)}
      />
      <fieldset className="flex flex-col gap-2">
        <legend className="mb-1 text-sm font-medium text-fg">Scopes</legend>
        {offered.map((scope) => (
          <label key={scope} className="flex items-center gap-2 text-sm">
            <input
              type="checkbox"
              checked={scopes.has(scope)}
              onChange={(event) => toggle(scope, event.target.checked)}
              className="size-4 accent-(--color-accent)"
            />
            {SCOPE_LABELS[scope]}
          </label>
        ))}
        {scopeError && <p className="text-xs text-danger">{scopeError}</p>}
      </fieldset>
      <Select
        label="Expires"
        value={expiry}
        options={EXPIRIES}
        onValueChange={setExpiry}
      />
      <div className="flex justify-end">
        <Button
          type="submit"
          loading={create.isPending}
          disabled={Boolean(scopeError)}
        >
          Create token
        </Button>
      </div>
    </form>
  );
}

function ShowSecret({
  created,
  onDone,
}: {
  created: CreatedApiToken;
  onDone: () => void;
}) {
  async function copy() {
    try {
      await navigator.clipboard.writeText(created.secret);
      toast({ tone: "success", title: "Token copied" });
    } catch {
      toast({
        tone: "error",
        title: "Couldn't copy",
        description: "Select the token and copy it by hand.",
      });
    }
  }
  return (
    <div className="flex flex-col gap-4">
      <p
        role="alert"
        className="flex items-start gap-2 rounded-md border border-warning/40 bg-surface-raised p-3 text-sm text-fg"
      >
        <TriangleAlert aria-hidden className="mt-0.5 size-4 shrink-0" />
        Copy {created.token.name} now and keep it somewhere safe. Once this
        dialog closes it can't be shown again; if you lose it, revoke it and
        create another.
      </p>
      <div className="flex items-center gap-2">
        <Input
          aria-label="Token"
          readOnly
          value={created.secret}
          className="font-mono"
          onFocus={(event) => event.currentTarget.select()}
        />
        <Button variant="secondary" onClick={() => void copy()}>
          <Copy aria-hidden className="size-4" />
          Copy
        </Button>
      </div>
      <div className="flex justify-end">
        <Button onClick={onDone}>Done</Button>
      </div>
    </div>
  );
}
