import { useMemo, useState } from "react";
import { useAuditAll } from "../api/queries";
import { Button, Card, Skeleton } from "../design/primitives";
import type { AuditEntry } from "../api/types";

/** Map action strings to human-readable labels and colors. */
const ACTION_META: Record<string, { label: string; color: string; icon: string }> = {
  create:   { label: "Created",     color: "text-green-400",      icon: "+" },
  forget:   { label: "Invalidated", color: "text-tier-procedural", icon: "×" },
  update:   { label: "Updated",     color: "text-accent",         icon: "~" },
  promote:  { label: "Re-tiered",   color: "text-tier-semantic",  icon: "↑" },
  embed_rebuild: { label: "Embed Rebuild", color: "text-text-muted", icon: "⟳" },
};

function actionMeta(action: string) {
  return ACTION_META[action] ?? { label: action, color: "text-text-muted", icon: "•" };
}

/** Format ISO timestamp to local readable string. */
function formatTs(iso: string): string {
  try {
    const d = new Date(iso);
    return d.toLocaleString(undefined, {
      month: "short", day: "numeric", hour: "2-digit", minute: "2-digit", second: "2-digit",
    });
  } catch {
    return iso.slice(0, 19).replace("T", " ");
  }
}

/** Format ISO timestamp to relative time. */
function relativeTime(iso: string): string {
  try {
    const diff = Date.now() - new Date(iso).getTime();
    const mins = Math.floor(diff / 60000);
    if (mins < 1) return "just now";
    if (mins < 60) return `${mins}m ago`;
    const hrs = Math.floor(mins / 60);
    if (hrs < 24) return `${hrs}h ago`;
    const days = Math.floor(hrs / 24);
    return `${days}d ago`;
  } catch {
    return "";
  }
}

/** Render a detail key-value pair. */
function DetailValue({ k, v }: { k: string; v: unknown }) {
  if (v === null || v === undefined) return null;
  const display = typeof v === "object" ? JSON.stringify(v) : String(v);
  return (
    <div className="flex gap-2 text-xs">
      <span className="text-text-muted min-w-[80px]">{k}</span>
      <span className="mono break-all">{display}</span>
    </div>
  );
}

function AuditRow({ entry, expanded, onToggle }: {
  entry: AuditEntry;
  expanded: boolean;
  onToggle: () => void;
}) {
  const meta = actionMeta(entry.action);

  return (
    <Card className="overflow-hidden">
      <button
        onClick={onToggle}
        className="flex items-center gap-3 w-full text-left p-3 text-sm hover:bg-surface-raised/40 transition-colors duration-100"
      >
        {/* Expand indicator */}
        <span className="text-text-muted text-xs w-3">{expanded ? "▾" : "▸"}</span>

        {/* Action icon */}
        <span className={`mono text-base w-4 text-center ${meta.color}`}>{meta.icon}</span>

        {/* Action label */}
        <span className={`label min-w-[100px] ${meta.color}`}>{meta.label}</span>

        {/* Timestamp */}
        <span className="text-text-muted text-xs min-w-[130px]">{formatTs(entry.ts)}</span>

        {/* Relative time */}
        <span className="text-text-muted text-xs min-w-[60px]">{relativeTime(entry.ts)}</span>

        {/* Memory ID (truncated) */}
        {entry.memory_id ? (
          <span className="mono text-xs text-accent truncate max-w-[160px]">{entry.memory_id}</span>
        ) : (
          <span className="text-text-muted text-xs">—</span>
        )}

        {/* Actor */}
        <span className="text-text-muted text-xs ml-auto">{entry.actor}</span>
      </button>

      {expanded && (
        <div className="px-6 pb-4 border-t border-border space-y-3">
          {/* Full details grid */}
          <div className="grid grid-cols-2 gap-x-8 gap-y-2 pt-3">
            <DetailValue k="Entry ID" v={entry.id} />
            <DetailValue k="Timestamp" v={entry.ts} />
            <DetailValue k="Action" v={entry.action} />
            <DetailValue k="Actor" v={entry.actor} />
            {entry.memory_id && (
              <div className="col-span-2">
                <DetailValue k="Memory ID" v={entry.memory_id} />
              </div>
            )}
          </div>

          {/* Details JSON */}
          {entry.details && Object.keys(entry.details).length > 0 && (
            <div className="space-y-1.5">
              <span className="label text-xs">Details</span>
              <div className="bg-surface-raised rounded-md p-3 space-y-1">
                {Object.entries(entry.details).map(([k, v]) => (
                  <DetailValue key={k} k={k} v={v} />
                ))}
              </div>
            </div>
          )}
        </div>
      )}
    </Card>
  );
}

/** Summary stats card at the top of the audit page. */
function AuditSummary({ entries }: { entries: AuditEntry[] }) {
  const stats = useMemo(() => {
    const byAction: Record<string, number> = {};
    const byActor: Record<string, number> = {};
    for (const e of entries) {
      byAction[e.action] = (byAction[e.action] ?? 0) + 1;
      byActor[e.actor] = (byActor[e.actor] ?? 0) + 1;
    }
    return { byAction, byActor, total: entries.length };
  }, [entries]);

  return (
    <Card className="p-5">
      <div className="flex gap-10 flex-wrap">
        <div className="flex flex-col gap-0.5">
          <span className="label">Total entries</span>
          <span className="display text-2xl">{stats.total}</span>
        </div>
        {Object.entries(stats.byAction).sort((a, b) => b[1] - a[1]).map(([action, count]) => {
          const meta = actionMeta(action);
          return (
            <div key={action} className="flex flex-col gap-0.5">
              <span className={`label ${meta.color}`}>{meta.label}</span>
              <span className="display text-2xl">{count}</span>
            </div>
          );
        })}
      </div>
    </Card>
  );
}

export function Audit() {
  const { data, isLoading, isError } = useAuditAll();
  const [filter, setFilter] = useState("");
  const [actionFilter, setActionFilter] = useState<string | null>(null);
  const [expandedId, setExpandedId] = useState<number | null>(null);

  const allEntries = data ?? [];

  // Get unique actions for filter buttons
  const actions = useMemo(() => {
    const set = new Set(allEntries.map((e) => e.action));
    return Array.from(set).sort();
  }, [allEntries]);

  const rows = useMemo(
    () =>
      allEntries.filter((e) => {
        if (actionFilter && e.action !== actionFilter) return false;
        if (!filter) return true;
        const q = filter.toLowerCase();
        return (
          e.action.toLowerCase().includes(q) ||
          (e.memory_id ?? "").toLowerCase().includes(q) ||
          e.actor.toLowerCase().includes(q) ||
          (e.details ? JSON.stringify(e.details).toLowerCase().includes(q) : false)
        );
      }),
    [allEntries, filter, actionFilter],
  );

  const exportCsv = () => {
    const header = "id,ts,actor,action,memory_id,details\n";
    const body = rows
      .map((e) =>
        `${e.id},"${e.ts}","${e.actor}","${e.action}","${e.memory_id ?? ""}","${e.details ? JSON.stringify(e.details).replace(/"/g, '""') : ""}"`
      )
      .join("\n");
    const url = URL.createObjectURL(
      new Blob([header + body], { type: "text/csv" }),
    );
    const a = document.createElement("a");
    a.href = url;
    a.download = "mnemos-audit.csv";
    a.click();
    URL.revokeObjectURL(url);
  };

  if (isLoading) {
    return (
      <div className="p-6 space-y-4">
        <Skeleton className="h-8 w-32" />
        <Skeleton className="h-24 w-full" />
        <Skeleton className="h-64 w-full" />
      </div>
    );
  }

  if (isError) {
    return (
      <div className="p-6 text-tier-procedural">
        Could not load the audit log. Is the daemon running?
      </div>
    );
  }

  return (
    <div className="p-6 space-y-5">
      <div className="flex items-center justify-between">
        <h1 className="display text-xl">Audit log</h1>
        <Button variant="ghost" onClick={exportCsv}>
          Export CSV
        </Button>
      </div>

      {/* Summary stats */}
      <AuditSummary entries={allEntries} />

      {/* Filters */}
      <div className="flex items-center gap-3 flex-wrap">
        <input
          className="bg-surface border border-border rounded-md px-2 py-1 mono text-sm w-64 placeholder:text-text-muted/50"
          placeholder="search actions, memories, details…"
          value={filter}
          onChange={(e) => setFilter(e.target.value)}
        />

        {/* Action filter chips */}
        <div className="flex gap-1.5 flex-wrap">
          <button
            className={`px-2 py-0.5 rounded text-xs mono transition-colors ${
              actionFilter === null
                ? "bg-accent/20 text-accent"
                : "bg-surface-raised text-text-muted hover:text-text"
            }`}
            onClick={() => setActionFilter(null)}
          >
            all
          </button>
          {actions.map((a) => {
            const meta = actionMeta(a);
            return (
              <button
                key={a}
                className={`px-2 py-0.5 rounded text-xs mono transition-colors ${
                  actionFilter === a
                    ? "bg-accent/20 text-accent"
                    : "bg-surface-raised text-text-muted hover:text-text"
                }`}
                onClick={() => setActionFilter(actionFilter === a ? null : a)}
              >
                {meta.label.toLowerCase()}
              </button>
            );
          })}
        </div>

        <span className="text-text-muted text-xs ml-auto mono">{rows.length} entries</span>
      </div>

      {/* Empty states */}
      {!rows.length && !filter && !actionFilter && (
        <Card className="p-5">
          <p className="text-text-muted text-sm font-body">
            No audit entries yet. The audit log records every memory creation,
            update, invalidation, and tier change automatically.
          </p>
        </Card>
      )}

      {!rows.length && (filter || actionFilter) && (
        <p className="text-text-muted text-sm">No entries match your filters.</p>
      )}

      {/* Audit entries list */}
      {!!rows.length && (
        <div className="space-y-1">
          {rows.map((e) => (
            <AuditRow
              key={e.id}
              entry={e}
              expanded={expandedId === e.id}
              onToggle={() => setExpandedId(expandedId === e.id ? null : e.id)}
            />
          ))}
        </div>
      )}
    </div>
  );
}
