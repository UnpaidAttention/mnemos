import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { http, HttpResponse } from "msw";
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

test("Promote-to-procedural posts to the promote endpoint", async () => {
  let promoted: { id?: string; body?: unknown } | null = null;
  server.use(
    http.post(
      "http://localhost:7423/v1/memories/:id/promote",
      async ({ params, request }) => {
        promoted = { id: String(params.id), body: await request.json() };
        return HttpResponse.json({});
      },
    ),
  );
  renderWithQuery(<Reflections />);
  // Wait for the first reflection card + its promote button to render.
  const buttons = await screen.findAllByRole("button", {
    name: /promote to procedural/i,
  });
  await userEvent.click(buttons[0]);
  await waitFor(() => {
    expect(promoted).not.toBeNull();
    expect(promoted!.body).toMatchObject({ tier: "procedural" });
  });
});

// P2-15: a failed promote must surface an error message near the button,
// not silently disappear.
test("promote failure surfaces an error near the button (P2-15)", async () => {
  server.use(
    http.post("http://localhost:7423/v1/memories/:id/promote", () =>
      HttpResponse.json({ error: "daemon error" }, { status: 500 }),
    ),
  );
  renderWithQuery(<Reflections />);
  const buttons = await screen.findAllByRole("button", {
    name: /promote to procedural/i,
  });
  await userEvent.click(buttons[0]);
  await waitFor(() => {
    expect(screen.getByTestId("promote-error")).toBeInTheDocument();
  });
  // Error element must have role="alert" so it is announced to screen readers.
  expect(screen.getByRole("alert")).toBeInTheDocument();
});
