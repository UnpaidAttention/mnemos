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

const connected = {
  ...base,
  connected: "full" as const,
};

describe("Connections", () => {
  beforeEach(() => {
    vi.mocked(client.listConnectors).mockResolvedValue([base]);
    vi.mocked(client.previewConnector).mockResolvedValue({ id: "claude-code", edits: [{ path: "~/.claude.json", before: "{}", after: "{...}", already_present: false }] });
    vi.mocked(client.connectConnector).mockResolvedValue({ id: "claude-code", connected: "full" });
    vi.mocked(client.disconnectConnector).mockResolvedValue({ id: "claude-code", connected: "none" });
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

  it("shows an alert when listConnectors rejects", async () => {
    vi.mocked(client.listConnectors).mockRejectedValue(new Error("daemon offline"));
    render(<Connections />);
    const alert = await screen.findByRole("alert");
    expect(alert).toBeInTheDocument();
    expect(alert.textContent).toMatch(/couldn't reach the daemon/i);
  });

  it("surfaces connectConnector error in the apply flow", async () => {
    vi.mocked(client.connectConnector).mockRejectedValue(new Error("config malformed"));
    render(<Connections />);
    fireEvent.click(await screen.findByRole("button", { name: /^connect$/i }));
    fireEvent.click(await screen.findByRole("button", { name: /apply/i }));
    await waitFor(() =>
      expect(screen.getByRole("alert").textContent).toBe("config malformed"),
    );
  });

  it("renders empty state when listConnectors returns an empty array", async () => {
    vi.mocked(client.listConnectors).mockResolvedValue([]);
    render(<Connections />);
    expect(
      await screen.findByText(/no ai tools detected/i),
    ).toBeInTheDocument();
  });

  it("calls disconnectConnector with the correct id when Disconnect is clicked", async () => {
    vi.mocked(client.listConnectors).mockResolvedValue([connected]);
    render(<Connections />);
    fireEvent.click(await screen.findByRole("button", { name: /disconnect/i }));
    await waitFor(() =>
      expect(client.disconnectConnector).toHaveBeenCalledWith("claude-code"),
    );
  });
});
