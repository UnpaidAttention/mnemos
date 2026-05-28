import { screen } from "@testing-library/react";
import { setupServer } from "msw/node";
import { handlers } from "../test/handlers";
import { renderWithQuery } from "../test/renderWithQuery";
import { Graph } from "./Graph";

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
  default: {
    assign: () => {},
    inferSettings: () => ({}),
  },
}));

const server = setupServer(...handlers);
beforeAll(() => server.listen({ onUnhandledRequest: "error" }));
afterEach(() => server.resetHandlers());
afterAll(() => server.close());

test("renders graph controls and the canvas container", async () => {
  renderWithQuery(<Graph />);
  expect(
    await screen.findByPlaceholderText(/highlight by query/i),
  ).toBeInTheDocument();
  expect(screen.getByLabelText(/community colors/i)).toBeInTheDocument();
});
