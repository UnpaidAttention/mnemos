import type { Memory, RecallHit } from "../api/types";
export const memFixture = (over: Partial<Memory> = {}): Memory => ({
  id: "mem_1", tier: "semantic", type: "fact", title: "Rust note", body: "Shaun prefers Rust",
  tags: [], entities: [], links: [], provenance: [], created_at: "2026-05-01T00:00:00+00:00",
  ingested_at: "2026-05-01T00:00:00+00:00", valid_at: "2026-05-01T00:00:00+00:00", invalid_at: null,
  superseded_by: null, strength: 1, importance: 0.5, last_accessed: "2026-05-01T00:00:00+00:00",
  access_count: 0, workspace: null, source_tool: null, mnemos_version: 1, ...over,
});
export const hitFixture = (): RecallHit => ({
  memory: memFixture(), score: 1.2, bm25_rank: 1, dense_rank: 2, dense_distance: 0.1, ppr_rank: 3,
  explain: { bm25_rank: 1, dense_rank: 2, dense_distance: 0.1, ppr_rank: 3, rrf_score: 0.05,
    weight_recency: 0.9, weight_importance: 1.5, weight_strength: 1, weight_tier: 1, rerank_score: null, final_score: 1.2 },
});
