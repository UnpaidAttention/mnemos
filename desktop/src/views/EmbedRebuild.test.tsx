import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { setupServer } from "msw/node";
import { http, HttpResponse } from "msw";
import { renderWithQuery } from "../test/renderWithQuery";
import { EmbedRebuild } from "./EmbedRebuild";

let started = false;
const server = setupServer(
  http.get("http://localhost:7423/v1/embed-rebuild/status", () =>
    HttpResponse.json(
      started ? { status: "running", processed: 4, total: 10 } : { status: "idle" },
    ),
  ),
  http.post("http://localhost:7423/v1/embed-rebuild/start", () => {
    started = true;
    return HttpResponse.json({ started: true });
  }),
);
beforeAll(() => server.listen());
afterEach(() => {
  server.resetHandlers();
  started = false;
});
afterAll(() => server.close());

test("starts a rebuild with the picked target", async () => {
  renderWithQuery(<EmbedRebuild />);
  const targetSel = await screen.findByLabelText(/target/i);
  await userEvent.selectOptions(targetSel, "bundled");
  await userEvent.click(screen.getByRole("button", { name: /start migration/i }));
  await waitFor(() => expect(started).toBe(true));
  expect(await screen.findByText(/4 of 10/i)).toBeInTheDocument();
});
