import { useMemo, useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { useEntities } from "../api/queries";
import { client } from "../api/client";
import { Button } from "../design/primitives";

interface MergeDialogProps {
  open: boolean;
  source: { id: string; name: string };
  onClose: () => void;
}

export function MergeDialog({ open, source, onClose }: MergeDialogProps) {
  const qc = useQueryClient();
  const { data: entities } = useEntities();
  const [query, setQuery] = useState("");
  const [picked, setPicked] = useState<{ id: string; name: string } | null>(null);
  const [busy, setBusy] = useState(false);

  const filtered = useMemo(
    () =>
      (entities ?? [])
        .filter(
          (e) =>
            e.id !== source.id &&
            e.name.toLowerCase().includes(query.toLowerCase()),
        )
        .slice(0, 12),
    [entities, query, source.id],
  );

  if (!open) return null;

  const submit = async () => {
    if (!picked) return;
    setBusy(true);
    try {
      await client.mergeEntities(source.id, picked.id);
      await qc.invalidateQueries({ queryKey: ["entities"] });
      await qc.invalidateQueries({ queryKey: ["entity", source.id] });
      await qc.invalidateQueries({ queryKey: ["graph"] });
      onClose();
    } finally {
      setBusy(false);
    }
  };

  return (
    <div
      className="fixed inset-0 z-50 flex items-start justify-center bg-black/30 pt-28"
      onClick={onClose}
    >
      <div
        role="dialog"
        aria-label="Merge entity"
        className="w-[34rem] rounded-lg bg-surface-raised shadow-floating border border-border p-4 space-y-3"
        onClick={(e) => e.stopPropagation()}
      >
        <div>
          <div className="label">Merge entity</div>
          <h2 className="display text-lg">{source.name}</h2>
          <p className="text-text-muted text-sm">
            All mentions and edges from{" "}
            <span className="mono">{source.id}</span> will move to the picked
            target. The source name is added as an alias.
          </p>
        </div>
        <input
          autoFocus
          className="w-full bg-surface border border-border rounded-md px-2 py-1 mono text-sm"
          placeholder="search target…"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
        />
        <ul className="max-h-56 overflow-y-auto border border-border rounded-md">
          {filtered.map((e) => (
            <li key={e.id}>
              <button
                onClick={() => setPicked({ id: e.id, name: e.name })}
                className={`w-full px-3 py-1.5 text-left text-sm hover:bg-surface ${
                  picked?.id === e.id ? "bg-surface-raised" : ""
                }`}
              >
                <span className="font-body">{e.name}</span>{" "}
                <span className="mono text-text-muted text-xs">{e.id}</span>
              </button>
            </li>
          ))}
          {!filtered.length && (
            <li className="px-3 py-2 text-sm text-text-muted">no matches</li>
          )}
        </ul>
        <div className="flex justify-end gap-2">
          <button onClick={onClose} className="label text-text-muted">
            Cancel
          </button>
          <Button onClick={submit} disabled={!picked || busy}>
            {busy ? "Merging…" : "Merge"}
          </Button>
        </div>
      </div>
    </div>
  );
}
