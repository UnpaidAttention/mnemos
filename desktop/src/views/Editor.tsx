import { useEffect, useState } from "react";
import { useParams, useNavigate } from "@tanstack/react-router";
import { useQueryClient } from "@tanstack/react-query";
import { useMemory } from "../api/queries";
import { client } from "../api/client";
import { CodeMirrorView } from "../components/CodeMirror";
import { Button, Skeleton, TierChip } from "../design/primitives";

export function Editor({ id: idProp }: { id?: string }) {
  const navigate = useNavigate();
  const params = useParams({ strict: false }) as { id?: string };
  const id = idProp ?? params.id ?? null;
  const { data: mem, isLoading, isError } = useMemory(id);
  const qc = useQueryClient();
  const [tags, setTags] = useState("");
  const [importance, setImportance] = useState(0.5);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    if (mem) {
      setTags(mem.tags.join(", "));
      setImportance(mem.importance);
    }
  }, [mem]);

  if (isLoading || !id) {
    return (
      <div className="p-6">
        <Skeleton className="h-8 w-64 mb-4" />
        <Skeleton className="h-64 w-full" />
      </div>
    );
  }

  if (isError) {
    return (
      <div className="p-6">
        <p className="text-tier-procedural font-body">
          Memory not found or daemon unreachable.
        </p>
      </div>
    );
  }

  if (!mem) return null;

  const invalid = !!mem.invalid_at;

  const save = async () => {
    setSaving(true);
    try {
      await client.patchMemory(mem.id, {
        tags: tags
          .split(",")
          .map((t) => t.trim())
          .filter(Boolean),
        importance,
      });
      await qc.invalidateQueries({ queryKey: ["memory", mem.id] });
      await qc.invalidateQueries({ queryKey: ["memories"] });
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="p-6 space-y-4 max-w-3xl">
      {/* Breadcrumb navigation */}
      <nav className="flex items-center gap-2 text-sm text-text-muted" aria-label="Breadcrumb">
        <button
          onClick={() => void navigate({ to: "/" })}
          className="hover:text-accent transition-colors duration-100"
          aria-label="Back to Browser"
        >
          ← Browser
        </button>
        <span aria-hidden>/</span>
        <span className="truncate text-text">{mem.title}</span>
      </nav>

      {invalid && (
        <div
          className="border border-dashed border-tier-procedural rounded-md px-3 py-2 text-sm text-tier-procedural"
          role="alert"
        >
          This memory is invalidated and read-only.
        </div>
      )}
      <div className="flex items-center gap-3">
        <input
          className={`display text-xl bg-transparent border-b border-border flex-1 outline-none ${invalid ? "line-through opacity-60" : ""}`}
          defaultValue={mem.title}
          readOnly
          aria-label="title"
        />
        <TierChip tier={mem.tier} />
      </div>

      <dl className="grid grid-cols-2 gap-x-8 gap-y-1 text-sm">
        <div className="flex justify-between">
          <dt className="text-text-muted">strength</dt>
          <dd className="mono">{mem.strength.toFixed(2)}</dd>
        </div>
        <div className="flex justify-between">
          <dt className="text-text-muted">access count</dt>
          <dd className="mono">{mem.access_count}</dd>
        </div>
        <div className="flex justify-between">
          <dt className="text-text-muted">valid from</dt>
          <dd className="mono">{mem.valid_at.slice(0, 10)}</dd>
        </div>
        {mem.invalid_at && (
          <div className="flex justify-between">
            <dt className="text-text-muted">invalidated</dt>
            <dd className="mono">{mem.invalid_at.slice(0, 10)}</dd>
          </div>
        )}
      </dl>

      <label className="block space-y-1">
        <span className="label">tags (comma-separated)</span>
        <input
          className="mono w-full bg-surface border border-border rounded-md px-2 py-1.5 focus-visible:outline focus-visible:outline-2 focus-visible:outline-accent"
          value={tags}
          onChange={(e) => setTags(e.target.value)}
          disabled={invalid}
          aria-label="tags"
        />
      </label>

      <label className="block space-y-1">
        <span className="label">
          importance:{" "}
          <span className="mono">{importance.toFixed(2)}</span>
        </span>
        <input
          type="range"
          min={0}
          max={1}
          step={0.05}
          value={importance}
          onChange={(e) => setImportance(Number(e.target.value))}
          className="w-full accent-accent"
          disabled={invalid}
          aria-label="importance"
        />
      </label>

      <div className="space-y-1">
        <span className="label">
          body{" "}
          <span className="text-text-muted normal-case tracking-normal text-xs">
            (read-only — edit the .md file on disk to change)
          </span>
        </span>
        <CodeMirrorView value={mem.body} />
      </div>

      {!invalid && (
        <Button onClick={save} disabled={saving}>
          {saving ? "Saving…" : "Save"}
        </Button>
      )}
    </div>
  );
}
