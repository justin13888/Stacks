import { createLazyFileRoute } from "@tanstack/react-router";

export const Route = createLazyFileRoute(
  "/_authenticated/settings/notifications",
)({
  component: NotificationsSettingsPage,
});

function NotificationsSettingsPage() {
  return (
    <div
      data-testid="notifications-settings-page"
      className="notifications-settings-page"
    >
      {/* Notification channel config page — placeholder */}
      <h1>Notifications</h1>
      <p>Configure notification channels here.</p>
    </div>
  );
}
