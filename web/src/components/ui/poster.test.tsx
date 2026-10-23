import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, test } from "vitest";
import { Poster } from "./poster";

describe("Poster", () => {
  test("loads lazily, blurred, and settles once loaded", () => {
    const { container } = render(
      <Poster src="https://img/dune.jpg" title="Dune" />,
    );
    const img = screen.getByRole("img", { name: "Dune" });
    expect(img).toHaveAttribute("loading", "lazy");
    expect(container.firstChild).toHaveAttribute("data-state", "loading");
    expect(img.className).toContain("blur-md");

    fireEvent.load(img);
    expect(container.firstChild).toHaveAttribute("data-state", "loaded");
    expect(img.className).not.toContain("blur-md");
  });

  test("a broken image falls back to generated art named by the title", () => {
    const { container } = render(
      <Poster src="https://img/broken.jpg" title="Dune" />,
    );
    fireEvent.error(screen.getByRole("img", { name: "Dune" }));
    expect(container.firstChild).toHaveAttribute("data-state", "fallback");
    expect(screen.getByRole("img", { name: "Dune" }).tagName).toBe("DIV");
  });

  test("no image at all shows the fallback", () => {
    const { container } = render(<Poster src={null} title="Frieren" />);
    expect(container.firstChild).toHaveAttribute("data-state", "fallback");
    expect(screen.getByRole("img", { name: "Frieren" })).toHaveTextContent(
      "Frieren",
    );
  });

  test("keeps the 2:3 poster ratio", () => {
    const { container } = render(<Poster src={null} title="Dune" />);
    expect((container.firstChild as HTMLElement).className).toContain(
      "aspect-[2/3]",
    );
  });
});
