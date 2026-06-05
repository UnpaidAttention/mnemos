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

// P2-15: a failed start must surface an error message near the button.
test("start failure surfaces an error message (P2-15)", async () => {
  server.use(
    http.post("http://localhost:7423/v1/embed-rebuild/start", () =>
      HttpResponse.json({ error: "daemon error" }, { status: 500 }),
    ),
  );
  renderWithQuery(<EmbedRebuild />);
  await screen.findByTestId("rebuild-idle");
  await userEvent.click(screen.getByRole("button", { name: /start migration/i }));
  await waitFor(() => {
    expect(screen.getByTestId("rebuild-action-error")).toBeInTheDocument();
  });
  expect(screen.getByRole("alert")).toBeInTheDocument();
});

// P2-15: abort failure also surfaces an error message.
test("abort failure surfaces an error message (P2-15)", async () => {
  // Put the server in running state so the Abort button is visible.
  server.use(
    http.get("http://localhost:7423/v1/embed-rebuild/status", () =>
      HttpResponse.json({ status: "running", processed: 2, total: 10 }),
    ),
    http.post("http://localhost:7423/v1/embed-rebuild/abort", () =>
      HttpResponse.json({ error: "abort failed" }, { status: 500 }),
    ),
  );
  renderWithQuery(<EmbedRebuild />);
  const abortBtn = await screen.findByRole("button", { name: /abort/i });
  await userEvent.click(abortBtn);
  await waitFor(() => {
    expect(screen.getByTestId("rebuild-action-error")).toBeInTheDocument();
  });
});
