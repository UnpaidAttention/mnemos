import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { renderWithQuery } from "../test/renderWithQuery";
import { AutonomySettings } from "./AutonomySettings";
import { client } from "../api/client";

vi.mock("../api/client", () => ({
  client: {
    getAutonomyConfig: vi.fn(),
    putAutonomyConfig: vi.fn(),
  },
}));

const defaultConfig = {
  capture: true,
  retention: "distill-and-prune" as const,
  recall_budget_chars: 1200,
};

describe("AutonomySettings", () => {
  beforeEach(() => {
    vi.mocked(client.getAutonomyConfig).mockResolvedValue(defaultConfig);
    vi.mocked(client.putAutonomyConfig).mockResolvedValue({
      saved: true,
      path: "/tmp/config.toml",
      restart_required_for: [],
    });
  });

  it("renders loaded config values", async () => {
    renderWithQuery(<AutonomySettings />);
    expect(await screen.findByRole("checkbox", { name: /capture/i })).toBeChecked();
    expect(await screen.findByRole("combobox", { name: /retention/i })).toHaveValue(
      "distill-and-prune",
    );
  });

  it("shows loading skeleton while config loads", () => {
    vi.mocked(client.getAutonomyConfig).mockReturnValue(new Promise(() => {}));
    renderWithQuery(<AutonomySettings />);
    // Skeleton components render with aria-hidden="true"; config never resolves
    // so the form fields should not yet exist
    expect(screen.queryByRole("checkbox")).not.toBeInTheDocument();
    expect(screen.queryByRole("combobox")).not.toBeInTheDocument();
  });

  it("calls putAutonomyConfig with correct payload on save", async () => {
    renderWithQuery(<AutonomySettings />);
    await screen.findByRole("checkbox", { name: /capture/i });

    // toggle capture off
    await userEvent.click(screen.getByRole("checkbox", { name: /capture/i }));
    await userEvent.click(screen.getByRole("button", { name: /save/i }));

    await waitFor(() =>
      expect(client.putAutonomyConfig).toHaveBeenCalledWith({
        capture: false,
        retention: "distill-and-prune",
        recall_budget_chars: 1200,
      }),
    );
  });

  it("shows saved confirmation after successful save", async () => {
    renderWithQuery(<AutonomySettings />);
    await screen.findByRole("button", { name: /save/i });
    await userEvent.click(screen.getByRole("button", { name: /save/i }));
    expect(await screen.findByText(/saved/i)).toBeInTheDocument();
  });

  it("shows error alert when save fails", async () => {
    vi.mocked(client.putAutonomyConfig).mockRejectedValue(new Error("daemon offline"));
    renderWithQuery(<AutonomySettings />);
    await screen.findByRole("button", { name: /save/i });
    await userEvent.click(screen.getByRole("button", { name: /save/i }));
    expect(await screen.findByRole("alert")).toBeInTheDocument();
  });

  it("renders retention select with distill-and-prune and keep-raw options", async () => {
    renderWithQuery(<AutonomySettings />);
    const select = await screen.findByRole("combobox", { name: /retention/i });
    expect(select).toBeInTheDocument();
    const options = Array.from((select as HTMLSelectElement).options).map((o) => o.value);
    expect(options).toContain("distill-and-prune");
    expect(options).toContain("keep-raw");
  });
});
