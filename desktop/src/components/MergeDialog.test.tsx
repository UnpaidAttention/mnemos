import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { http, HttpResponse } from "msw";
import { setupServer } from "msw/node";
import { renderWithQuery } from "../test/renderWithQuery";
import { MergeDialog } from "./MergeDialog";

const base = "http://localhost:7423";

let merged: unknown = null;
const server = setupServer(
  http.get(`${base}/v1/entities`, () =>
    HttpResponse.json({
      entities: [
        { id: "ent_a", name: "Rust", kind: "tool" },
        { id: "ent_b", name: "Tauri", kind: "tool" },
      ],
    }),
  ),
  http.post(`${base}/v1/entities/merge`, async ({ request }) => {
    merged = await request.json();
    return HttpResponse.json({ source: "ent_a", target: "ent_b", status: "merged" });
  }),
);

beforeAll(() => server.listen());
afterEach(() => {
  server.resetHandlers();
  merged = null;
});
afterAll(() => server.close());

test("merges source into the picked target", async () => {
  renderWithQuery(
    <MergeDialog open source={{ id: "ent_a", name: "Rust" }} onClose={() => {}} />,
  );
  // Wait for the entity list to load and "Tauri" to appear as a target option.
  await userEvent.click(await screen.findByText("Tauri"));
  await userEvent.click(screen.getByRole("button", { name: /^merge$/i }));
  await waitFor(() =>
    expect(merged).toMatchObject({ source: "ent_a", target: "ent_b" }),
  );
});

test("excludes the source entity from target list", async () => {
  renderWithQuery(
    <MergeDialog open source={{ id: "ent_a", name: "Rust" }} onClose={() => {}} />,
  );
  // Wait for entities to load.
  await screen.findByText("Tauri");
  // Picker options render as <button> elements inside the dialog list.
  // The source entity ("Rust") must not appear as a selectable option.
  const buttons = screen
    .getAllByRole("button")
    .filter((b) => b.textContent?.includes("Rust"));
  expect(buttons).toHaveLength(0);
});
