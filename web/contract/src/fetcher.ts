// The fetch mutator every generated operation calls (orval `override.mutator`).
//
// Generated functions resolve to `{ data, status, headers }`; this builds that
// envelope from a real `Response`, decoding the body by its media type so a
// JSON or problem+json body becomes a value and anything else stays a `Blob`.

let baseUrl = "";

/** Points every generated call at a running harness, e.g. `http://127.0.0.1:4321`. */
export function setBaseUrl(url: string): void {
  baseUrl = url.replace(/\/$/, "");
}

function isJson(contentType: string | null): boolean {
  if (contentType === null) return false;
  const essence = contentType.split(";")[0]?.trim().toLowerCase() ?? "";
  return essence === "application/json" || essence.endsWith("+json");
}

export async function contractFetch<T>(
  url: string,
  init: RequestInit,
): Promise<T> {
  const response = await fetch(`${baseUrl}${url}`, init);
  const contentType = response.headers.get("content-type");
  let data: unknown;
  if (response.status === 204 || response.status === 304) {
    data = undefined;
  } else if (isJson(contentType)) {
    data = await response.json();
  } else {
    data = await response.blob();
  }
  return { data, status: response.status, headers: response.headers } as T;
}
