import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { vi } from "vitest";
import { UpdateBanner } from "./UpdateBanner";

const mockDownloadAndInstall = vi.fn(async () => {});
vi.mock("@tauri-apps/plugin-updater", () => ({
  check: vi.fn(async () => ({
    version: "0.7.1",
    currentVersion: "0.7.0",
    downloadAndInstall: mockDownloadAndInstall,
  })),
}));

vi.mock("@tauri-apps/plugin-process", () => ({
  relaunch: vi.fn(async () => {}),
}));

beforeEach(() => {
  mockDownloadAndInstall.mockClear();
});

test("shows the banner with the available version", async () => {
  render(<UpdateBanner />);
  expect(await screen.findByText(/0\.7\.1/i)).toBeInTheDocument();
});

test("clicking Install kicks off downloadAndInstall + relaunch", async () => {
  render(<UpdateBanner />);
  const btn = await screen.findByRole("button", { name: /install/i });
  await userEvent.click(btn);
  await waitFor(() => expect(mockDownloadAndInstall).toHaveBeenCalledOnce());
});

test("clicking Later dismisses the banner", async () => {
  render(<UpdateBanner />);
  const laterBtn = await screen.findByRole("button", { name: /later/i });
  await userEvent.click(laterBtn);
  expect(screen.queryByText(/0\.7\.1/i)).not.toBeInTheDocument();
});
