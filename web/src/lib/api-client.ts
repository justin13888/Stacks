// The fetch mutator every orval-generated operation calls (orval
// `override.mutator`), and the only module in the app that calls `fetch`.
//
// - The session is the httpOnly `stacks_session` cookie the API sets, so every
//   request goes out with `credentials: "include"`; no token is ever held in
//   script or storage (ADR-0004).
// - A success body is decoded by its media type: JSON (and any `+json`) becomes
//   a value, an empty body `undefined`, anything else a `Blob`.
// - A failure is thrown as an `ApiProblem` built from the RFC 9457
//   problem+json body (ADR-0006), so TanStack Query sees it as an error.

/** The RFC 9457 problem body every API failure carries. */
export interface ProblemBody {
  type: string;
  status: number;
  title?: string;
  detail?: string;
  instance?: string;
  [extension: string]: unknown;
}

const PROBLEM_TYPE_PREFIX = "docs/adr/0006-error-model.md#";

/**
 * A failed API call: the HTTP status, the problem body and the headers.
 *
 * `P` is the operation's declared problem union, so a caller can narrow
 * `error.problem.type` to the literals the operation documents.
 */
export class ApiProblem<P = ProblemBody> extends Error {
  readonly status: number;
  readonly problem: P & ProblemBody;
  readonly headers: Headers;

  constructor(status: number, problem: P & ProblemBody, headers: Headers) {
    super(problem.detail ?? problem.title ?? `Request failed (${status})`);
    this.name = "ApiProblem";
    this.status = status;
    this.problem = problem;
    this.headers = headers;
  }

  /**
   * The problem's short name, the fragment of its `type` URI in the error
   * model (e.g. `upstream-unavailable`), or `undefined` for `about:blank`.
   */
  get kind(): string | undefined {
    const at = this.problem.type.lastIndexOf(PROBLEM_TYPE_PREFIX);
    if (at === -1) return undefined;
    return this.problem.type.slice(at + PROBLEM_TYPE_PREFIX.length);
  }

  /** Seconds the server asked the client to wait, from `Retry-After`. */
  get retryAfterSeconds(): number | undefined {
    const raw = this.headers.get("retry-after");
    if (raw === null) return undefined;
    const seconds = Number(raw);
    return Number.isFinite(seconds) && seconds >= 0 ? seconds : undefined;
  }

  /** Whether trying again later can succeed: an upstream or rate-limit failure. */
  get retryable(): boolean {
    return this.status === 429 || this.status === 502 || this.status === 503;
  }
}

export function isApiProblem(error: unknown): error is ApiProblem {
  return error instanceof ApiProblem;
}

/**
 * orval's mutator convention: a generated hook's error type is
 * `ErrorType<the operation's problem union>`, which is what this mutator
 * throws.
 */
export type ErrorType<P> = ApiProblem<P>;

function mediaType(contentType: string | null): string {
  return contentType?.split(";")[0]?.trim().toLowerCase() ?? "";
}

function isJson(contentType: string | null): boolean {
  const essence = mediaType(contentType);
  return essence === "application/json" || essence.endsWith("+json");
}

function isProblemBody(value: unknown): value is ProblemBody {
  return (
    typeof value === "object" &&
    value !== null &&
    typeof (value as { type?: unknown }).type === "string" &&
    typeof (value as { status?: unknown }).status === "number"
  );
}

async function readProblem(response: Response): Promise<ProblemBody> {
  if (isJson(response.headers.get("content-type"))) {
    try {
      const body: unknown = await response.json();
      if (isProblemBody(body)) return body;
    } catch {
      // A malformed body falls through to the synthesized problem below.
    }
  }
  // A proxy or gateway answered instead of the API: describe what we have.
  return {
    type: "about:blank",
    status: response.status,
    title: response.statusText || `HTTP ${response.status}`,
  };
}

async function readBody(response: Response): Promise<unknown> {
  if (response.status === 204 || response.status === 205) return undefined;
  if (response.headers.get("content-length") === "0") return undefined;
  if (isJson(response.headers.get("content-type"))) return response.json();
  return response.blob();
}

export async function apiFetch<T>(url: string, init?: RequestInit): Promise<T> {
  const headers = new Headers(init?.headers);
  if (!headers.has("accept")) {
    headers.set("accept", "application/json, application/problem+json");
  }
  if (typeof FormData !== "undefined" && init?.body instanceof FormData) {
    // fetch writes multipart/form-data with its boundary only when unset.
    headers.delete("content-type");
  }
  const response = await fetch(url, {
    ...init,
    headers,
    credentials: "include",
  });
  if (!response.ok) {
    throw new ApiProblem(
      response.status,
      await readProblem(response),
      response.headers,
    );
  }
  return (await readBody(response)) as T;
}
