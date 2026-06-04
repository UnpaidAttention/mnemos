import { screen } from "@testing-library/react";
import { setupServer } from "msw/node";
import { http, HttpResponse } from "msw";
import { renderWithQuery } from "../test/renderWithQuery";
import { Settings } from "./Settings";

const cfgResponse = {
  daemon: { host: "127.0.0.1", port: 7423 },
  embedder: {
    kind: "ollama",
    url: "http://localhost:11434",
    model: "nomic-embed-text",
    dim: 768,
    timeout_secs: 30,
  },
  llm: {
    kind: "ollama",
    url: "http://localhost:11434",
    model: "llama3",
    timeout_secs: 30,
  },
  retrieval: {
    default_k: 10,
    rrf_k: 60,
    ppr_alpha: 0.85,
    ppr_iterations: 50,
    reweight: { recency_decay: 0.02, importance_weight: 1 },
  },
  reflection: { salience_threshold: 10, max_sources: 30 },
  community: { min_community_size: 3 },
  sync: {
    kind: "none",
    interval_secs: 0,
    git: { remote: "", branch: "main" },
    s3: { remote: "" },
  },
  autonomy: {
    capture: true,
    retention: "distill-and-prune",
    recall_budget_chars: 1200,
  },
};

const server = setupServer(
  http.get("http://localhost:7423/v1/config", () => HttpResponse.json(cfgResponse)),
  http.get("http://localhost:7423/v1/connectors", () =>
    HttpResponse.json({ connectors: [] }),
  ),
);
beforeAll(() => server.listen());
afterEach(() => server.resetHandlers());
afterAll(() => server.close());

test("renders sectioned form with a Save button", async () => {
  renderWithQuery(<Settings />);
  expect(
    await screen.findByRole("button", { name: /save settings/i }),
  ).toBeInTheDocument();
  expect(screen.getByText("Sync")).toBeInTheDocument();
});
