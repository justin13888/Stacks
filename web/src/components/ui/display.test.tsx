// Skeleton, EmptyState, StatusBadge, Rating and ProgressRing: the read-only
// display components.

import { render, screen } from "@testing-library/react";
import { Inbox } from "lucide-react";
import { describe, expect, test } from "vitest";
import { EmptyState } from "./empty-state";
import { ProgressRing } from "./progress-ring";
import { Rating } from "./rating";
import { Skeleton } from "./skeleton";
import { StatusBadge } from "./status-badge";

describe("Skeleton", () => {
  test("is hidden from assistive technology", () => {
    const { container } = render(<Skeleton className="h-4" />);
    expect(container.firstChild).toHaveAttribute("aria-hidden", "true");
  });
});

describe("EmptyState", () => {
  test("names itself by its title and offers its action", () => {
    render(
      <EmptyState
        icon={<Inbox />}
        title="Nothing tracked yet"
        description="Search for a title to start."
        action={<button type="button">Search</button>}
      />,
    );
    const region = screen.getByRole("region", { name: "Nothing tracked yet" });
    expect(region).toHaveTextContent("Search for a title to start.");
    expect(
      screen.getByRole("heading", { name: "Nothing tracked yet" }),
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Search" })).toBeInTheDocument();
  });
});

describe("StatusBadge", () => {
  test("spells the status out", () => {
    render(<StatusBadge status="paused" />);
    expect(screen.getByText("Paused")).toBeInTheDocument();
  });

  test("a rewatch in progress reads as Rewatching", () => {
    render(<StatusBadge status="watching" rewatching />);
    expect(screen.getByText("Rewatching")).toBeInTheDocument();
  });
});

describe("Rating", () => {
  test("renders the user's score format", () => {
    const { rerender } = render(
      <Rating score={85} format="point_10_decimal" />,
    );
    expect(screen.getByText("8.5")).toBeInTheDocument();

    rerender(<Rating score={85} format="point_100" />);
    expect(screen.getByText("85")).toBeInTheDocument();

    rerender(<Rating score={85} format="point_3" />);
    expect(screen.getByText("Liked")).toBeInTheDocument();
  });

  test("five-point scores are stars with a spoken value", () => {
    const { container } = render(<Rating score={80} format="point_5" />);
    expect(
      screen.getByRole("img", { name: "Score: 4 / 5" }),
    ).toBeInTheDocument();
    expect(container.querySelectorAll("[data-filled]")).toHaveLength(4);
  });

  test("no score reads as Unrated", () => {
    render(<Rating score={null} format="point_10" />);
    expect(screen.getByText("Unrated")).toBeInTheDocument();
  });
});

describe("ProgressRing", () => {
  test("is a progressbar with its value and total", () => {
    render(<ProgressRing value={7} max={12} label="Episodes watched" />);
    const bar = screen.getByRole("progressbar", { name: "Episodes watched" });
    expect(bar).toHaveAttribute("aria-valuenow", "7");
    expect(bar).toHaveAttribute("aria-valuemax", "12");
    expect(bar).toHaveAttribute("aria-valuetext", "7 of 12");
  });

  test("with no known total it reports the count and invents no maximum", () => {
    render(<ProgressRing value={30} max={null} label="Episodes watched" />);
    const bar = screen.getByRole("progressbar", { name: "Episodes watched" });
    expect(bar).not.toHaveAttribute("aria-valuemax");
    expect(bar).toHaveAttribute("aria-valuetext", "30");
  });

  test("clamps to the total", () => {
    render(<ProgressRing value={15} max={12} label="Episodes watched" />);
    expect(screen.getByRole("progressbar")).toHaveAttribute(
      "aria-valuenow",
      "12",
    );
  });
});
