import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { setupServer } from "msw/node";
import { http, HttpResponse } from "msw";
import { vi } from "vitest";
import { renderWithQuery } from "../test/renderWithQuery";
import { SyncStatusPill } from "./SyncStatusPill";

const server = setupServer(
  http.get("http://localhost:7423/v1/sync/status", () =>
    HttpResponse.json({
      backend: "filesystem",
      ready: true,
      detail: "OS-managed",
      last_pushed_at: null,
      last_pulled_at: null,
      last_error: null,
    }),
  ),
);
beforeAll(() => server.listen());
afterEach(() => server.resetHandlers());
afterAll(() => server.close());

test("shows backend name and dispatches sync-pull on click", async () => {
  const onPull = vi.fn();
  window.addEventListener("mnemos:sync-pull", onPull);
  try {
    renderWithQuery(<SyncStatusPill />);
    const btn = await screen.findByRole("button", { name: /filesystem/i });
    await userEvent.click(btn);
    expect(onPull).toHaveBeenCalledOnce();
  } finally {
    window.removeEventListener("mnemos:sync-pull", onPull);
  }
});
