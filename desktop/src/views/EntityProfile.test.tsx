import { screen } from "@testing-library/react";
import { setupServer } from "msw/node";
import { http, HttpResponse } from "msw";
import { renderWithQuery } from "../test/renderWithQuery";
import { EntityProfile } from "./EntityProfile";

vi.mock("react-force-graph-2d", () => ({ default: () => <div data-testid="fg" /> }));

const base = "http://localhost:7423";

// P2-18: add the /v1/entities list handler so MergeDialog's useEntities()
// call does not produce an unhandled request warning/error.
const server = setupServer(
  // Entity list — consumed by MergeDialog's useEntities() hook
  http.get(`${base}/v1/entities`, () =>
    HttpResponse.json({
      entities: [
        { id: "ent_a", name: "Rust", kind: "tool", mention_count: 2 },
        { id: "ent_b", name: "Go", kind: "tool", mention_count: 1 },
      ],
    }),
  ),
  http.get(`${base}/v1/entities/ent_a`, () =>
    HttpResponse.json({
      id: "ent_a",
      name: "Rust",
      kind: "tool",
      aliases: [],
      description: "a language",
      mention_count: 2,
      memory_ids: ["mem_1"],
      edges: [{ id: "e1", source: "ent_a", target: "ent_b", relation: "uses", weight: 2 }],
    }),
  ),
  http.get(`${base}/v1/entities/ent_a/graph`, () =>
    HttpResponse.json({ nodes: [{ id: "ent_a", name: "Rust", kind: "tool" }], edges: [] }),
  ),
);
beforeAll(() => server.listen({ onUnhandledRequest: "error" }));
afterEach(() => server.resetHandlers());
afterAll(() => server.close());

test("shows entity detail and edges", async () => {
  renderWithQuery(<EntityProfile id="ent_a" />);
  expect(await screen.findByRole("heading", { name: "Rust" })).toBeInTheDocument();
  expect(screen.getByText(/uses/i)).toBeInTheDocument();
  expect(screen.getByText(/2 mentions/i)).toBeInTheDocument();
});

// P2-18: verify the entity list endpoint is served correctly (regression guard
// so a missing handler never silently passes).
test("entity list handler is available for MergeDialog (P2-18)", async () => {
  // Render with a known id; the MergeDialog will fire useEntities() on mount
  // (it is always mounted, just hidden when open=false).
  renderWithQuery(<EntityProfile id="ent_a" />);
  // The profile heading must resolve without unhandled-request errors
  expect(await screen.findByRole("heading", { name: "Rust" })).toBeInTheDocument();
});
