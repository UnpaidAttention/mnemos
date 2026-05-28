import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { setupServer } from "msw/node";
import { http, HttpResponse } from "msw";
import { renderWithQuery } from "../test/renderWithQuery";
import { QuickAdd } from "./QuickAdd";

let created: unknown = null;
const server = setupServer(
  http.post("http://localhost:7423/v1/memories", async ({ request }) => {
    created = await request.json();
    return HttpResponse.json({ id: "mem_new" });
  }),
);
beforeAll(() => server.listen());
afterEach(() => server.resetHandlers());
afterAll(() => server.close());

test("creates a memory", async () => {
  renderWithQuery(<QuickAdd open onClose={() => {}} />);
  const textarea = await screen.findByPlaceholderText(/what should mnemos remember/i);
  await userEvent.type(textarea, "Use Tauri 2");
  await userEvent.click(screen.getByRole("button", { name: /add memory/i }));
  expect(created).toMatchObject({ body: "Use Tauri 2" });
});

test("returns null when closed", () => {
  const { container } = renderWithQuery(<QuickAdd open={false} onClose={() => {}} />);
  expect(container.querySelector("[role='dialog']")).toBeNull();
});

test("calls onClose when Escape is pressed", async () => {
  const onClose = vi.fn();
  renderWithQuery(<QuickAdd open onClose={onClose} />);
  const textarea = await screen.findByPlaceholderText(/what should mnemos remember/i);
  await userEvent.type(textarea, "{Escape}");
  expect(onClose).toHaveBeenCalledOnce();
});
