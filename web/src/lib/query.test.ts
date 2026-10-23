import { describe, expect, test } from "vitest";
import { serializeQuery } from "./query";

describe("serializeQuery", () => {
  test("drops null and undefined; the server knows only absent", () => {
    expect(
      serializeQuery({ q: "dune", category: null, cursor: undefined }),
    ).toBe("q=dune");
  });

  test("repeats array members and encodes values", () => {
    expect(serializeQuery({ status: ["planning", "watching"], q: "a b" })).toBe(
      "status=planning&status=watching&q=a+b",
    );
  });
});
