import { http, HttpResponse } from "msw";
import { memFixture } from "./fixtures";
import {
  RICH_AUDIT,
  RICH_COMMUNITIES,
  RICH_EDGES,
  RICH_ENTITIES,
  RICH_ENTITY_DETAIL,
  RICH_ENTITY_NEIGHBORHOOD,
  RICH_GRAPH_NODES,
  RICH_MEMORIES,
  RICH_MEMORIES_BY_ID,
  RICH_PIPELINE,
  RICH_PPR_SCORES,
  RICH_REFLECTIONS,
  RICH_SEARCH_HITS,
} from "./data";

const base = "http://localhost:7423";

// Community summary memories (community-summary tier) — returned by /v1/communities.
const COMMUNITY_SUMMARIES = RICH_MEMORIES.filter(
  (m) => m.type === "community-summary",
);

export const handlers = [
  http.get(`${base}/v1/memories`, () =>
    HttpResponse.json({ memories: RICH_MEMORIES }),
  ),
  http.get(`${base}/v1/memories/missing`, () =>
    HttpResponse.json({ error: "memory not found" }, { status: 404 }),
  ),
  http.get(`${base}/v1/memories/:id`, ({ params }) => {
    const id = String(params.id);
    const hit = RICH_MEMORIES_BY_ID[id];
    if (hit) return HttpResponse.json(hit);
    // Unknown ids fall back to a synthesized memory keyed by the requested id
    // so unrelated tests (e.g. quick-add round-trip) keep working.
    return HttpResponse.json(memFixture({ id }));
  }),
  http.post(`${base}/v1/memories/search`, () =>
    HttpResponse.json({ hits: RICH_SEARCH_HITS }),
  ),
  http.get(`${base}/v1/pipelines`, () => HttpResponse.json(RICH_PIPELINE)),
  http.get(`${base}/v1/reflections`, () =>
    HttpResponse.json({ reflections: RICH_REFLECTIONS }),
  ),
  http.get(`${base}/v1/graph`, () =>
    HttpResponse.json({ nodes: RICH_GRAPH_NODES, edges: RICH_EDGES }),
  ),
  http.post(`${base}/v1/graph/ppr`, () =>
    HttpResponse.json({ scores: RICH_PPR_SCORES }),
  ),
  http.get(`${base}/v1/communities`, () =>
    HttpResponse.json({
      communities: RICH_COMMUNITIES,
      summaries: COMMUNITY_SUMMARIES,
    }),
  ),
  http.get(`${base}/v1/entities`, () =>
    HttpResponse.json({ entities: RICH_ENTITIES }),
  ),
  http.get(`${base}/v1/entities/:id`, ({ params }) => {
    const detail = RICH_ENTITY_DETAIL(String(params.id));
    if (!detail) {
      return HttpResponse.json({ error: "entity not found" }, { status: 404 });
    }
    return HttpResponse.json(detail);
  }),
  http.get(`${base}/v1/entities/:id/graph`, ({ params }) =>
    HttpResponse.json(RICH_ENTITY_NEIGHBORHOOD(String(params.id))),
  ),
  http.get(`${base}/v1/audit`, () =>
    HttpResponse.json({ entries: RICH_AUDIT }),
  ),
  // Per-memory audit: filter the global list down to entries that touched this id.
  http.get(`${base}/v1/memories/:id/audit`, ({ params }) => {
    const id = String(params.id);
    const entries = RICH_AUDIT.filter((e) => e.memory_id === id);
    return HttpResponse.json({ entries });
  }),
];
