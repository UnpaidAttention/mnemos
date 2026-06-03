# Correction-Learning — Design

- **Date:** 2026-06-03
- **Status:** Approved (brainstorming) — pending spec review → implementation plan
- **Components:** `crates/mnemos_core` (types, pipeline, storage), `crates/mnemos_daemon` (MCP tool, REST, session-end hook), `adapters/*` (instruction-file hint)
- **Goal:** Give connected AI tools a first-class way to capture corrections (wrong → right → why), have recurring ones hardened into durable rules, and surface them so the same mistake isn't repeated — turning Mnemos from a passive memory store into an active learning loop.

## Problem

Today Mnemos can *store* a correction only if the model happens to call `remember(tier=procedural)`, as freeform text. There is no correction concept, no reasoning requirement, no dedup, no hardening of recurring lessons, and no guaranteed surfacing. So "learn from mistakes" is incidental, not designed. This feature makes it deliberate, building on the existing tiers, `reflect` pipeline, decay/reinforcement, and recall rather than adding a parallel subsystem.

## Decisions (locked in brainstorming)

1. **Dual capture**: an explicit MCP `correct` tool **and** a session-end mining pass, joined by a dedup gate.
2. **Hybrid surfacing**: a bounded set of reflection-hardened rules is always injected into the session-start working set; individual corrections surface via relevance recall.
3. **"Why" is required** — a correction without reasoning is rejected at capture (mirrors the project's feedback-rules principle).
4. **Violation detection is v2** — capture + storage + surfacing first; the data model is designed so detection can be added later without rework.

## Data model

A correction is a **`Tier::Procedural`** memory with a new **`MemoryType::Correction`**. Its `body` is structured into four labeled sections:

- **Wrong** — what the AI did incorrectly.
- **Right** — the correct approach to use going forward.
- **Why** — the reasoning (REQUIRED; non-empty, ≥ a small threshold).
- **Trigger** — the situation/context this applies to (drives recall matching; also seeds `tags`/`entities`).

Reuses existing `Memory` fields — no schema change required beyond adding the `Correction` enum variant (`MemoryType`) and a `Reflection`-kind value:
- `supersedes` → set to a prior memory id when the mistake came from a stored belief, so the wrong belief is invalidated rather than left competing in recall.
- `importance` — starts elevated (corrections outrank ordinary facts).
- `source_tool` — which AI tool logged it.
- `tags`/`entities` — derived from Trigger for recall + clustering.

A **hardened rule** is a `Tier::Reflection` memory, `MemoryType` correction-kind, `reflects_on` its source corrections, marked **hardened** (high importance, decay-exempt). Marking mechanism: a reserved tag (e.g. `mnemos:hardened`) the working-set builder filters on — no schema change.

## Capture (two paths → one dedup gate)

**Path A — MCP `correct` tool.** Args: `{ wrong: string, right: string, why: string, trigger?: string, supersedes?: string }`. Validates `why` is non-empty (else returns a tool error telling the model to include the reason). Builds the structured body, writes a Procedural `Correction`. The instruction-file hint (Claude Code `CLAUDE.md`, Codex `AGENTS.md`) is extended: *"When the user corrects you and explains why, call `correct(wrong, right, why, trigger)` so the fix persists."*

**Path B — session-end mining.** A reflection-pipeline pass runs over the session's conversation chunks with a correction-mining system prompt that extracts `{wrong, right, why, trigger}` tuples the model didn't log explicitly. Skips tuples without a discernible reason (no fabricated "why"). Hooks into the existing session-end pipeline alongside the current reflect trigger.

**Dedup gate (both paths).** Before writing, dense-recall existing `Correction` memories filtered by overlapping trigger tags/entities; if cosine similarity to an existing correction exceeds a threshold, **reinforce** the existing one (bump `strength`/`importance`, append a `Provenance` entry, update `last_accessed`) instead of creating a duplicate. This is the same reinforcement path recall already uses.

## Hardening (reflection)

Extend `pipeline/reflect.rs` with a **`correction` reflection kind**. During the existing salience-triggered reflection, when **≥ N corrections (configurable, default 3) share a trigger cluster** (by tag/entity overlap or embedding proximity), synthesize a single **hardened rule**: a Reflection-tier memory tagged `mnemos:hardened`, `reflects_on` the source corrections, with elevated importance and decay exemption. The sources are marked reflected (existing `mark_reflected`). Re-running is idempotent (a cluster already represented by a hardened rule reinforces it rather than duplicating).

**Anti-weaponization (carried from the project's feedback-rules).** At capture time, reject a correction whose **Right** would disable a safety/validation/test step or otherwise weaken correctness (heuristic + keyword guard). Rejected corrections are not stored; the tool returns an explanatory error. This prevents the loop being used to "learn" to cut corners.

## Surfacing (hybrid)

- **Always-on:** the working-set builder for `mnemos://working` includes hardened rules tagged `mnemos:hardened`, capped at K (default 10) ranked by `importance × recency`, scoped to the current `workspace`/`source_tool` when set. Bounded regardless of how many corrections accumulate.
- **Relevance tail:** individual `Correction` memories surface through normal `recall` when the query/task matches their Trigger (they're ordinary procedural memories in the index).

## Lifecycle / decay

Corrections live in Procedural (slow decay). Each time one is recalled/applied, the existing access→strength reinforcement keeps it alive; un-applied corrections fade naturally. Hardened rules are decay-exempt while their cluster stays warm; if all sources go cold, the rule decays too. `supersede` keeps invalidated wrong beliefs out of recall.

## Surface area

- **MCP**: add the `correct` tool to `mcp/tools.rs` (list + dispatch). Mining hooks into the session-end pipeline; hardening extends `reflect`.
- **REST**: `POST /v1/corrections` (structured create, same validation as the tool) and `GET /v1/corrections` (list, newest-first, filterable by `hardened`). Corrections are also visible through existing memory endpoints since they're memories.
- **UI (optional, light)**: a "What Mnemos has learned" list (corrections + hardened rules) — can reuse the memory list; a dedicated view is a follow-up, not required for v1.
- **Adapters**: extend the instruction-file hint to mention `correct`.

## Error handling

- Missing/empty `why` → capture rejected with a clear, model-actionable message.
- Anti-weaponization match → rejected + reason; logged (not stored).
- `supersedes` id not found → warn, store the correction without the supersede link (don't fail the whole capture).
- Mining LLM unavailable/errored → skip mining for that session (tool path still works); never block session end.
- Dedup recall failure → fall back to creating the correction (favor capture over loss).

## Testing

- **Unit (core):** `Correction` creation + structured body round-trip; supersede sets `superseded_by` on the target; dedup reinforces instead of duplicating; hardening clusters ≥N and emits one `mnemos:hardened` reflection (mock LLM, existing `pipeline_reflect` pattern); anti-weaponization rejects a "skip the tests" correction.
- **Integration (daemon):** `correct` tool writes a retrievable Procedural correction; `recall` surfaces it by trigger; hardened rule appears in the working-set builder output; `GET /v1/corrections` lists it; missing-`why` returns a tool error.

## Scope boundary

- **In v1:** data model, both capture paths, dedup/reinforce, reflection hardening, hybrid surfacing, anti-weaponization, MCP tool + REST + hint.
- **Deferred to v2 (designed-for, not built):** **violation detection** — flagging/blocking when a stored correction is broken again. Requires an evaluation hook comparing in-progress actions against hardened rules; the trigger/rule data model supports it later.
- **Out of scope:** cross-project/global correction sharing (corrections stay per-vault); auto-tuning of decay/hardening thresholds (fixed defaults, config-exposed).

## Dependencies

Builds entirely on existing Mnemos systems: `Tier::{Procedural,Reflection}`, `MemoryType`, `pipeline/reflect.rs`, `pipeline/decay.rs`, retrieval reweight/reinforcement, `recall`, the session-end pipeline, and the MCP tool/resource layer. No new external crates anticipated.
