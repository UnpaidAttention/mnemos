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

/** Helper: navigate from step 0 to step 3 (connections). */
async function goToStep3() {
  // step 0 → 1 (embedder)
  await userEvent.click(await screen.findByRole("button", { name: /continue/i }));
  await screen.findByText(/bundled embedder ready/i);
  // step 1 → 2 (background service)
  await userEvent.click(await screen.findByRole("button", { name: /continue/i }));
  await screen.findByRole("heading", { name: /enable background memory/i });
  // enable service, then continue
  await userEvent.click(await screen.findByRole("button", { name: /enable background service/i }));
  await userEvent.click(await screen.findByRole("button", { name: /continue/i }));
  // now on step 3
  await screen.findByRole("heading", { name: /connect your ai tools/i });
}

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
  await goToStep3();
  await userEvent.click(await screen.findByRole("button", { name: /finish setup/i }));
  await waitFor(() => expect(onClose).toHaveBeenCalled());
});

// P1-17 — finish() error: wizard stays visible and shows an alert instead of trapping the user
test("shows an error alert on step 3 when completeFirstRun fails (P1-17)", async () => {
  server.use(
    http.post("http://localhost:7423/v1/first-run/complete", () =>
      HttpResponse.error(),
    ),
  );
  const onClose = vi.fn();
  renderWithQuery(<FirstRun onClose={onClose} />);
  await goToStep3();
  await userEvent.click(await screen.findByRole("button", { name: /finish setup/i }));

  // Error alert must appear
  const alert = await screen.findByRole("alert");
  expect(alert).toBeInTheDocument();
  expect(alert).toHaveTextContent(/could not reach the daemon/i);

  // onClose must NOT have been called — wizard is still shown
  expect(onClose).not.toHaveBeenCalled();

  // The Finish setup button is still rendered so the user can retry
  expect(screen.getByRole("button", { name: /finish setup/i })).toBeInTheDocument();
});

// P1-17 — finish() retry: successful retry clears the error and calls onClose
test("clears error and completes wizard on successful retry (P1-17)", async () => {
  let callCount = 0;
  server.use(
    http.post("http://localhost:7423/v1/first-run/complete", () => {
      callCount++;
      if (callCount === 1) return HttpResponse.error();
      return HttpResponse.json({ completed: true });
    }),
  );
  const onClose = vi.fn();
  renderWithQuery(<FirstRun onClose={onClose} />);
  await goToStep3();

  // First click → error
  await userEvent.click(await screen.findByRole("button", { name: /finish setup/i }));
  await screen.findByRole("alert");

  // Second click → success
  await userEvent.click(screen.getByRole("button", { name: /finish setup/i }));
  await waitFor(() => expect(onClose).toHaveBeenCalled());
  // Alert is gone after success
  await waitFor(() => expect(screen.queryByRole("alert")).not.toBeInTheDocument());
});
