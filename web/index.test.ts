import { expect, test } from "bun:test";
import { greet } from "./index";

test("greet returns greeting", () => {
  expect(greet("world")).toBe("Hello, world!");
});
