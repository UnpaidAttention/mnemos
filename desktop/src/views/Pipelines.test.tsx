import { screen } from "@testing-library/react";
import { setupServer } from "msw/node";
import { handlers } from "../test/handlers";
import { renderWithQuery } from "../test/renderWithQuery";
import { Pipelines } from "./Pipelines";

const server = setupServer(...handlers);
beforeAll(() => server.listen());
afterEach(() => server.resetHandlers());
afterAll(() => server.close());

test("shows pipeline counters and model", async () => {
  renderWithQuery(<Pipelines />);
  expect(await screen.findByText(/mock-llm/i)).toBeInTheDocument();
  expect(screen.getByText(/facts added/i)).toBeInTheDocument();
});
