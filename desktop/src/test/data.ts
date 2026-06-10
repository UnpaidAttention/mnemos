// Rich fixture dataset for MSW handlers — what the dev-mode browser worker
// and the node-mode test server return when the app asks for "real" data.
// Designed to look like a believable personal vault: ~10 memories spread
// across all five tiers, ~7 entities, ~10 edges, 4 reflections, a community
// summary, and ~12 audit entries. Existing tests assert specific titles
// ("Rust note", "Reflection (insight)") so those are kept as canonical
// members of the richer dataset rather than removed.

import type {
  AuditEntry,
  Entity,
  GraphEdge,
  Memory,
  PipelineStatus,
} from "../api/types";

const isoDaysAgo = (days: number): string => {
  const t = new Date("2026-05-27T12:00:00+00:00").getTime() - days * 86_400_000;
  return new Date(t).toISOString();
};

const baseMemory = (over: Partial<Memory>): Memory => ({
  id: "mem_?",
  tier: "semantic",
  type: "fact",
  title: "",
  body: "",
  tags: [],
  entities: [],
  links: [],
  provenance: [],
  created_at: isoDaysAgo(10),
  ingested_at: isoDaysAgo(10),
  valid_at: isoDaysAgo(10),
  invalid_at: null,
  superseded_by: null,
  strength: 0.7,
  importance: 0.5,
  last_accessed: isoDaysAgo(2),
  access_count: 3,
  workspace: null,
  source_tool: null,
  mnemos_version: 1,
  ...over,
});

export const RICH_MEMORIES: Memory[] = [
  // Canonical "Rust note" — kept as id mem_1 so existing tests pass.
  baseMemory({
    id: "mem_1",
    tier: "semantic",
    type: "fact",
    title: "Rust note",
    body: "Shaun prefers Rust for systems work and writes the mnemos daemon in it.",
    tags: ["rust", "preference"],
    entities: ["ent_rust", "ent_shaun"],
    provenance: [{ session: "sess_2026_05_20", chunks: ["c1", "c2"] }],
    valid_at: isoDaysAgo(27),
    importance: 0.78,
    strength: 0.92,
  }),
  baseMemory({
    id: "mem_2",
    tier: "working",
    type: "episode",
    title: "Plan 6 design polish pass",
    body: "Today's focus is enriching the MSW fixtures and refining the top bar for the desktop UI.",
    tags: ["plan-6", "ui", "today"],
    entities: ["ent_plan6"],
    valid_at: isoDaysAgo(0),
    importance: 0.6,
    strength: 0.55,
  }),
  baseMemory({
    id: "mem_3",
    tier: "episodic",
    type: "episode",
    title: "Tauri v2 migration call",
    body: "Discussed the v2 migration path with the team — IPC surface is mostly compatible, plugin model changes.",
    tags: ["tauri", "meeting"],
    entities: ["ent_tauri"],
    valid_at: isoDaysAgo(4),
    importance: 0.55,
    strength: 0.7,
  }),
  baseMemory({
    id: "mem_4",
    tier: "semantic",
    type: "fact",
    title: "Armellini ships dry van regional",
    body: "Armellini Logistics primarily moves dry-van freight on regional lanes east of the Mississippi.",
    tags: ["armellini", "logistics"],
    entities: ["ent_armellini"],
    valid_at: isoDaysAgo(12),
    importance: 0.7,
    strength: 0.85,
  }),
  baseMemory({
    id: "mem_5",
    tier: "procedural",
    type: "rule",
    title: "Always rebase against master before pushing",
    body: "Local workflow: rebase onto origin/master, run pnpm test, then push. Never merge.",
    tags: ["git", "workflow"],
    valid_at: isoDaysAgo(20),
    importance: 0.65,
    strength: 0.8,
  }),
  baseMemory({
    id: "mem_6",
    tier: "procedural",
    type: "rule",
    title: "Pin pnpm to 9.x in CI",
    body: "Use corepack to pin pnpm@9 — pnpm@10 broke the workspace overrides last sprint.",
    tags: ["ci", "pnpm"],
    valid_at: isoDaysAgo(18),
    invalid_at: isoDaysAgo(2),
    importance: 0.4,
    strength: 0.3,
  }),
  baseMemory({
    id: "mem_7",
    tier: "semantic",
    type: "fact",
    title: "Sigma.js handles 5k nodes smoothly",
    body: "Sigma.js with ForceAtlas2 stays interactive up to ~5k nodes on a M2 laptop; above that, prefer pre-computed layouts.",
    tags: ["graph", "performance"],
    entities: ["ent_sigma"],
    valid_at: isoDaysAgo(15),
    importance: 0.5,
    strength: 0.6,
  }),
  baseMemory({
    id: "mem_8",
    tier: "episodic",
    type: "episode",
    title: "McLeod incident postmortem",
    body: "EDI 214 feed stalled for ~40 minutes Tuesday; root cause was a backed-up queue on the partner side, not McLeod.",
    tags: ["mcleod", "incident", "edi"],
    entities: ["ent_mcleod", "ent_armellini"],
    valid_at: isoDaysAgo(7),
    importance: 0.75,
    strength: 0.72,
  }),
  // Weak-strength memory so the pulse-weak animation is visible in the browser.
  baseMemory({
    id: "mem_9",
    tier: "working",
    type: "fact",
    title: "Tentative: try Lucide v0.500 for new icons",
    body: "Lucide v0.500 ships hand-drawn variants; evaluate before committing to a redesign.",
    tags: ["icons", "lucide", "tentative"],
    valid_at: isoDaysAgo(1),
    importance: 0.25,
    strength: 0.15,
  }),
  // Community summary memory, referenced by /v1/communities.
  baseMemory({
    id: "mem_cs1",
    tier: "semantic",
    type: "community-summary",
    title: "Cluster: Rust + Tauri tooling",
    body: "Memories about Rust, Tauri, and the mnemos daemon form a tight cluster around desktop tooling.",
    tags: ["cluster", "auto-generated"],
    entities: ["ent_rust", "ent_tauri", "ent_sigma"],
    valid_at: isoDaysAgo(3),
    importance: 0.45,
    strength: 0.65,
  }),
];

export const RICH_REFLECTIONS: Memory[] = [
  // Canonical reflection — title kept verbatim so Reflections.test.tsx passes.
  baseMemory({
    id: "mem_r1",
    tier: "reflection",
    type: "reflection",
    title: "Reflection (insight)",
    body: "You return to Rust + Tauri tooling questions repeatedly — that's a stable interest, not a passing one.",
    tags: [],
    valid_at: isoDaysAgo(2),
    importance: 0.6,
    strength: 0.7,
  }),
  baseMemory({
    id: "mem_r2",
    tier: "reflection",
    type: "reflection",
    title: "Reflection (preference)",
    body: "You consistently prefer warm off-whites and avoid pure black backgrounds — codify in design tokens.",
    tags: ["design", "preference"],
    valid_at: isoDaysAgo(5),
    importance: 0.55,
    strength: 0.68,
  }),
  baseMemory({
    id: "mem_r3",
    tier: "reflection",
    type: "reflection",
    title: "Reflection (pattern)",
    body: "Most procedural memories you add are about CI pinning — a sign the toolchain is the weak link.",
    tags: ["pattern", "ci"],
    valid_at: isoDaysAgo(9),
    importance: 0.5,
    strength: 0.62,
  }),
  baseMemory({
    id: "mem_r4",
    tier: "reflection",
    type: "reflection",
    title: "Reflection (decision)",
    body: "Decision recorded: stay on pnpm 9 for the rest of Plan 6 rather than re-pinning mid-cycle.",
    tags: ["decision", "pnpm"],
    valid_at: isoDaysAgo(2),
    importance: 0.7,
    strength: 0.78,
  }),
];

export const RICH_ENTITIES: Entity[] = [
  { id: "ent_rust", name: "Rust", kind: "tool" },
  { id: "ent_tauri", name: "Tauri", kind: "tool" },
  { id: "ent_sigma", name: "Sigma.js", kind: "tool" },
  { id: "ent_shaun", name: "Shaun", kind: "person" },
  { id: "ent_armellini", name: "Armellini Logistics", kind: "organization" },
  { id: "ent_mcleod", name: "McLeod", kind: "tool" },
  { id: "ent_plan6", name: "Plan 6", kind: "project" },
];

// Community assignment for the graph (3 communities lit up).
const COMMUNITY_OF: Record<string, number> = {
  ent_rust: 0,
  ent_tauri: 0,
  ent_sigma: 0,
  ent_shaun: 1,
  ent_plan6: 1,
  ent_armellini: 2,
  ent_mcleod: 2,
};

const MENTIONS_OF: Record<string, number> = {
  ent_rust: 4,
  ent_tauri: 3,
  ent_sigma: 2,
  ent_shaun: 2,
  ent_armellini: 2,
  ent_mcleod: 1,
  ent_plan6: 1,
};

export const RICH_GRAPH_NODES = RICH_ENTITIES.map((e) => ({
  id: e.id,
  name: e.name,
  kind: e.kind ?? "entity",
  community_id: COMMUNITY_OF[e.id] ?? 0,
  mentions: MENTIONS_OF[e.id] ?? 1,
}));

export const RICH_EDGES: GraphEdge[] = [
  { id: "edge_1", source: "ent_rust", target: "ent_tauri", relation: "uses", weight: 3 },
  { id: "edge_2", source: "ent_tauri", target: "ent_sigma", relation: "depends_on", weight: 2 },
  { id: "edge_3", source: "ent_shaun", target: "ent_rust", relation: "mentions", weight: 4 },
  { id: "edge_4", source: "ent_shaun", target: "ent_plan6", relation: "works_on", weight: 3 },
  { id: "edge_5", source: "ent_plan6", target: "ent_tauri", relation: "depends_on", weight: 2 },
  { id: "edge_6", source: "ent_shaun", target: "ent_armellini", relation: "works_at", weight: 4 },
  { id: "edge_7", source: "ent_armellini", target: "ent_mcleod", relation: "uses", weight: 3 },
  { id: "edge_8", source: "ent_mcleod", target: "ent_armellini", relation: "ships_to", weight: 1 },
  { id: "edge_9", source: "ent_plan6", target: "ent_sigma", relation: "mentions", weight: 1 },
  { id: "edge_10", source: "ent_rust", target: "ent_sigma", relation: "mentions", weight: 1 },
];

// PPR scores — one community ("Rust + Tauri tooling") lit up.
export const RICH_PPR_SCORES: Record<string, number> = {
  ent_rust: 0.42,
  ent_tauri: 0.31,
  ent_sigma: 0.18,
  ent_shaun: 0.05,
  ent_plan6: 0.03,
  ent_armellini: 0.005,
  ent_mcleod: 0.005,
};

export const RICH_COMMUNITIES = [
  {
    community_id: 0,
    members: RICH_ENTITIES.filter((e) => COMMUNITY_OF[e.id] === 0),
  },
  {
    community_id: 1,
    members: RICH_ENTITIES.filter((e) => COMMUNITY_OF[e.id] === 1),
  },
  {
    community_id: 2,
    members: RICH_ENTITIES.filter((e) => COMMUNITY_OF[e.id] === 2),
  },
];

export const RICH_AUDIT: AuditEntry[] = [
  { id: 12, ts: isoDaysAgo(0), actor: "mnemos-cli", action: "create", memory_id: "mem_2", details: null },
  { id: 11, ts: isoDaysAgo(0), actor: "desktop", action: "update", memory_id: "mem_9", details: null },
  { id: 10, ts: isoDaysAgo(1), actor: "mnemos-cli", action: "create", memory_id: "mem_9", details: null },
  { id: 9, ts: isoDaysAgo(2), actor: "pipeline", action: "create", memory_id: "mem_r4", details: null },
  { id: 8, ts: isoDaysAgo(2), actor: "pipeline", action: "forget", memory_id: "mem_6", details: { reason: "superseded" } },
  { id: 7, ts: isoDaysAgo(3), actor: "pipeline", action: "create", memory_id: "mem_cs1", details: null },
  { id: 6, ts: isoDaysAgo(4), actor: "mnemos-cli", action: "create", memory_id: "mem_3", details: null },
  { id: 5, ts: isoDaysAgo(7), actor: "mnemos-cli", action: "create", memory_id: "mem_8", details: null },
  { id: 4, ts: isoDaysAgo(12), actor: "mnemos-cli", action: "create", memory_id: "mem_4", details: null },
  { id: 3, ts: isoDaysAgo(15), actor: "mnemos-cli", action: "create", memory_id: "mem_7", details: null },
  { id: 2, ts: isoDaysAgo(20), actor: "mnemos-cli", action: "create", memory_id: "mem_5", details: null },
  { id: 1, ts: isoDaysAgo(27), actor: "mnemos-cli", action: "create", memory_id: "mem_1", details: null },
];

export const RICH_PIPELINE: PipelineStatus = {
  enabled: true,
  llm_model: "mock-llm",
  counters: { completed: 47, failed: 2, facts_added: 138 },
  recent: [
    { session_id: "sess_2026_05_27_a", facts_added: 6, ok: true, at: isoDaysAgo(0) },
    { session_id: "sess_2026_05_26_b", facts_added: 4, ok: true, at: isoDaysAgo(1) },
    { session_id: "sess_2026_05_25_a", facts_added: 2, ok: false, at: isoDaysAgo(2) },
    { session_id: "sess_2026_05_24_a", facts_added: 9, ok: true, at: isoDaysAgo(3) },
    { session_id: "sess_2026_05_22_c", facts_added: 5, ok: true, at: isoDaysAgo(5) },
  ],
};

export const RICH_MEMORIES_BY_ID: Record<string, Memory> = Object.fromEntries(
  [...RICH_MEMORIES, ...RICH_REFLECTIONS].map((m) => [m.id, m]),
);

export const RICH_ENTITY_DETAIL = (id: string) => {
  const ent = RICH_ENTITIES.find((e) => e.id === id);
  if (!ent) return null;
  const edges = RICH_EDGES.filter(
    (e) => e.source === id || e.target === id,
  );
  const memory_ids = RICH_MEMORIES.filter((m) => m.entities.includes(id)).map(
    (m) => m.id,
  );
  return {
    ...ent,
    aliases: [],
    description: null,
    mention_count: MENTIONS_OF[id] ?? memory_ids.length,
    memory_ids,
    edges,
  };
};

export const RICH_ENTITY_NEIGHBORHOOD = (id: string) => {
  const edges = RICH_EDGES.filter(
    (e) => e.source === id || e.target === id,
  );
  const ids = new Set<string>([id]);
  edges.forEach((e) => {
    ids.add(e.source);
    ids.add(e.target);
  });
  const nodes = RICH_GRAPH_NODES.filter((n) => ids.has(n.id));
  return { nodes, edges };
};

// Search hits — top 5 with varied rank profiles. Always includes mem_1 first
// so Search.test.tsx (which asserts "Rust note" appears) continues to pass.
export const RICH_SEARCH_HITS = [
  {
    memory: RICH_MEMORIES_BY_ID.mem_1,
    score: 1.42,
    bm25_rank: 1,
    dense_rank: 2,
    dense_distance: 0.12,
    ppr_rank: 1,
    explain: {
      bm25_rank: 1,
      dense_rank: 2,
      dense_distance: 0.12,
      ppr_rank: 1,
      rrf_score: 0.072,
      weight_recency: 0.85,
      weight_importance: 1.4,
      weight_strength: 1.0,
      weight_tier: 1.0,
      rerank_score: null,
      final_score: 1.42,
    },
  },
  {
    memory: RICH_MEMORIES_BY_ID.mem_3,
    score: 1.18,
    bm25_rank: 3,
    dense_rank: 1,
    dense_distance: 0.09,
    ppr_rank: 4,
    explain: {
      bm25_rank: 3,
      dense_rank: 1,
      dense_distance: 0.09,
      ppr_rank: 4,
      rrf_score: 0.061,
      weight_recency: 0.92,
      weight_importance: 1.1,
      weight_strength: 0.95,
      weight_tier: 0.9,
      rerank_score: null,
      final_score: 1.18,
    },
  },
  {
    memory: RICH_MEMORIES_BY_ID.mem_7,
    score: 0.96,
    bm25_rank: 4,
    dense_rank: 3,
    dense_distance: 0.21,
    ppr_rank: 3,
    explain: {
      bm25_rank: 4,
      dense_rank: 3,
      dense_distance: 0.21,
      ppr_rank: 3,
      rrf_score: 0.048,
      weight_recency: 0.7,
      weight_importance: 1.0,
      weight_strength: 0.9,
      weight_tier: 1.0,
      rerank_score: null,
      final_score: 0.96,
    },
  },
  {
    memory: RICH_MEMORIES_BY_ID.mem_4,
    score: 0.82,
    bm25_rank: 2,
    dense_rank: 6,
    dense_distance: 0.34,
    ppr_rank: 6,
    explain: {
      bm25_rank: 2,
      dense_rank: 6,
      dense_distance: 0.34,
      ppr_rank: 6,
      rrf_score: 0.039,
      weight_recency: 0.78,
      weight_importance: 1.3,
      weight_strength: 1.0,
      weight_tier: 1.0,
      rerank_score: null,
      final_score: 0.82,
    },
  },
  {
    memory: RICH_MEMORIES_BY_ID.mem_8,
    score: 0.71,
    bm25_rank: 5,
    dense_rank: 4,
    dense_distance: 0.28,
    ppr_rank: 5,
    explain: {
      bm25_rank: 5,
      dense_rank: 4,
      dense_distance: 0.28,
      ppr_rank: 5,
      rrf_score: 0.034,
      weight_recency: 0.88,
      weight_importance: 1.2,
      weight_strength: 0.92,
      weight_tier: 1.0,
      rerank_score: null,
      final_score: 0.71,
    },
  },
];
