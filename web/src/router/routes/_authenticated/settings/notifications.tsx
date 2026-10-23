import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";
import { useEffect, useState } from "react";
import type {
  ApiNotificationsDtoChannelResponse,
  ApiNotificationsDtoNotificationPreference,
  ApiNotificationsDtoPreferencesResponse,
} from "@/lib/legacy-api";
import { authFetch } from "@/lib/auth";

export const Route = createFileRoute("/_authenticated/settings/notifications")({
  component: NotificationsSettingsPage,
});

const CHANNEL_TYPES = [
  { value: "discord", label: "Discord" },
  { value: "ntfy", label: "ntfy" },
  { value: "webhook", label: "Generic Webhook" },
  { value: "email", label: "Email" },
] as const;

interface CreateChannelPayload {
  channel_type: string;
  name: string;
  config: Record<string, unknown>;
}

function NotificationsSettingsPage() {
  const queryClient = useQueryClient();
  const channelsKey = ["notification-channels"];
  const preferencesKey = ["notification-preferences"];

  const channels = useQuery({
    queryKey: channelsKey,
    queryFn: ({ signal }) =>
      authFetch<ApiNotificationsDtoChannelResponse[]>({
        url: "/api/v1/users/me/notification-channels",
        method: "GET",
        signal,
      }),
  });

  const preferences = useQuery({
    queryKey: preferencesKey,
    queryFn: ({ signal }) =>
      authFetch<ApiNotificationsDtoPreferencesResponse>({
        url: "/api/v1/users/me/notification-preferences",
        method: "GET",
        signal,
      }),
  });

  const createChannel = useMutation({
    mutationFn: (payload: CreateChannelPayload) =>
      authFetch<ApiNotificationsDtoChannelResponse>({
        url: "/api/v1/users/me/notification-channels",
        method: "POST",
        data: payload,
      }),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: channelsKey });
    },
  });

  const deleteChannel = useMutation({
    mutationFn: (id: string) =>
      authFetch<void>({
        url: `/api/v1/users/me/notification-channels/${id}`,
        method: "DELETE",
      }),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: channelsKey });
      queryClient.invalidateQueries({ queryKey: preferencesKey });
    },
  });

  const updatePreferences = useMutation({
    mutationFn: (prefs: ApiNotificationsDtoNotificationPreference[]) =>
      authFetch<ApiNotificationsDtoPreferencesResponse>({
        url: "/api/v1/users/me/notification-preferences",
        method: "PUT",
        data: { preferences: prefs },
      }),
    onSuccess: (next) => {
      queryClient.setQueryData(preferencesKey, next);
    },
  });

  // Local editable copy of the preferences so checkboxes feel responsive
  const [draftPrefs, setDraftPrefs] = useState<
    ApiNotificationsDtoNotificationPreference[]
  >([]);

  useEffect(() => {
    if (preferences.data) setDraftPrefs(preferences.data.preferences);
  }, [preferences.data]);

  const [channelType, setChannelType] = useState<string>(
    CHANNEL_TYPES[0].value,
  );
  const [channelName, setChannelName] = useState("");
  const [channelConfigJson, setChannelConfigJson] = useState("{}");
  const [configError, setConfigError] = useState<string | null>(null);

  const handleCreate = (e: React.FormEvent) => {
    e.preventDefault();
    setConfigError(null);
    let config: Record<string, unknown>;
    try {
      const parsed = JSON.parse(channelConfigJson);
      if (
        typeof parsed !== "object" ||
        parsed === null ||
        Array.isArray(parsed)
      ) {
        throw new Error("Config must be a JSON object.");
      }
      config = parsed as Record<string, unknown>;
    } catch (err) {
      setConfigError(
        err instanceof Error ? err.message : "Invalid JSON for config",
      );
      return;
    }
    createChannel.mutate(
      { channel_type: channelType, name: channelName.trim(), config },
      {
        onSuccess: () => {
          setChannelName("");
          setChannelConfigJson("{}");
        },
      },
    );
  };

  const togglePreference = (idx: number, enabled: boolean) => {
    setDraftPrefs((prev) => {
      const next = prev.map((p, i) => (i === idx ? { ...p, enabled } : p));
      updatePreferences.mutate(next);
      return next;
    });
  };

  return (
    <div
      data-testid="notifications-settings-page"
      className="notifications-settings-page"
    >
      <h1>Notifications</h1>

      <section className="notifications-channels">
        <h2>Channels</h2>
        {channels.isLoading ? (
          <p>Loading channels…</p>
        ) : channels.data && channels.data.length > 0 ? (
          <ul className="notifications-channel-list">
            {channels.data.map((ch) => (
              <li key={ch.id} className="notifications-channel-row">
                <span className="notifications-channel-name">{ch.name}</span>
                <span className="notifications-channel-type">
                  {ch.channel_type}
                </span>
                <span className="notifications-channel-state">
                  {ch.enabled ? "enabled" : "disabled"}
                </span>
                <button
                  type="button"
                  onClick={() => deleteChannel.mutate(ch.id)}
                  disabled={deleteChannel.isPending}
                >
                  Delete
                </button>
              </li>
            ))}
          </ul>
        ) : (
          <p>No channels yet. Add one below.</p>
        )}

        <form className="notifications-create-form" onSubmit={handleCreate}>
          <label htmlFor="channel-type">Type</label>
          <select
            id="channel-type"
            value={channelType}
            onChange={(e) => setChannelType(e.target.value)}
          >
            {CHANNEL_TYPES.map((t) => (
              <option key={t.value} value={t.value}>
                {t.label}
              </option>
            ))}
          </select>

          <label htmlFor="channel-name">Name</label>
          <input
            id="channel-name"
            type="text"
            value={channelName}
            onChange={(e) => setChannelName(e.target.value)}
            required
          />

          <label htmlFor="channel-config">Config (JSON)</label>
          <textarea
            id="channel-config"
            value={channelConfigJson}
            onChange={(e) => setChannelConfigJson(e.target.value)}
            rows={4}
          />
          {configError && (
            <p className="notifications-config-error">{configError}</p>
          )}

          <button
            type="submit"
            disabled={createChannel.isPending || !channelName.trim()}
          >
            {createChannel.isPending ? "Adding…" : "Add channel"}
          </button>
        </form>
      </section>

      <section className="notifications-preferences">
        <h2>Preferences</h2>
        {preferences.isLoading ? (
          <p>Loading preferences…</p>
        ) : draftPrefs.length === 0 ? (
          <p>No preferences configured yet.</p>
        ) : (
          <ul className="notifications-preference-list">
            {draftPrefs.map((pref, idx) => (
              <li
                key={`${pref.event_type}-${pref.channel_id}`}
                className="notifications-preference-row"
              >
                <span className="notifications-preference-event">
                  {pref.event_type}
                </span>
                <span className="notifications-preference-channel">
                  → {pref.channel_id}
                </span>
                <label className="notifications-preference-toggle">
                  <input
                    type="checkbox"
                    checked={pref.enabled}
                    onChange={(e) => togglePreference(idx, e.target.checked)}
                  />
                  Enabled
                </label>
              </li>
            ))}
          </ul>
        )}
      </section>
    </div>
  );
}
