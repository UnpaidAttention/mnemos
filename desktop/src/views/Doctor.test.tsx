import { screen } from "@testing-library/react";
import { setupServer } from "msw/node";
import { http, HttpResponse } from "msw";
import { renderWithQuery } from "../test/renderWithQuery";
import { Doctor } from "./Doctor";

const server = setupServer(
  http.get("http://localhost:7423/v1/doctor", () =>
    HttpResponse.json({
      checks: [
        { name: "schema_version", status: "ok", detail: "v8" },
        { name: "file_db_drift", status: "warn", detail: "0 issues" },
        { name: "embedder", status: "fail", detail: "Ollama unreachable" },
      ],
      report: { files_scanned: 4, db_rows: 4, issues: [] },
    }),
  ),
);
beforeAll(() => server.listen());
afterEach(() => server.resetHandlers());
afterAll(() => server.close());

test("renders the check rows with failures first", async () => {
  renderWithQuery(<Doctor />);
  expect(await screen.findByText(/schema_version/i)).toBeInTheDocument();
  expect(screen.getByText(/Ollama unreachable/i)).toBeInTheDocument();
  const rows = screen.getAllByTestId("doctor-row");
  expect(rows[0]).toHaveTextContent(/embedder/i);
});
