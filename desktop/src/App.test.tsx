import { render, screen } from "@testing-library/react";
import App from "./App";

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

// App.tsx mounts a first-run check + sync status pill + a WebSocket
// connection on launch. Stub fetch + WebSocket so none of those reach
// the real network and trip Vitest's unhandled-rejection guard.
const fetchStub = vi.fn(async (input: RequestInfo | URL) => {
  const url = typeof input === "string" ? input : input.toString();
  if (url.includes("/v1/first-run")) {
    return new Response(
      JSON.stringify({ completed_at: "2026-01-01T00:00:00Z" }),
      { headers: { "content-type": "application/json" } },
    );
  }
  if (url.includes("/v1/sync/status")) {
    return new Response(
      JSON.stringify({
        backend: "none",
        ready: false,
        detail: "sync disabled",
        last_pushed_at: null,
        last_pulled_at: null,
        last_error: null,
      }),
      { headers: { "content-type": "application/json" } },
    );
  }
  return new Response("{}", {
    headers: { "content-type": "application/json" },
  });
});
class StubWebSocket {
  onopen: (() => void) | null = null;
  onmessage: ((e: MessageEvent) => void) | null = null;
  onclose: (() => void) | null = null;
  onerror: (() => void) | null = null;
  readyState = 0;
  close() { /* no-op */ }
  send() { /* no-op */ }
}

beforeAll(() => {
  vi.stubGlobal("fetch", fetchStub);
  vi.stubGlobal("WebSocket", StubWebSocket);
});
afterAll(() => {
  vi.unstubAllGlobals();
});

test("renders the app shell with the mnemos brand", async () => {
  render(<App />);
  expect(await screen.findByText(/mnemos/i)).toBeInTheDocument();
});
