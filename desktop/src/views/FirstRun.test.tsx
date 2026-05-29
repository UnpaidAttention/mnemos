import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { setupServer } from "msw/node";
import { http, HttpResponse } from "msw";
import { renderWithQuery } from "../test/renderWithQuery";
import { FirstRun } from "./FirstRun";

const server = setupServer(
  http.post("http://localhost:7423/v1/first-run/complete", () =>
    HttpResponse.json({ completed: true }),
  ),
);
beforeAll(() => server.listen());
afterEach(() => server.resetHandlers());
afterAll(() => server.close());

test("step 1 confirms bundled embedder is ready", async () => {
  renderWithQuery(<FirstRun onClose={() => {}} />);
  await userEvent.click(await screen.findByRole("button", { name: /continue/i }));
  expect(await screen.findByText(/bundled embedder ready/i)).toBeInTheDocument();
  expect(screen.queryByText(/checking ollama/i)).not.toBeInTheDocument();
});

test("wizard completes via finish setup", async () => {
  const onClose = vi.fn();
  renderWithQuery(<FirstRun onClose={onClose} />);
  await userEvent.click(await screen.findByRole("button", { name: /continue/i }));
  await screen.findByText(/bundled embedder ready/i);
  await userEvent.click(await screen.findByRole("button", { name: /continue/i }));
  await userEvent.click(await screen.findByRole("button", { name: /finish setup/i }));
  await waitFor(() => expect(onClose).toHaveBeenCalled());
});
