// The query-string serializer every generated operation calls (orval
// `override.paramsSerializer`).
//
// `null` and `undefined` both mean "absent": the server reads an `Option<T>`
// query parameter as missing or present, never as a JSON `null`, so neither
// is written to the query string.

export function serializeQuery(
  params: Readonly<Record<string, unknown>> | undefined,
): string {
  const search = new URLSearchParams();
  for (const [key, value] of Object.entries(params ?? {})) {
    if (value === undefined || value === null) continue;
    if (Array.isArray(value)) {
      for (const item of value) search.append(key, String(item));
    } else {
      search.append(key, String(value));
    }
  }
  return search.toString();
}
