import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { setupServer } from "msw/node";
import { http, HttpResponse } from "msw";
import { vi } from "vitest";
import { renderWithQuery } from "../test/renderWithQuery";
import { FirstRun } from "./FirstRun";

// Mock tauri API so all commands resolve in test env
vi.mock("../api/tauri", () => ({
  pickVaultDir: vi.fn(),
  daemonStatus: vi.fn(),
  moveVault: vi.fn(),
  enableService: vi.fn().mockResolvedValue({ enabled: true }),
  checkOllama: vi.fn().mockResolvedValue({
    installed: false,
    running: false,
    version: null,
    models: [],
  }),
  installOllama: vi.fn().mockResolvedValue(null),
  pullModel: vi.fn().mockResolvedValue(null),
  applyLlmConfig: vi.fn().mockResolvedValue(null),
  applyEmbedderConfig: vi.fn().mockResolvedValue(null),
  checkDownloadedModels: vi.fn().mockResolvedValue({ files: [] }),
  downloadBundledModel: vi.fn().mockResolvedValue(null),
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

/**
 * Navigate from step 0 through to the finish screen (step 5).
 * Uses bundled models (no Ollama) for both embedder and LLM.
 */
async function goToFinish() {
  // step 0 → 1 (search model)
  await userEvent.click(await screen.findByRole("button", { name: /continue/i }));
  await screen.findByRole("heading", { name: /choose search model/i });
  // step 1 → 2 (learning model) — bundled is default, just continue
  await userEvent.click(await screen.findByRole("button", { name: /continue/i }));
  await screen.findByRole("heading", { name: /choose learning model/i });
  // step 2 → 3 (background service) — bundled is default, just continue
  await userEvent.click(await screen.findByRole("button", { name: /continue/i }));
  await screen.findByRole("heading", { name: /enable background memory/i });
  // enable service, then continue
  await userEvent.click(await screen.findByRole("button", { name: /enable background service/i }));
  await userEvent.click(await screen.findByRole("button", { name: /continue/i }));
  // step 4 (connect tools) → step 5 (done)
  await screen.findByRole("heading", { name: /connect your ai tools/i });
  await userEvent.click(await screen.findByRole("button", { name: /continue/i }));
  await screen.findByRole("heading", { name: /you.re all set/i });
}

test("step 1 shows search model picker with bundled option", async () => {
  renderWithQuery(<FirstRun onClose={() => {}} />);
  await userEvent.click(await screen.findByRole("button", { name: /continue/i }));
  expect(await screen.findByRole("heading", { name: /choose search model/i })).toBeInTheDocument();
  expect(screen.getByText(/MiniLM-L6-v2/)).toBeInTheDocument();
});

test("step 2 shows learning model picker", async () => {
  renderWithQuery(<FirstRun onClose={() => {}} />);
  // step 0 → 1
  await userEvent.click(await screen.findByRole("button", { name: /continue/i }));
  await screen.findByRole("heading", { name: /choose search model/i });
  // step 1 → 2
  await userEvent.click(await screen.findByRole("button", { name: /continue/i }));
  expect(await screen.findByRole("heading", { name: /choose learning model/i })).toBeInTheDocument();
  expect(screen.getByText(/Phi-4 Mini/)).toBeInTheDocument();
});

test("wizard completes via finish setup from done step", async () => {
  const onClose = vi.fn();
  renderWithQuery(<FirstRun onClose={onClose} />);
  await goToFinish();
  await userEvent.click(await screen.findByRole("button", { name: /finish setup/i }));
  await waitFor(() => expect(onClose).toHaveBeenCalled());
});

test("shows an error alert when completeFirstRun fails", async () => {
  server.use(
    http.post("http://localhost:7423/v1/first-run/complete", () =>
      HttpResponse.error(),
    ),
  );
  const onClose = vi.fn();
  renderWithQuery(<FirstRun onClose={onClose} />);
  await goToFinish();
  await userEvent.click(await screen.findByRole("button", { name: /finish setup/i }));

  const alert = await screen.findByRole("alert");
  expect(alert).toBeInTheDocument();
  expect(alert).toHaveTextContent(/could not reach the daemon/i);
  expect(onClose).not.toHaveBeenCalled();
  expect(screen.getByRole("button", { name: /finish setup/i })).toBeInTheDocument();
});

test("clears error and completes wizard on successful retry", async () => {
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
  await goToFinish();

  // First click → error
  await userEvent.click(await screen.findByRole("button", { name: /finish setup/i }));
  await screen.findByRole("alert");

  // Second click → success
  await userEvent.click(screen.getByRole("button", { name: /finish setup/i }));
  await waitFor(() => expect(onClose).toHaveBeenCalled());
  await waitFor(() => expect(screen.queryByRole("alert")).not.toBeInTheDocument());
});
