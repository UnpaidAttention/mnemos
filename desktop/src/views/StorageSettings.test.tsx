import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { StorageSettings } from "./StorageSettings";
import * as tauri from "../api/tauri";
import { client } from "../api/client";

vi.mock("../api/tauri");
vi.mock("../api/client", () => ({
  client: { getConfig: vi.fn() },
}));

describe("StorageSettings", () => {
  beforeEach(() => {
    vi.mocked(client.getConfig).mockResolvedValue({ vault: { root: "/home/u/.local/share/mnemos" } });
  });

  it("shows the current vault path", async () => {
    render(<StorageSettings />);
    expect(await screen.findByText("/home/u/.local/share/mnemos")).toBeInTheDocument();
  });

  it("moves the vault when a folder is picked and confirmed", async () => {
    vi.mocked(tauri.pickVaultDir).mockResolvedValue("/data/mnemos");
    vi.mocked(tauri.moveVault).mockResolvedValue({ moved_to: "/data/mnemos" });

    render(<StorageSettings />);
    fireEvent.click(await screen.findByRole("button", { name: /change location/i }));
    fireEvent.click(await screen.findByRole("button", { name: /move my data/i }));

    await waitFor(() => expect(tauri.moveVault).toHaveBeenCalledWith("/data/mnemos"));
    expect(await screen.findByText(/moved to/i)).toBeInTheDocument();
  });

  it("shows an error when the config fetch fails", async () => {
    vi.mocked(client.getConfig).mockRejectedValueOnce(new Error("daemon down"));

    render(<StorageSettings />);

    expect(
      await screen.findByText(/couldn't reach the daemon to read your storage location/i),
    ).toBeInTheDocument();
  });
});
