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

export function Pipelines() {
  const { data, isLoading, isError } = usePipelines();
  const qc = useQueryClient();
  const [busy, setBusy] = useState<string | null>(null);
  // P2-15: surface trigger errors near the maintenance buttons
  const [triggerError, setTriggerError] = useState<string | null>(null);

  const trigger = async (which: "decay" | "communities") => {
    setTriggerError(null);
    setBusy(which);
    try {
      if (which === "decay") {
        await client.runDecay();
      } else {
        await client.runCommunities();
      }
      await qc.invalidateQueries({ queryKey: ["pipelines"] });
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

      <Card className="p-5">
        <div className="label mb-3">
          Learning pipeline ·{" "}
          <span className="mono text-accent">{modelLabel}</span>
        </div>
        <div className="flex gap-10">
          <Stat label="completed" value={data.counters.completed} />
          <Stat label="failed" value={data.counters.failed} />
          <Stat label="facts added" value={data.counters.facts_added} />
        </div>
      </Card>

      <div className="space-y-2">
        <div className="label">Maintenance</div>
        <div className="flex gap-2">
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
      </div>

      <div className="space-y-2">
        <div className="label">Recent runs</div>
        {data.recent.length === 0 ? (
          <p className="text-text-muted text-sm font-body">
            No pipeline runs recorded yet. Runs appear here after the first
            ingestion session completes.
          </p>
        ) : (
          <ul className="text-sm mono space-y-1">
            {data.recent.map((r, i) => (
              <li
                key={i}
                className={`flex items-center gap-3 ${r.ok ? "" : "text-tier-procedural"}`}
              >
                <span className="text-text-muted">{r.at.slice(0, 16)}</span>
                <span>{r.session_id.slice(0, 12)}</span>
                <span>+{r.facts_added} facts</span>
                {!r.ok && <span className="label text-tier-procedural">failed</span>}
              </li>
            ))}
          </ul>
        )}
      </div>
    </div>
  );
}
