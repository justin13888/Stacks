// The multipart body builder for `uploadPoster` (orval
// `override.operations.uploadPoster.formData`).
//
// The generated `PosterUpload.file` is `unknown`, because kynos 0.4 describes
// a file part as the empty schema (ADR-0002). This narrows it to a `Blob` at
// run time rather than casting, so a caller passing anything else fails here
// with a clear message instead of sending a malformed part.

import type { PosterUpload } from "../generated/client.ts";

export function posterUploadFormData(body: PosterUpload): FormData {
  if (!(body.file instanceof Blob)) {
    throw new TypeError("PosterUpload.file must be a Blob or File");
  }
  const form = new FormData();
  form.append("caption", body.caption);
  form.append("file", body.file);
  return form;
}
