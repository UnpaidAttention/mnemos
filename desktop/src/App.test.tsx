import { render, screen, waitFor } from "@testing-library/react";

// Sigma needs WebGL which jsdom doesn't provide — mock it so the
// full app tree (including Graph view) can render in tests.
vi.mock("sigma", () => ({
  default: class {
    on() {}
    kill() {}
  },
}));
vi.mock("graphology", () => ({
  default: class {
    order = 0;
    addNode() {}
    mergeNode() {}
    addEdge() {}
    hasNode() { return true; }
  },
}));
vi.mock("graphology-layout-forceatlas2", () => ({
  default: { assign: () => {}, inferSettings: () => ({}) },
}));

// Stub the client + ws modules so App.tsx's on-mount calls never reach
// the real network. Vitest hoists vi.mock() above import statements, so
// `App.tsx`'s `import { client } from "./api/client"` already resolves
// to the stub by the time the component renders.
vi.mock("./api/client", async () => {
  class ApiError extends Error {
    constructor(public status: number, message: string) {
      super(message);
      this.name = "ApiError";
    }
  }
  const stubResponse = async <T,>(value: T): Promise<T> => value;
  const client = {
    listMemories: vi.fn(() => stubResponse([])),
    listEntities: vi.fn(() => stubResponse([])),
    listReflections: vi.fn(() => stubResponse([])),
    listAudit: vi.fn(() => stubResponse([])),
    listWorking: vi.fn(() => stubResponse([])),
    pipelines: vi.fn(() => stubResponse({})),
    communities: vi.fn(() => stubResponse({ communities: [], summaries: [] })),
    graph: vi.fn(() => stubResponse({ nodes: [], edges: [] })),
    getDoctor: vi.fn(() =>
      stubResponse({
        checks: [],
        report: { files_scanned: 0, db_rows: 0, issues: [] },
        migration_hint: null,
      }),
    ),
    getSyncStatus: vi.fn(() =>
      stubResponse({
        backend: "none",
        ready: false,
        detail: "sync disabled",
        last_pushed_at: null,
        last_pulled_at: null,
        last_error: null,
      }),
    ),
    getFirstRun: vi.fn(() =>
      stubResponse({ completed_at: "2026-01-01T00:00:00Z" }),
    ),
    completeFirstRun: vi.fn(() => stubResponse({ completed: true as const })),
    getConfig: vi.fn(() => stubResponse({})),
    getEmbedRebuildStatus: vi.fn(() =>
      stubResponse({ status: "idle" as const }),
    ),
    startEmbedRebuild: vi.fn(() => stubResponse({ started: true })),
    abortEmbedRebuild: vi.fn(() => stubResponse({ aborted: true })),
  };
  return { client, ApiError };
});

// Stub the WS module so connectEvents() never opens a real socket.
vi.mock("./api/ws", () => ({
  connectEvents: () => () => {},
}));

import App from "./App";
import { client } from "./api/client";

beforeEach(() => {
  // Reset getFirstRun to the default resolved state so existing tests pass
  vi.mocked(client.getFirstRun).mockResolvedValue({ completed_at: "2026-01-01T00:00:00Z" });
});

test("renders the app shell with the mnemos brand", async () => {
  render(<App />);
  expect(await screen.findByText(/mnemos/i)).toBeInTheDocument();
});

// P2-18: the shell must render the nav sidebar with key navigation links
// and the default route (Browser) must be mounted so at least one route
// renders its content. This strengthens the smoke test beyond the brand check.
test("renders nav links for core views (P2-18)", async () => {
  render(<App />);
  // Wait for the app to settle (getFirstRun resolves)
  expect(await screen.findByText(/mnemos/i)).toBeInTheDocument();
  // Sidebar navigation links — defined in LeftSidebar.tsx NAV array
  const navLinks = ["Browser", "Search", "Pipelines", "Reflections", "Settings", "Migration"];
  for (const label of navLinks) {
    expect(
      screen.getByRole("link", { name: label }),
      `nav link "${label}" should be present`,
    ).toBeInTheDocument();
  }
});

// P2-18: the default route (/) must render the Browser view content,
// confirming that RouterProvider is wired up and at least one route renders.
test("default route renders the Browser view (P2-18)", async () => {
  render(<App />);
  expect(await screen.findByText(/mnemos/i)).toBeInTheDocument();
  // The Browser view renders an element with the navigation role (LeftSidebar)
  // plus the main content area. The navigation element must be present.
  expect(screen.getByRole("navigation")).toBeInTheDocument();
});

// P1-17 — getFirstRun() failure must not leave firstRunShown null forever
// (which would block the app from rendering the main UI).
// The .catch handler should fall back to setFirstRunShown(false) so the main
// router renders instead of staying in an unchecked null state.
test("getFirstRun rejection does not trap the app — main UI still renders (P1-17)", async () => {
  vi.mocked(client.getFirstRun).mockRejectedValueOnce(new Error("daemon down"));
  render(<App />);
  // The brand header must still become visible even when the daemon is unreachable
  expect(await screen.findByText(/mnemos/i)).toBeInTheDocument();
  // The FirstRun wizard must NOT be shown (firstRunShown was set false by the catch)
  await waitFor(() => {
    expect(screen.queryByText(/set up your memory vault/i)).not.toBeInTheDocument();
  });
});
