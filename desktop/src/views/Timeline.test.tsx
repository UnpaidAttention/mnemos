import { screen } from "@testing-library/react";
import { setupServer } from "msw/node";
import { handlers } from "../test/handlers";
import { renderWithQuery } from "../test/renderWithQuery";
import { Timeline } from "./Timeline";

const server = setupServer(...handlers);
beforeAll(() => server.listen({ onUnhandledRequest: "error" }));
afterEach(() => server.resetHandlers());
afterAll(() => server.close());

test("renders a timeline bar for a memory", async () => {
  renderWithQuery(<Timeline />);
  expect(await screen.findByText("Rust note")).toBeInTheDocument();
});

test("renders an SVG with the bi-temporal aria-label", async () => {
  renderWithQuery(<Timeline />);
  await screen.findByText("Rust note");
  expect(screen.getByRole("img", { name: /bi-temporal memory timeline/i })).toBeInTheDocument();
});

test("shows the time-travel cursor slider", async () => {
  renderWithQuery(<Timeline />);
  await screen.findByText("Rust note");
  expect(screen.getByLabelText(/time-travel cursor/i)).toBeInTheDocument();
});
