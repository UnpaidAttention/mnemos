import { useState } from "react";
import { Link } from "@tanstack/react-router";
import { useSearch } from "../api/queries";
import { useUiStore } from "../store/ui";
import { RankBars } from "../components/RankBars";
import { Button, Card, Skeleton, TierChip } from "../design/primitives";
import type { SearchReq } from "../api/types";

export function Search() {
  const [draft, setDraft] = useState("");
  const [req, setReq] = useState<SearchReq | null>(null);
  const [graph, setGraph] = useState(true);
  const [global, setGlobal] = useState(false);
  const { data: hits, isLoading, isError } = useSearch(req);
  const select = useUiStore((s) => s.select);

  const run = () => {
    if (!draft.trim()) return;
    setReq({ query: draft.trim(), k: 20, explain: true, graph, global });
  };

  return (
    <div className="p-6 space-y-4">
      <h1 className="display text-xl">Search</h1>

      <div className="flex gap-2">
        <input
          className="flex-1 bg-surface border border-border rounded-md px-3 py-2 font-body focus-visible:outline focus-visible:outline-2 focus-visible:outline-accent"
          placeholder="Search memories…"
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && run()}
          aria-label="search query"
        />
        <Button onClick={run}>Search</Button>
      </div>

      <div className="flex gap-4 label">
        <label className="flex items-center gap-1.5 cursor-pointer">
          <input
            type="checkbox"
            checked={graph}
            onChange={(e) => setGraph(e.target.checked)}
            className="accent-accent"
          />
          graph (PPR)
        </label>
        <label className="flex items-center gap-1.5 cursor-pointer">
          <input
            type="checkbox"
            checked={global}
            onChange={(e) => setGlobal(e.target.checked)}
            className="accent-accent"
          />
          global (communities)
        </label>
      </div>

      {isLoading && <Skeleton className="h-24 w-full" />}

      {isError && (
        <p className="text-tier-procedural text-sm font-body">
          Search failed. Is the daemon running?
        </p>
      )}

      {req && !isLoading && !isError && hits?.length === 0 && (
        <div className="py-8 text-center">
          <p className="text-text-muted font-body">
            No matches for <span className="mono">&ldquo;{req.query}&rdquo;</span>.
          </p>
          <p className="text-text-muted text-sm mt-1">
            Try fewer terms or toggle the graph and global filters.
          </p>
        </div>
      )}

      <div className="space-y-2">
        {hits?.map((h) => {
          const invalid = !!h.memory.invalid_at;
          return (
            <Card
              key={h.memory.id}
              className={`p-3 space-y-2 ${invalid ? "opacity-60 border-dashed" : ""}`}
            >
              <button
                onClick={() => select(h.memory.id)}
                className="flex w-full items-center justify-between text-left"
              >
                <span className={`font-body ${invalid ? "line-through" : ""}`}>
                  {h.memory.title}
                </span>
                <TierChip tier={h.memory.tier} />
              </button>
              <RankBars explain={h.explain} />
              <div className="flex items-center gap-3 border-t border-border/60 pt-2">
                <Link
                  to={`/editor/${h.memory.id}` as "/"}
                  className="label text-accent hover:underline"
                >
                  edit
                </Link>
                <span className="label mono text-[0.65rem] text-text-muted opacity-70">
                  {h.memory.valid_at.slice(0, 10)}
                </span>
              </div>
            </Card>
          );
        })}
      </div>
    </div>
  );
}
