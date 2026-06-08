import { useEffect, useMemo, useRef, useState } from "react";
import { Link } from "@tanstack/react-router";
import { client } from "../api/client";
import { useUiStore } from "../store/ui";
import { Button, Card, Skeleton, TierChip } from "../design/primitives";
import type { Memory } from "../api/types";

type Tab = "memories" | "corrections" | "hardened";

const TAB_LABELS: Record<Tab, string> = {
  memories: "Memories",
  corrections: "Corrections",
  hardened: "Hardened rules",
};

function MemoryRow({
  memory,
  onDelete,
}: {
  memory: Memory;
  onDelete: (id: string) => void;
}) {
  const [expanded, setExpanded] = useState(false);
  const select = useUiStore((s) => s.select);
  const invalid = !!memory.invalid_at;

  return (
    <div
      className={`border-b border-border last:border-b-0 ${invalid ? "opacity-60" : ""}`}
    >
      {/* Summary row */}
      <button
        onClick={() => setExpanded(!expanded)}
        className="flex items-start justify-between gap-4 py-3 w-full text-left hover:bg-surface-raised/40 transition-colors duration-100 px-1 rounded"
      >
        <div className="min-w-0 flex-1 space-y-0.5">
          <div className="flex items-center gap-2">
            <span className="text-text-muted text-xs">{expanded ? "▾" : "▸"}</span>
            <div className={`font-body text-sm font-medium truncate ${invalid ? "line-through" : ""}`}>
              {memory.title}
            </div>
            <TierChip tier={memory.tier} />
            {memory.source_tool && (
              <span className="text-[10px] px-1.5 py-0.5 rounded-full bg-surface border border-border text-text-muted font-mono">
                {memory.source_tool}
              </span>
            )}
          </div>
          <div className="label truncate max-w-prose pl-5">{memory.body.slice(0, 120)}{memory.body.length > 120 ? "…" : ""}</div>
        </div>
        <span className="label mono text-[0.65rem] text-text-muted shrink-0 pt-1">
          {memory.valid_at.slice(0, 10)}
        </span>
      </button>

      {/* Expanded detail panel */}
      {expanded && (
        <div className="px-6 pb-4 space-y-3 border-l-2 border-accent/30 ml-2">
          {/* Full body */}
          <div className="text-sm font-body whitespace-pre-wrap bg-surface rounded-md p-3 border border-border max-h-64 overflow-y-auto">
            {memory.body}
          </div>

          {/* Metadata grid */}
          <dl className="grid grid-cols-3 gap-x-6 gap-y-1 text-xs">
            <div className="flex justify-between">
              <dt className="text-text-muted">Strength</dt>
              <dd className="mono">{memory.strength.toFixed(2)}</dd>
            </div>
            <div className="flex justify-between">
              <dt className="text-text-muted">Importance</dt>
              <dd className="mono">{memory.importance.toFixed(2)}</dd>
            </div>
            <div className="flex justify-between">
              <dt className="text-text-muted">Accesses</dt>
              <dd className="mono">{memory.access_count}</dd>
            </div>
          </dl>

          {/* Tags */}
          {memory.tags.length > 0 && (
            <div className="flex flex-wrap gap-1">
              {memory.tags.map((tag) => (
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
              to={`/editor/${memory.id}` as "/"}
              className="label text-accent hover:underline text-xs"
            >
              Open in editor →
            </Link>
            <button
              onClick={() => select(memory.id)}
              className="label text-accent hover:underline text-xs"
            >
              Inspect
            </button>
            {!invalid && (
              <Button
                variant="ghost"
                className="shrink-0 text-tier-procedural hover:text-tier-procedural text-xs ml-auto"
                onClick={() => onDelete(memory.id)}
              >
                Delete
              </Button>
            )}
          </div>
        </div>
      )}
    </div>
  );
}

export function Knowledge() {
  const [tab, setTab] = useState<Tab>("memories");
  const [memories, setMemories] = useState<Memory[] | null>(null);
  const [corrections, setCorrections] = useState<Memory[] | null>(null);
  const [hardened, setHardened] = useState<Memory[] | null>(null);
  const [search, setSearch] = useState("");
  const [selectedSourceTool, setSelectedSourceTool] = useState<string>("all");
  const [error, setError] = useState<string | null>(null);
  const mounted = useRef(true);

  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);

  useEffect(() => {
    setError(null);
    const run = async () => {
      try {
        const [mems, cors, hard] = await Promise.all([
          client.listMemories({ limit: 100 }),
          client.listCorrections({ hardened: false }),
          client.listCorrections({ hardened: true }),
        ]);
        if (!mounted.current) return;
        setMemories(mems);
        setCorrections(cors);
        setHardened(hard);
      } catch (e) {
        if (mounted.current) {
          setError(e instanceof Error ? e.message : "Failed to load knowledge");
        }
      }
    };
    void run();
  }, []);

  const handleDelete = async (id: string) => {
    try {
      await client.forgetMemory(id);
      if (!mounted.current) return;
      setMemories((prev) => prev?.filter((m) => m.id !== id) ?? null);
      setCorrections((prev) => prev?.filter((m) => m.id !== id) ?? null);
      setHardened((prev) => prev?.filter((m) => m.id !== id) ?? null);
    } catch (e) {
      if (mounted.current) {
        setError(e instanceof Error ? e.message : "Failed to delete item");
      }
    }
  };

  const activeItems: Memory[] | null =
    tab === "memories" ? memories : tab === "corrections" ? corrections : hardened;

  const sourceTools = useMemo(() => {
    const tools = new Set<string>();
    const gather = (list: Memory[] | null) => {
      if (!list) return;
      for (const m of list) {
        if (m.source_tool) {
          tools.add(m.source_tool);
        }
      }
    };
    gather(memories);
    gather(corrections);
    gather(hardened);
    return Array.from(tools).sort();
  }, [memories, corrections, hardened]);

  const filtered = useMemo(() => {
    if (activeItems === null) return null;
    let items = activeItems;
    if (selectedSourceTool !== "all") {
      items = items.filter((m) => m.source_tool === selectedSourceTool);
    }
    if (search.trim() !== "") {
      items = items.filter(
        (m) =>
          m.title.toLowerCase().includes(search.toLowerCase()) ||
          m.body.toLowerCase().includes(search.toLowerCase()),
      );
    }
    return items;
  }, [activeItems, selectedSourceTool, search]);

  const loading = memories === null && corrections === null && hardened === null && !error;

  if (error) {
    return (
      <div className="p-6 max-w-3xl space-y-4">
        <h1 className="display text-xl">Knowledge</h1>
        <p role="alert" className="label text-tier-procedural">
          {error}
        </p>
      </div>
    );
  }

  return (
    <div className="p-6 max-w-3xl space-y-4">
      <h1 className="display text-xl">Knowledge</h1>

      {/* Tab bar */}
      <div className="flex gap-1 border-b border-border">
        {(Object.keys(TAB_LABELS) as Tab[]).map((t) => (
          <button
            key={t}
            onClick={() => setTab(t)}
            className={`px-3 py-1.5 text-sm rounded-t-md transition-colors ${
              tab === t
                ? "bg-surface border border-b-surface border-border text-accent font-body"
                : "text-text-muted hover:text-text font-body"
            }`}
          >
            {TAB_LABELS[t]}
            {t === "memories" && memories !== null && (
              <span className="ml-1.5 label">({memories.length})</span>
            )}
            {t === "corrections" && corrections !== null && (
              <span className="ml-1.5 label">({corrections.length})</span>
            )}
            {t === "hardened" && hardened !== null && (
              <span className="ml-1.5 label">({hardened.length})</span>
            )}
          </button>
        ))}
      </div>

      {/* Filters (Search + Source Tool select) */}
      <div className="flex gap-2">
        <input
          type="search"
          placeholder="Search…"
          className="flex-1 bg-surface border border-border rounded-md px-3 py-1.5 text-sm font-body focus:outline-none focus:ring-1 focus:ring-accent"
          value={search}
          onChange={(e) => setSearch(e.target.value)}
        />
        <select
          value={selectedSourceTool}
          onChange={(e) => setSelectedSourceTool(e.target.value)}
          className="bg-surface border border-border rounded-md px-3 py-1.5 text-sm font-body focus:outline-none focus:ring-1 focus:ring-accent"
        >
          <option value="all">All Sources</option>
          {sourceTools.map((t) => (
            <option key={t} value={t}>
              {t}
            </option>
          ))}
        </select>
      </div>

      {/* Hint */}
      <p className="text-xs text-text-muted">Click any row to expand details, view the full body, and manage the memory.</p>

      {/* Content */}
      {loading ? (
        <Card className="p-4 space-y-2">
          <Skeleton className="h-8 w-full" />
          <Skeleton className="h-8 w-full" />
          <Skeleton className="h-8 w-3/4" />
        </Card>
      ) : filtered !== null && filtered.length === 0 ? (
        <Card className="p-6 text-center">
          <p className="font-body text-text-muted text-sm">No items found.</p>
        </Card>
      ) : (
        <Card className="p-4">
          {filtered?.map((m) => (
            <MemoryRow key={m.id} memory={m} onDelete={(id) => void handleDelete(id)} />
          ))}
        </Card>
      )}
    </div>
  );
}
