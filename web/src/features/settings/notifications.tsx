import { type QueryClient, useQueryClient } from "@tanstack/react-query";
import { BellRing, Pencil, Plus, Send, Trash2, X } from "lucide-react";
import { type FormEvent, useState } from "react";
import {
  getListNotificationChannelsQueryKey,
  getListNotificationSubscriptionsQueryKey,
  useCreateNotificationChannel,
  useCreateNotificationSubscription,
  useDeleteNotificationChannel,
  useListNotificationChannels,
  useListNotificationSubscriptions,
  useTestNotificationChannel,
  useUpdateNotificationChannel,
  useUpdateNotificationSubscription,
} from "@/api/notifications/notifications";
import {
  type Channel,
  NotificationEvent,
  type Subscription,
  WebhookMethod,
} from "@/api/schemas";
import { Button, IconButton } from "@/components/ui/button";
import { ConfirmDialog } from "@/components/ui/confirm-dialog";
import { Dialog, DialogContent } from "@/components/ui/dialog";
import { EmptyState } from "@/components/ui/empty-state";
import { ErrorState, reportFailure } from "@/components/ui/error-state";
import { Input } from "@/components/ui/input";
import { Select } from "@/components/ui/select";
import { Skeleton } from "@/components/ui/skeleton";
import { toast } from "@/components/ui/toast";
import {
  type ChannelDraft,
  type ChannelErrors,
  draftOf,
  emptyDraft,
  toChannelPatch,
  toNewChannel,
  validateChannel,
} from "./channel-form";
import { Meta, SettingsSection } from "./section";

const PAGE = 200;

export const EVENT_LABELS: Record<NotificationEvent, string> = {
  new_episode: "A new episode is out",
  upcoming_release: "A release is coming up",
  import_completed: "An import finished",
};

const TYPE_LABELS = { webhook: "Webhook", apprise: "Apprise" } as const;

function invalidateNotifications(queryClient: QueryClient) {
  return Promise.all([
    queryClient.invalidateQueries({
      queryKey: getListNotificationChannelsQueryKey(),
    }),
    queryClient.invalidateQueries({
      queryKey: getListNotificationSubscriptionsQueryKey(),
    }),
  ]);
}

/** Delivery channels (webhook, Apprise) and the events each one receives. */
export function NotificationsSection() {
  const channels = useListNotificationChannels({ limit: PAGE });
  const subscriptions = useListNotificationSubscriptions({ limit: PAGE });
  const [adding, setAdding] = useState(false);

  let body: React.ReactNode;
  if (channels.isPending || subscriptions.isPending) {
    body = <Skeleton className="h-24" />;
  } else if (channels.isError || subscriptions.isError) {
    body = (
      <ErrorState
        error={channels.error ?? subscriptions.error}
        onRetry={() => {
          void channels.refetch();
          void subscriptions.refetch();
        }}
      />
    );
  } else if (channels.data.items.length === 0) {
    body = (
      <EmptyState
        className="py-6"
        icon={<BellRing />}
        title="No channels"
        description="Add a webhook or an Apprise server to hear about new episodes and finished imports."
      />
    );
  } else {
    body = (
      <ul className="flex flex-col gap-3">
        {channels.data.items.map((channel) => (
          <ChannelCard
            key={channel.id}
            channel={channel}
            subscriptions={subscriptions.data.items.filter(
              (s) => s.channel_id === channel.id,
            )}
          />
        ))}
      </ul>
    );
  }

  return (
    <SettingsSection
      title="Notifications"
      description="Where Stacks tells you about new episodes, upcoming releases and finished imports."
      actions={
        <Button variant="secondary" size="sm" onClick={() => setAdding(true)}>
          <Plus aria-hidden className="size-4" />
          Add channel
        </Button>
      }
    >
      {body}
      <ChannelDialog open={adding} onOpenChange={setAdding} />
    </SettingsSection>
  );
}

function ChannelCard({
  channel,
  subscriptions,
}: {
  channel: Channel;
  subscriptions: Subscription[];
}) {
  const queryClient = useQueryClient();
  const [editing, setEditing] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const settle = () => invalidateNotifications(queryClient);
  const update = useUpdateNotificationChannel({
    mutation: { onError: reportFailure, onSettled: settle },
  });
  const remove = useDeleteNotificationChannel({
    mutation: {
      onSuccess: () => {
        setDeleting(false);
        toast({ title: `Deleted ${channel.name}` });
      },
      onError: (error) => {
        setDeleting(false);
        reportFailure(error);
      },
      onSettled: settle,
    },
  });
  // The server delivers a sample through the channel and answers once the
  // receiver accepted it; a refusal's problem names its status or error.
  const test = useTestNotificationChannel({
    mutation: {
      onSuccess: () =>
        toast({
          tone: "success",
          title: "Test sent",
          description: `${channel.name} accepted the test notification.`,
        }),
      onError: reportFailure,
    },
  });
  return (
    <li className="flex flex-col gap-3 rounded-lg border border-line p-3">
      <div className="flex flex-wrap items-center gap-2">
        <div className="flex min-w-0 flex-1 flex-col">
          <span className="truncate font-medium text-fg">{channel.name}</span>
          <Meta>
            {TYPE_LABELS[channel.config.type]} ·{" "}
            <span className="break-all">{channel.config.url}</span>
          </Meta>
        </div>
        <label className="flex items-center gap-2 text-sm">
          <input
            type="checkbox"
            checked={channel.enabled}
            disabled={update.isPending}
            onChange={(event) =>
              update.mutate({
                id: channel.id,
                data: { enabled: event.target.checked },
              })
            }
            className="size-4 accent-(--color-accent)"
          />
          Enabled
        </label>
        <Button
          variant="secondary"
          size="sm"
          aria-label={`Send a test to ${channel.name}`}
          loading={test.isPending}
          onClick={() => test.mutate({ id: channel.id })}
        >
          <Send aria-hidden className="size-4" />
          Send test
        </Button>
        <IconButton
          label={`Edit ${channel.name}`}
          icon={<Pencil aria-hidden className="size-4" />}
          onClick={() => setEditing(true)}
        />
        <IconButton
          label={`Delete ${channel.name}`}
          icon={<Trash2 aria-hidden className="size-4" />}
          onClick={() => setDeleting(true)}
        />
      </div>
      <Subscriptions channel={channel} subscriptions={subscriptions} />
      <ChannelDialog
        open={editing}
        onOpenChange={setEditing}
        channel={channel}
      />
      <ConfirmDialog
        open={deleting}
        onOpenChange={setDeleting}
        title={`Delete ${channel.name}?`}
        description="It stops receiving notifications, and its subscriptions go with it. This can't be undone: only a backup that includes notification channel settings brings it back."
        confirmLabel="Delete channel"
        pending={remove.isPending}
        onConfirm={() => remove.mutate({ id: channel.id })}
      />
    </li>
  );
}

/** One checkbox per event: ticked while an enabled subscription routes it here. */
function Subscriptions({
  channel,
  subscriptions,
}: {
  channel: Channel;
  subscriptions: Subscription[];
}) {
  const queryClient = useQueryClient();
  const settle = () => invalidateNotifications(queryClient);
  const create = useCreateNotificationSubscription({
    mutation: { onError: reportFailure, onSettled: settle },
  });
  const update = useUpdateNotificationSubscription({
    mutation: { onError: reportFailure, onSettled: settle },
  });
  const busy = create.isPending || update.isPending;
  function toggle(event: NotificationEvent, on: boolean) {
    const existing = subscriptions.find((s) => s.event === event);
    // Unticking disables rather than deletes, so ticking again restores it.
    if (existing) update.mutate({ id: existing.id, data: { enabled: on } });
    else if (on) create.mutate({ data: { channel_id: channel.id, event } });
  }
  return (
    <fieldset className="flex flex-col gap-1.5">
      <legend className="mb-1 text-xs font-medium text-fg-muted">
        Notify {channel.name} when
      </legend>
      {Object.values(NotificationEvent).map((event) => {
        const on = subscriptions.some((s) => s.event === event && s.enabled);
        return (
          <label key={event} className="flex items-center gap-2 text-sm">
            <input
              type="checkbox"
              checked={on}
              disabled={busy}
              onChange={(e) => toggle(event, e.target.checked)}
              className="size-4 accent-(--color-accent)"
            />
            {EVENT_LABELS[event]}
          </label>
        );
      })}
    </fieldset>
  );
}

function ChannelDialog({
  open,
  onOpenChange,
  channel,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  channel?: Channel;
}) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        title={channel ? `Edit ${channel.name}` : "Add a channel"}
        description={
          channel
            ? "Secrets and header values are never shown; leave them blank to keep them."
            : "A webhook receives each notification as JSON; Apprise relays it to the services it knows."
        }
        className="w-[min(92vw,36rem)] overflow-y-auto"
      >
        {open && (
          <ChannelForm channel={channel} onDone={() => onOpenChange(false)} />
        )}
      </DialogContent>
    </Dialog>
  );
}

function ChannelForm({
  channel,
  onDone,
}: {
  channel?: Channel;
  onDone: () => void;
}) {
  const queryClient = useQueryClient();
  const [draft, setDraft] = useState<ChannelDraft>(() =>
    channel ? draftOf(channel) : emptyDraft(),
  );
  const [errors, setErrors] = useState<ChannelErrors>({});
  const mutation = {
    onSuccess: () => {
      toast({
        tone: "success",
        title: channel ? "Channel saved" : "Channel added",
      });
      onDone();
    },
    onError: reportFailure,
    onSettled: () => invalidateNotifications(queryClient),
  };
  const create = useCreateNotificationChannel({ mutation });
  const update = useUpdateNotificationChannel({ mutation });

  function set<K extends keyof ChannelDraft>(key: K, value: ChannelDraft[K]) {
    setDraft((current) => ({ ...current, [key]: value }));
  }

  function submit(event: FormEvent) {
    event.preventDefault();
    const found = validateChannel(draft);
    setErrors(found);
    if (Object.keys(found).length > 0) return;
    if (!channel) {
      create.mutate({ data: toNewChannel(draft) });
      return;
    }
    const patch = toChannelPatch(channel, draft);
    if (patch) update.mutate({ id: channel.id, data: patch });
    else onDone();
  }

  const webhook = draft.type === "webhook";
  const existingHeaders =
    channel?.config.type === "webhook" ? channel.config.header_names : [];
  const hasSecret =
    channel?.config.type === "webhook" && channel.config.has_secret;

  return (
    <form onSubmit={submit} noValidate className="flex flex-col gap-4">
      <Input
        label="Name"
        value={draft.name}
        maxLength={100}
        error={errors.name}
        onChange={(event) => set("name", event.target.value)}
      />
      {!channel && (
        <Select
          label="Type"
          value={draft.type}
          options={[
            { value: "webhook", label: "Webhook" },
            { value: "apprise", label: "Apprise" },
          ]}
          onValueChange={(value) => set("type", value)}
        />
      )}
      <Input
        label="URL"
        type="url"
        placeholder={
          webhook
            ? "https://example.com/hooks/stacks"
            : "https://apprise.example.com/notify/my-key"
        }
        value={draft.url}
        error={errors.url}
        onChange={(event) => set("url", event.target.value)}
      />
      {webhook ? (
        <>
          <Select
            label="Method"
            value={draft.method}
            options={Object.values(WebhookMethod).map((value) => ({
              value,
              label: value,
            }))}
            onValueChange={(value) => set("method", value)}
          />
          <HeadersField
            draft={draft}
            existing={existingHeaders}
            error={errors.headers}
            onChange={set}
          />
          <Input
            label="Signing secret"
            type="password"
            autoComplete="off"
            hint={
              hasSecret
                ? "Deliveries are signed. Enter a new secret to replace it."
                : "Optional. Each delivery's body is signed with HMAC-SHA256, sent as X-Stacks-Signature."
            }
            value={draft.secret}
            onChange={(event) => set("secret", event.target.value)}
          />
          {hasSecret && (
            <label className="flex items-center gap-2 text-sm">
              <input
                type="checkbox"
                checked={draft.clearSecret}
                disabled={Boolean(draft.secret)}
                onChange={(event) => set("clearSecret", event.target.checked)}
                className="size-4 accent-(--color-accent)"
              />
              Stop signing deliveries
            </label>
          )}
        </>
      ) : (
        <Input
          label="Tags"
          hint="Comma-separated Apprise tags; leave empty to notify all of the key's."
          value={draft.tags}
          onChange={(event) => set("tags", event.target.value)}
        />
      )}
      <div className="flex justify-end gap-2">
        <Button variant="ghost" onClick={onDone}>
          Cancel
        </Button>
        <Button type="submit" loading={create.isPending || update.isPending}>
          {channel ? "Save channel" : "Add channel"}
        </Button>
      </div>
    </form>
  );
}

function HeadersField({
  draft,
  existing,
  error,
  onChange,
}: {
  draft: ChannelDraft;
  existing: readonly string[];
  error?: string;
  onChange: <K extends keyof ChannelDraft>(
    key: K,
    value: ChannelDraft[K],
  ) => void;
}) {
  if (!draft.replaceHeaders) {
    return (
      <div className="flex flex-col gap-1.5">
        <span className="text-sm font-medium text-fg">Extra headers</span>
        <p className="text-xs text-fg-muted">
          {existing.length
            ? `Sends ${existing.join(", ")}.`
            : "Sends no extra headers."}
        </p>
        <Button
          variant="secondary"
          size="sm"
          className="self-start"
          onClick={() => onChange("replaceHeaders", true)}
        >
          Replace headers
        </Button>
      </div>
    );
  }
  const headers = draft.headers;
  function update(index: number, key: "name" | "value", value: string) {
    onChange(
      "headers",
      headers.map((h, i) => (i === index ? { ...h, [key]: value } : h)),
    );
  }
  return (
    <fieldset className="flex flex-col gap-2">
      <legend className="mb-1 text-sm font-medium text-fg">
        Extra headers
      </legend>
      {headers.map((header, index) => (
        // biome-ignore lint/suspicious/noArrayIndexKey: rows have no identity but their place
        <div key={index} className="flex items-center gap-2">
          <Input
            aria-label={`Header ${index + 1} name`}
            placeholder="Authorization"
            value={header.name}
            onChange={(event) => update(index, "name", event.target.value)}
          />
          <Input
            aria-label={`Header ${index + 1} value`}
            type="password"
            autoComplete="off"
            value={header.value}
            onChange={(event) => update(index, "value", event.target.value)}
          />
          <IconButton
            label={`Remove header ${index + 1}`}
            icon={<X aria-hidden className="size-4" />}
            onClick={() =>
              onChange(
                "headers",
                headers.filter((_, i) => i !== index),
              )
            }
          />
        </div>
      ))}
      {error && <p className="text-xs text-danger">{error}</p>}
      <Button
        variant="secondary"
        size="sm"
        className="self-start"
        onClick={() =>
          onChange("headers", [...headers, { name: "", value: "" }])
        }
      >
        <Plus aria-hidden className="size-4" />
        Add header
      </Button>
    </fieldset>
  );
}
