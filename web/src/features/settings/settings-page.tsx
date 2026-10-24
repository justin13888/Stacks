import type { User } from "@/api/schemas";
import { PageHeader } from "@/components/layout/page";
import { ApiTokensSection } from "./api-tokens";
import { NotificationsSection } from "./notifications";
import { ProfileForm } from "./profile-form";
import { SessionsSection } from "./sessions";
import { SettingsNav } from "./settings-nav";

/** `/settings`: the account, its sessions, API tokens and notifications. */
export function SettingsPage({ user }: { user: User }) {
  return (
    <>
      <PageHeader
        title="Settings"
        description="Your account, how Stacks shows things, and where it reaches you."
      />
      <SettingsNav />
      <div className="flex flex-col gap-6">
        <ProfileForm key={user.id} user={user} />
        <SessionsSection />
        <ApiTokensSection role={user.role} />
        <NotificationsSection />
      </div>
    </>
  );
}
