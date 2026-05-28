import { http, HttpResponse } from "msw";
import { hitFixture, memFixture } from "./fixtures";
const base = "http://localhost:7423";
export const handlers = [
  http.get(`${base}/v1/memories`, () => HttpResponse.json({ memories: [memFixture()] })),
  http.get(`${base}/v1/memories/missing`, () => HttpResponse.json({ error: "memory not found" }, { status: 404 })),
  http.get(`${base}/v1/memories/:id`, ({ params }) => HttpResponse.json(memFixture({ id: String(params.id) }))),
  http.post(`${base}/v1/memories/search`, () => HttpResponse.json({ hits: [hitFixture()] })),
  http.get(`${base}/v1/pipelines`, () => HttpResponse.json({ enabled: true, llm_model: "mock-llm", counters: { completed: 1, failed: 0, facts_added: 3 }, recent: [] })),
  http.get(`${base}/v1/reflections`, () => HttpResponse.json({ reflections: [memFixture({ id: "mem_r", tier: "reflection", type: "reflection", title: "Reflection (insight)" })] })),
  http.get(`${base}/v1/graph`, () => HttpResponse.json({ nodes: [{ id: "ent_a", name: "Rust", kind: "tool", community_id: 0, mentions: 2 }, { id: "ent_b", name: "Tauri", kind: "tool", community_id: 0, mentions: 1 }], edges: [{ id: "edge_1", source: "ent_a", target: "ent_b", relation: "uses", weight: 2 }] })),
  http.get(`${base}/v1/communities`, () => HttpResponse.json({ communities: [{ community_id: 0, members: [{ id: "ent_a", name: "Rust" }] }], summaries: [] })),
  http.get(`${base}/v1/entities`, () => HttpResponse.json({ entities: [{ id: "ent_a", name: "Rust", kind: "tool" }] })),
  http.get(`${base}/v1/audit`, () => HttpResponse.json({ entries: [{ id: 1, ts: "2026-05-01T00:00:00+00:00", actor: "mnemos-cli", action: "create", memory_id: "mem_1", details: null }] })),
];
