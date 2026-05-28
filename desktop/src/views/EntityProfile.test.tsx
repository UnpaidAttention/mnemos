import { screen } from "@testing-library/react";
import { setupServer } from "msw/node";
import { http, HttpResponse } from "msw";
import { renderWithQuery } from "../test/renderWithQuery";
import { EntityProfile } from "./EntityProfile";

vi.mock("react-force-graph-2d", () => ({ default: () => <div data-testid="fg" /> }));

const base = "http://localhost:7423";
const server = setupServer(
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
beforeAll(() => server.listen());
afterEach(() => server.resetHandlers());
afterAll(() => server.close());

test("shows entity detail and edges", async () => {
  renderWithQuery(<EntityProfile id="ent_a" />);
  expect(await screen.findByRole("heading", { name: "Rust" })).toBeInTheDocument();
  expect(screen.getByText(/uses/i)).toBeInTheDocument();
  expect(screen.getByText(/2 mentions/i)).toBeInTheDocument();
});
