import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { Connections } from "./Connections";
import { client } from "../api/client";

vi.mock("../api/client", () => ({
  client: {
    listConnectors: vi.fn(),
    previewConnector: vi.fn(),
    connectConnector: vi.fn(),
    disconnectConnector: vi.fn(),
  },
}));

const base = {
  id: "claude-code", display_name: "Claude Code", kind: "detectable" as const,
  deprecated: null, installed: true, connected: "none" as const,
  manual_snippet: null, edits: [{ path: "~/.claude.json", present: false }],
};

describe("Connections", () => {
  beforeEach(() => {
    vi.mocked(client.listConnectors).mockResolvedValue([base]);
    vi.mocked(client.previewConnector).mockResolvedValue({ id: "claude-code", edits: [{ path: "~/.claude.json", before: "{}", after: "{...}", already_present: false }] });
    vi.mocked(client.connectConnector).mockResolvedValue({ id: "claude-code", connected: "full" });
  });

  it("lists detected tools with status", async () => {
    render(<Connections />);
    expect(await screen.findByText("Claude Code")).toBeInTheDocument();
    expect(screen.getByText(/installed/i)).toBeInTheDocument();
  });

  it("previews then connects on confirm", async () => {
    render(<Connections />);
    fireEvent.click(await screen.findByRole("button", { name: /^connect$/i }));
    fireEvent.click(await screen.findByRole("button", { name: /apply/i }));
    await waitFor(() => expect(client.connectConnector).toHaveBeenCalledWith("claude-code"));
  });
});
