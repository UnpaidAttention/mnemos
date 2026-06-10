import { useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { usePipelines } from "../api/queries";
import { client } from "../api/client";
import { Button, Card, Skeleton } from "../design/primitives";

function Stat({ label, value }: { label: string; value: number | string }) {
  return (
    <div className="flex flex-col gap-0.5">
      <span className="label">{label}</span>
      <span className="display text-2xl">{value}</span>
    </div>
  );
}

function SmallStat({ label, value }: { label: string; value: number | string }) {
  return (
    <div className="flex justify-between text-xs">
      <span className="text-text-muted">{label}</span>
      <span className="mono">{value}</span>
    </div>
  );
}

/** Progress bar driven by the REST API response (data.backfill).
 *  WS events trigger query invalidation → re-fetch → prop update. */
function BackfillProgressBar({ backfill }: {
  backfill: { processed: number; total: number; entities_linked: number; errors: number } | null;
}) {
  if (!backfill || backfill.total === 0) return null;

  const { processed, total, entities_linked, errors } = backfill;
  const pct = total > 0 ? Math.round((processed / total) * 100) : 0;

  return (
    <Card className="p-4 mt-2">
      <div className="flex items-center justify-between mb-2">
        <span className="label">Backfill in progress</span>
        <span className="mono text-sm text-accent">{processed}/{total} memories</span>
      </div>

      {/* Progress bar */}
      <div className="w-full h-2 rounded-full bg-surface-raised overflow-hidden">
        <div
          className="h-full rounded-full transition-all duration-300 ease-out"
          style={{
            width: `${pct}%`,
            background: "linear-gradient(90deg, var(--color-accent), var(--color-tier-semantic))",
          }}
        />
      </div>

      <div className="flex gap-6 mt-3 text-xs">
        <div className="flex gap-1.5">
          <span className="text-text-muted">Progress</span>
          <span className="mono text-accent">{pct}%</span>
        </div>
        <div className="flex gap-1.5">
          <span className="text-text-muted">Entities linked</span>
          <span className="mono">{entities_linked}</span>
        </div>
        {errors > 0 && (
          <div className="flex gap-1.5">
            <span className="text-text-muted">Errors</span>
            <span className="mono text-tier-procedural">{errors}</span>
          </div>
        )}
      </div>
    </Card>
  );
}

export function Pipelines() {
  const { data, isLoading, isError } = usePipelines();
  const qc = useQueryClient();
  const [busy, setBusy] = useState<string | null>(null);
  const [expandedRun, setExpandedRun] = useState<number | null>(null);
  // P2-15: surface trigger errors near the maintenance buttons
  const [triggerError, setTriggerError] = useState<string | null>(null);
  const [backfillResult, setBackfillResult] = useState<{
    memories_processed: number;
    entities_linked: number;
    edges_created: number;
    reflections_created: number;
    communities_found: number;
    errors: number;
  } | null>(null);

  const trigger = async (which: "decay" | "communities" | "backfill") => {
    setTriggerError(null);
    setBusy(which);
    try {
      if (which === "decay") {
        await client.runDecay();
      } else if (which === "communities") {
        await client.runCommunities();
      } else {
        const result = await client.runBackfill();
        setBackfillResult(result);
      }
      await qc.invalidateQueries({ queryKey: ["pipelines"] });
      // Refresh graph + entities after backfill/communities
      if (which === "backfill" || which === "communities") {
        await qc.invalidateQueries({ queryKey: ["graph"] });
        await qc.invalidateQueries({ queryKey: ["entities"] });
        await qc.invalidateQueries({ queryKey: ["reflections"] });
      }
    } catch (err) {
      setTriggerError(
        err instanceof Error ? err.message : `Failed to run ${which}`,
      );
    } finally {
      setBusy(null);
    }
  };

  if (isLoading) {
    return (
      <div className="p-6 space-y-4">
        <Skeleton className="h-8 w-32" />
        <Skeleton className="h-40 w-full" />
      </div>
    );
  }

  if (isError || !data) {
    return (
      <div className="p-6">
        <p className="text-tier-procedural font-body">
          Could not load pipeline status. Is the daemon running?
        </p>
      </div>
    );
  }

  const modelLabel = data.enabled
    ? (data.llm_model ?? "unknown model")
    : "disabled (no LLM configured)";

  return (
    <div className="p-6 space-y-6">
      <h1 className="display text-xl">Pipelines</h1>

      {/* Status card */}
      <Card className="p-5">
        <div className="flex items-center gap-3 mb-3">
          <span
            className={`h-2.5 w-2.5 rounded-full ${data.enabled ? "bg-green-500" : "bg-text-muted"}`}
            aria-label={data.enabled ? "Pipeline active" : "Pipeline disabled"}
          />
          <span className="label">
            Learning pipeline ·{" "}
            <span className="mono text-accent">{modelLabel}</span>
          </span>
        </div>
        <div className="flex gap-10">
          <Stat label="completed" value={data.counters.completed} />
          <Stat label="failed" value={data.counters.failed} />
          <Stat label="facts added" value={data.counters.facts_added} />
        </div>
      </Card>

      {/* Live backfill progress */}
      <BackfillProgressBar backfill={data.backfill ?? null} />

      {/* Maintenance */}
      <div className="space-y-2">
        <div className="label">Maintenance</div>
        <div className="flex gap-2 flex-wrap">
          <Button
            variant="ghost"
            onClick={() => void trigger("decay")}
            disabled={busy === "decay"}
          >
            {busy === "decay" ? "Running…" : "Run decay"}
          </Button>
          <Button
            variant="ghost"
            onClick={() => void trigger("communities")}
            disabled={busy === "communities" || !data.enabled}
            title={!data.enabled ? "Enable an LLM model to detect communities" : undefined}
          >
            {busy === "communities" ? "Detecting…" : "Detect communities"}
          </Button>
          <Button
            variant="ghost"
            onClick={() => void trigger("backfill")}
            disabled={!!data.backfill || busy === "backfill" || !data.enabled}
            title={!data.enabled ? "Enable an LLM model to run backfill" : data.backfill ? "Backfill is already running" : "Retroactively extract entities and relationships from all existing memories"}
          >
            {data.backfill ? `Processing ${data.backfill.processed}/${data.backfill.total}…` : busy === "backfill" ? "Starting…" : "Backfill entities"}
          </Button>
        </div>
        {triggerError && (
          <p
            role="alert"
            className="text-sm text-tier-procedural"
            data-testid="trigger-error"
          >
            {triggerError}
          </p>
        )}
        {backfillResult && !data.backfill && (
          <Card className="p-4 mt-2">
            <div className="label mb-2">Backfill results</div>
            <div className="flex gap-6 flex-wrap">
              <Stat label="memories" value={backfillResult.memories_processed} />
              <Stat label="entities" value={backfillResult.entities_linked} />
              <Stat label="edges" value={backfillResult.edges_created} />
              <Stat label="reflections" value={backfillResult.reflections_created} />
              <Stat label="communities" value={backfillResult.communities_found} />
              {backfillResult.errors > 0 && (
                <Stat label="errors" value={backfillResult.errors} />
              )}
            </div>
          </Card>
        )}
      </div>

      {/* Recent runs */}
      <div className="space-y-2">
        <div className="label">Recent runs</div>
        {data.recent.length === 0 ? (
          <Card className="p-5">
            <p className="text-text-muted text-sm font-body">
              The learning pipeline is active and monitoring for conversations.
              Pipeline runs appear here automatically as you use MCP-connected
              tools like Claude Code or Codex. If you have existing memories,
              click <strong>Backfill entities</strong> above to process them.
            </p>
          </Card>
        ) : (
          <div className="space-y-1">
            {data.recent.map((r, i) => {
              const isExpanded = expandedRun === i;
              return (
                <Card key={i} className="overflow-hidden">
                  <button
                    onClick={() => setExpandedRun(isExpanded ? null : i)}
                    className={`flex items-center gap-3 w-full text-left p-3 text-sm mono hover:bg-surface-raised/40 transition-colors duration-100 ${r.ok ? "" : "text-tier-procedural"}`}
                  >
                    <span className="text-text-muted text-xs">{isExpanded ? "▾" : "▸"}</span>
                    <span
                      className={`h-2 w-2 rounded-full shrink-0 ${r.ok ? "bg-green-500" : "bg-tier-procedural"}`}
                    />
                    <span className="text-text-muted">{r.at.slice(0, 16).replace("T", " ")}</span>
                    <span className="truncate">{r.session_id.slice(0, 12)}…</span>
                    <span className="text-accent">+{r.facts_added} facts</span>
                    {!r.ok && <span className="label text-tier-procedural">failed</span>}
                  </button>

                  {isExpanded && (
                    <div className="px-6 pb-4 space-y-2 border-t border-border">
                      <div className="grid grid-cols-2 gap-x-6 gap-y-1 pt-3">
                        <SmallStat label="Session ID" value={r.session_id} />
                        <SmallStat label="Timestamp" value={r.at.slice(0, 19).replace("T", " ")} />
                        <SmallStat label="Facts added" value={r.facts_added} />
                        <SmallStat label="Status" value={r.ok ? "✓ Completed" : "✗ Failed"} />
                      </div>
                    </div>
                  )}
                </Card>
              );
            })}
          </div>
        )}
      </div>
    </div>
  );
}
