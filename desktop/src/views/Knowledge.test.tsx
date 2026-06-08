import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { renderWithQuery } from "../test/renderWithQuery";
import { Knowledge } from "./Knowledge";
import { client } from "../api/client";
import { memFixture } from "../test/fixtures";
import type { Memory } from "../api/types";

vi.mock("../api/client", () => ({
  client: {
    listMemories: vi.fn(),
    listCorrections: vi.fn(),
    forgetMemory: vi.fn(),
  },
}));

const correction: Memory = memFixture({
  id: "cor_1",
  tier: "procedural",
  type: "rule",
  title: "Don't use purple accents",
  body: "Use teal instead",
});

const hardened: Memory = memFixture({
  id: "hrd_1",
  tier: "reflection",
  type: "reflection",
  title: "Hard rule: always test first",
  body: "TDD matters",
  tags: ["mnemos:hardened"],
});

const factMemory: Memory = memFixture({
  id: "mem_2",
  tier: "semantic",
  type: "fact",
  title: "Rust is fast",
  body: "Rust compiles to native code",
});

describe("Knowledge", () => {
  beforeEach(() => {
    vi.mocked(client.listMemories).mockResolvedValue([factMemory]);
    vi.mocked(client.listCorrections).mockImplementation(
      (opts: { hardened?: boolean; limit?: number } = {}) =>
        Promise.resolve(opts.hardened ? [hardened] : [correction]),
    );
    vi.mocked(client.forgetMemory).mockResolvedValue({ id: "mem_2", status: "forgotten" });
  });

  it("renders memories list from client data", async () => {
    renderWithQuery(<Knowledge />);
    expect(await screen.findByText("Rust is fast")).toBeInTheDocument();
  });

  it("renders corrections from client data when Corrections tab clicked", async () => {
    renderWithQuery(<Knowledge />);
    const tab = await screen.findByRole("button", { name: /corrections/i });
    await userEvent.click(tab);
    expect(await screen.findByText("Don't use purple accents")).toBeInTheDocument();
  });

  it("renders hardened rules from client data when Hardened rules tab clicked", async () => {
    renderWithQuery(<Knowledge />);
    const tab = await screen.findByRole("button", { name: /hardened rules/i });
    await userEvent.click(tab);
    expect(await screen.findByText("Hard rule: always test first")).toBeInTheDocument();
  });

  it("shows empty state when no memories found", async () => {
    vi.mocked(client.listMemories).mockResolvedValue([]);
    vi.mocked(client.listCorrections).mockResolvedValue([] as import("../api/types").Memory[]);
    renderWithQuery(<Knowledge />);
    expect(await screen.findByText(/no items/i)).toBeInTheDocument();
  });

  it("calls forgetMemory when delete is clicked", async () => {
    renderWithQuery(<Knowledge />);
    // Wait for list to render, then expand the row to reveal the Delete button
    const title = await screen.findByText("Rust is fast");
    await userEvent.click(title);
    const deleteBtns = await screen.findAllByRole("button", { name: /delete/i });
    await userEvent.click(deleteBtns[0]);
    await waitFor(() => expect(client.forgetMemory).toHaveBeenCalled());
  });

  it("filters items by search query", async () => {
    vi.mocked(client.listMemories).mockResolvedValue([
      factMemory,
      memFixture({ id: "mem_3", title: "TypeScript basics", body: "TS is typed JS" }),
    ]);
    renderWithQuery(<Knowledge />);
    await screen.findByText("Rust is fast");
    const searchInput = screen.getByPlaceholderText(/search/i);
    await userEvent.type(searchInput, "TypeScript");
    expect(screen.queryByText("Rust is fast")).not.toBeInTheDocument();
    expect(screen.getByText("TypeScript basics")).toBeInTheDocument();
  });
});
