import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { setupServer } from "msw/node";
import { http, HttpResponse } from "msw";
import { handlers } from "../test/handlers";
import { renderWithQuery } from "../test/renderWithQuery";
import { Editor } from "./Editor";

const server = setupServer(...handlers);
beforeAll(() => server.listen());
afterEach(() => server.resetHandlers());
afterAll(() => server.close());

test("loads a memory and saves importance via PATCH", async () => {
  let patched: unknown = null;
  server.use(http.patch("http://localhost:7423/v1/memories/:id", async ({ request }) => {
    patched = await request.json();
    return HttpResponse.json({ ...(await import("../test/fixtures")).memFixture(), importance: 0.9 });
  }));
  renderWithQuery(<Editor id="mem_1" />);
  expect(await screen.findByDisplayValue(/Rust note/i)).toBeInTheDocument();
  await userEvent.click(screen.getByRole("button", { name: /save/i }));
  expect(patched).toHaveProperty("importance");
});
