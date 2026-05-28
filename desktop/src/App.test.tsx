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

test("renders the app shell with the mnemos brand", async () => {
  render(<App />);
  expect(await screen.findByText(/mnemos/i)).toBeInTheDocument();
});
