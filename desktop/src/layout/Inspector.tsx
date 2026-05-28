import { useUiStore } from "../store/ui";
import { useMemory, useAudit } from "../api/queries";
import { TierChip } from "../design/primitives";

export function Inspector() {
  const { selectedMemoryId, inspectorOpen } = useUiStore();
  const { data: mem } = useMemory(selectedMemoryId);
  const { data: audit } = useAudit(selectedMemoryId);
  if (!inspectorOpen) return null;
  return (
    <aside role="complementary" className="w-80 shrink-0 border-l border-border bg-surface p-4 overflow-y-auto">
      <div className="label mb-2">Inspector</div>
      {!selectedMemoryId && <p className="text-sm text-text-muted">Select a memory to inspect.</p>}
      {mem && (
        <div className="space-y-3">
          <h2 className={`display text-base ${mem.invalid_at ? "line-through opacity-60" : ""}`}>{mem.title}</h2>
          <TierChip tier={mem.tier} />
          <dl className="text-sm space-y-1">
            <div className="flex justify-between"><dt className="text-text-muted">strength</dt><dd className="mono">{mem.strength.toFixed(2)}</dd></div>
            <div className="flex justify-between"><dt className="text-text-muted">importance</dt><dd className="mono">{mem.importance.toFixed(2)}</dd></div>
            <div className="flex justify-between"><dt className="text-text-muted">valid</dt><dd className="mono">{mem.valid_at.slice(0, 10)}</dd></div>
          </dl>
          {!!mem.provenance.length && (
            <div><div className="label">provenance</div><ul className="text-xs mono">{mem.provenance.map((p, i) => <li key={i}>{p.session ?? "—"} · {p.chunks.length} chunks</li>)}</ul></div>
          )}
          <div>
            <div className="label">audit</div>
            <ul className="text-xs mono space-y-0.5">{(audit ?? []).map((a) => <li key={a.id}>{a.ts.slice(0, 16)} · {a.action}</li>)}</ul>
          </div>
        </div>
      )}
    </aside>
  );
}
