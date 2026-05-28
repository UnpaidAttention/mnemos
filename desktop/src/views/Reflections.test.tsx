import { screen } from "@testing-library/react";
import { setupServer } from "msw/node";
import { handlers } from "../test/handlers";
import { renderWithQuery } from "../test/renderWithQuery";
import { Reflections } from "./Reflections";

const server = setupServer(...handlers);
beforeAll(() => server.listen());
afterEach(() => server.resetHandlers());
afterAll(() => server.close());

test("lists reflections and offers Reflect now", async () => {
  renderWithQuery(<Reflections />);
  expect(await screen.findByText(/Reflection \(insight\)/i)).toBeInTheDocument();
  expect(screen.getByRole("button", { name: /reflect now/i })).toBeInTheDocument();
});
