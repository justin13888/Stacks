// Type-level assertions over the generated client. Checked by `tsc --strict`
// (`bun run typecheck`); nothing here runs.
//
// Two kinds of assertion:
//
// - `@ts-expect-error` lines prove a wrong use is refused. If the generated
//   types loosen, the error disappears and tsc fails on the unused directive.
// - `Equal<…>` pins prove a type is exactly what is claimed. The two pinned
//   under "Known gaps" record a kynos 0.4 shortcoming as it stands today; when
//   the upstream fix lands, the pin stops holding and tsc fails, which is the
//   signal to replace it with the stricter assertion beside it (ADR-0002).

import type {
  Caller,
  downloadPosterResponse,
  getProgressResponse,
  getTitleResponse,
  ListTitlesParams,
  PageTag,
  PageTitle,
  Poster,
  PosterUpload,
  Problem,
  Progress,
  Title,
  uploadPosterResponse,
} from "../generated/client.ts";

type Equal<A, B> =
  (<T>() => T extends A ? 1 : 2) extends <T>() => T extends B ? 1 : 2
    ? true
    : false;
function assertType<T extends true>(): T | undefined {
  return undefined;
}

// --- Nullable fields beside required ones ---

assertType<Equal<Title["name"], string>>();
assertType<Equal<Title["year"], number | null | undefined>>();
declare const title: Title;
// @ts-expect-error a nullable field is not a plain number
const year: number = title.year;
void year;

// --- `$ref` with a sibling description, and a nullable `$ref` ---

assertType<Equal<Title["kind"], "movie" | "show">>();
assertType<Equal<Title["poster"], Poster | null | undefined>>();

// --- The generic Page<T> at two instantiations: distinct types ---

assertType<Equal<PageTitle["items"], Title[]>>();
declare const tags: PageTag;
// @ts-expect-error a page of tags is not a page of titles
const titles: PageTitle = tags;
void titles;

// --- The tagged union narrows on its discriminator ---

function describe(progress: Progress): string {
  switch (progress.state) {
    case "planned":
      // @ts-expect-error `episode` exists only on the watching branch
      return String(progress.episode);
    case "watching":
      return `episode ${progress.episode}`;
    case "completed":
      return `score ${progress.score ?? "none"}`;
  }
}
void describe;
// @ts-expect-error an unknown discriminator value is refused
const unknownState: Progress = { state: "dropped" };
void unknownState;

// --- Problem bodies are typed per status ---

function problemType(response: getTitleResponse): string | undefined {
  if (response.status === 404) {
    assertType<
      Equal<
        typeof response.data.type,
        "https://stacks.example/problems/title-not-found"
      >
    >();
    return response.data.type;
  }
  if (response.status === 200) {
    // @ts-expect-error a 200 carries a Title, which has no problem `type`
    return response.data.type;
  }
  return response.data.detail;
}
void problemType;

function uploadFailure(response: uploadPosterResponse): number {
  switch (response.status) {
    case 201:
      return response.data.bytes;
    case 422:
      // The 422 is either kynos's own refusal or the harness's NotPng.
      assertType<
        Equal<
          typeof response.data.type,
          "about:blank" | "https://stacks.example/problems/not-png"
        >
      >();
      return response.data.status;
    default:
      return response.data.status;
  }
}
void uploadFailure;

// Every declared status, and no other, is a member of the union.
assertType<Equal<getProgressResponse["status"], 200 | 400 | 404>>();
assertType<
  Equal<uploadPosterResponse["status"], 201 | 400 | 404 | 413 | 415 | 422>
>();
declare const problem: Problem;
assertType<Equal<typeof problem.status, number>>();

// --- A binary download is a Blob ---

function download(response: downloadPosterResponse): Blob | undefined {
  if (response.status === 200) {
    assertType<Equal<typeof response.data, Blob>>();
    return response.data;
  }
  return undefined;
}
void download;

// --- Optional query parameters may be omitted ---

const noParams: ListTitlesParams = {};
const someParams: ListTitlesParams = { limit: 10, kind: "show" };
void noParams;
void someParams;
// @ts-expect-error a parameter of the wrong type is refused
const badLimit: ListTitlesParams = { limit: "ten" };
void badLimit;
// @ts-expect-error a value outside the enum is refused
const badKind: ListTitlesParams = { kind: "anime" };
void badKind;

// --- Authenticated operations ---

assertType<Equal<Caller, { subject: string; via: string }>>();

// --- Known gaps (kynos 0.4.0) ---
//
// Optional query parameters are described as `T | null`, so `null` is
// accepted where only absence is meant. The fixed form, once upstream lands:
//   assertType<Equal<ListTitlesParams["limit"], number | undefined>>();
//   // @ts-expect-error an optional query parameter is not nullable
//   const nullLimit: ListTitlesParams = { limit: null };
assertType<Equal<ListTitlesParams["limit"], number | null | undefined>>();

// A multipart file part is described as the empty schema, so it is `unknown`
// rather than `Blob`. The fixed form, once upstream lands:
//   assertType<Equal<PosterUpload["file"], Blob>>();
//   // @ts-expect-error a file part is not a string
//   const textFile: PosterUpload = { caption: "x", file: "not a blob" };
assertType<Equal<PosterUpload["file"], unknown>>();
