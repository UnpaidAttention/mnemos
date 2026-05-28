import { screen } from "@testing-library/react";
import { setupServer } from "msw/node";
import { handlers } from "../test/handlers";
import { renderWithQuery } from "../test/renderWithQuery";
import { Browser } from "./Browser";

const server = setupServer(...handlers);
beforeAll(() => server.listen());
afterEach(() => server.resetHandlers());
afterAll(() => server.close());

test("lists memories with their tier", async () => {
  renderWithQuery(<Browser />);
  expect(await screen.findByText("Rust note")).toBeInTheDocument();
  // The richer fixture set contains multiple semantic-tier memories — at
  // least one tier chip labeled "semantic" must render alongside the row.
  expect(screen.getAllByText(/semantic/i).length).toBeGreaterThan(0);
});
