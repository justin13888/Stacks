import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, test } from "vitest";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "./tabs";

describe("Tabs", () => {
  test("arrow keys move between tabs and show the matching panel", async () => {
    render(
      <Tabs defaultValue="grid">
        <TabsList aria-label="View">
          <TabsTrigger value="grid">Grid</TabsTrigger>
          <TabsTrigger value="board">Board</TabsTrigger>
        </TabsList>
        <TabsContent value="grid">Grid view</TabsContent>
        <TabsContent value="board">Board view</TabsContent>
      </Tabs>,
    );
    expect(screen.getByRole("tablist", { name: "View" })).toBeInTheDocument();
    const grid = screen.getByRole("tab", { name: "Grid" });
    expect(grid).toHaveAttribute("aria-selected", "true");
    expect(screen.getByRole("tabpanel", { name: "Grid" })).toHaveTextContent(
      "Grid view",
    );

    grid.focus();
    await userEvent.keyboard("{ArrowRight}");
    const board = screen.getByRole("tab", { name: "Board" });
    expect(board).toHaveFocus();
    expect(board).toHaveAttribute("aria-selected", "true");
    expect(screen.getByRole("tabpanel", { name: "Board" })).toHaveTextContent(
      "Board view",
    );
  });
});
