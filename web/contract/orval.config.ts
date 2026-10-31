import { defineConfig } from "orval";

// The contract harness's own orval project (ADR-0002). It reads the 3.1
// document `mise run contract:spec` emits from api/examples/contract.rs, exactly
// as emitted: every gap is closed here or upstream, never by editing the spec.
export default defineConfig({
  contract: {
    input: {
      target: "./openapi.json",
    },
    output: {
      mode: "single",
      target: "./generated/client.ts",
      client: "react-query",
      httpClient: "fetch",
      clean: true,
      override: {
        mutator: {
          path: "./src/fetcher.ts",
          name: "contractFetch",
        },
        // kynos 0.4 describes an `Option<T>` query parameter as `T | null`
        // rather than as merely not required, and orval's default serializer
        // would send a `null` as the literal string "null", which the server
        // rejects or misreads. Absent is the only "no value" the server knows,
        // so this serializer drops `null` with `undefined`. It is the web
        // app's own serializer, so the harness tests what the app ships. The
        // type half of the gap is upstream: getkono/kynos#992 (ADR-0002).
        paramsSerializer: {
          path: "../src/lib/query.ts",
          name: "serializeQuery",
        },
        operations: {
          // kynos describes a multipart file part as the empty schema, which
          // OAS 3.1 defines as `application/octet-stream`, but orval only
          // recognises a string-typed file part, so it types this one
          // `unknown` and its inline `FormData.append` does not typecheck.
          // This builder narrows the part to a `Blob` at run time instead of
          // casting (ADR-0002).
          uploadPoster: {
            formData: {
              path: "./src/form-data.ts",
              name: "posterUploadFormData",
            },
          },
        },
        fetch: {
          // Each operation resolves to `{ data, status, headers }`, a union
          // over every declared status, so a problem body is a typed value a
          // caller narrows on `status` rather than an untyped throw.
          includeHttpResponseReturnType: true,
        },
      },
    },
  },
});
