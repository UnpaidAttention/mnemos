import { useQueryClient } from "@tanstack/react-query";
import { useDoctor } from "../api/queries";
import { Button, Card, Skeleton } from "../design/primitives";

const STATUS_COLOR: Record<string, string> = {
  ok: "var(--tier-semantic)",
  warn: "var(--tier-working)",
  fail: "var(--tier-procedural)",
};

const STATUS_ORDER: Record<string, number> = { fail: 0, warn: 1, ok: 2 };

export function Doctor() {
  const qc = useQueryClient();
  const { data, isLoading, isError } = useDoctor();
  if (isLoading) {
    return (
      <div className="p-6">
        <Skeleton className="h-64 w-full" />
      </div>
    );
  }
  if (isError || !data) {
    return <div className="p-6 text-tier-procedural">Could not load diagnostics.</div>;
  }
  const refresh = () => qc.invalidateQueries({ queryKey: ["doctor"] });
  const sortedChecks = [...data.checks].sort(
    (a, b) => (STATUS_ORDER[a.status] ?? 99) - (STATUS_ORDER[b.status] ?? 99),
  );
  return (
    <div className="p-6 space-y-4 max-w-3xl">
      <div className="flex items-center justify-between">
        <h1 className="display text-xl">Doctor</h1>
        <Button variant="ghost" onClick={refresh}>
          Refresh
        </Button>
      </div>
      <div className="space-y-2">
        {sortedChecks.map((c) => (
          <Card key={c.name} className="p-3" data-testid="doctor-row">
            <div className="flex items-center gap-3">
              <span
                aria-hidden
                className="inline-block h-2.5 w-2.5 rounded-full"
                style={{ background: STATUS_COLOR[c.status] }}
              />
              <div className="flex-1">
                <div className="font-body">{c.name}</div>
                <div className="label text-text-muted">{c.detail}</div>
              </div>
              <span className="label">{c.status}</span>
            </div>
          </Card>
        ))}
      </div>
      <details className="text-sm">
        <summary className="label cursor-pointer">file/DB drift report</summary>
        <pre className="mono text-xs bg-surface border border-border rounded-md p-3 mt-2 overflow-auto">
          {JSON.stringify(data.report, null, 2)}
        </pre>
      </details>
    </div>
  );
}
