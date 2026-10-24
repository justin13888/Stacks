import { describe, expect, test } from "vitest";
import type { Channel } from "@/api/schemas";
import {
  draftOf,
  emptyDraft,
  toChannelPatch,
  toNewChannel,
  validateChannel,
} from "./channel-form";

const WEBHOOK: Channel = {
  id: "0190a8e0-0000-7000-8000-0000000000e1",
  name: "Home hook",
  enabled: true,
  config: {
    type: "webhook",
    url: "https://hooks.example.com/stacks",
    method: "POST",
    header_names: ["Authorization"],
    has_secret: true,
  },
  created_at: "2026-10-01T00:00:00Z",
  updated_at: "2026-10-01T00:00:00Z",
};

const APPRISE: Channel = {
  ...WEBHOOK,
  name: "Phone",
  config: {
    type: "apprise",
    url: "https://apprise.example.com/notify/key",
    tags: ["tv"],
  },
};

describe("channel form", () => {
  test("an unchanged draft is no change, and keeps secrets and headers", () => {
    expect(toChannelPatch(WEBHOOK, draftOf(WEBHOOK))).toBeNull();
    expect(toChannelPatch(APPRISE, draftOf(APPRISE))).toBeNull();
  });

  test("only what differs is sent, under the channel's own type", () => {
    const draft = {
      ...draftOf(WEBHOOK),
      name: "Renamed",
      method: "PUT" as const,
    };
    expect(toChannelPatch(WEBHOOK, draft)).toEqual({
      name: "Renamed",
      config: { type: "webhook", method: "PUT" },
    });
  });

  test("a new secret replaces the old; clearing stops signing", () => {
    expect(
      toChannelPatch(WEBHOOK, { ...draftOf(WEBHOOK), secret: "new" })?.config,
    ).toEqual({ type: "webhook", secret: "new" });
    expect(
      toChannelPatch(WEBHOOK, { ...draftOf(WEBHOOK), clearSecret: true })
        ?.config,
    ).toEqual({ type: "webhook", secret: null });
  });

  test("replacing headers with none removes them all", () => {
    expect(
      toChannelPatch(WEBHOOK, {
        ...draftOf(WEBHOOK),
        replaceHeaders: true,
        headers: [],
      })?.config,
    ).toEqual({ type: "webhook", headers: null });
  });

  test("Apprise tags are compared as a list and cleared with null", () => {
    expect(
      toChannelPatch(APPRISE, { ...draftOf(APPRISE), tags: " tv , anime" })
        ?.config,
    ).toEqual({ type: "apprise", tags: ["tv", "anime"] });
    expect(
      toChannelPatch(APPRISE, { ...draftOf(APPRISE), tags: "" })?.config,
    ).toEqual({ type: "apprise", tags: null });
  });

  test("a new webhook leaves out what was not given", () => {
    expect(
      toNewChannel({
        ...emptyDraft(),
        name: " Hook ",
        url: "http://localhost:9000/in",
      }),
    ).toEqual({
      name: "Hook",
      config: {
        type: "webhook",
        url: "http://localhost:9000/in",
        method: "POST",
        headers: null,
        secret: null,
      },
    });
  });

  test("validation follows the declared shape", () => {
    expect(validateChannel(emptyDraft())).toEqual({
      name: "Name the channel.",
      url: "Enter an absolute http:// or https:// URL.",
    });
    expect(
      validateChannel({
        ...emptyDraft(),
        name: "x".repeat(101),
        url: "/relative",
      }),
    ).toMatchObject({ name: expect.any(String), url: expect.any(String) });
    expect(
      validateChannel({
        ...emptyDraft(),
        name: "Hook",
        url: "https://example.com",
        headers: [
          { name: "X-A", value: "1" },
          { name: "x-a", value: "2" },
        ],
      }).headers,
    ).toBe("Each header can be set once.");
    expect(
      validateChannel({
        ...draftOf(WEBHOOK),
        headers: [{ name: "Bad Name", value: "1" }],
      }),
    ).toEqual({});
  });
});
