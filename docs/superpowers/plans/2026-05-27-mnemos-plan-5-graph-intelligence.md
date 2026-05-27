# Mnemos Plan 5 — Graph Intelligence (PPR retrieval, reflection, communities)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Turn the entity graph Plan 4 builds into retrieval power and higher-order memory. Add (1) HippoRAG-style Personalized PageRank as a third retriever fused into hybrid recall — so a query about "Rust" surfaces the "Tauri desktop UI" memory via graph hops even with no shared words; (2) importance-triggered **reflection** — when enough new knowledge accrues, the daemon summarizes recent memories into durable typed insights; (3) **community detection** — clustering the entity graph and writing per-cluster summaries that power thematic/global queries. End state: recall is multi-hop and graph-aware, the system periodically distills what it has learned, and "what are the themes in my memory?" has an answer.

**Architecture:** A new dependency-free `graph` module in `mnemos_core` builds an in-memory `MemoryGraph` from `entity_edges` + `entity_mentions` and runs two hand-rolled, deterministic algorithms: Personalized PageRank (power iteration) and Louvain community detection. PPR plugs into the existing RRF fusion as a third ranked list alongside BM25 and dense. Reflection and community-summarization are new LLM pipeline stages (using the Plan 4 `LlmProvider` + `MockLlm`); reflection is triggered by a salience accumulator off the pipeline runner, community detection by an on-demand maintenance endpoint. New memories are written into the existing `reflection` tier (`kind = reflection` for insights, `kind = community_summary` for clusters), with `memory_links(reflects_on)` provenance.

**Tech Stack:** Rust 2021, existing stack (libsql, axum, tokio). **No new dependencies** — PPR and Louvain are hand-rolled in pure Rust. All algorithm tests are deterministic with fixed graphs; all pipeline tests use `MockLlm`/`MockEmbedder` (no Ollama, no network).

---

## Plan sequence context

Plan 5 of 7, producing **v0.4.0**. Built on v0.3.0 (Plan 4: the entity graph, `LlmProvider`/`MockLlm`, the `PipelineRunner`, decay). Subsequent:
- Plan 6: Tauri + React desktop UI (visualizes the graph, PPR overlay, community hulls, reflection viewer).
- Plan 7: sync backends, additional adapters, packaging.

Schema migration is additive (v5 adds reflection-state + community-membership tables); v0.3.0 vaults upgrade transparently.

---

## Deviations from the design spec (intentional, approved)

The design spec (`docs/superpowers/specs/2026-05-22-mnemos-memory-provider-design.md`) names `petgraph` + a `leiden_clustering` crate for the graph work. This plan **hand-rolls the graph algorithms with zero new dependencies**, by explicit decision:

| Spec says | This plan does | Why |
|---|---|---|
| `petgraph` for graph structures | Hand-rolled `MemoryGraph` (HashMap adjacency) | Personal-scale graph; avoids a dep; fully controlled + testable |
| Hierarchical **Leiden** (`leiden_clustering` crate) | Hand-rolled single-level **Louvain** modularity | The crate is immature (CI-risk, cf. the `ort` RC breakage); Leiden's advantage only shows at million-node scale. Leiden refinement is noted as a future enhancement. |
| PPR via `petgraph` | Hand-rolled power iteration | ~80 LOC, deterministic, no dep |

All three are small, well-understood algorithms. Determinism (fixed iteration order, no network/model) keeps CI reproducible.

---

## What this plan defers

| Capability | Why | Target |
|---|---|---|
| Hierarchical/Leiden community refinement | Single-level Louvain is sufficient at personal scale | Later increment if needed |
| Daily scheduled community detection | On-demand endpoint ships the capability; a timer is trivial to add later (mirrors Plan 4's decay worker) | Plan 6/7 |
| `mnemos reflect` / `mnemos communities` CLI | These are LLM-driven and the CLI has no LLM wiring; exposed via REST + MCP instead (the daemon owns the LLM). `mnemos decay` stays (no LLM needed). | When CLI gets a daemon-client path |
| MCP `mnemos://reflections/recent` resource | Redundant with the `list_reflections` tool for v0.4.0 | Later |
| Auto-promotion of preferences → procedural tier | Adds a human-in-the-loop UI affordance; belongs with the UI | Plan 6 |
| PPR graph caching | Graph is rebuilt per recall (cheap at personal scale); caching/invalidation is an optimization | Later |

---

## Hard prerequisites

- Plan 4 (`v0.3.0`) shipped; CI green on Linux + macOS.
- The entity graph storage (`entities`, `entity_mentions`, `entity_edges`) and `link_entities`/`update_graph` pipeline stages exist (Plan 4).
- `LlmProvider` + `MockLlm` + `PipelineRunner` exist (Plan 4).

---

## File structure produced by this plan

```
crates/mnemos_core/src/
├── graph/                      # NEW — dependency-free graph algorithms
│   ├── mod.rs                  # MemoryGraph struct + re-exports
│   ├── build.rs                # MemoryGraph::load(&Storage)
│   ├── ppr.rs                  # personalized_pagerank + ppr_rank_memories
│   └── community.rs            # louvain modularity community detection
├── retrieval/
│   ├── mod.rs                  # MODIFIED: RecallOpts.graph flag; RecallHit/Explain gain ppr_rank
│   ├── graph_recall.rs         # NEW: select_seeds + ppr ranked-id list for fusion
│   └── hybrid.rs               # MODIFIED: hybrid_recall_full (3-way: bm25+dense+ppr)
├── pipeline/
│   ├── reflect.rs              # NEW: reflect() — recent memories → typed reflection memories
│   └── community.rs            # NEW: detect_and_summarize() — louvain + LLM summaries
├── providers/mock_llm.rs       # MODIFIED: TASK=reflect, TASK=community markers
├── storage/
│   ├── migrations.rs           # MODIFIED: v5 (reflection_state + entity_communities)
│   ├── memory_ops.rs           # MODIFIED: add_memory_link, recent_unreflected, mark_reflected, list_by_kind
│   ├── reflection_ops.rs       # NEW: salience accumulator get/bump/reset
│   └── community_ops.rs        # NEW: store/read entity community membership
└── vault.rs                    # MODIFIED: remember_reflection helper (tier=reflection + links)

crates/mnemos_daemon/src/
├── config.rs                   # MODIFIED: [retrieval] ppr_*, [reflection], [community]
├── events.rs                   # MODIFIED: ReflectionCompleted, CommunityDetected
├── pipeline_runner.rs          # MODIFIED: salience bump + reflection trigger after a session
├── routes/
│   ├── recall_helper.rs        # MODIFIED: build graph + use hybrid_recall_full; global mode
│   ├── memories.rs             # MODIFIED: search gains `graph` + `global` flags
│   ├── reflections.rs          # NEW: POST/GET /v1/reflections
│   ├── pipelines.rs            # MODIFIED: POST /v1/maintenance/communities
│   └── mod.rs                  # MODIFIED: mount reflections router
└── mcp/tools.rs                # MODIFIED: reflect + list_reflections tools; recall `global` arg

README.md / CHANGELOG.md / Cargo.toml   # MODIFIED: v0.4.0
```

---

## Conventions (same as Plans 1-4)

- TDD: failing test → confirm fail → implement → confirm pass → commit. Pure-config/doc tasks skip the failing-test step.
- `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` green at every commit.
- Commit message `<type>: <subject>`; reference Plan 5 / Task N in the body.
- All paths relative to `/home/jons/AntiGravityProjects/mnemos/`.
- CI runs default features only; all tests use `MockLlm` / `MockEmbedder` — never require Ollama. Graph-algorithm tests use fixed in-memory graphs (no LLM/embedder at all).
- Reflection/community memories live in the existing `Tier::Reflection` with `MemoryType::Reflection` (insights) or `MemoryType::CommunitySummary` (clusters). Both already exist in `types.rs`.

---

## Task 1: `MemoryGraph` structure + builder

A dependency-free in-memory projection of the entity graph. Nodes = entities (indexed); undirected weighted edges; bidirectional memory↔entity mention maps. Builder methods (`add_edge`, `add_mention`) are used by both `load()` (Task 2) and the deterministic algorithm tests.

**Files:**
- Modify: `crates/mnemos_core/src/lib.rs` (add `pub mod graph;`)
- Create: `crates/mnemos_core/src/graph/mod.rs`

- [ ] **Step 1: Write the failing test** — create `crates/mnemos_core/src/graph/mod.rs` with ONLY the test module:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_nodes_edges_and_mentions() {
        let mut g = MemoryGraph::new();
        g.add_edge("A", "B", 1.0);
        g.add_edge("B", "C", 2.0);
        g.add_edge("A", "B", 1.0); // accumulates onto the existing edge
        g.add_mention("mem1", "A");
        g.add_mention("mem1", "A"); // idempotent
        g.add_mention("mem2", "C");

        assert_eq!(g.node_count(), 3);
        let a = g.index_of("A").unwrap();
        let b = g.index_of("B").unwrap();
        // A-B weight accumulated to 2.0; A's degree = 2.0 (only neighbor B)
        assert_eq!(g.degree(a), 2.0);
        // B touches A(2.0) + C(2.0) => degree 4.0
        assert_eq!(g.degree(b), 4.0);
        assert_eq!(g.memories_for_entity(a), &["mem1".to_string()]);
        assert_eq!(g.entities_for_memory("mem2").unwrap(), &[g.index_of("C").unwrap()]);
        assert!(!g.is_empty());
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p mnemos_core --lib graph::tests`
Expected: FAIL — `MemoryGraph` / module not found.

- [ ] **Step 3: Prepend the implementation above the test module**

```rust
//! Dependency-free in-memory projection of the entity graph, backing
//! Personalized PageRank ([`ppr`]) and Louvain community detection
//! ([`community`]). Built from `entity_edges` + `entity_mentions` by
//! [`MemoryGraph::load`] ([`build`]).

pub mod build;
pub mod community;
pub mod ppr;

use std::collections::HashMap;

/// Entities are nodes (indexed `0..node_count`); edges are undirected and
/// weighted (active edges only); memory↔entity mentions are tracked both ways.
#[derive(Debug, Default, Clone)]
pub struct MemoryGraph {
    entity_ids: Vec<String>,
    index_of: HashMap<String, usize>,
    /// adj[i] = [(neighbor_index, weight), ...]
    adj: Vec<Vec<(usize, f64)>>,
    /// Sum of incident edge weights per node (PPR normalization + Louvain).
    degree: Vec<f64>,
    mem_to_entities: HashMap<String, Vec<usize>>,
    entity_to_mems: Vec<Vec<String>>,
}

impl MemoryGraph {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn node_count(&self) -> usize {
        self.entity_ids.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entity_ids.is_empty()
    }

    pub fn index_of(&self, entity_id: &str) -> Option<usize> {
        self.index_of.get(entity_id).copied()
    }

    pub fn entity_id(&self, i: usize) -> &str {
        &self.entity_ids[i]
    }

    pub fn neighbors(&self, i: usize) -> &[(usize, f64)] {
        &self.adj[i]
    }

    pub fn degree(&self, i: usize) -> f64 {
        self.degree[i]
    }

    /// Total edge weight `m` = sum of degrees / 2 (used by Louvain modularity).
    pub fn total_weight(&self) -> f64 {
        self.degree.iter().sum::<f64>() / 2.0
    }

    pub fn memories_for_entity(&self, i: usize) -> &[String] {
        &self.entity_to_mems[i]
    }

    pub fn entities_for_memory(&self, memory_id: &str) -> Option<&Vec<usize>> {
        self.mem_to_entities.get(memory_id)
    }

    fn ensure_node(&mut self, entity_id: &str) -> usize {
        if let Some(&i) = self.index_of.get(entity_id) {
            return i;
        }
        let i = self.entity_ids.len();
        self.entity_ids.push(entity_id.to_string());
        self.index_of.insert(entity_id.to_string(), i);
        self.adj.push(Vec::new());
        self.degree.push(0.0);
        self.entity_to_mems.push(Vec::new());
        i
    }

    /// Add an undirected weighted edge (creating nodes as needed). A repeated
    /// edge accumulates its weight. Self-loops are ignored.
    pub fn add_edge(&mut self, a: &str, b: &str, weight: f64) {
        let ia = self.ensure_node(a);
        let ib = self.ensure_node(b);
        if ia == ib {
            return;
        }
        Self::accumulate(&mut self.adj[ia], ib, weight);
        Self::accumulate(&mut self.adj[ib], ia, weight);
        self.degree[ia] += weight;
        self.degree[ib] += weight;
    }

    fn accumulate(list: &mut Vec<(usize, f64)>, neighbor: usize, weight: f64) {
        if let Some(e) = list.iter_mut().find(|(n, _)| *n == neighbor) {
            e.1 += weight;
        } else {
            list.push((neighbor, weight));
        }
    }

    /// Record that `memory_id` mentions `entity_id` (creating the node as
    /// needed). Idempotent in both directions.
    pub fn add_mention(&mut self, memory_id: &str, entity_id: &str) {
        let i = self.ensure_node(entity_id);
        if !self.entity_to_mems[i].iter().any(|m| m == memory_id) {
            self.entity_to_mems[i].push(memory_id.to_string());
        }
        let v = self.mem_to_entities.entry(memory_id.to_string()).or_default();
        if !v.contains(&i) {
            v.push(i);
        }
    }
}
```

- [ ] **Step 4: Add the module to `lib.rs`** — add `pub mod graph;` alongside the other `pub mod` lines.

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p mnemos_core --lib graph::tests`
Expected: PASS.

> Note: `graph/mod.rs` declares `pub mod build; pub mod community; pub mod ppr;` — those files are created in Tasks 2, 11, 3 respectively. To keep the crate compiling after THIS task, also create the three files as empty stubs now (`touch` them with a single line each: `// implemented in a later task`). Tasks 2/3/11 replace them. Without the stub files the `pub mod` lines won't compile.

- [ ] **Step 6: Create the three stub files** so the module declarations resolve:

```bash
printf '// MemoryGraph::load — implemented in Plan 5 Task 2\n' > crates/mnemos_core/src/graph/build.rs
printf '// PPR — implemented in Plan 5 Task 3\n' > crates/mnemos_core/src/graph/ppr.rs
printf '// Louvain — implemented in Plan 5 Task 11\n' > crates/mnemos_core/src/graph/community.rs
```

Re-run `cargo test -p mnemos_core --lib graph::tests` → PASS, and `cargo build -p mnemos_core` → clean.

- [ ] **Step 7: Commit**

```bash
cargo fmt --all && cargo clippy -p mnemos_core --all-targets -- -D warnings
git add crates/mnemos_core/src/lib.rs crates/mnemos_core/src/graph/mod.rs crates/mnemos_core/src/graph/build.rs crates/mnemos_core/src/graph/ppr.rs crates/mnemos_core/src/graph/community.rs
git commit -m "feat: add dependency-free MemoryGraph structure (Plan 5 Task 1)"
```

---

## Task 2: `MemoryGraph::load` from storage

Build the graph from active `entity_edges` and mentions of still-valid memories.

**Files:**
- Replace: `crates/mnemos_core/src/graph/build.rs` (stub from Task 1)
- Test: `crates/mnemos_core/tests/graph_build.rs` (new)

- [ ] **Step 1: Write the failing test** — create `crates/mnemos_core/tests/graph_build.rs`:

```rust
use mnemos_core::graph::MemoryGraph;
use mnemos_core::paths::Paths;
use mnemos_core::storage::entity_ops::{link_entity_mention, upsert_edge, upsert_entity};
use mnemos_core::vault::{RememberOpts, Vault};
use chrono::Utc;
use tempfile::TempDir;

#[tokio::test]
async fn load_builds_graph_from_edges_and_mentions() {
    let tmp = TempDir::new().unwrap();
    let v = Vault::open(Paths::with_root(tmp.path())).await.unwrap();

    let mem = v.remember("graph seed", RememberOpts::default()).await.unwrap();
    let a = upsert_entity(v.storage(), "Rust", "tool").await.unwrap();
    let b = upsert_entity(v.storage(), "Tauri", "tool").await.unwrap();
    upsert_edge(v.storage(), &a, &b, "uses", &mem, Utc::now()).await.unwrap();
    link_entity_mention(v.storage(), &mem, &a).await.unwrap();

    let g = MemoryGraph::load(v.storage()).await.unwrap();
    assert_eq!(g.node_count(), 2);
    let ai = g.index_of(&a).unwrap();
    assert_eq!(g.degree(ai), 1.0);
    assert_eq!(g.memories_for_entity(ai), &[mem.clone()]);
    assert_eq!(g.entities_for_memory(&mem).unwrap(), &[ai]);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p mnemos_core --test graph_build`
Expected: FAIL — `MemoryGraph::load` not found.

- [ ] **Step 3: Replace `crates/mnemos_core/src/graph/build.rs`**

```rust
//! Build a [`MemoryGraph`] from storage.

use crate::error::Result;
use crate::graph::MemoryGraph;
use crate::storage::Storage;

impl MemoryGraph {
    /// Build the graph from active entity edges plus mentions by still-valid
    /// memories. Invalid memories' mentions are excluded so PPR only ranks
    /// memories that are currently valid.
    pub async fn load(storage: &Storage) -> Result<Self> {
        let mut g = MemoryGraph::new();
        let conn = storage.conn()?;

        let mut edges = conn
            .query(
                "SELECT source_entity_id, target_entity_id, weight
                   FROM entity_edges WHERE invalid_at IS NULL",
                (),
            )
            .await?;
        while let Some(r) = edges.next().await? {
            let a: String = r.get(0)?;
            let b: String = r.get(1)?;
            let w: f64 = r.get(2)?;
            g.add_edge(&a, &b, w.max(0.0));
        }
        drop(edges);

        let mut mentions = conn
            .query(
                "SELECT em.memory_id, em.entity_id
                   FROM entity_mentions em
                   JOIN memories m ON m.id = em.memory_id
                  WHERE m.invalid_at IS NULL",
                (),
            )
            .await?;
        while let Some(r) = mentions.next().await? {
            let memory_id: String = r.get(0)?;
            let entity_id: String = r.get(1)?;
            g.add_mention(&memory_id, &entity_id);
        }

        Ok(g)
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p mnemos_core --test graph_build`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
cargo fmt --all && cargo clippy -p mnemos_core --all-targets -- -D warnings
git add crates/mnemos_core/src/graph/build.rs crates/mnemos_core/tests/graph_build.rs
git commit -m "feat: MemoryGraph::load from entity edges + mentions (Plan 5 Task 2)"
```

---

## Task 3: Personalized PageRank

Power iteration with restart on the entity graph, plus memory ranking by summed PPR mass of mentioned entities. Pure functions; deterministic.

**Files:**
- Replace: `crates/mnemos_core/src/graph/ppr.rs` (stub from Task 1)

- [ ] **Step 1: Write the failing test** — create `crates/mnemos_core/src/graph/ppr.rs` with ONLY the test module:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::MemoryGraph;

    fn path_graph() -> MemoryGraph {
        // A — B — C, one memory per node.
        let mut g = MemoryGraph::new();
        g.add_edge("A", "B", 1.0);
        g.add_edge("B", "C", 1.0);
        g.add_mention("memA", "A");
        g.add_mention("memB", "B");
        g.add_mention("memC", "C");
        g
    }

    #[test]
    fn ppr_concentrates_mass_near_the_seed() {
        let g = path_graph();
        let seed = g.index_of("A").unwrap();
        let scores = personalized_pagerank(&g, &[seed], 0.85, 50);
        let a = scores[g.index_of("A").unwrap()];
        let b = scores[g.index_of("B").unwrap()];
        let c = scores[g.index_of("C").unwrap()];
        assert!(a > b, "seed A ({a}) should outrank B ({b})");
        assert!(b > c, "B ({b}) should outrank farther C ({c})");
    }

    #[test]
    fn no_seeds_yields_zero_vector() {
        let g = path_graph();
        let scores = personalized_pagerank(&g, &[], 0.85, 30);
        assert!(scores.iter().all(|&s| s == 0.0));
    }

    #[test]
    fn ranks_memories_by_entity_mass() {
        let g = path_graph();
        let seed = g.index_of("A").unwrap();
        let scores = personalized_pagerank(&g, &[seed], 0.85, 50);
        let ranked = ppr_rank_memories(&g, &scores);
        assert_eq!(ranked[0].id, "memA");
        // memC is reachable (multi-hop) and present in the ranking.
        assert!(ranked.iter().any(|r| r.id == "memC"));
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p mnemos_core --lib graph::ppr`
Expected: FAIL — functions not found.

- [ ] **Step 3: Prepend the implementation above the test module**

```rust
//! Personalized PageRank (random walk with restart) over the entity graph,
//! via power iteration. Hand-rolled; deterministic.

use crate::graph::MemoryGraph;
use crate::retrieval::rrf::RankedId;
use std::collections::HashMap;

/// Personalized PageRank. Returns a score per node index (sums to ~1). The
/// restart distribution is uniform over `seeds`; mass at dangling (edgeless)
/// nodes is redistributed to the restart set. With no seeds, returns zeros.
pub fn personalized_pagerank(
    graph: &MemoryGraph,
    seeds: &[usize],
    alpha: f64,
    iterations: usize,
) -> Vec<f64> {
    let n = graph.node_count();
    if n == 0 || seeds.is_empty() {
        return vec![0.0; n];
    }
    let mut restart = vec![0.0; n];
    let seed_mass = 1.0 / seeds.len() as f64;
    for &s in seeds {
        if s < n {
            restart[s] += seed_mass;
        }
    }
    let mut r = restart.clone();
    for _ in 0..iterations {
        let mut next = vec![0.0; n];
        let mut dangling = 0.0;
        for i in 0..n {
            let deg = graph.degree(i);
            if deg <= 0.0 {
                dangling += r[i];
                continue;
            }
            for &(j, w) in graph.neighbors(i) {
                next[j] += r[i] * (w / deg);
            }
        }
        for (i, slot) in next.iter_mut().enumerate() {
            *slot = (1.0 - alpha) * restart[i] + alpha * (*slot + dangling * restart[i]);
        }
        r = next;
    }
    r
}

/// Rank memories by the summed PPR score of the entities they mention.
/// Deterministic: sorted by score descending, then memory id ascending.
pub fn ppr_rank_memories(graph: &MemoryGraph, scores: &[f64]) -> Vec<RankedId> {
    let mut acc: HashMap<&str, f64> = HashMap::new();
    for i in 0..graph.node_count() {
        let s = scores.get(i).copied().unwrap_or(0.0);
        if s <= 0.0 {
            continue;
        }
        for mem in graph.memories_for_entity(i) {
            *acc.entry(mem.as_str()).or_insert(0.0) += s;
        }
    }
    let mut scored: Vec<(&str, f64)> = acc.into_iter().collect();
    scored.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.0.cmp(b.0))
    });
    scored
        .into_iter()
        .enumerate()
        .map(|(i, (id, _))| RankedId {
            id: id.to_string(),
            rank: i + 1,
        })
        .collect()
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p mnemos_core --lib graph::ppr`
Expected: PASS (3 tests).

- [ ] **Step 5: Commit**

```bash
cargo fmt --all && cargo clippy -p mnemos_core --all-targets -- -D warnings
git add crates/mnemos_core/src/graph/ppr.rs
git commit -m "feat: Personalized PageRank over the entity graph (Plan 5 Task 3)"
```

---

## Task 4: PPR seed selection + memory ranking (`graph_recall`)

Seeds for PPR are the entities mentioned by the top BM25 hits for the query (the HippoRAG passage-seeding variant — no NER/entity-name table needed). `graph_rank` returns a `RankedId` list ready for RRF fusion.

**Files:**
- Create: `crates/mnemos_core/src/retrieval/graph_recall.rs`
- Modify: `crates/mnemos_core/src/retrieval/mod.rs` (add `pub mod graph_recall;`)
- Test: `crates/mnemos_core/tests/graph_recall.rs` (new)

- [ ] **Step 1: Write the failing test** — create `crates/mnemos_core/tests/graph_recall.rs`:

```rust
use mnemos_core::graph::MemoryGraph;
use mnemos_core::paths::Paths;
use mnemos_core::retrieval::graph_recall::graph_rank;
use mnemos_core::storage::entity_ops::{link_entity_mention, upsert_edge, upsert_entity};
use mnemos_core::vault::{RememberOpts, Vault};
use chrono::Utc;
use tempfile::TempDir;

#[tokio::test]
async fn graph_rank_surfaces_multi_hop_memory() {
    let tmp = TempDir::new().unwrap();
    let v = Vault::open(Paths::with_root(tmp.path())).await.unwrap();

    // mem1 contains the query words; mem2 does NOT — it is only reachable via
    // the entity graph (mem1->Rust -edge- Tauri<-mem2).
    let mem1 = v.remember("alpha rust topic", RememberOpts::default()).await.unwrap();
    let mem2 = v.remember("zebra unrelated words", RememberOpts::default()).await.unwrap();
    let rust = upsert_entity(v.storage(), "Rust", "tool").await.unwrap();
    let tauri = upsert_entity(v.storage(), "Tauri", "tool").await.unwrap();
    upsert_edge(v.storage(), &rust, &tauri, "uses", &mem1, Utc::now()).await.unwrap();
    link_entity_mention(v.storage(), &mem1, &rust).await.unwrap();
    link_entity_mention(v.storage(), &mem2, &tauri).await.unwrap();

    let g = MemoryGraph::load(v.storage()).await.unwrap();
    let ranked = graph_rank(v.storage(), &g, "alpha rust", 0.85, 30, 5).await.unwrap();

    assert!(ranked.iter().any(|r| r.id == mem1), "seed memory present");
    assert!(
        ranked.iter().any(|r| r.id == mem2),
        "multi-hop memory reachable via the graph should be ranked"
    );
}

#[tokio::test]
async fn graph_rank_empty_without_seeds() {
    let tmp = TempDir::new().unwrap();
    let v = Vault::open(Paths::with_root(tmp.path())).await.unwrap();
    let g = MemoryGraph::load(v.storage()).await.unwrap();
    let ranked = graph_rank(v.storage(), &g, "nothing here", 0.85, 30, 5).await.unwrap();
    assert!(ranked.is_empty());
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p mnemos_core --test graph_recall`
Expected: FAIL — `graph_rank` not found.

- [ ] **Step 3: Create `crates/mnemos_core/src/retrieval/graph_recall.rs`**

```rust
//! Graph retriever: seed PPR from the query's BM25 neighborhood, then rank
//! memories by PPR mass. Produces a `RankedId` list for RRF fusion.

use crate::error::Result;
use crate::graph::ppr::{personalized_pagerank, ppr_rank_memories};
use crate::graph::MemoryGraph;
use crate::retrieval::bm25::bm25_recall;
use crate::retrieval::rrf::RankedId;
use crate::retrieval::RecallOpts;
use crate::storage::Storage;
use std::collections::BTreeSet;

/// PPR seeds = entity node-indices mentioned by the top `seed_hits` BM25 results
/// for `query`. Returns a deterministic (sorted) list of node indices.
pub async fn select_seeds(
    storage: &Storage,
    graph: &MemoryGraph,
    query: &str,
    seed_hits: usize,
) -> Result<Vec<usize>> {
    let opts = RecallOpts {
        k: seed_hits.max(1),
        ..Default::default()
    };
    let hits = bm25_recall(storage, query, opts).await?;
    let mut seeds: BTreeSet<usize> = BTreeSet::new();
    for h in &hits {
        if let Some(entities) = graph.entities_for_memory(&h.memory.id) {
            for &e in entities {
                seeds.insert(e);
            }
        }
    }
    Ok(seeds.into_iter().collect())
}

/// Rank memories for `query` via Personalized PageRank seeded on the query's
/// BM25 neighborhood. Empty when the graph is empty or no seeds are found.
pub async fn graph_rank(
    storage: &Storage,
    graph: &MemoryGraph,
    query: &str,
    alpha: f64,
    iterations: usize,
    seed_hits: usize,
) -> Result<Vec<RankedId>> {
    if graph.is_empty() {
        return Ok(vec![]);
    }
    let seeds = select_seeds(storage, graph, query, seed_hits).await?;
    if seeds.is_empty() {
        return Ok(vec![]);
    }
    let scores = personalized_pagerank(graph, &seeds, alpha, iterations);
    Ok(ppr_rank_memories(graph, &scores))
}
```

- [ ] **Step 4: Declare the module** in `crates/mnemos_core/src/retrieval/mod.rs` (with the other `pub mod` lines):

```rust
pub mod graph_recall;
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p mnemos_core --test graph_recall`
Expected: PASS (2 tests).

- [ ] **Step 6: Commit**

```bash
cargo fmt --all && cargo clippy -p mnemos_core --all-targets -- -D warnings
git add crates/mnemos_core/src/retrieval/graph_recall.rs crates/mnemos_core/src/retrieval/mod.rs crates/mnemos_core/tests/graph_recall.rs
git commit -m "feat: PPR seed selection + graph_rank retriever (Plan 5 Task 4)"
```

---

## Task 5: Fuse PPR into hybrid recall (3-way RRF)

Add `ppr_rank` to `RecallHit`/`Explain` and `graph` + PPR params to `RecallOpts`. Refactor `hybrid.rs` so a single `hybrid_recall_full` does BM25 + Dense + (optional) PPR → RRF → reweight → optional rerank; the existing `hybrid_recall` / `hybrid_recall_with_rerank` become thin wrappers (graph = None).

**Files:**
- Modify: `crates/mnemos_core/src/retrieval/mod.rs` (RecallOpts fields + RecallHit/Explain `ppr_rank`)
- Modify: `crates/mnemos_core/src/retrieval/hybrid.rs` (rewrite)
- Test: `crates/mnemos_core/tests/hybrid_graph_fusion.rs` (new)

- [ ] **Step 1: Write the failing test** — create `crates/mnemos_core/tests/hybrid_graph_fusion.rs`:

```rust
use mnemos_core::graph::MemoryGraph;
use mnemos_core::paths::Paths;
use mnemos_core::retrieval::hybrid::hybrid_recall_full;
use mnemos_core::retrieval::RecallOpts;
use mnemos_core::storage::entity_ops::{link_entity_mention, upsert_edge, upsert_entity};
use mnemos_core::vault::{RememberOpts, Vault};
use chrono::Utc;
use tempfile::TempDir;

#[tokio::test]
async fn graph_fusion_pulls_in_multi_hop_memory() {
    let tmp = TempDir::new().unwrap();
    let v = Vault::open(Paths::with_root(tmp.path())).await.unwrap();

    let mem1 = v.remember("alpha rust topic", RememberOpts::default()).await.unwrap();
    let mem2 = v.remember("zebra unrelated words", RememberOpts::default()).await.unwrap();
    let rust = upsert_entity(v.storage(), "Rust", "tool").await.unwrap();
    let tauri = upsert_entity(v.storage(), "Tauri", "tool").await.unwrap();
    upsert_edge(v.storage(), &rust, &tauri, "uses", &mem1, Utc::now()).await.unwrap();
    link_entity_mention(v.storage(), &mem1, &rust).await.unwrap();
    link_entity_mention(v.storage(), &mem2, &tauri).await.unwrap();

    let g = MemoryGraph::load(v.storage()).await.unwrap();
    let opts = RecallOpts { k: 10, explain: true, ..Default::default() };
    // No embedder (BM25 + PPR only); BM25 alone would never return mem2.
    let hits = hybrid_recall_full(v.storage(), None, None, Some(&g), "alpha rust", opts)
        .await
        .unwrap();

    let m2 = hits.iter().find(|h| h.memory.id == mem2);
    assert!(m2.is_some(), "graph fusion should surface the multi-hop memory");
    assert!(m2.unwrap().ppr_rank.is_some(), "mem2 came from the PPR retriever");
    // sanity: mem1 still present
    assert!(hits.iter().any(|h| h.memory.id == mem1));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p mnemos_core --test hybrid_graph_fusion`
Expected: FAIL — `hybrid_recall_full` / `ppr_rank` not found.

- [ ] **Step 3: Extend `RecallOpts`, `RecallHit`, `Explain`** in `retrieval/mod.rs`.

In `RecallOpts`, add three fields (after `rerank`):

```rust
    /// Include the graph (PPR) retriever in fusion when a graph is supplied.
    pub graph: bool,
    /// PPR restart probability complement (`alpha`). Canonical 0.85.
    pub ppr_alpha: f64,
    /// PPR power-iteration count.
    pub ppr_iterations: usize,
```

In the `Default for RecallOpts` impl, add:

```rust
            graph: true,
            ppr_alpha: 0.85,
            ppr_iterations: 30,
```

In `RecallHit`, add (after `dense_distance`):

```rust
    /// Rank of this memory in the graph (PPR) retriever's results, if matched.
    pub ppr_rank: Option<usize>,
```

In `Explain`, add (after `dense_distance`):

```rust
    pub ppr_rank: Option<usize>,
```

- [ ] **Step 4: Rewrite `crates/mnemos_core/src/retrieval/hybrid.rs`**

```rust
//! Hybrid retrieval orchestrator. Runs BM25, Dense, and (optionally) graph PPR,
//! fuses with RRF, applies reweighting, optionally reranks, returns top-k.

use crate::error::Result;
use crate::graph::MemoryGraph;
use crate::providers::{Embedder, Reranker};
use crate::retrieval::bm25::bm25_recall;
use crate::retrieval::dense::dense_recall;
use crate::retrieval::graph_recall::graph_rank;
use crate::retrieval::reweight::apply_reweight_with_breakdown;
use crate::retrieval::rrf::{rrf_fuse, RankedId};
use crate::retrieval::{Explain, RecallHit, RecallOpts};
use crate::storage::memory_ops::get_memory;
use crate::storage::Storage;
use std::collections::HashMap;

/// Seed-hit count for the PPR retriever.
const PPR_SEED_HITS: usize = 5;

/// Full hybrid recall: BM25 + Dense + (optional) graph PPR → RRF → reweight →
/// optional rerank. `graph`/`reranker` may be `None`; PPR is skipped unless a
/// non-empty graph is supplied and `opts.graph` is true.
pub async fn hybrid_recall_full(
    storage: &Storage,
    embedder: Option<&dyn Embedder>,
    reranker: Option<&dyn Reranker>,
    graph: Option<&MemoryGraph>,
    query: &str,
    opts: RecallOpts,
) -> Result<Vec<RecallHit>> {
    let stage_k = opts.k * 5;
    let stage_opts = RecallOpts {
        k: stage_k,
        explain: false,
        rerank: false,
        graph: false,
        ..opts.clone()
    };

    let bm25 = bm25_recall(storage, query, stage_opts.clone()).await?;
    let dense = if let Some(e) = embedder {
        dense_recall(storage, e, query, stage_opts.clone()).await?
    } else {
        vec![]
    };
    let ppr_ranked: Vec<RankedId> = match graph {
        Some(g) if opts.graph && !g.is_empty() => {
            graph_rank(storage, g, query, opts.ppr_alpha, opts.ppr_iterations, PPR_SEED_HITS).await?
        }
        _ => vec![],
    };

    let bm25_ranked: Vec<RankedId> = bm25
        .iter()
        .enumerate()
        .map(|(i, h)| RankedId { id: h.memory.id.clone(), rank: i + 1 })
        .collect();
    let dense_ranked: Vec<RankedId> = dense
        .iter()
        .enumerate()
        .map(|(i, h)| RankedId { id: h.memory.id.clone(), rank: i + 1 })
        .collect();

    let fused = rrf_fuse(&[&bm25_ranked, &dense_ranked, &ppr_ranked], opts.rrf_k);

    let bm25_rank_by_id: HashMap<&str, usize> =
        bm25_ranked.iter().map(|r| (r.id.as_str(), r.rank)).collect();
    let dense_rank_by_id: HashMap<&str, usize> =
        dense_ranked.iter().map(|r| (r.id.as_str(), r.rank)).collect();
    let ppr_rank_by_id: HashMap<&str, usize> =
        ppr_ranked.iter().map(|r| (r.id.as_str(), r.rank)).collect();
    let dense_dist_by_id: HashMap<&str, f32> = dense
        .iter()
        .filter_map(|h| h.dense_distance.map(|d| (h.memory.id.as_str(), d)))
        .collect();

    let mut hits: Vec<RecallHit> = Vec::with_capacity(fused.len());
    for f in fused.iter() {
        let memory = get_memory(storage, &f.id).await?;
        if !opts.include_invalid && memory.invalid_at.is_some() {
            continue;
        }
        let bw = apply_reweight_with_breakdown(f.score, &memory, &opts.reweight);
        let explain = if opts.explain {
            Some(Explain {
                bm25_rank: bm25_rank_by_id.get(f.id.as_str()).copied(),
                dense_rank: dense_rank_by_id.get(f.id.as_str()).copied(),
                dense_distance: dense_dist_by_id.get(f.id.as_str()).copied(),
                ppr_rank: ppr_rank_by_id.get(f.id.as_str()).copied(),
                rrf_score: f.score,
                weight_recency: bw.recency,
                weight_importance: bw.importance,
                weight_strength: bw.strength,
                weight_tier: bw.tier,
                rerank_score: None,
                final_score: bw.final_score,
            })
        } else {
            None
        };
        hits.push(RecallHit {
            memory,
            score: bw.final_score,
            bm25_rank: bm25_rank_by_id.get(f.id.as_str()).copied(),
            dense_rank: dense_rank_by_id.get(f.id.as_str()).copied(),
            dense_distance: dense_dist_by_id.get(f.id.as_str()).copied(),
            ppr_rank: ppr_rank_by_id.get(f.id.as_str()).copied(),
            explain,
        });
    }

    hits.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    hits.truncate(opts.k);

    if opts.rerank {
        if let Some(rr) = reranker {
            let candidates: Vec<String> = hits
                .iter()
                .map(|h| format!("{}\n\n{}", h.memory.title, h.memory.body))
                .collect();
            let scores = rr.rerank(query, &candidates).await?;
            if scores.len() != hits.len() {
                return Err(crate::error::MnemosError::Internal(format!(
                    "reranker returned {} scores for {} candidates",
                    scores.len(),
                    hits.len()
                )));
            }
            for (h, s) in hits.iter_mut().zip(scores.iter()) {
                let score_f64 = f64::from(*s);
                h.score = score_f64;
                if let Some(e) = h.explain.as_mut() {
                    e.rerank_score = Some(score_f64);
                    e.final_score = score_f64;
                }
            }
            hits.sort_by(|a, b| {
                b.score
                    .partial_cmp(&a.score)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
        }
    }

    Ok(hits)
}

/// BM25 + Dense fusion (no graph, no rerank). Back-compatible wrapper.
pub async fn hybrid_recall(
    storage: &Storage,
    embedder: Option<&dyn Embedder>,
    query: &str,
    opts: RecallOpts,
) -> Result<Vec<RecallHit>> {
    hybrid_recall_full(storage, embedder, None, None, query, opts).await
}

/// Hybrid recall with an optional cross-encoder reranker (no graph).
pub async fn hybrid_recall_with_rerank(
    storage: &Storage,
    embedder: Option<&dyn Embedder>,
    reranker: Option<&dyn Reranker>,
    query: &str,
    opts: RecallOpts,
) -> Result<Vec<RecallHit>> {
    hybrid_recall_full(storage, embedder, reranker, None, query, opts).await
}
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p mnemos_core --test hybrid_graph_fusion && cargo test -p mnemos_core --test hybrid_retrieval`
Expected: PASS (new test + the existing hybrid suite still green — wrappers preserve behavior).

- [ ] **Step 6: Commit**

```bash
cargo fmt --all && cargo clippy -p mnemos_core --all-targets -- -D warnings
git add crates/mnemos_core/src/retrieval/mod.rs crates/mnemos_core/src/retrieval/hybrid.rs crates/mnemos_core/tests/hybrid_graph_fusion.rs
git commit -m "feat: fuse PPR into hybrid recall as a third RRF retriever (Plan 5 Task 5)"
```

---

## Task 6: Wire graph recall into the daemon

The daemon's shared recall helper builds the graph and routes through `hybrid_recall_full`; the REST search + MCP recall expose a `graph` flag (default on).

**Files:**
- Modify: `crates/mnemos_daemon/src/routes/recall_helper.rs`
- Modify: `crates/mnemos_daemon/src/routes/memories.rs` (search `graph` flag)
- Modify: `crates/mnemos_daemon/src/mcp/tools.rs` (recall `graph` arg + descriptor)
- Test: add to `crates/mnemos_daemon/tests/memories.rs`

- [ ] **Step 1: Write the test** — append to `crates/mnemos_daemon/tests/memories.rs`. The HTTP fixture doesn't expose the vault for entity wiring, so the multi-hop behavior is covered by the core `hybrid_graph_fusion` test (Task 5); here we verify the daemon accepts the `graph` flag and standard recall still works through the graph-enabled path:

```rust
#[tokio::test]
async fn search_accepts_graph_flag() {
    let (app, token) = fixture().await;
    let (_, _) = call(
        app.clone(),
        "POST",
        "/v1/memories",
        Some(&token),
        r#"{"body":"alpha rust topic","tier":"semantic"}"#,
    )
    .await;
    let (s, b) = call(
        app,
        "POST",
        "/v1/memories/search",
        Some(&token),
        r#"{"query":"alpha rust","k":10,"graph":true}"#,
    )
    .await;
    assert_eq!(s, axum::http::StatusCode::OK, "{b}");
    let v: serde_json::Value = serde_json::from_str(&b).unwrap();
    assert!(v["hits"].as_array().unwrap().iter().any(|h| h["memory"]["body"] == "alpha rust topic"));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p mnemos_daemon --test memories search_accepts_graph_flag`
Expected: FAIL — `graph` field not on the search request (deserialize ignores unknown fields, so this may instead pass trivially; if so, proceed — the real change is wiring the helper). If it compiles and passes immediately, treat Steps 3-4 as the implementation and re-run to confirm still green.

- [ ] **Step 3: Update `recall_helper.rs`** to build the graph and call `hybrid_recall_full`:

```rust
//! Shared recall path used by both the REST search endpoint and the MCP recall
//! tool, so retriever wiring (embedder, reranker, graph) lives in one place.

use mnemos_core::error::Result;
use mnemos_core::graph::MemoryGraph;
use mnemos_core::retrieval::hybrid::hybrid_recall_full;
use mnemos_core::retrieval::{RecallHit, RecallOpts};

use crate::state::AppState;

/// Run hybrid recall: BM25 + Dense + (optional) graph PPR, with reranking when
/// requested + configured. The graph is built per-call from storage and is
/// skipped automatically when empty.
pub async fn recall(state: &AppState, query: &str, opts: RecallOpts) -> Result<Vec<RecallHit>> {
    let embedder = state.vault.embedder().cloned();
    let embedder_ref = embedder.as_ref().map(|a| a.as_ref());

    let graph = if opts.graph {
        let g = MemoryGraph::load(state.vault.storage()).await?;
        if g.is_empty() {
            None
        } else {
            Some(g)
        }
    } else {
        None
    };

    let reranker = state.reranker.clone();
    let reranker_ref = reranker.as_ref().map(|a| a.as_ref());

    hybrid_recall_full(
        state.vault.storage(),
        embedder_ref,
        reranker_ref,
        graph.as_ref(),
        query,
        opts,
    )
    .await
}
```

- [ ] **Step 4: Add the `graph` flag to the search request + MCP recall.**

In `routes/memories.rs`, add to `SearchReq`:

```rust
    #[serde(default = "default_true")]
    graph: bool,
```

and the helper near the other defaults:

```rust
fn default_true() -> bool {
    true
}
```

and set it on the `RecallOpts` built in `search` (add `graph: req.graph,` to the struct literal).

In `mcp/tools.rs`, add `"graph": { "type": "boolean", "default": true }` to the `recall` tool's `inputSchema.properties`, and in the `recall` fn set `graph: args["graph"].as_bool().unwrap_or(true),` on the `RecallOpts`.

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p mnemos_daemon --test memories && cargo test -p mnemos_daemon --test mcp`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
cargo fmt --all && cargo clippy -p mnemos_daemon --all-targets -- -D warnings
git add crates/mnemos_daemon/src/routes/recall_helper.rs crates/mnemos_daemon/src/routes/memories.rs crates/mnemos_daemon/src/mcp/tools.rs crates/mnemos_daemon/tests/memories.rs
git commit -m "feat: wire graph PPR into daemon recall (Plan 5 Task 6)"
```

---

## Task 7: Schema v5 + salience accumulator (`reflection_ops`)

Reflection is triggered by a salience accumulator that rises as new knowledge accrues. Add the bookkeeping: a `reflected_at` column on memories (so a memory is reflected-over at most once) and a single-row `reflection_state` table.

**Files:**
- Modify: `crates/mnemos_core/src/storage/migrations.rs` (v5)
- Create: `crates/mnemos_core/src/storage/reflection_ops.rs`
- Modify: `crates/mnemos_core/src/storage/mod.rs` (add `pub mod reflection_ops;`)
- Test: `crates/mnemos_core/tests/reflection_state.rs` (new)

- [ ] **Step 1: Write the failing test** — create `crates/mnemos_core/tests/reflection_state.rs`:

```rust
use mnemos_core::storage::reflection_ops::{bump_salience, get_salience, reset_salience};
use mnemos_core::storage::Storage;
use chrono::Utc;
use tempfile::TempDir;

#[tokio::test]
async fn salience_accumulates_and_resets() {
    let tmp = TempDir::new().unwrap();
    let storage = Storage::open(&tmp.path().join("s.db")).await.unwrap();
    assert!(storage.schema_version().await.unwrap() >= 5);

    assert_eq!(get_salience(&storage).await.unwrap(), 0.0);
    let after = bump_salience(&storage, 3.0).await.unwrap();
    assert_eq!(after, 3.0);
    let after2 = bump_salience(&storage, 2.5).await.unwrap();
    assert_eq!(after2, 5.5);
    reset_salience(&storage, Utc::now()).await.unwrap();
    assert_eq!(get_salience(&storage).await.unwrap(), 0.0);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p mnemos_core --test reflection_state`
Expected: FAIL — module / functions / schema v5 not present.

- [ ] **Step 3: Add migration v5** in `migrations.rs`. After the `current < 4` block, add:

```rust
        if current < 5 {
            migration_v5(&conn).await?;
            conn.execute(
                "INSERT OR IGNORE INTO schema_migrations (version) VALUES (5)",
                (),
            )
            .await?;
        }
```

and the migration fn + statements:

```rust
async fn migration_v5(conn: &libsql::Connection) -> Result<()> {
    for stmt in V5_STATEMENTS {
        conn.execute(stmt, ()).await?;
    }
    Ok(())
}

const V5_STATEMENTS: &[&str] = &[
    // Stamped once a memory has been included in a reflection pass.
    "ALTER TABLE memories ADD COLUMN reflected_at TEXT",
    // Single-row salience accumulator driving reflection triggers.
    "CREATE TABLE IF NOT EXISTS reflection_state (
        id               INTEGER PRIMARY KEY CHECK(id = 1),
        salience         REAL NOT NULL DEFAULT 0,
        last_reflected_at TEXT
    )",
    "INSERT OR IGNORE INTO reflection_state (id, salience) VALUES (1, 0)",
];
```

- [ ] **Step 4: Bump stale schema-version assertions.** Existing tests assert the latest version is 4. Update each `assert_eq!(..., 4)` schema-version check to `5` in: `crates/mnemos_core/tests/schema_v1.rs`, `crates/mnemos_core/tests/schema_v2.rs`, `crates/mnemos_core/tests/storage_open.rs` (grep for `schema_version` to find them).

- [ ] **Step 5: Create `crates/mnemos_core/src/storage/reflection_ops.rs`**

```rust
//! Salience accumulator backing reflection triggers.

use crate::error::{MnemosError, Result};
use crate::storage::Storage;
use chrono::{DateTime, Utc};
use libsql::params;

/// Current accumulated salience.
pub async fn get_salience(storage: &Storage) -> Result<f64> {
    let conn = storage.conn()?;
    let mut rows = conn
        .query("SELECT salience FROM reflection_state WHERE id = 1", ())
        .await?;
    match rows.next().await? {
        Some(r) => Ok(r.get::<f64>(0)?),
        None => Ok(0.0),
    }
}

/// Add `delta` to the accumulator; returns the new value.
pub async fn bump_salience(storage: &Storage, delta: f64) -> Result<f64> {
    let (conn, _g) = storage.write_conn().await?;
    conn.execute(
        "UPDATE reflection_state SET salience = salience + ? WHERE id = 1",
        params![delta],
    )
    .await?;
    drop(_g);
    get_salience(storage).await
}

/// Reset the accumulator to zero and record the reflection time.
pub async fn reset_salience(storage: &Storage, now: DateTime<Utc>) -> Result<()> {
    let (conn, _g) = storage.write_conn().await?;
    let n = conn
        .execute(
            "UPDATE reflection_state SET salience = 0, last_reflected_at = ? WHERE id = 1",
            params![now.to_rfc3339()],
        )
        .await?;
    if n == 0 {
        return Err(MnemosError::Internal("reflection_state row missing".into()));
    }
    Ok(())
}
```

- [ ] **Step 6: Declare the module** in `storage/mod.rs`: add `pub mod reflection_ops;`.

- [ ] **Step 7: Run tests to verify they pass**

Run: `cargo test -p mnemos_core --test reflection_state && cargo test -p mnemos_core`
Expected: PASS (and the bumped schema-version tests pass).

- [ ] **Step 8: Commit**

```bash
cargo fmt --all && cargo clippy -p mnemos_core --all-targets -- -D warnings
git add crates/mnemos_core/src/storage/migrations.rs crates/mnemos_core/src/storage/reflection_ops.rs crates/mnemos_core/src/storage/mod.rs crates/mnemos_core/tests/reflection_state.rs crates/mnemos_core/tests/schema_v1.rs crates/mnemos_core/tests/schema_v2.rs crates/mnemos_core/tests/storage_open.rs
git commit -m "feat: schema v5 salience accumulator + reflection_ops (Plan 5 Task 7)"
```

---

## Task 8: Memory-link + reflection-source helpers + `Vault::remember_reflection`

The primitives reflection needs: link memories, find recent un-reflected memories, mark them reflected, list by kind (for global recall later), and a Vault convenience to write a reflection-tier memory with `reflects_on` provenance.

**Files:**
- Modify: `crates/mnemos_core/src/storage/memory_ops.rs`
- Modify: `crates/mnemos_core/src/vault.rs`
- Test: `crates/mnemos_core/tests/reflection_helpers.rs` (new)

- [ ] **Step 1: Write the failing test** — create `crates/mnemos_core/tests/reflection_helpers.rs`:

```rust
use mnemos_core::paths::Paths;
use mnemos_core::storage::memory_ops::{
    add_memory_link, list_by_kind, mark_reflected, recent_unreflected,
};
use mnemos_core::types::MemoryType;
use mnemos_core::vault::{RememberOpts, Vault};
use mnemos_core::Tier;
use chrono::Utc;
use tempfile::TempDir;

#[tokio::test]
async fn unreflected_query_and_mark() {
    let tmp = TempDir::new().unwrap();
    let v = Vault::open(Paths::with_root(tmp.path())).await.unwrap();
    let a = v.remember("fact a", RememberOpts::default()).await.unwrap();
    let _b = v.remember("fact b", RememberOpts::default()).await.unwrap();

    let pending = recent_unreflected(v.storage(), 10).await.unwrap();
    assert_eq!(pending.len(), 2);

    mark_reflected(v.storage(), &[a.clone()], Utc::now()).await.unwrap();
    let pending2 = recent_unreflected(v.storage(), 10).await.unwrap();
    assert_eq!(pending2.len(), 1);
    assert!(pending2.iter().all(|m| m.id != a));
}

#[tokio::test]
async fn remember_reflection_links_sources() {
    let tmp = TempDir::new().unwrap();
    let v = Vault::open(Paths::with_root(tmp.path())).await.unwrap();
    let src = v.remember("source fact", RememberOpts::default()).await.unwrap();

    let refl = v
        .remember_reflection(
            "Shaun prefers Rust",
            Some("Reflection (preference)".into()),
            MemoryType::Reflection,
            vec!["preference".into()],
            &[src.clone()],
            vec![],
        )
        .await
        .unwrap();

    let mem = v.get(&refl).await.unwrap();
    assert_eq!(mem.tier, Tier::Reflection);
    assert_eq!(mem.kind, MemoryType::Reflection);

    // reflects_on link exists
    let conn = v.storage().conn().unwrap();
    let mut rows = conn
        .query(
            "SELECT COUNT(*) FROM memory_links WHERE source_id = ? AND target_id = ? AND kind = 'reflects_on'",
            libsql::params![refl.clone(), src.clone()],
        )
        .await
        .unwrap();
    let n: i64 = rows.next().await.unwrap().unwrap().get(0).unwrap();
    assert_eq!(n, 1);

    // direct add_memory_link + list_by_kind smoke
    add_memory_link(v.storage(), &refl, &src, "related").await.unwrap();
    let reflections = list_by_kind(v.storage(), MemoryType::Reflection, 10).await.unwrap();
    assert_eq!(reflections.len(), 1);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p mnemos_core --test reflection_helpers`
Expected: FAIL — helpers / `remember_reflection` not found.

- [ ] **Step 3: Append helpers to `memory_ops.rs`**

```rust
/// Insert a typed link between two memories. Idempotent.
pub async fn add_memory_link(
    storage: &Storage,
    source_id: &str,
    target_id: &str,
    kind: &str,
) -> Result<()> {
    let (conn, _g) = storage.write_conn().await?;
    conn.execute(
        "INSERT OR IGNORE INTO memory_links (source_id, target_id, kind) VALUES (?, ?, ?)",
        params![source_id.to_string(), target_id.to_string(), kind.to_string()],
    )
    .await?;
    Ok(())
}

/// Recent valid semantic memories that have not yet been included in a
/// reflection pass, newest first.
pub async fn recent_unreflected(storage: &Storage, limit: usize) -> Result<Vec<Memory>> {
    let conn = storage.conn()?;
    let mut rows = conn
        .query(
            "SELECT id, tier, kind, title, body,
                    tags_json, entities_json, links_json, provenance_json,
                    created_at, ingested_at, valid_at, invalid_at, superseded_by,
                    strength, importance, last_accessed, access_count,
                    workspace, source_tool, mnemos_version
               FROM memories
              WHERE tier = 'semantic' AND invalid_at IS NULL AND reflected_at IS NULL
              ORDER BY created_at DESC
              LIMIT ?",
            params![limit as i64],
        )
        .await?;
    let mut out = Vec::new();
    while let Some(row) = rows.next().await? {
        out.push(row_to_memory(&row)?);
    }
    Ok(out)
}

/// Stamp `reflected_at` on the given memories.
pub async fn mark_reflected(storage: &Storage, ids: &[String], at: DateTime<Utc>) -> Result<()> {
    if ids.is_empty() {
        return Ok(());
    }
    let (conn, _g) = storage.write_conn().await?;
    let ts = at.to_rfc3339();
    for id in ids {
        conn.execute(
            "UPDATE memories SET reflected_at = ? WHERE id = ?",
            params![ts.clone(), id.clone()],
        )
        .await?;
    }
    Ok(())
}

/// List valid memories of a given kind, newest first.
pub async fn list_by_kind(storage: &Storage, kind: MemoryType, limit: usize) -> Result<Vec<Memory>> {
    let kind_str = serde_json::to_string(&kind)?.trim_matches('"').to_string();
    let conn = storage.conn()?;
    let mut rows = conn
        .query(
            "SELECT id, tier, kind, title, body,
                    tags_json, entities_json, links_json, provenance_json,
                    created_at, ingested_at, valid_at, invalid_at, superseded_by,
                    strength, importance, last_accessed, access_count,
                    workspace, source_tool, mnemos_version
               FROM memories
              WHERE kind = ? AND invalid_at IS NULL
              ORDER BY created_at DESC
              LIMIT ?",
            params![kind_str, limit as i64],
        )
        .await?;
    let mut out = Vec::new();
    while let Some(row) = rows.next().await? {
        out.push(row_to_memory(&row)?);
    }
    Ok(out)
}
```

- [ ] **Step 4: Add `Vault::remember_reflection`** in `vault.rs` (inside `impl Vault`, e.g. after `patch`). Add `add_memory_link` to the `use crate::storage::memory_ops::{...}` import:

```rust
    /// Write a reflection-tier memory and link it back to its source memories
    /// with `reflects_on` edges.
    pub async fn remember_reflection(
        &self,
        body: &str,
        title: Option<String>,
        kind: MemoryType,
        tags: Vec<String>,
        reflects_on: &[String],
        provenance: Vec<Provenance>,
    ) -> Result<String> {
        let id = self
            .remember(
                body,
                RememberOpts {
                    title,
                    tier: Tier::Reflection,
                    kind,
                    tags,
                    provenance,
                    source_tool: Some("mnemos-reflection".into()),
                    ..Default::default()
                },
            )
            .await?;
        for src in reflects_on {
            add_memory_link(&self.storage, &id, src, "reflects_on").await?;
        }
        Ok(id)
    }
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p mnemos_core --test reflection_helpers`
Expected: PASS (2 tests).

- [ ] **Step 6: Commit**

```bash
cargo fmt --all && cargo clippy -p mnemos_core --all-targets -- -D warnings
git add crates/mnemos_core/src/storage/memory_ops.rs crates/mnemos_core/src/vault.rs crates/mnemos_core/tests/reflection_helpers.rs
git commit -m "feat: reflection source helpers + Vault::remember_reflection (Plan 5 Task 8)"
```

---

## Task 9: `reflect()` pipeline stage + `MockLlm` TASK=reflect

Synthesize recent un-reflected memories into typed reflection memories.

**Files:**
- Modify: `crates/mnemos_core/src/providers/mock_llm.rs` (add `TASK=reflect` branch)
- Create: `crates/mnemos_core/src/pipeline/reflect.rs`
- Modify: `crates/mnemos_core/src/pipeline/mod.rs` (add `pub mod reflect;`)
- Test: `crates/mnemos_core/tests/pipeline_reflect.rs` (new)

- [ ] **Step 1: Write the failing test** — create `crates/mnemos_core/tests/pipeline_reflect.rs`:

```rust
use mnemos_core::paths::Paths;
use mnemos_core::pipeline::reflect::reflect;
use mnemos_core::providers::mock_llm::MockLlm;
use mnemos_core::storage::memory_ops::ListFilter;
use mnemos_core::vault::{RememberOpts, Vault};
use mnemos_core::Tier;
use tempfile::TempDir;

#[tokio::test]
async fn reflect_creates_typed_reflection_and_marks_sources() {
    let tmp = TempDir::new().unwrap();
    let v = Vault::open(Paths::with_root(tmp.path())).await.unwrap();
    // The MockLlm reads REFLECT:<kind>|<text> markers out of the memory bodies.
    let src = v
        .remember(
            "We compared editors. REFLECT:preference|Shaun prefers Rust over Go",
            RememberOpts::default(),
        )
        .await
        .unwrap();

    let created = reflect(&v, &MockLlm::new(), 20).await.unwrap();
    assert_eq!(created.len(), 1);

    let refl = v
        .list(ListFilter { tiers: Some(vec![Tier::Reflection]), ..Default::default() })
        .await
        .unwrap();
    assert_eq!(refl.len(), 1);
    assert_eq!(refl[0].body, "Shaun prefers Rust over Go");
    assert!(refl[0].tags.iter().any(|t| t == "preference"));

    // source is marked reflected → not returned again
    let pending = mnemos_core::storage::memory_ops::recent_unreflected(v.storage(), 10)
        .await
        .unwrap();
    assert!(pending.iter().all(|m| m.id != src));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p mnemos_core --test pipeline_reflect`
Expected: FAIL — `reflect` not found.

- [ ] **Step 3: Add the `TASK=reflect` branch to `MockLlm`** in `mock_llm.rs`. In the `complete` method's `if/else` chain, add a branch (before the final `else`):

```rust
        } else if req.system.contains("TASK=reflect") {
            // One reflection per `REFLECT:<kind>|<text>` occurrence (kind optional).
            let reflections: Vec<_> = content
                .lines()
                .filter_map(|l| l.find("REFLECT:").map(|i| &l[i + "REFLECT:".len()..]))
                .map(|rest| {
                    let rest = rest.trim();
                    match rest.split_once('|') {
                        Some((kind, text)) => json!({ "kind": kind.trim(), "text": text.trim() }),
                        None => json!({ "kind": "insight", "text": rest }),
                    }
                })
                .filter(|v| !v["text"].as_str().unwrap_or("").is_empty())
                .collect();
            json!({ "reflections": reflections }).to_string()
```

Also extend the doc comment's marker list to mention `TASK=reflect`.

- [ ] **Step 4: Create `crates/mnemos_core/src/pipeline/reflect.rs`**

```rust
//! Reflection stage: synthesize recent un-reflected memories into durable,
//! typed reflection-tier memories with `reflects_on` provenance.

use crate::error::{MnemosError, Result};
use crate::pipeline::extract_json;
use crate::providers::{CompletionRequest, LlmProvider};
use crate::storage::memory_ops::{mark_reflected, recent_unreflected};
use crate::types::MemoryType;
use crate::vault::Vault;
use chrono::Utc;
use serde::Deserialize;

pub const REFLECT_SYSTEM: &str = "TASK=reflect\n\
You review recent memories and synthesize higher-level, durable insights. Each \
reflection has a `kind` (one of: preference, pattern, insight, decision) and \
standalone `text`. Respond ONLY with JSON \
{\"reflections\":[{\"kind\":\"...\",\"text\":\"...\"}]}.";

#[derive(Deserialize)]
struct ReflectOut {
    #[serde(default)]
    reflections: Vec<ReflectionIn>,
}

#[derive(Deserialize)]
struct ReflectionIn {
    #[serde(default)]
    kind: Option<String>,
    text: String,
}

/// Reflect over up to `max_sources` recent un-reflected semantic memories.
/// Writes one reflection-tier memory per synthesized insight (linked to all
/// sources) and marks the sources reflected. Returns the new memory ids.
pub async fn reflect(vault: &Vault, llm: &dyn LlmProvider, max_sources: usize) -> Result<Vec<String>> {
    let sources = recent_unreflected(vault.storage(), max_sources).await?;
    if sources.is_empty() {
        return Ok(vec![]);
    }
    let corpus = sources
        .iter()
        .map(|m| format!("- {}", m.body))
        .collect::<Vec<_>>()
        .join("\n");
    let raw = llm.complete(&CompletionRequest::new(REFLECT_SYSTEM, corpus)).await?;
    let parsed: ReflectOut = serde_json::from_str(extract_json(&raw))
        .map_err(|e| MnemosError::Internal(format!("reflect parse failed: {e}; raw={raw}")))?;

    let source_ids: Vec<String> = sources.iter().map(|m| m.id.clone()).collect();
    let mut created = Vec::new();
    for r in parsed.reflections {
        let text = r.text.trim().to_string();
        if text.is_empty() {
            continue;
        }
        let kind_tag = r.kind.unwrap_or_else(|| "insight".into());
        let title = format!("Reflection ({kind_tag})");
        let id = vault
            .remember_reflection(
                &text,
                Some(title),
                MemoryType::Reflection,
                vec![kind_tag],
                &source_ids,
                vec![],
            )
            .await?;
        created.push(id);
    }
    // Mark sources reflected even if nothing was synthesized, so the same window
    // is not reprocessed on the next trigger.
    mark_reflected(vault.storage(), &source_ids, Utc::now()).await?;
    Ok(created)
}
```

- [ ] **Step 5: Declare the module** in `pipeline/mod.rs`: add `pub mod reflect;`.

- [ ] **Step 6: Run tests to verify they pass**

Run: `cargo test -p mnemos_core --test pipeline_reflect && cargo test -p mnemos_core --lib providers::mock_llm`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
cargo fmt --all && cargo clippy -p mnemos_core --all-targets -- -D warnings
git add crates/mnemos_core/src/providers/mock_llm.rs crates/mnemos_core/src/pipeline/reflect.rs crates/mnemos_core/src/pipeline/mod.rs crates/mnemos_core/tests/pipeline_reflect.rs
git commit -m "feat: reflect() pipeline stage + MockLlm reflect marker (Plan 5 Task 9)"
```

---

## Task 10: Reflection trigger in the runner + config + events

Wire reflection into the daemon: a `[reflection]` config block, two new events, and a salience-driven trigger after each successful session pipeline.

**Files:**
- Modify: `crates/mnemos_daemon/src/config.rs` (`[reflection]` + `[community]` blocks)
- Modify: `crates/mnemos_daemon/src/events.rs` (`ReflectionCompleted`, `CommunityDetected`)
- Modify: `crates/mnemos_daemon/src/pipeline_runner.rs` (salience bump + trigger)
- Test: `crates/mnemos_daemon/tests/reflection_trigger.rs` (new)

- [ ] **Step 1: Write the failing test** — create `crates/mnemos_daemon/tests/reflection_trigger.rs`:

```rust
use mnemos_core::paths::Paths;
use mnemos_core::providers::mock_llm::MockLlm;
use mnemos_core::storage::memory_ops::ListFilter;
use mnemos_core::vault::Vault;
use mnemos_core::Tier;
use mnemos_daemon::config::Config;
use mnemos_daemon::events::Event;
use mnemos_daemon::build_app_full;
use std::sync::Arc;
use std::time::Duration;
use tempfile::TempDir;

#[tokio::test]
async fn session_pipeline_triggers_reflection_at_threshold() {
    let tmp = Box::leak(Box::new(TempDir::new().unwrap()));
    let vault = Vault::open(Paths::with_root(tmp.path())).await.unwrap();
    // Low threshold so a single fact triggers reflection.
    let mut cfg = Config::default();
    cfg.reflection.salience_threshold = 1.0;
    let (_app, state, handle) =
        build_app_full(cfg, vault, None, Some(Arc::new(MockLlm::new()))).await.unwrap();
    let handle = handle.unwrap();
    let mut rx = state.events.subscribe();

    // Seed a session whose chunk extracts a fact that ALSO carries a REFLECT marker,
    // so the reflection pass produces a reflection from the new semantic memory.
    {
        let (conn, _g) = state.vault.storage().write_conn().await.unwrap();
        conn.execute("INSERT INTO sessions (id, started_at) VALUES ('sess_r','2026-01-01T00:00:00+00:00')", ()).await.unwrap();
        conn.execute(
            "INSERT INTO chunks (id, session_id, speaker, ordinal, body, created_at)
                 VALUES ('chunk_r','sess_r','user',0,'FACT: REFLECT:insight|Shaun ships Rust daily','2026-01-01T00:00:00+00:00')",
            (),
        ).await.unwrap();
    }
    state.events.publish(Event::SessionEnded { id: "sess_r".into() });

    let got = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Ok(Event::ReflectionCompleted { reflections_created }) = rx.recv().await {
                return reflections_created;
            }
        }
    })
    .await
    .expect("reflection completes within 5s");
    assert!(got >= 1);

    let refl = state
        .vault
        .list(ListFilter { tiers: Some(vec![Tier::Reflection]), ..Default::default() })
        .await
        .unwrap();
    assert!(!refl.is_empty());

    handle.shutdown().await;
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p mnemos_daemon --test reflection_trigger`
Expected: FAIL — `cfg.reflection` / `Event::ReflectionCompleted` not present.

- [ ] **Step 3: Add config blocks** in `config.rs`. Add fields to `Config`:

```rust
    pub reflection: ReflectionConfig,
    pub community: CommunityConfig,
```

and the structs + defaults:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ReflectionConfig {
    /// Salience accumulator threshold that triggers a reflection pass.
    pub salience_threshold: f64,
    /// Max recent un-reflected memories considered per reflection pass.
    pub max_sources: usize,
}

impl Default for ReflectionConfig {
    fn default() -> Self {
        Self { salience_threshold: 5.0, max_sources: 20 }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct CommunityConfig {
    /// Minimum entities for a community to be summarized.
    pub min_community_size: usize,
}

impl Default for CommunityConfig {
    fn default() -> Self {
        Self { min_community_size: 3 }
    }
}
```

- [ ] **Step 4: Add events** to the `Event` enum in `events.rs` (after `PipelineFailed`):

```rust
    ReflectionCompleted {
        reflections_created: usize,
    },
    CommunityDetected {
        communities: usize,
    },
```

- [ ] **Step 5: Add the trigger to the runner** in `pipeline_runner.rs`. Add imports:

```rust
use mnemos_core::pipeline::reflect::reflect;
use mnemos_core::storage::reflection_ops::{bump_salience, reset_salience};
```

In `process_session`, after the `Ok(n)` arm publishes `PipelineCompleted`, call a new helper (pass the llm + n). Add at the end of the `Ok(n) =>` block, before the closing brace:

```rust
            maybe_reflect(state, llm.as_ref(), n).await;
```

(`llm` is the `Arc<dyn LlmProvider>` already cloned at the top of `process_session`.) Then add the helper:

```rust
/// Bump salience by the number of facts added; if it crosses the configured
/// threshold, run a reflection pass, reset the accumulator, and emit an event.
async fn maybe_reflect(state: &AppState, llm: &dyn LlmProvider, added: usize) {
    if added == 0 {
        return;
    }
    let salience = match bump_salience(state.vault.storage(), added as f64).await {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!(error = %e, "salience bump failed");
            return;
        }
    };
    if salience < state.config.reflection.salience_threshold {
        return;
    }
    match reflect(&state.vault, llm, state.config.reflection.max_sources).await {
        Ok(created) => {
            let _ = reset_salience(state.vault.storage(), chrono::Utc::now()).await;
            state.events.publish(Event::ReflectionCompleted {
                reflections_created: created.len(),
            });
        }
        Err(e) => tracing::warn!(error = %e, "reflection pass failed"),
    }
}
```

- [ ] **Step 6: Run tests to verify they pass**

Run: `cargo test -p mnemos_daemon --test reflection_trigger`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
cargo fmt --all && cargo clippy -p mnemos_daemon --all-targets -- -D warnings
git add crates/mnemos_daemon/src/config.rs crates/mnemos_daemon/src/events.rs crates/mnemos_daemon/src/pipeline_runner.rs crates/mnemos_daemon/tests/reflection_trigger.rs
git commit -m "feat: salience-triggered reflection in the runner + config/events (Plan 5 Task 10)"
```

---

## Task 11: Reflection REST endpoints + MCP tools

Manual `reflect` trigger and `list_reflections` over REST and MCP.

**Files:**
- Create: `crates/mnemos_daemon/src/routes/reflections.rs`
- Modify: `crates/mnemos_daemon/src/routes/mod.rs` (declare + mount)
- Modify: `crates/mnemos_daemon/src/mcp/tools.rs` (reflect + list_reflections)
- Test: `crates/mnemos_daemon/tests/reflections.rs` (new)

- [ ] **Step 1: Write the failing test** — create `crates/mnemos_daemon/tests/reflections.rs`:

```rust
use axum::http::StatusCode;
use mnemos_core::paths::Paths;
use mnemos_core::providers::mock_llm::MockLlm;
use mnemos_core::vault::{RememberOpts, Vault};
use mnemos_daemon::{build_app_full, config::Config};
use std::sync::Arc;
use tempfile::TempDir;

#[tokio::test]
async fn reflect_endpoint_creates_and_lists() {
    let tmp = Box::leak(Box::new(TempDir::new().unwrap()));
    let vault = Vault::open(Paths::with_root(tmp.path())).await.unwrap();
    vault
        .remember(
            "REFLECT:pattern|Shaun ships on Fridays",
            RememberOpts::default(),
        )
        .await
        .unwrap();
    let (app, state, handle) =
        build_app_full(Config::default(), vault, None, Some(Arc::new(MockLlm::new()))).await.unwrap();
    let token = state.token.clone();

    let (s, b) = call(app.clone(), "POST", "/v1/reflections", Some(&token), "{}").await;
    assert_eq!(s, StatusCode::OK, "{b}");
    let v: serde_json::Value = serde_json::from_str(&b).unwrap();
    assert!(v["created"].as_array().unwrap().len() >= 1);

    let (s2, b2) = call(app, "GET", "/v1/reflections", Some(&token), "").await;
    assert_eq!(s2, StatusCode::OK);
    let v2: serde_json::Value = serde_json::from_str(&b2).unwrap();
    assert!(v2["reflections"].as_array().unwrap().iter().any(|r| r["body"] == "Shaun ships on Fridays"));

    if let Some(h) = handle { h.shutdown().await; }
}

#[tokio::test]
async fn reflect_endpoint_409_without_llm() {
    let tmp = Box::leak(Box::new(TempDir::new().unwrap()));
    let vault = Vault::open(Paths::with_root(tmp.path())).await.unwrap();
    let (app, state) = mnemos_daemon::build_app(Config::default(), vault).await.unwrap();
    let (s, _) = call(app, "POST", "/v1/reflections", Some(&state.token), "{}").await;
    assert_eq!(s, StatusCode::CONFLICT);
}

async fn call(app: axum::Router, method: &str, uri: &str, auth: Option<&str>, body: &str) -> (StatusCode, String) {
    use axum::body::Body;
    use http_body_util::BodyExt;
    use tower::ServiceExt;
    let mut req = axum::http::Request::builder().method(method).uri(uri).header("content-type", "application/json");
    if let Some(t) = auth { req = req.header("authorization", format!("Bearer {t}")); }
    let req = req.body(Body::from(body.to_string())).unwrap();
    let resp = app.oneshot(req).await.unwrap();
    let s = resp.status();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    (s, String::from_utf8_lossy(&bytes).to_string())
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p mnemos_daemon --test reflections`
Expected: FAIL — route not mounted (404, not 200/409).

- [ ] **Step 3: Create `crates/mnemos_daemon/src/routes/reflections.rs`**

```rust
//! Reflection endpoints: trigger a reflection pass + list reflection memories.

use axum::{
    extract::{Query, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use mnemos_core::pipeline::reflect::reflect;
use mnemos_core::storage::memory_ops::ListFilter;
use mnemos_core::Tier;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::error::ApiError;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/v1/reflections", post(run_reflect).get(list_reflections))
}

async fn run_reflect(State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    let llm = state.llm.clone().ok_or_else(|| {
        ApiError::new(StatusCode::CONFLICT, "no LLM configured; reflection unavailable")
    })?;
    let created = reflect(&state.vault, llm.as_ref(), state.config.reflection.max_sources).await?;
    if !created.is_empty() {
        let _ = mnemos_core::storage::reflection_ops::reset_salience(
            state.vault.storage(),
            chrono::Utc::now(),
        )
        .await;
    }
    state
        .events
        .publish(crate::events::Event::ReflectionCompleted { reflections_created: created.len() });
    Ok(Json(json!({ "created": created })))
}

#[derive(Debug, Deserialize)]
struct ListQuery {
    #[serde(default = "default_limit")]
    limit: usize,
}

fn default_limit() -> usize {
    50
}

async fn list_reflections(
    State(state): State<AppState>,
    Query(q): Query<ListQuery>,
) -> Result<Json<Value>, ApiError> {
    let reflections = state
        .vault
        .list(ListFilter {
            tiers: Some(vec![Tier::Reflection]),
            limit: Some(q.limit),
            ..Default::default()
        })
        .await?;
    Ok(Json(json!({ "reflections": reflections })))
}
```

- [ ] **Step 4: Mount it** in `routes/mod.rs` — add `pub mod reflections;` and `.merge(reflections::router())` to the `authed` chain.

- [ ] **Step 5: Add MCP tools** in `mcp/tools.rs`. Add two descriptors to the `descriptors()` vec:

```rust
        json!({
            "name": "reflect",
            "description": "Run a reflection pass now: synthesize recent memories into typed reflections.",
            "inputSchema": { "type": "object", "properties": {} }
        }),
        json!({
            "name": "list_reflections",
            "description": "List reflection-tier memories.",
            "inputSchema": {
                "type": "object",
                "properties": { "limit": { "type": "integer", "default": 50 } }
            }
        }),
```

Add match arms in `call()`:

```rust
        "reflect" => reflect_tool(state, args).await,
        "list_reflections" => list_reflections_tool(state, args).await,
```

And the implementations:

```rust
async fn reflect_tool(state: &AppState, _args: &Value) -> anyhow::Result<Value> {
    let llm = state
        .llm
        .clone()
        .ok_or_else(|| anyhow::anyhow!("no LLM configured; reflection unavailable"))?;
    let created =
        mnemos_core::pipeline::reflect::reflect(&state.vault, llm.as_ref(), state.config.reflection.max_sources)
            .await?;
    Ok(tool_content_json(json!({ "created": created })))
}

async fn list_reflections_tool(state: &AppState, args: &Value) -> anyhow::Result<Value> {
    use mnemos_core::storage::memory_ops::ListFilter;
    let limit = args["limit"].as_u64().map(|n| n as usize);
    let reflections = state
        .vault
        .list(ListFilter {
            tiers: Some(vec![mnemos_core::Tier::Reflection]),
            limit,
            ..Default::default()
        })
        .await?;
    Ok(tool_content_json(json!({ "reflections": reflections })))
}
```

- [ ] **Step 6: Run tests to verify they pass**

Run: `cargo test -p mnemos_daemon --test reflections && cargo test -p mnemos_daemon --test mcp`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
cargo fmt --all && cargo clippy -p mnemos_daemon --all-targets -- -D warnings
git add crates/mnemos_daemon/src/routes/reflections.rs crates/mnemos_daemon/src/routes/mod.rs crates/mnemos_daemon/src/mcp/tools.rs crates/mnemos_daemon/tests/reflections.rs
git commit -m "feat: reflection REST endpoints + MCP tools (Plan 5 Task 11)"
```

---

## Task 12: Louvain community detection

Single-level Louvain modularity optimization over the entity graph. Pure, deterministic (fixed node order, lowest-id tie-break).

**Files:**
- Replace: `crates/mnemos_core/src/graph/community.rs` (stub from Task 1)

- [ ] **Step 1: Write the failing test** — create `crates/mnemos_core/src/graph/community.rs` with ONLY the test module:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::MemoryGraph;

    #[test]
    fn separates_two_triangles() {
        // Two dense triangles joined by a single weak bridge → two communities.
        let mut g = MemoryGraph::new();
        for (a, b) in [("A", "B"), ("B", "C"), ("A", "C")] {
            g.add_edge(a, b, 1.0);
        }
        for (a, b) in [("D", "E"), ("E", "F"), ("D", "F")] {
            g.add_edge(a, b, 1.0);
        }
        g.add_edge("C", "D", 0.3); // weak bridge

        let comm = louvain(&g);
        let c = |name: &str| comm[g.index_of(name).unwrap()];
        assert_eq!(c("A"), c("B"));
        assert_eq!(c("B"), c("C"));
        assert_eq!(c("D"), c("E"));
        assert_eq!(c("E"), c("F"));
        assert_ne!(c("A"), c("D"), "the two triangles are distinct communities");
        let distinct: std::collections::BTreeSet<usize> = comm.iter().copied().collect();
        assert_eq!(distinct.len(), 2);
    }

    #[test]
    fn isolated_nodes_are_singletons() {
        let mut g = MemoryGraph::new();
        g.add_mention("m", "X"); // node with no edges
        g.add_mention("m", "Y");
        let comm = louvain(&g);
        assert_eq!(comm.len(), 2);
        assert_ne!(comm[0], comm[1]);
    }

    #[test]
    fn empty_graph_empty_result() {
        assert!(louvain(&MemoryGraph::new()).is_empty());
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p mnemos_core --lib graph::community`
Expected: FAIL — `louvain` not found.

- [ ] **Step 3: Prepend the implementation above the test module**

```rust
//! Single-level Louvain modularity community detection. Hand-rolled,
//! deterministic. Returns a contiguous community id (`0..k`) per node index.

use crate::graph::MemoryGraph;
use std::collections::HashMap;

/// Detect communities by greedy local modularity optimization. Each node starts
/// in its own community; nodes repeatedly move to the neighboring community that
/// most increases modularity until no move helps. Deterministic: nodes are
/// considered in index order and ties favor the lower community id.
pub fn louvain(graph: &MemoryGraph) -> Vec<usize> {
    let n = graph.node_count();
    if n == 0 {
        return vec![];
    }
    let m = graph.total_weight();
    if m <= 0.0 {
        // No edges: every node is its own community.
        return renumber(&(0..n).collect::<Vec<_>>());
    }
    let two_m = 2.0 * m;
    let mut comm: Vec<usize> = (0..n).collect();
    let mut sigma_tot: Vec<f64> = (0..n).map(|i| graph.degree(i)).collect();

    let mut improved = true;
    let mut guard = 0;
    while improved && guard < 100 {
        improved = false;
        guard += 1;
        for i in 0..n {
            let ki = graph.degree(i);
            let ci = comm[i];

            // Weight from i to each neighboring community.
            let mut k_i_to: HashMap<usize, f64> = HashMap::new();
            for &(j, w) in graph.neighbors(i) {
                if j == i {
                    continue;
                }
                *k_i_to.entry(comm[j]).or_insert(0.0) += w;
            }

            // Tentatively remove i from its community.
            sigma_tot[ci] -= ki;

            // Baseline: returning to ci.
            let mut best_comm = ci;
            let mut best_gain = k_i_to.get(&ci).copied().unwrap_or(0.0) - sigma_tot[ci] * ki / two_m;

            // Evaluate neighbor communities in deterministic (sorted) order.
            let mut cands: Vec<usize> = k_i_to.keys().copied().collect();
            cands.sort_unstable();
            for c in cands {
                if c == ci {
                    continue;
                }
                let gain = k_i_to[&c] - sigma_tot[c] * ki / two_m;
                if gain > best_gain + 1e-12 {
                    best_gain = gain;
                    best_comm = c;
                }
            }

            sigma_tot[best_comm] += ki;
            if best_comm != ci {
                comm[i] = best_comm;
                improved = true;
            }
        }
    }
    renumber(&comm)
}

/// Map arbitrary community labels to contiguous ids `0..k` (first-seen order).
fn renumber(comm: &[usize]) -> Vec<usize> {
    let mut map: HashMap<usize, usize> = HashMap::new();
    let mut next = 0;
    comm.iter()
        .map(|&c| {
            *map.entry(c).or_insert_with(|| {
                let v = next;
                next += 1;
                v
            })
        })
        .collect()
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p mnemos_core --lib graph::community`
Expected: PASS (3 tests).

- [ ] **Step 5: Commit**

```bash
cargo fmt --all && cargo clippy -p mnemos_core --all-targets -- -D warnings
git add crates/mnemos_core/src/graph/community.rs
git commit -m "feat: Louvain community detection over the entity graph (Plan 5 Task 12)"
```

---

## Task 13: Schema v6 + community membership storage (`community_ops`)

Persist entity→community membership so the UI and global queries can navigate clusters.

**Files:**
- Modify: `crates/mnemos_core/src/storage/migrations.rs` (v6)
- Create: `crates/mnemos_core/src/storage/community_ops.rs`
- Modify: `crates/mnemos_core/src/storage/mod.rs` (add `pub mod community_ops;`)
- Test: `crates/mnemos_core/tests/community_ops.rs` (new)

- [ ] **Step 1: Write the failing test** — create `crates/mnemos_core/tests/community_ops.rs`:

```rust
use mnemos_core::storage::community_ops::{community_members, list_community_ids, store_communities};
use mnemos_core::storage::Storage;
use chrono::Utc;
use tempfile::TempDir;

#[tokio::test]
async fn store_and_read_membership() {
    let tmp = TempDir::new().unwrap();
    let storage = Storage::open(&tmp.path().join("c.db")).await.unwrap();
    assert!(storage.schema_version().await.unwrap() >= 6);

    let assignments = vec![
        ("ent_a".to_string(), 0usize),
        ("ent_b".to_string(), 0usize),
        ("ent_c".to_string(), 1usize),
    ];
    store_communities(&storage, &assignments, Utc::now()).await.unwrap();

    assert_eq!(list_community_ids(&storage).await.unwrap(), vec![0, 1]);
    let mut m0 = community_members(&storage, 0).await.unwrap();
    m0.sort();
    assert_eq!(m0, vec!["ent_a".to_string(), "ent_b".to_string()]);

    // A re-run fully replaces membership.
    store_communities(&storage, &[("ent_a".to_string(), 5usize)], Utc::now()).await.unwrap();
    assert_eq!(list_community_ids(&storage).await.unwrap(), vec![5]);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p mnemos_core --test community_ops`
Expected: FAIL — module / schema v6 not present.

- [ ] **Step 3: Add migration v6** in `migrations.rs`. After the `current < 5` block:

```rust
        if current < 6 {
            migration_v6(&conn).await?;
            conn.execute(
                "INSERT OR IGNORE INTO schema_migrations (version) VALUES (6)",
                (),
            )
            .await?;
        }
```

```rust
async fn migration_v6(conn: &libsql::Connection) -> Result<()> {
    for stmt in V6_STATEMENTS {
        conn.execute(stmt, ()).await?;
    }
    Ok(())
}

const V6_STATEMENTS: &[&str] = &[
    "CREATE TABLE IF NOT EXISTS entity_communities (
        entity_id    TEXT PRIMARY KEY,
        community_id INTEGER NOT NULL,
        detected_at  TEXT NOT NULL
    )",
    "CREATE INDEX IF NOT EXISTS idx_entity_communities_cid ON entity_communities(community_id)",
];
```

- [ ] **Step 4: Bump stale schema-version assertions** from `5` to `6` in `tests/schema_v1.rs`, `tests/schema_v2.rs`, `tests/storage_open.rs` (grep `schema_version`).

- [ ] **Step 5: Create `crates/mnemos_core/src/storage/community_ops.rs`**

```rust
//! Persistence for entity→community membership.

use crate::error::Result;
use crate::storage::Storage;
use chrono::{DateTime, Utc};
use libsql::params;

/// Fully replace community membership with the given `(entity_id, community_id)`
/// assignments (stale entities are removed).
pub async fn store_communities(
    storage: &Storage,
    assignments: &[(String, usize)],
    now: DateTime<Utc>,
) -> Result<()> {
    let (conn, _g) = storage.write_conn().await?;
    let tx = conn.transaction().await?;
    tx.execute("DELETE FROM entity_communities", ()).await?;
    let ts = now.to_rfc3339();
    for (entity_id, community_id) in assignments {
        tx.execute(
            "INSERT OR REPLACE INTO entity_communities (entity_id, community_id, detected_at)
                 VALUES (?, ?, ?)",
            params![entity_id.clone(), *community_id as i64, ts.clone()],
        )
        .await?;
    }
    tx.commit().await?;
    Ok(())
}

/// Entity ids belonging to a community.
pub async fn community_members(storage: &Storage, community_id: usize) -> Result<Vec<String>> {
    let conn = storage.conn()?;
    let mut rows = conn
        .query(
            "SELECT entity_id FROM entity_communities WHERE community_id = ?",
            params![community_id as i64],
        )
        .await?;
    let mut out = Vec::new();
    while let Some(r) = rows.next().await? {
        out.push(r.get::<String>(0)?);
    }
    Ok(out)
}

/// Distinct community ids, ascending.
pub async fn list_community_ids(storage: &Storage) -> Result<Vec<usize>> {
    let conn = storage.conn()?;
    let mut rows = conn
        .query(
            "SELECT DISTINCT community_id FROM entity_communities ORDER BY community_id",
            (),
        )
        .await?;
    let mut out = Vec::new();
    while let Some(r) = rows.next().await? {
        out.push(r.get::<i64>(0)? as usize);
    }
    Ok(out)
}
```

- [ ] **Step 6: Declare the module** in `storage/mod.rs`: add `pub mod community_ops;`.

- [ ] **Step 7: Run tests to verify they pass**

Run: `cargo test -p mnemos_core --test community_ops && cargo test -p mnemos_core`
Expected: PASS.

- [ ] **Step 8: Commit**

```bash
cargo fmt --all && cargo clippy -p mnemos_core --all-targets -- -D warnings
git add crates/mnemos_core/src/storage/migrations.rs crates/mnemos_core/src/storage/community_ops.rs crates/mnemos_core/src/storage/mod.rs crates/mnemos_core/tests/community_ops.rs crates/mnemos_core/tests/schema_v1.rs crates/mnemos_core/tests/schema_v2.rs crates/mnemos_core/tests/storage_open.rs
git commit -m "feat: schema v6 + entity community membership storage (Plan 5 Task 13)"
```

---

## Task 14: `detect_and_summarize` pipeline + `MockLlm` TASK=community + `entity_names`

Run Louvain, persist membership, and write one `community_summary` memory per community (>= min size) summarized by the LLM.

**Files:**
- Modify: `crates/mnemos_core/src/storage/entity_ops.rs` (add `entity_names`)
- Modify: `crates/mnemos_core/src/providers/mock_llm.rs` (add `TASK=community` branch)
- Create: `crates/mnemos_core/src/pipeline/community.rs`
- Modify: `crates/mnemos_core/src/pipeline/mod.rs` (add `pub mod community;`)
- Test: `crates/mnemos_core/tests/pipeline_community.rs` (new)

- [ ] **Step 1: Write the failing test** — create `crates/mnemos_core/tests/pipeline_community.rs`:

```rust
use mnemos_core::paths::Paths;
use mnemos_core::pipeline::community::detect_and_summarize;
use mnemos_core::providers::mock_llm::MockLlm;
use mnemos_core::storage::community_ops::list_community_ids;
use mnemos_core::storage::entity_ops::{upsert_edge, upsert_entity};
use mnemos_core::storage::memory_ops::ListFilter;
use mnemos_core::types::MemoryType;
use mnemos_core::vault::Vault;
use chrono::Utc;
use tempfile::TempDir;

#[tokio::test]
async fn detects_and_summarizes_communities() {
    let tmp = TempDir::new().unwrap();
    let v = Vault::open(Paths::with_root(tmp.path())).await.unwrap();

    // Triangle {A,B,C} + edge {D,E}, weak bridge C-D.
    let e = |n: &str| async move { upsert_entity(v.storage(), n, "concept").await.unwrap() };
    let a = e("Alpha").await; let b = e("Beta").await; let c = e("Gamma").await;
    let d = e("Delta").await; let f = e("Epsilon").await;
    let m = "mem_x";
    for (x, y) in [(&a, &b), (&b, &c), (&a, &c)] {
        upsert_edge(v.storage(), x, y, "rel", m, Utc::now()).await.unwrap();
    }
    upsert_edge(v.storage(), &d, &f, "rel", m, Utc::now()).await.unwrap();
    upsert_edge(v.storage(), &c, &d, "rel", m, Utc::now()).await.unwrap();

    let created = detect_and_summarize(&v, &MockLlm::new(), 2).await.unwrap();
    assert!(!created.is_empty(), "at least one community summarized");

    // membership persisted
    assert!(!list_community_ids(v.storage()).await.unwrap().is_empty());

    // summaries are community_summary memories
    let summaries = v
        .list(ListFilter { ..Default::default() })
        .await
        .unwrap()
        .into_iter()
        .filter(|m| m.kind == MemoryType::CommunitySummary)
        .count();
    assert_eq!(summaries, created.len());
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p mnemos_core --test pipeline_community`
Expected: FAIL — `detect_and_summarize` not found.

- [ ] **Step 3: Add `entity_names` to `entity_ops.rs`**

```rust
/// Resolve entity names for the given ids (skips ids that no longer exist).
pub async fn entity_names(storage: &Storage, ids: &[String]) -> Result<Vec<String>> {
    let conn = storage.conn()?;
    let mut out = Vec::new();
    for id in ids {
        let mut rows = conn
            .query("SELECT name FROM entities WHERE id = ?", params![id.clone()])
            .await?;
        if let Some(r) = rows.next().await? {
            out.push(r.get::<String>(0)?);
        }
    }
    Ok(out)
}
```

- [ ] **Step 4: Add the `TASK=community` branch to `MockLlm`** in `mock_llm.rs` (before the final `else`):

```rust
        } else if req.system.contains("TASK=community") {
            // Deterministic summary: echo the entity list back.
            json!({ "title": "Community summary", "summary": content.trim() }).to_string()
```

Extend the doc comment's marker list to mention `TASK=community`.

- [ ] **Step 5: Create `crates/mnemos_core/src/pipeline/community.rs`**

```rust
//! Community detection stage: Louvain over the entity graph, persist membership,
//! and write one `community_summary` memory per community (>= min size).

use crate::error::{MnemosError, Result};
use crate::graph::community::louvain;
use crate::graph::MemoryGraph;
use crate::pipeline::extract_json;
use crate::providers::{CompletionRequest, LlmProvider};
use crate::storage::community_ops::store_communities;
use crate::storage::entity_ops::entity_names;
use crate::types::MemoryType;
use crate::vault::Vault;
use chrono::Utc;
use serde::Deserialize;
use std::collections::BTreeMap;

pub const COMMUNITY_SYSTEM: &str = "TASK=community\n\
You are given the named entities of one knowledge-graph community. Write a \
concise summary of the theme that connects them. Respond ONLY with JSON \
{\"title\":\"...\",\"summary\":\"...\"}.";

#[derive(Deserialize)]
struct CommunityOut {
    #[serde(default)]
    title: Option<String>,
    summary: String,
}

/// Detect communities, persist membership, and summarize each community of at
/// least `min_size` entities into a `community_summary` reflection memory.
/// Returns the new summary memory ids.
pub async fn detect_and_summarize(
    vault: &Vault,
    llm: &dyn LlmProvider,
    min_size: usize,
) -> Result<Vec<String>> {
    let graph = MemoryGraph::load(vault.storage()).await?;
    if graph.is_empty() {
        return Ok(vec![]);
    }
    let comm = louvain(&graph);

    // Persist membership for every entity.
    let assignments: Vec<(String, usize)> = comm
        .iter()
        .enumerate()
        .map(|(i, &c)| (graph.entity_id(i).to_string(), c))
        .collect();
    store_communities(vault.storage(), &assignments, Utc::now()).await?;

    // Group node indices by community.
    let mut groups: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for (i, &c) in comm.iter().enumerate() {
        groups.entry(c).or_default().push(i);
    }

    let mut created = Vec::new();
    for (cid, members) in groups {
        if members.len() < min_size {
            continue;
        }
        let ids: Vec<String> = members.iter().map(|&i| graph.entity_id(i).to_string()).collect();
        let names = entity_names(vault.storage(), &ids).await?;
        if names.is_empty() {
            continue;
        }
        let prompt = format!("Community {cid} entities: {}", names.join(", "));
        let raw = llm.complete(&CompletionRequest::new(COMMUNITY_SYSTEM, prompt)).await?;
        let parsed: CommunityOut = serde_json::from_str(extract_json(&raw))
            .map_err(|e| MnemosError::Internal(format!("community parse failed: {e}; raw={raw}")))?;
        let summary = parsed.summary.trim().to_string();
        if summary.is_empty() {
            continue;
        }
        let title = parsed.title.unwrap_or_else(|| format!("Community {cid}"));
        let id = vault
            .remember_reflection(
                &summary,
                Some(title),
                MemoryType::CommunitySummary,
                vec!["community".into()],
                &[],
                vec![],
            )
            .await?;
        created.push(id);
    }
    Ok(created)
}
```

- [ ] **Step 6: Declare the module** in `pipeline/mod.rs`: add `pub mod community;`.

- [ ] **Step 7: Run tests to verify they pass**

Run: `cargo test -p mnemos_core --test pipeline_community && cargo test -p mnemos_core --lib providers::mock_llm`
Expected: PASS.

- [ ] **Step 8: Commit**

```bash
cargo fmt --all && cargo clippy -p mnemos_core --all-targets -- -D warnings
git add crates/mnemos_core/src/storage/entity_ops.rs crates/mnemos_core/src/providers/mock_llm.rs crates/mnemos_core/src/pipeline/community.rs crates/mnemos_core/src/pipeline/mod.rs crates/mnemos_core/tests/pipeline_community.rs
git commit -m "feat: community detection + summarization pipeline (Plan 5 Task 14)"
```

---

## Task 15: Global-mode recall (GraphRAG)

A `global_recall` that retrieves over `community_summary` memories — answers thematic/global queries. Exposed via a `global` flag on search + MCP recall.

**Files:**
- Modify: `crates/mnemos_core/src/retrieval/graph_recall.rs` (add `global_recall`)
- Modify: `crates/mnemos_daemon/src/routes/recall_helper.rs` (add `global` helper)
- Modify: `crates/mnemos_daemon/src/routes/memories.rs` (search `global` flag)
- Modify: `crates/mnemos_daemon/src/mcp/tools.rs` (recall `global` arg)
- Test: add to `crates/mnemos_daemon/tests/memories.rs`

- [ ] **Step 1: Write the failing test** — append to `crates/mnemos_daemon/tests/memories.rs`:

```rust
#[tokio::test]
async fn global_search_returns_only_community_summaries() {
    let (app, token) = fixture().await;
    // A community summary and a normal semantic memory, both mentioning "rust".
    let (_, _) = call(app.clone(), "POST", "/v1/memories", Some(&token),
        r#"{"body":"themes around rust tooling and editors","tier":"reflection","kind":"community_summary"}"#).await;
    let (_, _) = call(app.clone(), "POST", "/v1/memories", Some(&token),
        r#"{"body":"rust borrow checker note","tier":"semantic","kind":"fact"}"#).await;

    let (s, b) = call(app, "POST", "/v1/memories/search", Some(&token),
        r#"{"query":"rust","k":10,"global":true}"#).await;
    assert_eq!(s, axum::http::StatusCode::OK, "{b}");
    let v: serde_json::Value = serde_json::from_str(&b).unwrap();
    let hits = v["hits"].as_array().unwrap();
    assert!(hits.iter().any(|h| h["memory"]["body"] == "themes around rust tooling and editors"));
    assert!(hits.iter().all(|h| h["memory"]["kind"] == "community_summary"),
        "global mode returns only community summaries");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p mnemos_daemon --test memories global_search`
Expected: FAIL — `global` not honored (returns non-summary hits).

- [ ] **Step 3: Add `global_recall`** to `crates/mnemos_core/src/retrieval/graph_recall.rs`:

```rust
use crate::providers::Embedder;
use crate::retrieval::dense::dense_recall;
use crate::retrieval::RecallHit;
use crate::types::MemoryType;

/// Global-mode (GraphRAG) recall: retrieve over `community_summary` memories
/// only. Uses dense KNN when an embedder is available, else BM25.
pub async fn global_recall(
    storage: &Storage,
    embedder: Option<&dyn Embedder>,
    query: &str,
    k: usize,
) -> Result<Vec<RecallHit>> {
    let opts = RecallOpts {
        k: k.max(1) * 5,
        graph: false,
        ..Default::default()
    };
    let base = if let Some(e) = embedder {
        dense_recall(storage, e, query, opts).await?
    } else {
        crate::retrieval::bm25::bm25_recall(storage, query, opts).await?
    };
    let mut hits: Vec<RecallHit> = base
        .into_iter()
        .filter(|h| h.memory.kind == MemoryType::CommunitySummary)
        .collect();
    hits.truncate(k);
    Ok(hits)
}
```

- [ ] **Step 4: Add a `global` helper** to `recall_helper.rs`:

```rust
/// Global-mode recall over community summaries.
pub async fn global(state: &AppState, query: &str, k: usize) -> Result<Vec<RecallHit>> {
    let embedder = state.vault.embedder().cloned();
    let embedder_ref = embedder.as_ref().map(|a| a.as_ref());
    mnemos_core::retrieval::graph_recall::global_recall(state.vault.storage(), embedder_ref, query, k).await
}
```

- [ ] **Step 5: Add the `global` flag to search + MCP.** In `routes/memories.rs` `SearchReq`, add:

```rust
    #[serde(default)]
    global: bool,
```

and at the top of the `search` handler, branch before building the standard recall:

```rust
    if req.global {
        let hits = crate::routes::recall_helper::global(&state, &req.query, req.k).await?;
        return Ok(Json(serde_json::json!({ "hits": hits })));
    }
```

In `mcp/tools.rs` `recall` tool: add `"global": { "type": "boolean", "default": false }` to the descriptor properties, and in the `recall` fn, near the top:

```rust
    if args["global"].as_bool().unwrap_or(false) {
        let k = args["k"].as_u64().unwrap_or(10) as usize;
        let hits = crate::routes::recall_helper::global(state, query, k).await?;
        return Ok(tool_content_json(json!({ "hits": hits })));
    }
```

(Place this after `query` is bound.)

- [ ] **Step 6: Run tests to verify they pass**

Run: `cargo test -p mnemos_daemon --test memories && cargo test -p mnemos_daemon --test mcp`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings
git add crates/mnemos_core/src/retrieval/graph_recall.rs crates/mnemos_daemon/src/routes/recall_helper.rs crates/mnemos_daemon/src/routes/memories.rs crates/mnemos_daemon/src/mcp/tools.rs crates/mnemos_daemon/tests/memories.rs
git commit -m "feat: global-mode recall over community summaries (Plan 5 Task 15)"
```

---

## Task 16: Community detection endpoint

On-demand `POST /v1/maintenance/communities` to run detection + summarization (daemon owns the LLM).

**Files:**
- Modify: `crates/mnemos_daemon/src/routes/pipelines.rs`
- Test: add to `crates/mnemos_daemon/tests/pipelines.rs`

- [ ] **Step 1: Write the failing test** — append to `crates/mnemos_daemon/tests/pipelines.rs`:

```rust
#[tokio::test]
async fn communities_endpoint_runs_detection() {
    use mnemos_core::providers::mock_llm::MockLlm;
    use mnemos_core::storage::entity_ops::{upsert_edge, upsert_entity};
    use std::sync::Arc;

    let tmp = Box::leak(Box::new(TempDir::new().unwrap()));
    let vault = Vault::open(Paths::with_root(tmp.path())).await.unwrap();
    // Build a small graph so there is something to cluster.
    let a = upsert_entity(vault.storage(), "A", "c").await.unwrap();
    let b = upsert_entity(vault.storage(), "B", "c").await.unwrap();
    let c = upsert_entity(vault.storage(), "C", "c").await.unwrap();
    for (x, y) in [(&a, &b), (&b, &c), (&a, &c)] {
        upsert_edge(vault.storage(), x, y, "rel", "m", chrono::Utc::now()).await.unwrap();
    }
    let (app, state, handle) =
        build_app_full(Config::default(), vault, None, Some(Arc::new(MockLlm::new()))).await.unwrap();

    let (s, b) = call(app, "POST", "/v1/maintenance/communities", Some(&state.token), "{}").await;
    assert_eq!(s, StatusCode::OK, "{b}");
    let v: serde_json::Value = serde_json::from_str(&b).unwrap();
    assert!(v["summaries"].as_array().unwrap().len() >= 1);

    if let Some(h) = handle { h.shutdown().await; }
}

#[tokio::test]
async fn communities_endpoint_409_without_llm() {
    let tmp = Box::leak(Box::new(TempDir::new().unwrap()));
    let vault = Vault::open(Paths::with_root(tmp.path())).await.unwrap();
    let (app, state) = build_app(Config::default(), vault).await.unwrap();
    let (s, _) = call(app, "POST", "/v1/maintenance/communities", Some(&state.token), "{}").await;
    assert_eq!(s, StatusCode::CONFLICT);
}
```

> Ensure `tests/pipelines.rs` imports `build_app_full` and `build_app` (add to the existing `use mnemos_daemon::{...}` line if missing).

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p mnemos_daemon --test pipelines communities_endpoint`
Expected: FAIL — route not mounted.

- [ ] **Step 3: Add the route + handler** to `routes/pipelines.rs`. Add to the router:

```rust
        .route("/v1/maintenance/communities", post(run_communities))
```

Add the handler + import:

```rust
use mnemos_core::pipeline::community::detect_and_summarize;
```

```rust
async fn run_communities(State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    let llm = state.llm.clone().ok_or_else(|| {
        ApiError::new(
            axum::http::StatusCode::CONFLICT,
            "no LLM configured; community detection unavailable",
        )
    })?;
    let summaries =
        detect_and_summarize(&state.vault, llm.as_ref(), state.config.community.min_community_size)
            .await?;
    state
        .events
        .publish(crate::events::Event::CommunityDetected { communities: summaries.len() });
    Ok(Json(json!({ "summaries": summaries })))
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p mnemos_daemon --test pipelines`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
cargo fmt --all && cargo clippy -p mnemos_daemon --all-targets -- -D warnings
git add crates/mnemos_daemon/src/routes/pipelines.rs crates/mnemos_daemon/tests/pipelines.rs
git commit -m "feat: POST /v1/maintenance/communities endpoint (Plan 5 Task 16)"
```

---

## Task 17: Thread `[retrieval]` PPR config into recall

Make the PPR parameters configurable (the spec's `ppr_alpha`/`ppr_iterations`).

**Files:**
- Modify: `crates/mnemos_daemon/src/config.rs` (`RetrievalConfig` gains `ppr_alpha`, `ppr_iterations`)
- Modify: `crates/mnemos_daemon/src/routes/recall_helper.rs` (apply config to opts)
- Test: add to `crates/mnemos_daemon/tests/config.rs`

- [ ] **Step 1: Write the failing test** — append to `crates/mnemos_daemon/tests/config.rs`:

```rust
#[test]
fn retrieval_ppr_defaults() {
    use mnemos_daemon::config::Config;
    let cfg = Config::default();
    assert_eq!(cfg.retrieval.ppr_alpha, 0.85);
    assert_eq!(cfg.retrieval.ppr_iterations, 30);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p mnemos_daemon --test config retrieval_ppr_defaults`
Expected: FAIL — fields not present.

- [ ] **Step 3: Add the fields** to `RetrievalConfig` in `config.rs`:

```rust
    pub ppr_alpha: f64,
    pub ppr_iterations: usize,
```

and in its `Default` impl:

```rust
            ppr_alpha: 0.85,
            ppr_iterations: 30,
```

- [ ] **Step 4: Apply config in `recall_helper.rs`** — in `recall`, after building `opts` is received, override the PPR params from config before calling `hybrid_recall_full`:

```rust
pub async fn recall(state: &AppState, query: &str, mut opts: RecallOpts) -> Result<Vec<RecallHit>> {
    opts.ppr_alpha = state.config.retrieval.ppr_alpha;
    opts.ppr_iterations = state.config.retrieval.ppr_iterations;

    let embedder = state.vault.embedder().cloned();
    // ... rest unchanged ...
```

(Change the `opts: RecallOpts` parameter to `mut opts: RecallOpts` and add the two override lines at the top.)

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p mnemos_daemon --test config && cargo test -p mnemos_daemon --test memories`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
cargo fmt --all && cargo clippy -p mnemos_daemon --all-targets -- -D warnings
git add crates/mnemos_daemon/src/config.rs crates/mnemos_daemon/src/routes/recall_helper.rs crates/mnemos_daemon/tests/config.rs
git commit -m "feat: configurable PPR alpha/iterations (Plan 5 Task 17)"
```

---

## Task 18: Release v0.4.0 — version bump, README, CHANGELOG, tag

**Files:**
- Modify: `Cargo.toml` (workspace `version = "0.4.0"`)
- Modify: `README.md`, `CHANGELOG.md`

- [ ] **Step 1: Bump the workspace version** in `Cargo.toml`:

```toml
version = "0.4.0"
```

- [ ] **Step 2: Update `README.md`** — add a section after the v0.3.0 pipeline section:

```markdown
## Graph intelligence (v0.4.0)

Recall is now graph-aware, and the system distills what it learns.

- **Graph PPR retrieval (HippoRAG)** — a third retriever fused into hybrid recall
  via RRF. It seeds Personalized PageRank from the query's BM25 neighborhood and
  walks the entity graph, surfacing memories connected by relationships even when
  they share no words. On by default; disable per-query with `"graph": false`.
- **Reflection** — when the salience accumulator crosses a threshold (after
  session pipelines add enough new knowledge), the daemon synthesizes recent
  memories into typed reflection-tier memories (preference / pattern / insight /
  decision), linked back to their sources.
- **Community detection (GraphRAG)** — Louvain clustering of the entity graph;
  each cluster gets an LLM-written `community_summary`. Global/thematic queries
  retrieve over these summaries.

### New endpoints

| Method | Path | Purpose |
|--------|------|---------|
| `POST` | `/v1/reflections` | Run a reflection pass now |
| `GET`  | `/v1/reflections` | List reflection-tier memories |
| `POST` | `/v1/maintenance/communities` | Run community detection + summarization |
| `POST` | `/v1/memories/search` (`"graph": bool`, `"global": bool`) | Graph-fused recall; global community-summary recall |

New MCP tools: `reflect`, `list_reflections`; `recall` gains `graph` and `global` args.

### Config

```toml
[retrieval]
ppr_alpha = 0.85
ppr_iterations = 30

[reflection]
salience_threshold = 5.0
max_sources = 20

[community]
min_community_size = 3
```

> Note: PPR and community detection (Louvain) are hand-rolled and dependency-free.
> Hierarchical Leiden refinement is a future enhancement.
```

- [ ] **Step 3: Update `CHANGELOG.md`** — add at the top:

```markdown
## [0.4.0] - 2026-05-27

### Added
- Graph PPR retriever (HippoRAG-style): dependency-free `MemoryGraph` +
  Personalized PageRank, fused into hybrid recall as a third RRF list. `RecallHit`
  gains `ppr_rank`; `RecallOpts` gains `graph`/`ppr_alpha`/`ppr_iterations`.
- Reflection: salience-triggered `reflect()` pipeline writing typed reflection
  memories with `reflects_on` links; `POST/GET /v1/reflections`; MCP `reflect` +
  `list_reflections`; `[reflection]` config.
- Community detection: dependency-free Louvain + LLM `community_summary` memories;
  `POST /v1/maintenance/communities`; global-mode recall (`"global": true`);
  `[community]` config.
- Schema v5 (salience accumulator + `memories.reflected_at`) and v6
  (`entity_communities`).
- Events `ReflectionCompleted`, `CommunityDetected`.

### Changed
- `hybrid_recall` / `hybrid_recall_with_rerank` are now thin wrappers over a
  unified `hybrid_recall_full` that includes the optional graph retriever.

### Notes
- PPR and Louvain are hand-rolled (no `petgraph` / `leiden_clustering`) for
  determinism and zero dependency risk. Hierarchical Leiden refinement deferred.
```

- [ ] **Step 4: Release gate**

Run: `cargo fmt --all -- --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`
Expected: all green (Ollama live tests remain `#[ignore]`).

- [ ] **Step 5: Commit and tag** (do NOT push — the user reviews and pushes)

```bash
git add Cargo.toml README.md CHANGELOG.md
git commit -m "chore: release v0.4.0 — graph PPR retrieval, reflection, communities (Plan 5 Task 18)"
git tag -a v0.4.0 -m "v0.4.0 — graph PPR retrieval, reflection, community detection"
```

---

## Done

After all tasks: recall fuses BM25 + dense + graph PPR (multi-hop, explainable via `ppr_rank`); the daemon reflects on accumulated knowledge into typed insights; and the entity graph is clustered into summarized communities for global queries — all dependency-free and deterministic in CI.

**Next:** Plan 6 (Tauri + React desktop UI: graph view with PPR overlay + community hulls, reflection viewer, bi-temporal timeline, inspector).
