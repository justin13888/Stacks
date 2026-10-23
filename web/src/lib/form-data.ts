// The multipart body builder for `createImport` (orval
// `override.operations.createImport.formData`).
//
// The generated `NewImport.file` is `unknown`: kynos describes a file part as
// the empty schema, which OAS 3.1 defines as `application/octet-stream`, and
// orval only recognises a string-typed one (ADR 0002). This narrows it to a
// `Blob` at run time rather than casting, so a caller passing anything else
// fails here with a clear message instead of sending a malformed part.

import type { NewImport } from "@/api/schemas";

export function newImportFormData(body: NewImport): FormData {
  if (!(body.file instanceof Blob)) {
    throw new TypeError("NewImport.file must be a Blob or File");
  }
  const form = new FormData();
  form.append("source", body.source);
  if (body.conflict_strategy != null) {
    form.append("conflict_strategy", body.conflict_strategy);
  }
  form.append("file", body.file);
  return form;
}
