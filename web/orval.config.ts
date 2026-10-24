import { defineConfig } from "orval";

// The web app's typed client, generated from the committed web/openapi.json
// exactly as the API emits it (`just gen-openapi`). The choices below follow
// the contract harness (web/contract, ADR-0002) and the web-stack ADR
// (ADR-0007). src/api/ is gitignored: `just codegen` regenerates it, and it
// needs no Rust toolchain.
export default defineConfig({
  stacks: {
    input: {
      target: "./openapi.json",
    },
    output: {
      mode: "tags-split",
      target: "./src/api/index.ts",
      schemas: "./src/api/schemas",
      client: "react-query",
      httpClient: "fetch",
      clean: true,
      override: {
        // The only code that talks to the API. It sends the session cookie
        // and turns every problem+json answer into a thrown `ApiProblem`, so
        // TanStack Query sees failures as errors (ADR-0007).
        mutator: {
          path: "./src/lib/api-client.ts",
          name: "apiFetch",
        },
        // kynos 0.4 describes an `Option<T>` query parameter as `T | null`;
        // orval's default serializer would send `null` as the string "null",
        // which the server refuses. Absent is the only "no value" the server
        // knows (getkono/kynos#992, ADR-0002).
        paramsSerializer: {
          path: "./src/lib/query.ts",
          name: "serializeQuery",
        },
        // kynos describes a multipart file part as the empty schema, which
        // OAS 3.1 defines as `application/octet-stream`; orval only
        // recognises a string-typed file part, so it types this one
        // `unknown`. The builder narrows it to a `Blob` at run time instead
        // of casting (ADR-0002). One builder, set here rather than under
        // `operations`, serves every multipart operation (`createImport`,
        // `restoreBackup`).
        formData: {
          path: "./src/lib/form-data.ts",
          name: "multipartFormData",
        },
        fetch: {
          // A hook resolves to the success body; a failure is a thrown
          // `ApiProblem` rather than a `{ data, status }` envelope.
          includeHttpResponseReturnType: false,
        },
      },
    },
  },
});
