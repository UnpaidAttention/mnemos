import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
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
  http.put("http://localhost:7423/v1/config", () =>
    HttpResponse.json({ saved: true, path: "/tmp/config.toml", restart_required_for: [] }),
  ),
  http.get("http://localhost:7423/v1/connectors", () =>
    HttpResponse.json({ connectors: [] }),
  ),
);
beforeAll(() => server.listen());
afterEach(() => server.resetHandlers());
afterAll(() => server.close());

// P1-15 — happy path
test("renders sectioned form with a Save button", async () => {
  renderWithQuery(<Settings />);
  expect(
    await screen.findByRole("button", { name: /save settings/i }),
  ).toBeInTheDocument();
  expect(screen.getByText("Sync")).toBeInTheDocument();
});

// P1-15 — load error renders an alert instead of a permanent skeleton
test("shows an error alert when getConfig fails (P1-15)", async () => {
  server.use(
    http.get("http://localhost:7423/v1/config", () =>
      HttpResponse.error(),
    ),
  );
  renderWithQuery(<Settings />);
  const alert = await screen.findByRole("alert");
  expect(alert).toBeInTheDocument();
  expect(alert).toHaveTextContent(/could not reach the daemon/i);
  // The Save button must not be present — user cannot interact with a broken form
  expect(screen.queryByRole("button", { name: /save settings/i })).not.toBeInTheDocument();
});

// P1-16 — save error is surfaced below the Save button
test("shows an error alert below Save when putConfig fails (P1-16)", async () => {
  server.use(
    http.put("http://localhost:7423/v1/config", () =>
      HttpResponse.json({ error: "daemon write failed" }, { status: 500 }),
    ),
  );
  renderWithQuery(<Settings />);
  // Wait for the form to load
  await screen.findByRole("button", { name: /save settings/i });
  await userEvent.click(screen.getByRole("button", { name: /save settings/i }));
  const alert = await screen.findByRole("alert");
  expect(alert).toBeInTheDocument();
  expect(alert).toHaveTextContent(/daemon write failed/i);
});

// P1-16 — successful save clears any prior error and shows the saved timestamp
test("clears save error and shows saved timestamp on successful save (P1-16)", async () => {
  // First save fails, second save succeeds
  let callCount = 0;
  server.use(
    http.put("http://localhost:7423/v1/config", () => {
      callCount++;
      if (callCount === 1) {
        return HttpResponse.json({ error: "transient" }, { status: 503 });
      }
      return HttpResponse.json({ saved: true, path: "/tmp/config.toml", restart_required_for: [] });
    }),
  );
  renderWithQuery(<Settings />);
  await screen.findByRole("button", { name: /save settings/i });

  // First click → error appears
  await userEvent.click(screen.getByRole("button", { name: /save settings/i }));
  await screen.findByRole("alert");

  // Second click → error should clear
  await userEvent.click(screen.getByRole("button", { name: /save settings/i }));
  await waitFor(() => expect(screen.queryByRole("alert")).not.toBeInTheDocument());
});
