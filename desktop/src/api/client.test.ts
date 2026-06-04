import { http, HttpResponse } from "msw";
import { setupServer } from "msw/node";
import { handlers } from "../test/handlers";
import { MnemosClient } from "./client";

const server = setupServer(...handlers);
beforeAll(() => server.listen({ onUnhandledRequest: "error" }));
afterEach(() => server.resetHandlers());
afterAll(() => server.close());

const client = new MnemosClient("http://localhost:7423", async () => "test-token");

test("listMemories returns the memories array", async () => {
  const mems = await client.listMemories({ limit: 10 });
  expect(mems.length).toBeGreaterThan(0);
  expect(mems[0]).toHaveProperty("title");
});

test("search returns hits with explain ranks", async () => {
  const hits = await client.search({ query: "rust", k: 5, explain: true });
  expect(hits[0].memory).toHaveProperty("id");
  expect(hits[0].explain).toHaveProperty("rrf_score");
});

test("a non-OK response throws ApiError with status", async () => {
  await expect(client.getMemory("missing")).rejects.toMatchObject({ status: 404 });
});

test("getAutonomyConfig normalizes an unrecognised retention value to distill-and-prune", async () => {
  server.use(
    http.get("http://localhost:7423/v1/config", () =>
      HttpResponse.json({ autonomy: { capture: true, retention: "totally-invalid", recall_budget_chars: 800 } }),
    ),
  );
  const cfg = await client.getAutonomyConfig();
  expect(cfg.retention).toBe("distill-and-prune");
  expect(cfg.recall_budget_chars).toBe(800);
});
