// The multipart body builder for every multipart operation (`createImport`,
// `restoreBackup`; orval `override.formData`).
//
// The generated file members (`NewImport.file`, `BackupUpload.file`) are
// `unknown`: kynos describes a file part as the empty schema, which OAS 3.1
// defines as `application/octet-stream`, and orval only recognises a
// string-typed one (ADR 0002). This narrows each member at run time rather
// than casting: a `Blob` becomes a file part, a string or number a text part,
// and anything else fails here with a clear message instead of sending a
// malformed part. Absent members are left out.

import type { BackupUpload, NewImport } from "@/api/schemas";

export function multipartFormData(body: NewImport | BackupUpload): FormData {
  const form = new FormData();
  for (const [name, value] of Object.entries(body)) {
    if (value == null) {
      continue;
    }
    if (value instanceof Blob) {
      form.append(name, value);
    } else if (typeof value === "string" || typeof value === "number") {
      form.append(name, String(value));
    } else {
      throw new TypeError(
        `multipart member ${name} must be a Blob, a string or a number`,
      );
    }
  }
  if (!(form.get("file") instanceof Blob)) {
    throw new TypeError("the upload's file must be a Blob or File");
  }
  return form;
}
