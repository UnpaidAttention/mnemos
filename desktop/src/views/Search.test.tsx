import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { setupServer } from "msw/node";
import { handlers } from "../test/handlers";
import { renderWithQuery } from "../test/renderWithQuery";
import { Search } from "./Search";

const server = setupServer(...handlers);
beforeAll(() => server.listen());
afterEach(() => server.resetHandlers());
afterAll(() => server.close());

test("runs a search and shows a result with rank bars", async () => {
  renderWithQuery(<Search />);
  // RouterProvider renders asynchronously; await the input before interacting
  const input = await screen.findByPlaceholderText(/search/i);
  await userEvent.type(input, "rust");
  await userEvent.click(screen.getByRole("button", { name: /search/i }));
  expect(await screen.findByText("Rust note")).toBeInTheDocument();
  // At least one PPR label should appear (rank bar and/or checkbox label)
  expect(screen.getAllByText(/PPR/i).length).toBeGreaterThan(0);
});
