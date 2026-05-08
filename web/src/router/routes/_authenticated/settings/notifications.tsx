import { createFileRoute } from "@tanstack/react-router";
import {
  useApiNotificationsHandlersGetPreferences,
  useApiNotificationsHandlersListChannels,
} from "@/api/notifications/notifications";

export const Route = createFileRoute("/_authenticated/settings/notifications")({
  component: NotificationsSettingsPage,
});

function NotificationsSettingsPage() {
  const channels = useApiNotificationsHandlersListChannels();
  const preferences = useApiNotificationsHandlersGetPreferences();

  return (
    <div
      data-testid="notifications-settings-page"
      className="notifications-settings-page"
    >
      <h1>Notifications</h1>

      <h2>Channels</h2>
      {channels.isLoading ? (
        <p>Loading channels…</p>
      ) : (channels.data ?? []).length === 0 ? (
        <p>No channels configured. Create one via the API for now.</p>
      ) : (
        <ul>
          {(channels.data ?? []).map((c) => (
            <li key={c.id}>
              <code>{c.channel_type}</code> — {c.name}{" "}
              {c.enabled ? "" : "(disabled)"}
            </li>
          ))}
        </ul>
      )}

      <h2>Preferences</h2>
      {preferences.isLoading ? (
        <p>Loading preferences…</p>
      ) : (
        <pre>
          {JSON.stringify(
            (preferences.data?.preferences ?? []) as unknown,
            null,
            2,
          )}
        </pre>
      )}
    </div>
  );
}
