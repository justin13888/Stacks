import { describe, expect, test } from "vitest";
import { multipartFormData } from "./form-data";

describe("multipartFormData", () => {
  test("writes text members and the file, leaving absent ones out", () => {
    const file = new File(["[]"], "watchlist.json", {
      type: "application/json",
    });
    const form = multipartFormData({
      source: "tmdb_json",
      conflict_strategy: null,
      file,
    });
    expect(form.get("source")).toBe("tmdb_json");
    expect(form.has("conflict_strategy")).toBe(false);
    expect(form.get("file")).toBeInstanceOf(Blob);
  });

  test("builds a backup upload, which has only a file", () => {
    const form = multipartFormData({ file: new Blob(["{}"]) });
    expect([...form.keys()]).toEqual(["file"]);
  });

  test("refuses a file that is not a Blob", () => {
    expect(() => multipartFormData({ file: "not a file" })).toThrow(TypeError);
    expect(() =>
      multipartFormData({ source: "mal_xml", file: { bytes: [] } }),
    ).toThrow(TypeError);
  });
});
