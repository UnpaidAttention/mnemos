import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { setupServer } from "msw/node";
import { http, HttpResponse } from "msw";
import { vi } from "vitest";
import { renderWithQuery } from "../test/renderWithQuery";
import { FirstRun } from "./FirstRun";

// Mock enableService from tauri so it resolves in test env
vi.mock("../api/tauri", () => ({
  pickVaultDir: vi.fn(),
  daemonStatus: vi.fn(),
  moveVault: vi.fn(),
  enableService: vi.fn().mockResolvedValue({ enabled: true }),
}));

const server = setupServer(
  http.post("http://localhost:7423/v1/first-run/complete", () =>
    HttpResponse.json({ completed: true }),
  ),
  http.get("http://localhost:7423/v1/connectors", () =>
    HttpResponse.json({ connectors: [] }),
  ),
);
beforeAll(() => server.listen());
afterEach(() => server.resetHandlers());
afterAll(() => server.close());

test("step 1 confirms bundled embedder is ready", async () => {
  renderWithQuery(<FirstRun onClose={() => {}} />);
  await userEvent.click(await screen.findByRole("button", { name: /continue/i }));
  expect(await screen.findByText(/bundled embedder ready/i)).toBeInTheDocument();
  expect(screen.queryByText(/checking ollama/i)).not.toBeInTheDocument();
});

test("step 2 is the background service step", async () => {
  renderWithQuery(<FirstRun onClose={() => {}} />);
  // step 0 → 1
  await userEvent.click(await screen.findByRole("button", { name: /continue/i }));
  await screen.findByText(/bundled embedder ready/i);
  // step 1 → 2
  await userEvent.click(await screen.findByRole("button", { name: /continue/i }));
  expect(await screen.findByRole("heading", { name: /enable background memory/i })).toBeInTheDocument();
});

test("wizard completes via finish setup from connections step", async () => {
  const onClose = vi.fn();
  renderWithQuery(<FirstRun onClose={onClose} />);
  // step 0 → 1 (embedder)
  await userEvent.click(await screen.findByRole("button", { name: /continue/i }));
  await screen.findByText(/bundled embedder ready/i);
  // step 1 → 2 (background service)
  await userEvent.click(await screen.findByRole("button", { name: /continue/i }));
  await screen.findByRole("heading", { name: /enable background memory/i });
  // enable the service (which resolves via mock), then Continue appears
  await userEvent.click(await screen.findByRole("button", { name: /enable background service/i }));
  // step 2 → 3 (connections)
  await userEvent.click(await screen.findByRole("button", { name: /continue/i }));
  // finish
  await userEvent.click(await screen.findByRole("button", { name: /finish setup/i }));
  await waitFor(() => expect(onClose).toHaveBeenCalled());
});
