import { useEffect, useRef, useState } from "react";
import { client } from "../api/client";
import { Button, Card, Skeleton } from "../design/primitives";
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
  return (
    <div className="flex items-start justify-between gap-4 py-2 border-b border-border last:border-b-0">
      <div className="min-w-0 flex-1 space-y-0.5">
        <div className="font-body text-sm font-medium truncate">{memory.title}</div>
        <div className="label truncate max-w-prose">{memory.body}</div>
      </div>
      <Button
        variant="ghost"
        className="shrink-0 text-tier-procedural hover:text-tier-procedural"
        onClick={() => onDelete(memory.id)}
      >
        Delete
      </Button>
    </div>
  );
}

export function Knowledge() {
  const [tab, setTab] = useState<Tab>("memories");
  const [memories, setMemories] = useState<Memory[] | null>(null);
  const [corrections, setCorrections] = useState<Memory[] | null>(null);
  const [hardened, setHardened] = useState<Memory[] | null>(null);
  const [search, setSearch] = useState("");
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

  const filtered =
    activeItems === null
      ? null
      : search.trim() === ""
        ? activeItems
        : activeItems.filter(
            (m) =>
              m.title.toLowerCase().includes(search.toLowerCase()) ||
              m.body.toLowerCase().includes(search.toLowerCase()),
          );

  const loading = memories === null && corrections === null && hardened === null && !error;

  return (
    <div className="p-6 max-w-3xl space-y-4">
      <h1 className="display text-xl">Knowledge</h1>

      {error && (
        <p role="alert" className="label text-tier-procedural">
          {error}
        </p>
      )}

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

      {/* Search */}
      <input
        type="search"
        placeholder="Search…"
        className="w-full bg-surface border border-border rounded-md px-3 py-1.5 text-sm font-body focus:outline-none focus:ring-1 focus:ring-accent"
        value={search}
        onChange={(e) => setSearch(e.target.value)}
      />

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
