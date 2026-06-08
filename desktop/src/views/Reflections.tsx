import { useState } from "react";
import { Link } from "@tanstack/react-router";
import { useQueryClient } from "@tanstack/react-query";
import { useReflections } from "../api/queries";
import { client } from "../api/client";
import { useUiStore } from "../store/ui";
import { Button, Card, Skeleton, TierChip } from "../design/primitives";

export function Reflections() {
  const { data, isLoading, isError } = useReflections();
  const qc = useQueryClient();
  const select = useUiStore((s) => s.select);
  const [busy, setBusy] = useState(false);
  const [expandedId, setExpandedId] = useState<string | null>(null);
  // P2-15: local error state surfaced near the promote button
  const [promoteError, setPromoteError] = useState<string | null>(null);

  const reflectNow = async () => {
    setBusy(true);
    try {
      await client.reflect();
      await qc.invalidateQueries({ queryKey: ["reflections"] });
    } finally {
      setBusy(false);
    }
  };

  const promote = async (id: string) => {
    setPromoteError(null);
    try {
      await client.promoteMemory(id, "procedural");
      await qc.invalidateQueries({ queryKey: ["reflections"] });
      await qc.invalidateQueries({ queryKey: ["memories"] });
    } catch (err) {
      setPromoteError(
        err instanceof Error ? err.message : "Failed to promote memory",
      );
    }
  };

  if (isLoading) {
    return (
      <div className="p-6 space-y-2">
        {Array.from({ length: 4 }).map((_, i) => (
          <Skeleton key={i} className="h-16 w-full" />
        ))}
      </div>
    );
  }

  if (isError) {
    return (
      <div className="p-6 text-tier-procedural">
        Could not load reflections. Is the daemon running?
      </div>
    );
  }

  return (
    <div className="p-6 space-y-4">
      <div className="flex items-center justify-between">
        <h1 className="display text-xl">Reflections</h1>
        <Button onClick={reflectNow} disabled={busy}>
          {busy ? "Reflecting…" : "Reflect now"}
        </Button>
      </div>

      {!data?.length && (
        <p className="text-text-muted font-body">
          No reflections yet. Mnemos synthesizes reflections from your semantic
          memories as your knowledge base grows. Click{" "}
          <strong>Reflect now</strong> to generate one manually, or wait for the
          system to generate them automatically after pipeline runs.
        </p>
      )}

      {promoteError && (
        <p
          role="alert"
          className="text-sm text-tier-procedural"
          data-testid="promote-error"
        >
          {promoteError}
        </p>
      )}

      <div className="space-y-2">
        {data?.map((r) => {
          const isExpanded = expandedId === r.id;
          return (
            <Card key={r.id} className="overflow-hidden">
              {/* Summary row */}
              <button
                onClick={() => setExpandedId(isExpanded ? null : r.id)}
                className="block w-full text-left p-3 hover:bg-surface-raised/40 transition-colors duration-100"
              >
                <div className="flex items-start gap-2">
                  <span className="text-text-muted text-xs mt-0.5">{isExpanded ? "▾" : "▸"}</span>
                  <div className="min-w-0 flex-1">
                    <div className="flex items-center gap-2 mb-1">
                      <span className="font-body text-sm font-medium">{r.title}</span>
                      <TierChip tier={r.tier} />
                    </div>
                    <div className="text-sm text-text-muted font-body line-clamp-2">
                      {r.body.slice(0, 200)}{r.body.length > 200 ? "…" : ""}
                    </div>
                  </div>
                  <span className="label mono text-[0.65rem] text-text-muted shrink-0">
                    {r.valid_at.slice(0, 10)}
                  </span>
                </div>
              </button>

              {/* Expanded detail */}
              {isExpanded && (
                <div className="px-6 pb-4 space-y-3 border-t border-border">
                  {/* Full body */}
                  <div className="text-sm font-body whitespace-pre-wrap bg-surface rounded-md p-3 border border-border max-h-64 overflow-y-auto mt-3">
                    {r.body}
                  </div>

                  {/* Metadata */}
                  <dl className="grid grid-cols-3 gap-x-6 gap-y-1 text-xs">
                    <div className="flex justify-between">
                      <dt className="text-text-muted">Strength</dt>
                      <dd className="mono">{r.strength.toFixed(2)}</dd>
                    </div>
                    <div className="flex justify-between">
                      <dt className="text-text-muted">Importance</dt>
                      <dd className="mono">{r.importance.toFixed(2)}</dd>
                    </div>
                    <div className="flex justify-between">
                      <dt className="text-text-muted">Accesses</dt>
                      <dd className="mono">{r.access_count}</dd>
                    </div>
                  </dl>

                  {/* Tags */}
                  {r.tags.length > 0 && (
                    <div className="flex flex-wrap gap-1">
                      {r.tags.map((tag) => (
                        <span
                          key={tag}
                          className="label mono text-[0.65rem] text-text-muted border border-border rounded-sm px-1.5 py-0.5"
                        >
                          {tag}
                        </span>
                      ))}
                    </div>
                  )}

                  {/* Actions */}
                  <div className="flex items-center gap-3 pt-1">
                    <Link
                      to={`/editor/${r.id}` as "/"}
                      className="label text-accent hover:underline text-xs"
                    >
                      Open in editor →
                    </Link>
                    <button
                      onClick={() => select(r.id)}
                      className="label text-accent hover:underline text-xs"
                    >
                      Inspect
                    </button>
                    <button
                      onClick={() => void promote(r.id)}
                      className="label text-accent hover:underline text-xs"
                    >
                      Promote to procedural
                    </button>
                  </div>
                </div>
              )}
            </Card>
          );
        })}
      </div>
    </div>
  );
}
