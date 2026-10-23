import { LogOut, Monitor, Moon, Sun } from "lucide-react";
import type { Theme, User } from "@/api/schemas";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { useSignOut } from "@/hooks/use-auth";
import { useTheme } from "@/hooks/use-theme";
import { cn } from "@/lib/cn";

export const THEME_OPTIONS: readonly {
  value: Theme;
  label: string;
  icon: typeof Sun;
}[] = [
  { value: "system", label: "System", icon: Monitor },
  { value: "light", label: "Light", icon: Sun },
  { value: "dark", label: "Dark", icon: Moon },
];

function initials(name: string): string {
  const parts = name.trim().split(/\s+/).filter(Boolean);
  const letters = parts.slice(0, 2).map((part) => part[0]?.toUpperCase() ?? "");
  return letters.join("") || "?";
}

export function Avatar({
  user,
  className,
}: {
  user: User;
  className?: string;
}) {
  return (
    <span
      aria-hidden
      className={cn(
        "inline-flex size-8 shrink-0 items-center justify-center rounded-full bg-accent-soft text-xs font-semibold text-accent",
        className,
      )}
    >
      {initials(user.display_name)}
    </span>
  );
}

/** The account menu: theme choice and sign-out. */
export function UserMenu({ user }: { user: User }) {
  const { theme, setTheme } = useTheme();
  const signOut = useSignOut();

  return (
    <DropdownMenu>
      <DropdownMenuTrigger
        aria-label={`Account menu for ${user.display_name}`}
        className="flex w-full items-center gap-3 rounded-md p-2 text-left text-sm hover:bg-surface-raised focus-visible:outline-2 focus-visible:outline-ring"
      >
        <Avatar user={user} />
        <span className="flex min-w-0 flex-col">
          <span className="truncate font-medium text-fg">
            {user.display_name}
          </span>
          {user.email && (
            <span className="truncate text-xs text-fg-subtle">
              {user.email}
            </span>
          )}
        </span>
      </DropdownMenuTrigger>
      <DropdownMenuContent side="top" align="start" className="w-56">
        <DropdownMenuLabel>Theme</DropdownMenuLabel>
        <DropdownMenuRadioGroup
          value={theme}
          onValueChange={(value) => {
            const option = THEME_OPTIONS.find((o) => o.value === value);
            if (option) setTheme(option.value);
          }}
        >
          {THEME_OPTIONS.map(({ value, label, icon: Icon }) => (
            <DropdownMenuRadioItem key={value} value={value}>
              <Icon aria-hidden className="size-4 text-fg-muted" />
              {label}
            </DropdownMenuRadioItem>
          ))}
        </DropdownMenuRadioGroup>
        <DropdownMenuSeparator />
        <DropdownMenuItem
          onSelect={() => signOut.mutate()}
          disabled={signOut.isPending}
        >
          <LogOut aria-hidden className="size-4 text-fg-muted" />
          Sign out
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
