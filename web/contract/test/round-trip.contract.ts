// Runtime round trip: every harness shape through the generated client
// against the real kynos server `test/harness.ts` started.

import { beforeAll, describe, expect, inject, test } from "vitest";
import {
  downloadPoster,
  getMe,
  getProgress,
  getSession,
  getTitle,
  listTags,
  listTitles,
  uploadPoster,
} from "../generated/client.ts";
import { setBaseUrl } from "../src/fetcher.ts";

const PROBLEMS = "https://stacks.example/problems/";
const SPIRITED_AWAY = "00000000-0000-0000-0000-000000000001";
const REPLY_1988 = "00000000-0000-0000-0000-000000000002";
const UNTITLED = "00000000-0000-0000-0000-000000000003";
const MISSING = "00000000-0000-0000-0000-000000000063";
const PNG = new Uint8Array([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);

beforeAll(() => {
  setBaseUrl(inject("baseUrl"));
});

describe("nullable and required fields", () => {
  test("a present nullable field is its value", async () => {
    const response = await getTitle(SPIRITED_AWAY);
    expect(response.status).toBe(200);
    if (response.status !== 200) return;
    expect(response.data.name).toBe("Spirited Away");
    expect(response.data.original_name).toBe("千と千尋の神隠し");
    expect(response.data.year).toBe(2001);
    expect(response.data.kind).toBe("movie");
    expect(Number.isNaN(Date.parse(response.data.added_at))).toBe(false);
  });

  test("an absent value arrives as null", async () => {
    const response = await getTitle(UNTITLED);
    expect(response.status).toBe(200);
    if (response.status !== 200) return;
    expect(response.data.original_name).toBeNull();
    expect(response.data.year).toBeNull();
  });
});

describe("problem bodies", () => {
  test("a domain error is a typed problem", async () => {
    const response = await getTitle(MISSING);
    expect(response.status).toBe(404);
    if (response.status !== 404) return;
    expect(response.data.type).toBe(`${PROBLEMS}title-not-found`);
    expect(response.data.status).toBe(404);
    expect(response.data.title).toBe("Title not found");
    expect(response.headers.get("content-type")).toContain(
      "application/problem+json",
    );
  });

  test("an extractor rejection is an about:blank problem", async () => {
    const response = await getTitle("not-a-uuid");
    expect(response.status).toBe(400);
    if (response.status !== 400) return;
    expect(response.data.type).toBe("about:blank");
  });
});

describe("tagged union", () => {
  test.each([
    [SPIRITED_AWAY, { state: "completed", score: 10 }],
    [REPLY_1988, { state: "watching", episode: 7 }],
    [UNTITLED, { state: "planned" }],
  ])("progress of %s", async (id, expected) => {
    const response = await getProgress(id);
    expect(response.status).toBe(200);
    if (response.status !== 200) return;
    expect(response.data).toEqual(expected);
  });
});

describe("generic Page<T>", () => {
  test("titles page with optional query parameters", async () => {
    const all = await listTitles();
    expect(all.status).toBe(200);
    if (all.status !== 200) return;
    expect(all.data.items).toHaveLength(3);
    expect(all.data.next_cursor).toBeNull();

    const first = await listTitles({ limit: 1, kind: "movie" });
    expect(first.status).toBe(200);
    if (first.status !== 200) return;
    expect(first.data.items.map((title) => title.id)).toEqual([SPIRITED_AWAY]);
    expect(first.data.next_cursor).toBe("1");

    const cursor = first.data.next_cursor;
    if (cursor == null) throw new Error("expected a next cursor");
    const second = await listTitles({ limit: 1, kind: "movie", cursor });
    expect(second.status).toBe(200);
    if (second.status !== 200) return;
    expect(second.data.items.map((title) => title.id)).toEqual([UNTITLED]);
    expect(second.data.next_cursor).toBeNull();
  });

  test("a null optional parameter is sent as absent", async () => {
    // The spec types this `number | null` (ADR-0002); the serializer drops
    // the null rather than sending the string "null", which would be a 400.
    const response = await listTitles({ limit: null, cursor: null });
    expect(response.status).toBe(200);
  });

  test("a constraint violation is a 400 problem", async () => {
    const response = await listTitles({ limit: 0 });
    expect(response.status).toBe(400);
  });

  test("tags page", async () => {
    const response = await listTags();
    expect(response.status).toBe(200);
    expect(response.data.items.map((tag) => tag.label)).toEqual([
      "comfort",
      "rewatch",
    ]);
  });
});

describe("multipart upload and binary download", () => {
  test("an uploaded poster downloads byte for byte", async () => {
    const file = new Blob([PNG], { type: "image/png" });
    const uploaded = await uploadPoster(REPLY_1988, {
      caption: "Key art",
      file,
    });
    expect(uploaded.status).toBe(201);
    if (uploaded.status !== 201) return;
    expect(uploaded.data).toEqual({
      url: `/titles/${REPLY_1988}/poster`,
      caption: "Key art",
      bytes: PNG.length,
    });
    expect(uploaded.headers.get("location")).toBe(uploaded.data.url);

    const downloaded = await downloadPoster(REPLY_1988);
    expect(downloaded.status).toBe(200);
    if (downloaded.status !== 200) return;
    expect(downloaded.data).toBeInstanceOf(Blob);
    expect(new Uint8Array(await downloaded.data.arrayBuffer())).toEqual(PNG);
    expect(downloaded.headers.get("content-type")).toBe("image/png");
    expect(downloaded.headers.get("content-disposition")).toContain(
      "attachment",
    );

    // The `$ref` property now carries the poster.
    const title = await getTitle(REPLY_1988);
    expect(title.status).toBe(200);
    if (title.status !== 200) return;
    expect(title.data.poster).toEqual(uploaded.data);
  });

  test("the wrong media type is the harness's own 422", async () => {
    const file = new Blob(["GIF89a"], { type: "image/gif" });
    const response = await uploadPoster(UNTITLED, { caption: "Key art", file });
    expect(response.status).toBe(422);
    if (response.status !== 422) return;
    expect(response.data.type).toBe(`${PROBLEMS}not-png`);
  });

  test("a non-Blob file part is refused before sending", async () => {
    await expect(
      uploadPoster(UNTITLED, { caption: "Key art", file: "not a blob" }),
    ).rejects.toThrow(TypeError);
  });

  test("no poster yet is a typed 404", async () => {
    const response = await downloadPoster(UNTITLED);
    expect(response.status).toBe(404);
    if (response.status !== 404) return;
    expect(response.data.type).toBe(`${PROBLEMS}poster-not-found`);
  });
});

describe("authentication", () => {
  // Node's fetch keeps no cookie jar, so the session cookie is sent by hand,
  // exactly as a browser would send the one the server set.
  test("cookie session", async () => {
    const ok = await getSession({
      headers: { cookie: "stacks_session=session-ada" },
    });
    expect(ok.status).toBe(200);
    if (ok.status !== 200) return;
    expect(ok.data).toEqual({ subject: "ada", via: "session" });

    const anonymous = await getSession();
    expect(anonymous.status).toBe(401);

    const suspended = await getSession({
      headers: { cookie: "stacks_session=session-suspended" },
    });
    expect(suspended.status).toBe(403);
  });

  test("bearer token", async () => {
    const ok = await getMe({
      headers: { authorization: "Bearer stk_contract" },
    });
    expect(ok.status).toBe(200);
    if (ok.status !== 200) return;
    expect(ok.data).toEqual({ subject: "ada", via: "token" });

    const wrong = await getMe({ headers: { authorization: "Bearer nope" } });
    expect(wrong.status).toBe(401);
    if (wrong.status !== 401) return;
    expect(wrong.headers.get("www-authenticate")).toContain("Bearer");
  });
});
