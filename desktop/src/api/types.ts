export type Tier = "working" | "episodic" | "semantic" | "procedural" | "reflection";
export type MemoryType =
  | "fact" | "episode" | "reflection" | "rule" | "identity" | "project" | "entity" | "community-summary";

export interface Provenance { session?: string | null; chunks: string[]; }

export interface Memory {
  id: string;
  tier: Tier;
  type: MemoryType;
  title: string;
  body: string;
  tags: string[];
  entities: string[];
  links: string[];
  provenance: Provenance[];
  created_at: string;
  ingested_at: string;
  valid_at: string;
  invalid_at: string | null;
  superseded_by: string | null;
  strength: number;
  importance: number;
  last_accessed: string;
  access_count: number;
  workspace: string | null;
  source_tool: string | null;
  mnemos_version: number;
}

export interface Explain {
  bm25_rank: number | null;
  dense_rank: number | null;
  dense_distance: number | null;
  ppr_rank: number | null;
  rrf_score: number;
  weight_recency: number;
  weight_importance: number;
  weight_strength: number;
  weight_tier: number;
  rerank_score: number | null;
  final_score: number;
}

export interface RecallHit {
  memory: Memory;
  score: number;
  bm25_rank: number | null;
  dense_rank: number | null;
  dense_distance: number | null;
  ppr_rank: number | null;
  explain: Explain | null;
}

export interface Entity {
  id: string;
  name: string;
  type?: string;
  kind?: string;
  aliases?: string[];
  description?: string | null;
}
export interface EntityMemoryPreview {
  id: string;
  title: string;
  body_preview: string;
  tier?: string;
  created_at?: string;
}
export interface CoMentionedEntity {
  id: string;
  name: string;
  kind: string;
  shared_memory_count: number;
}
export interface EnrichedEdge extends GraphEdge {
  source_name: string;
  target_name: string;
  source_kind: string;
  target_kind: string;
}
export interface EntityDetail extends Entity {
  mention_count: number;
  memory_ids: string[];
  memories?: EntityMemoryPreview[];
  edges: EnrichedEdge[];
  co_mentioned_entities: CoMentionedEntity[];
  created_at?: string;
  community?: { id: number; summary?: string | null } | null;
}
export interface GraphNode { id: string; name: string; kind: string; community_id?: number; mentions?: number; }
export interface GraphEdge { id: string; source: string; target: string; relation: string; weight: number; }
export interface Graph { nodes: GraphNode[]; edges: GraphEdge[]; }

export interface PipelineStatus {
  enabled: boolean;
  llm_model: string | null;
  counters: { completed: number; failed: number; facts_added: number };
  recent: { session_id: string; facts_added: number; ok: boolean; at: string }[];
  backfill?: { processed: number; total: number; entities_linked: number; errors: number } | null;
}

export interface AuditEntry {
  id: number;
  ts: string;
  actor: string;
  action: string;
  memory_id: string | null;
  details: Record<string, unknown> | null;
}

export interface SearchReq {
  query: string;
  k?: number;
  tier?: Tier[];
  workspace?: string;
  include_invalid?: boolean;
  explain?: boolean;
  rerank?: boolean;
  graph?: boolean;
  global?: boolean;
}
