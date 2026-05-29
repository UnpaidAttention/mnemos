import { render, screen } from "@testing-library/react";

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
    listMemories: () => stubResponse([]),
    listEntities: () => stubResponse([]),
    listReflections: () => stubResponse([]),
    listAudit: () => stubResponse([]),
    listWorking: () => stubResponse([]),
    pipelines: () => stubResponse({}),
    communities: () => stubResponse({ communities: [], summaries: [] }),
    graph: () => stubResponse({ nodes: [], edges: [] }),
    getDoctor: () =>
      stubResponse({
        checks: [],
        report: { files_scanned: 0, db_rows: 0, issues: [] },
      }),
    getSyncStatus: () =>
      stubResponse({
        backend: "none",
        ready: false,
        detail: "sync disabled",
        last_pushed_at: null,
        last_pulled_at: null,
        last_error: null,
      }),
    getFirstRun: () =>
      stubResponse({ completed_at: "2026-01-01T00:00:00Z" }),
    completeFirstRun: () => stubResponse({ completed: true as const }),
    getConfig: () => stubResponse({}),
  };
  return { client, ApiError };
});

// Stub the WS module so connectEvents() never opens a real socket.
vi.mock("./api/ws", () => ({
  connectEvents: () => () => {},
}));

import App from "./App";

test("renders the app shell with the mnemos brand", async () => {
  render(<App />);
  expect(await screen.findByText(/mnemos/i)).toBeInTheDocument();
});
