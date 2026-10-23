import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, test, vi } from "vitest";
import { ApiProblem } from "@/lib/api-client";
import { ErrorState } from "./error-state";

const MODEL =
  "https://github.com/justin13888/Stacks/blob/master/docs/adr/0006-error-model.md#";

function problem(status: number, kind: string, headers: HeadersInit = {}) {
  return new ApiProblem(
    status,
    { type: `${MODEL}${kind}`, status, title: "Upstream" },
    new Headers(headers),
  );
}

describe("ErrorState", () => {
  test("an unavailable provider is explained and can be retried", async () => {
    const onRetry = vi.fn();
    render(
      <ErrorState
        error={problem(503, "upstream-unavailable", { "Retry-After": "20" })}
        onRetry={onRetry}
      />,
    );
    const alert = screen.getByRole("alert");
    expect(alert).toHaveTextContent("A metadata provider is unavailable");
    expect(alert).toHaveTextContent("Try again in 20 s.");
    await userEvent.click(screen.getByRole("button", { name: "Try again" }));
    expect(onRetry).toHaveBeenCalledOnce();
  });

  test("a rate-limited provider says so", () => {
    render(
      <ErrorState
        error={problem(503, "upstream-rate-limited", { "Retry-After": "120" })}
      />,
    );
    expect(screen.getByRole("alert")).toHaveTextContent(
      "A metadata provider is busy",
    );
    expect(screen.getByRole("alert")).toHaveTextContent("Try again in 2 min.");
    expect(screen.queryByRole("button")).not.toBeInTheDocument();
  });

  test("any other problem shows its own title and detail", () => {
    const error = new ApiProblem(
      404,
      {
        type: `${MODEL}title-not-found`,
        status: 404,
        title: "Title not found",
        detail: "No such title.",
      },
      new Headers(),
    );
    render(<ErrorState error={error} />);
    expect(
      screen.getByRole("heading", { name: "Title not found" }),
    ).toBeInTheDocument();
    expect(screen.getByRole("alert")).toHaveTextContent("No such title.");
  });

  test("a network failure reads as offline", () => {
    render(<ErrorState error={new TypeError("Failed to fetch")} />);
    expect(screen.getByRole("alert")).toHaveTextContent(
      "You appear to be offline",
    );
  });
});
