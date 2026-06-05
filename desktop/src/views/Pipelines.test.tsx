import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { http, HttpResponse } from "msw";
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

// P2-15: a failed decay trigger must surface an error near the button,
// not silently disappear.
test("trigger failure surfaces an error near maintenance buttons (P2-15)", async () => {
  server.use(
    http.post("http://localhost:7423/v1/maintenance/decay", () =>
      HttpResponse.json({ error: "daemon error" }, { status: 500 }),
    ),
  );
  renderWithQuery(<Pipelines />);
  const btn = await screen.findByRole("button", { name: /run decay/i });
  await userEvent.click(btn);
  await waitFor(() => {
    expect(screen.getByTestId("trigger-error")).toBeInTheDocument();
  });
  expect(screen.getByRole("alert")).toBeInTheDocument();
});
