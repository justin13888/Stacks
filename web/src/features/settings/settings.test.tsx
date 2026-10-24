// Settings in the real app: preferences that change rendering everywhere,
// sessions, API tokens shown once, and notification channels.

import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, test } from "vitest";
import type {
  ApiToken,
  Channel,
  LibraryEntry,
  Session,
  Subscription,
  User,
} from "@/api/schemas";
import { expectNoSeriousA11yViolations } from "@/features/library/axe";
import { fakeApi, makeTitle } from "@/features/library/fake-api";
import { json, makeUser, renderApp } from "@/test/utils";
import { describeDevice } from "./sessions";

const FRIEREN = makeTitle({
  ref: "anilist:show:154587",
  title: "Frieren: Beyond Journey's End",
  original_title: "葬送のフリーレン",
  category: "anime",
});

const ENTRY: LibraryEntry = {
  title: { ...FRIEREN, library: null },
  status: "watching",
  score: 80,
  favourite: false,
  notes: null,
  progress: {
    runs: 1,
    rewatches: 0,
    plays: 0,
    current_run_plays: 0,
    current_run_episodes: 0,
  },
  added_at: "2026-01-01T00:00:00Z",
  updated_at: "2026-10-01T00:00:00Z",
};

const THIS_BROWSER: Session = {
  id: "0190a8e0-0000-7000-8000-0000000000aa",
  current: true,
  user_agent:
    "Mozilla/5.0 (X11; Linux x86_64; rv:140.0) Gecko/20100101 Firefox/140.0",
  created_at: "2026-10-01T00:00:00Z",
  last_seen_at: "2026-10-20T00:00:00Z",
  expires_at: "2026-11-01T00:00:00Z",
};

const PHONE: Session = {
  id: "0190a8e0-0000-7000-8000-0000000000ab",
  current: false,
  user_agent:
    "Mozilla/5.0 (iPhone; CPU iPhone OS 19_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/19.0 Mobile/15E148 Safari/604.1",
  ip: "203.0.113.7",
  created_at: "2026-09-01T00:00:00Z",
  last_seen_at: "2026-10-19T00:00:00Z",
  expires_at: "2026-11-01T00:00:00Z",
};

const OLD_TOKEN: ApiToken = {
  id: "0190a8e0-0000-7000-8000-0000000000d1",
  name: "Old script",
  prefix: "stk_ab12",
  scopes: ["read"],
  created_at: "2026-01-01T00:00:00Z",
  last_used_at: null,
  expires_at: null,
};

const NOW = "2026-10-20T12:00:00Z";

interface Server {
  sessions?: Session[];
  tokens?: ApiToken[];
  channels?: Channel[];
  subscriptions?: Subscription[];
}

/** A settings API: a stateful store of sessions, tokens and channels. */
function settingsServer(seed: Server = {}) {
  const sessions = [...(seed.sessions ?? [THIS_BROWSER])];
  let tokens = [...(seed.tokens ?? [])];
  let channels = [...(seed.channels ?? [])];
  const subscriptions = [...(seed.subscriptions ?? [])];
  let user: User = makeUser();
  const fake = fakeApi({
    titles: [FRIEREN],
    extra: (request, path) => {
      const method = request.method;
      // The fake has read the body already, and recorded it as a write.
      // biome-ignore lint/suspicious/noExplicitAny: a test double's request body
      const body: any = method === "GET" ? undefined : fake.writes.at(-1)?.body;
      if (method === "PATCH" && path === "/me") {
        user = {
          ...user,
          ...(body.display_name ? { display_name: body.display_name } : {}),
          preferences: { ...user.preferences, ...body.preferences },
        };
        return json(user);
      }
      if (method === "GET" && path === "/library") {
        return json({ items: [ENTRY], next_cursor: null });
      }
      if (method === "GET" && path === "/sessions") {
        return json({ items: sessions, next_cursor: null });
      }
      const sessionMatch = path.match(/^\/sessions\/([^/]+)$/);
      if (method === "DELETE" && sessionMatch) {
        const at = sessions.findIndex((s) => s.id === sessionMatch[1]);
        sessions.splice(at, 1);
        return new Response(null, { status: 204 });
      }
      if (method === "GET" && path === "/me/api-tokens") {
        return json({ items: tokens, next_cursor: null });
      }
      if (method === "POST" && path === "/me/api-tokens") {
        const token: ApiToken = {
          id: "0190a8e0-0000-7000-8000-0000000000d2",
          name: body.name,
          prefix: "stk_9f8e",
          scopes: body.scopes,
          created_at: NOW,
          expires_at: body.expires_at ?? null,
          last_used_at: null,
        };
        tokens = [token, ...tokens];
        return json({ token, secret: "stk_9f8e_secret-value-shown-once" }, 201);
      }
      const tokenMatch = path.match(/^\/me\/api-tokens\/([^/]+)$/);
      if (method === "DELETE" && tokenMatch) {
        tokens = tokens.filter((t) => t.id !== tokenMatch[1]);
        return new Response(null, { status: 204 });
      }
      if (method === "GET" && path === "/notification-channels") {
        return json({ items: channels, next_cursor: null });
      }
      if (method === "POST" && path === "/notification-channels") {
        const config =
          body.config.type === "webhook"
            ? {
                type: "webhook" as const,
                url: body.config.url,
                method: body.config.method ?? "POST",
                header_names: Object.keys(body.config.headers ?? {}),
                has_secret: Boolean(body.config.secret),
              }
            : {
                type: "apprise" as const,
                url: body.config.url,
                tags: body.config.tags ?? [],
              };
        const channel: Channel = {
          id: "0190a8e0-0000-7000-8000-0000000000e1",
          name: body.name,
          enabled: true,
          config,
          created_at: NOW,
          updated_at: NOW,
        };
        channels = [...channels, channel];
        return json(channel, 201);
      }
      if (method === "GET" && path === "/notification-subscriptions") {
        return json({ items: subscriptions, next_cursor: null });
      }
      if (method === "POST" && path === "/notification-subscriptions") {
        const subscription: Subscription = {
          id: "0190a8e0-0000-7000-8000-0000000000f1",
          channel_id: body.channel_id,
          event: body.event,
          enabled: true,
          created_at: NOW,
          updated_at: NOW,
        };
        subscriptions.push(subscription);
        return json(subscription, 201);
      }
      return undefined;
    },
  });
  return fake;
}

async function chooseOption(name: string, option: string | RegExp) {
  await userEvent.click(screen.getByRole("combobox", { name }));
  await userEvent.click(await screen.findByRole("option", { name: option }));
}

describe("settings", () => {
  test("saved preferences change how every page renders scores and titles", async () => {
    const fake = settingsServer();
    const { router } = renderApp("/settings");
    await screen.findByRole(
      "heading",
      { name: "Profile and preferences" },
      {
        timeout: 5000,
      },
    );

    await chooseOption("Score format", /5 stars/);
    await chooseOption("Title language", "Native");
    await userEvent.click(screen.getByRole("button", { name: "Save changes" }));

    await screen.findByText("Settings saved");
    expect(fake.writesTo("/me")).toEqual([
      {
        method: "PATCH",
        path: "/me",
        body: {
          preferences: { score_format: "point_5", title_language: "native" },
        },
      },
    ]);
    // Nothing left to save once it is saved.
    expect(screen.getByRole("button", { name: "Save changes" })).toBeDisabled();

    await router.navigate({ to: "/library" });
    const card = await screen.findByRole(
      "article",
      { name: "葬送のフリーレン" },
      { timeout: 5000 },
    );
    expect(
      within(card).getByRole("img", { name: "Score: 4 / 5" }),
    ).toBeInTheDocument();
  });

  test("an empty display name can't be saved", async () => {
    const fake = settingsServer();
    renderApp("/settings");
    const name = await screen.findByLabelText("Display name", undefined, {
      timeout: 5000,
    });
    await userEvent.clear(name);
    expect(screen.getByText("Your name can't be empty.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Save changes" })).toBeDisabled();
    expect(fake.writesTo("/me")).toEqual([]);
  });

  test("a session is revoked only after confirming, and this browser can't be", async () => {
    const fake = settingsServer({ sessions: [THIS_BROWSER, PHONE] });
    renderApp("/settings");
    await screen.findByText("Safari on iOS", undefined, { timeout: 5000 });
    expect(screen.getByText("This browser")).toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "Revoke Firefox on Linux" }),
    ).not.toBeInTheDocument();

    await userEvent.click(
      screen.getByRole("button", { name: "Revoke Safari on iOS" }),
    );
    const dialog = await screen.findByRole("dialog", {
      name: "Revoke this session?",
    });
    await userEvent.click(
      within(dialog).getByRole("button", { name: "Cancel" }),
    );
    expect(fake.writesTo("/sessions")).toEqual([]);

    await userEvent.click(
      screen.getByRole("button", { name: "Revoke Safari on iOS" }),
    );
    await userEvent.click(
      await screen.findByRole("button", { name: "Revoke session" }),
    );
    await waitFor(() =>
      expect(
        screen.queryByRole("button", { name: "Revoke Safari on iOS" }),
      ).not.toBeInTheDocument(),
    );
    expect(fake.writesTo("/sessions")).toEqual([
      { method: "DELETE", path: `/sessions/${PHONE.id}`, body: undefined },
    ]);
  });

  test("a new API token is shown once, and can't be read back after the dialog closes", async () => {
    const fake = settingsServer();
    renderApp("/settings");
    await userEvent.click(
      await screen.findByRole(
        "button",
        { name: "New token" },
        { timeout: 5000 },
      ),
    );
    const form = await screen.findByRole("dialog", { name: "New API token" });
    await userEvent.type(within(form).getByLabelText("Name"), "Home sync");
    await userEvent.click(
      within(form).getByRole("checkbox", { name: /^Write/ }),
    );
    await chooseOption("Expires", "Never");
    await userEvent.click(
      within(form).getByRole("button", { name: "Create token" }),
    );

    const shown = await screen.findByRole("dialog", {
      name: "Copy your token",
    });
    expect(within(shown).getByLabelText("Token")).toHaveValue(
      "stk_9f8e_secret-value-shown-once",
    );
    expect(within(shown).getByRole("alert")).toHaveTextContent(
      "can't be shown again",
    );
    expect(fake.writesTo("/me/api-tokens")).toEqual([
      {
        method: "POST",
        path: "/me/api-tokens",
        body: {
          name: "Home sync",
          scopes: ["read", "write"],
          expires_at: null,
        },
      },
    ]);

    await userEvent.click(within(shown).getByRole("button", { name: "Done" }));
    await waitFor(() =>
      expect(screen.queryByRole("dialog")).not.toBeInTheDocument(),
    );
    // The listing shows the token by its prefix, never its secret.
    expect(await screen.findByText("Home sync")).toBeInTheDocument();
    expect(
      screen.queryByDisplayValue("stk_9f8e_secret-value-shown-once"),
    ).not.toBeInTheDocument();
    expect(document.body.textContent?.includes("secret-value-shown-once")).toBe(
      false,
    );

    // Opening the dialog again starts a new token rather than showing the old.
    await userEvent.click(screen.getByRole("button", { name: "New token" }));
    expect(
      await screen.findByRole("dialog", { name: "New API token" }),
    ).toBeInTheDocument();
    expect(
      screen.queryByDisplayValue("stk_9f8e_secret-value-shown-once"),
    ).not.toBeInTheDocument();
  });

  test("revoking a token asks first, then removes it", async () => {
    const fake = settingsServer({ tokens: [OLD_TOKEN] });
    renderApp("/settings");
    await userEvent.click(
      await screen.findByRole(
        "button",
        { name: "Revoke Old script" },
        { timeout: 5000 },
      ),
    );
    await userEvent.click(
      await screen.findByRole("button", { name: "Revoke token" }),
    );
    await waitFor(() =>
      expect(screen.queryByText("Old script")).not.toBeInTheDocument(),
    );
    expect(fake.writesTo("/me/api-tokens")).toEqual([
      {
        method: "DELETE",
        path: `/me/api-tokens/${OLD_TOKEN.id}`,
        body: undefined,
      },
    ]);
    expect(screen.getByText("No API tokens")).toBeInTheDocument();
  });

  test("the channel form checks the declared config shape before sending it", async () => {
    const fake = settingsServer();
    renderApp("/settings");
    await userEvent.click(
      await screen.findByRole(
        "button",
        { name: "Add channel" },
        { timeout: 5000 },
      ),
    );
    const dialog = await screen.findByRole("dialog", { name: "Add a channel" });
    await userEvent.click(
      within(dialog).getByRole("button", { name: "Add channel" }),
    );
    expect(within(dialog).getByText("Name the channel.")).toBeInTheDocument();
    expect(
      within(dialog).getByText("Enter an absolute http:// or https:// URL."),
    ).toBeInTheDocument();

    await userEvent.type(within(dialog).getByLabelText("Name"), "Home hook");
    await userEvent.type(
      within(dialog).getByLabelText("URL"),
      "ftp://hooks.example.com",
    );
    await userEvent.click(
      within(dialog).getByRole("button", { name: "Add header" }),
    );
    await userEvent.type(
      within(dialog).getByLabelText("Header 1 name"),
      "Bad Name",
    );
    await userEvent.click(
      within(dialog).getByRole("button", { name: "Add channel" }),
    );
    expect(
      within(dialog).getByText("Enter an absolute http:// or https:// URL."),
    ).toBeInTheDocument();
    expect(within(dialog).getByText(/A header name can't/)).toBeInTheDocument();
    expect(fake.writesTo("/notification-channels")).toEqual([]);

    const url = within(dialog).getByLabelText("URL");
    await userEvent.clear(url);
    await userEvent.type(url, "https://hooks.example.com/stacks");
    const headerName = within(dialog).getByLabelText("Header 1 name");
    await userEvent.clear(headerName);
    await userEvent.type(headerName, "Authorization");
    await userEvent.type(
      within(dialog).getByLabelText("Header 1 value"),
      "Bearer abc",
    );
    await userEvent.type(
      within(dialog).getByLabelText("Signing secret"),
      "shh",
    );
    await userEvent.click(
      within(dialog).getByRole("button", { name: "Add channel" }),
    );

    await screen.findByText("Home hook");
    expect(fake.writesTo("/notification-channels")).toEqual([
      {
        method: "POST",
        path: "/notification-channels",
        body: {
          name: "Home hook",
          config: {
            type: "webhook",
            url: "https://hooks.example.com/stacks",
            method: "POST",
            headers: { Authorization: "Bearer abc" },
            secret: "shh",
          },
        },
      },
    ]);
  });

  test("an Apprise channel sends its tags, and an event subscribes it", async () => {
    const fake = settingsServer();
    renderApp("/settings");
    await userEvent.click(
      await screen.findByRole(
        "button",
        { name: "Add channel" },
        { timeout: 5000 },
      ),
    );
    const dialog = await screen.findByRole("dialog", { name: "Add a channel" });
    await userEvent.type(within(dialog).getByLabelText("Name"), "Phone");
    await chooseOption("Type", "Apprise");
    await userEvent.type(
      within(dialog).getByLabelText("URL"),
      "https://apprise.example.com/notify/key",
    );
    await userEvent.type(within(dialog).getByLabelText("Tags"), "tv, anime");
    await userEvent.click(
      within(dialog).getByRole("button", { name: "Add channel" }),
    );
    await screen.findByText("Phone");
    expect(fake.writesTo("/notification-channels")[0]?.body).toEqual({
      name: "Phone",
      config: {
        type: "apprise",
        url: "https://apprise.example.com/notify/key",
        tags: ["tv", "anime"],
      },
    });

    await userEvent.click(
      screen.getByRole("checkbox", { name: "A new episode is out" }),
    );
    await waitFor(() =>
      expect(fake.writesTo("/notification-subscriptions")).toEqual([
        {
          method: "POST",
          path: "/notification-subscriptions",
          body: {
            channel_id: "0190a8e0-0000-7000-8000-0000000000e1",
            event: "new_episode",
          },
        },
      ]),
    );
    expect(
      await screen.findByRole("checkbox", { name: "A new episode is out" }),
    ).toBeChecked();
  });

  test("the page has no serious accessibility violations", async () => {
    settingsServer({ sessions: [THIS_BROWSER, PHONE], tokens: [OLD_TOKEN] });
    const { container } = renderApp("/settings");
    await screen.findByText("Old script", undefined, { timeout: 5000 });
    await expectNoSeriousA11yViolations(container);
  });
});

describe("describeDevice", () => {
  test.each([
    [THIS_BROWSER.user_agent, "Firefox on Linux"],
    [PHONE.user_agent, "Safari on iOS"],
    [
      "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/140.0 Safari/537.36 Edg/140.0",
      "Edge on Windows",
    ],
    ["curl/8.0", "curl/8.0"],
    [null, "Unknown device"],
  ])("%s reads as %s", (agent, expected) => {
    expect(describeDevice(agent)).toBe(expected);
  });
});
