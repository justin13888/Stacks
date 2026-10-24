import { useQueryClient } from "@tanstack/react-query";
import { type FormEvent, useState } from "react";
import { getGetMeQueryKey, useUpdateMe } from "@/api/account/account";
import type {
  Preferences,
  ScoreFormat,
  Spoilers,
  Theme,
  TitleLanguage,
  User,
} from "@/api/schemas";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Select, type SelectOption } from "@/components/ui/select";
import { toast } from "@/components/ui/toast";
import { reportFailure } from "@/features/titles/use-tracking";
import { sessionQueryOptions } from "@/hooks/use-auth";
import { formatScore } from "@/lib/score-format";
import { SettingsSection } from "./section";

const SCORE_FORMATS: readonly SelectOption<ScoreFormat>[] = (
  [
    ["point_10_decimal", "10 points, with decimals"],
    ["point_10", "10 points"],
    ["point_100", "100 points"],
    ["point_5", "5 stars"],
    ["point_3", "Disliked / Okay / Liked"],
  ] as const
).map(([value, label]) => ({
  value,
  label: `${label} (${formatScore(85, value)})`,
}));

const TITLE_LANGUAGES: readonly SelectOption<TitleLanguage>[] = [
  { value: "english", label: "English" },
  { value: "romaji", label: "Romaji" },
  { value: "native", label: "Native" },
];

const SPOILERS: readonly SelectOption<Spoilers>[] = [
  { value: "hide", label: "Hide episodes I haven't seen" },
  { value: "show", label: "Show everything" },
];

const THEMES: readonly SelectOption<Theme>[] = [
  { value: "system", label: "System" },
  { value: "light", label: "Light" },
  { value: "dark", label: "Dark" },
];

/** Name and display preferences, saved together through `PATCH /me`. */
export function ProfileForm({ user }: { user: User }) {
  const queryClient = useQueryClient();
  const [name, setName] = useState(user.display_name);
  // Only the preferences edited here are held; the rest are read from the
  // saved user, so a change made elsewhere (the theme in the account menu)
  // shows here and isn't sent back by Save.
  const [edits, setEdits] = useState<Partial<Preferences>>({});
  const preferences: Preferences = { ...user.preferences, ...edits };
  const update = useUpdateMe({
    mutation: {
      onSuccess: (saved) => {
        // Drop the edits the save stored; one made while it ran stays.
        setEdits((current) =>
          Object.fromEntries(
            Object.entries(current).filter(
              ([key, value]) =>
                saved.preferences[key as keyof Preferences] !== value,
            ),
          ),
        );
        // The session holds the user every page reads its preferences
        // from, so the change shows app-wide at once.
        queryClient.setQueryData(getGetMeQueryKey(), saved);
        queryClient.setQueryData(sessionQueryOptions.queryKey, (current) =>
          current ? { ...current, user: saved } : current,
        );
        toast({ tone: "success", title: "Settings saved" });
      },
      onError: reportFailure,
    },
  });

  const trimmed = name.trim();
  const nameError =
    trimmed.length === 0 ? "Your name can't be empty." : undefined;
  const changedPreferences = (
    Object.keys(preferences) as (keyof Preferences)[]
  ).filter((key) => preferences[key] !== user.preferences[key]);
  const dirty = trimmed !== user.display_name || changedPreferences.length > 0;

  function set<K extends keyof Preferences>(key: K, value: Preferences[K]) {
    setEdits((current) => ({ ...current, [key]: value }));
  }

  function submit(event: FormEvent) {
    event.preventDefault();
    if (nameError || !dirty) return;
    update.mutate({
      data: {
        ...(trimmed !== user.display_name ? { display_name: trimmed } : {}),
        ...(changedPreferences.length > 0
          ? {
              preferences: Object.fromEntries(
                changedPreferences.map((key) => [key, preferences[key]]),
              ),
            }
          : {}),
      },
    });
  }

  return (
    <SettingsSection
      title="Profile and preferences"
      description="How Stacks shows titles and scores to you. Scores are always stored out of 100, so changing the format never changes a score."
    >
      <form onSubmit={submit} className="flex flex-col gap-4">
        <Input
          label="Display name"
          value={name}
          maxLength={100}
          error={nameError}
          onChange={(event) => setName(event.target.value)}
        />
        <div className="grid gap-4 sm:grid-cols-2">
          <Select
            label="Score format"
            value={preferences.score_format}
            options={SCORE_FORMATS}
            onValueChange={(value) => set("score_format", value)}
          />
          <Select
            label="Title language"
            value={preferences.title_language}
            options={TITLE_LANGUAGES}
            onValueChange={(value) => set("title_language", value)}
          />
          <Select
            label="Spoilers"
            value={preferences.spoilers}
            options={SPOILERS}
            onValueChange={(value) => set("spoilers", value)}
          />
          <Select
            label="Theme"
            value={preferences.theme}
            options={THEMES}
            onValueChange={(value) => set("theme", value)}
          />
        </div>
        <div className="flex justify-end">
          <Button
            type="submit"
            loading={update.isPending}
            disabled={!dirty || Boolean(nameError)}
          >
            Save changes
          </Button>
        </div>
      </form>
    </SettingsSection>
  );
}
