import { useMemo, useState } from "react";
import { useAuditAll } from "../api/queries";
import { Button, Skeleton } from "../design/primitives";

export function Audit() {
  const { data, isLoading, isError } = useAuditAll();
  const [filter, setFilter] = useState("");

  const rows = useMemo(
    () =>
      (data ?? []).filter(
        (e) =>
          !filter ||
          e.action.includes(filter) ||
          (e.memory_id ?? "").includes(filter),
      ),
    [data, filter],
  );

  const exportCsv = () => {
    const header = "ts,actor,action,memory_id\n";
    const body = rows
      .map((e) => `${e.ts},${e.actor},${e.action},${e.memory_id ?? ""}`)
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
      <div className="p-6">
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
    <div className="p-6 space-y-3">
      <div className="flex items-center justify-between">
        <h1 className="display text-xl">Audit log</h1>
        <Button variant="ghost" onClick={exportCsv}>
          Export CSV
        </Button>
      </div>

      <input
        className="bg-surface border border-border rounded-md px-2 py-1 mono text-sm w-64"
        placeholder="filter action / memory…"
        value={filter}
        onChange={(e) => setFilter(e.target.value)}
      />

      {!rows.length && !filter && (
        <p className="text-text-muted">No audit entries yet.</p>
      )}

      {!rows.length && filter && (
        <p className="text-text-muted">No entries match "{filter}".</p>
      )}

      {!!rows.length && (
        <table className="w-full text-sm mono">
          <thead>
            <tr className="label text-left">
              <th className="pb-1">ts</th>
              <th className="pb-1">action</th>
              <th className="pb-1">memory</th>
              <th className="pb-1">actor</th>
            </tr>
          </thead>
          <tbody>
            {rows.map((e) => (
              <tr key={e.id} className="border-t border-border">
                <td className="py-0.5">{e.ts.slice(0, 16)}</td>
                <td className="py-0.5">{e.action}</td>
                <td className="py-0.5">{e.memory_id ?? "—"}</td>
                <td className="py-0.5">{e.actor}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </div>
  );
}
