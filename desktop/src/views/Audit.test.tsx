import { screen } from "@testing-library/react";
import { setupServer } from "msw/node";
import { handlers } from "../test/handlers";
import { renderWithQuery } from "../test/renderWithQuery";
import { Audit } from "./Audit";

const server = setupServer(...handlers);
beforeAll(() => server.listen());
afterEach(() => server.resetHandlers());
afterAll(() => server.close());

test("shows audit entries and an Export CSV button", async () => {
  renderWithQuery(<Audit />);
  // The new UI displays action labels as "Created" (capitalized).
  const rows = await screen.findAllByText("Created");
  expect(rows.length).toBeGreaterThan(0);
  expect(screen.getByRole("button", { name: /export csv/i })).toBeInTheDocument();
});
