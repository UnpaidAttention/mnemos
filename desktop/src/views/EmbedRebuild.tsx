import { useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { client } from "../api/client";
import { useEmbedRebuildStatus } from "../api/queries";
import { Button, Card, Skeleton } from "../design/primitives";

/// One row per supported target embedder backend. Defaults reflect the
/// canonical model and dimension for each backend so the user does not
/// have to type them in the v0.8.0 UI.
const TARGETS = [
  { value: "bundled", label: "Bundled (MiniLM)", model: "all-MiniLM-L6-v2", dim: 384 },
  {
    value: "ollama",
    label: "Ollama (nomic-embed-text)",
    model: "nomic-embed-text",
    dim: 768,
  },
  {
    value: "openai",
    label: "OpenAI (text-embedding-3-small)",
    model: "text-embedding-3-small",
    dim: 1536,
  },
  { value: "mock", label: "Mock (tests)", model: "mock", dim: 384 },
] as const;

type RebuildStatus =
  | { status: "idle" }
  | { status: "running"; processed: number; total: number }
  | {
      status: "completed";
      processed: number;
      skipped: number;
      total: number;
      swapped: boolean;
    }
  | { status: "failed"; error: string; processed: number };

export function EmbedRebuild() {
  const qc = useQueryClient();
  const { data, isLoading } = useEmbedRebuildStatus();
  const [target, setTarget] = useState<string>("bundled");
  const [busy, setBusy] = useState(false);

  const start = async () => {
    const t = TARGETS.find((x) => x.value === target);
    if (!t) return;
    setBusy(true);
    try {
      await client.startEmbedRebuild(t.value, t.model, t.dim);
      // Don't wait for the polling interval — refetch immediately so the
      // UI flips from idle → running on the same render cycle as the WS
      // event arrives (or in tests where there is no WS).
      await qc.invalidateQueries({ queryKey: ["embed-rebuild", "status"] });
    } finally {
      setBusy(false);
    }
  };

  const abort = async () => {
    await client.abortEmbedRebuild();
    await qc.invalidateQueries({ queryKey: ["embed-rebuild", "status"] });
  };

  if (isLoading) {
    return (
      <div className="p-6">
        <Skeleton className="h-32 w-full" />
      </div>
    );
  }

  const status: RebuildStatus = data ?? { status: "idle" };

  return (
    <div className="p-6 max-w-2xl space-y-4">
      <h1 className="display text-xl">Migrate embedder</h1>
      <p className="text-text-muted font-body">
        Re-embeds every memory with the chosen backend. Atomic and resumable — safe to abort
        and restart. The old embeddings are kept as a backup for 7 days.
      </p>

      {status.status === "running" && (
        <Card className="p-4 space-y-2" data-testid="rebuild-running">
          <div className="label">Running</div>
          <div className="text-sm">
            {status.processed} of {status.total}
          </div>
          <div className="h-2 w-full bg-surface border border-border rounded-full overflow-hidden">
            <div
              className="h-full bg-tier-working transition-all"
              style={{
                width: `${Math.round((status.processed / Math.max(status.total, 1)) * 100)}%`,
              }}
            />
          </div>
          <Button variant="ghost" onClick={() => void abort()}>
            Abort
          </Button>
        </Card>
      )}

      {status.status === "completed" && (
        <Card className="p-4" data-testid="rebuild-completed">
          <div className="label">Completed</div>
          <p>
            Processed {status.processed}, skipped {status.skipped} of {status.total}.
          </p>
        </Card>
      )}

      {status.status === "failed" && (
        <Card className="p-4 text-tier-procedural" data-testid="rebuild-failed">
          <div className="label">Failed</div>
          <p>{status.error}</p>
        </Card>
      )}

      {status.status === "idle" && (
        <Card className="p-4 space-y-3" data-testid="rebuild-idle">
          <label className="flex flex-col gap-1">
            <span className="label">Target</span>
            <select
              aria-label="target"
              value={target}
              onChange={(e) => setTarget(e.target.value)}
              className="bg-surface border border-border rounded-md px-2 py-1 text-sm"
            >
              {TARGETS.map((t) => (
                <option key={t.value} value={t.value}>
                  {t.label}
                </option>
              ))}
            </select>
          </label>
          <Button onClick={() => void start()} disabled={busy}>
            {busy ? "Starting…" : "Start migration"}
          </Button>
        </Card>
      )}
    </div>
  );
}
