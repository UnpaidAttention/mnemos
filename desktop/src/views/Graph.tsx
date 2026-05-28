import { useState } from "react";
import { useNavigate } from "@tanstack/react-router";
import { useQuery } from "@tanstack/react-query";
import { useGraph } from "../api/queries";
import { client } from "../api/client";
import { GraphCanvas } from "../components/GraphCanvas";
import { Skeleton } from "../design/primitives";

export function Graph() {
  const { data, isLoading, isError } = useGraph();
  const [q, setQ] = useState("");
  const [activeQuery, setActiveQuery] = useState<string | null>(null);
  const [byCommunity, setByCommunity] = useState(true);
  const navigate = useNavigate();

  const { data: pprScores } = useQuery({
    queryKey: ["graph-ppr", activeQuery],
    queryFn: () => client.graphPpr(activeQuery!),
    enabled: !!activeQuery,
  });

  const handleSelect = (id: string) => {
    // Router type-tree only infers "/" — build string path to avoid typecheck failure
    const path: string = `/entity/${id}`;
    void navigate({ to: path });
  };

  return (
    <div className="flex h-full flex-col">
      {/* Controls bar */}
      <div className="flex items-center gap-3 border-b border-border bg-surface px-4 py-3 shrink-0">
        <input
          className="bg-bg border border-border rounded-md px-3 py-1.5 font-body text-sm w-72
                     focus-visible:outline focus-visible:outline-2 focus-visible:outline-accent"
          placeholder="Highlight by query (PPR)…"
          value={q}
          aria-label="highlight by query"
          onChange={(e) => setQ(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && setActiveQuery(q.trim() || null)}
        />
        <label className="label flex items-center gap-1.5 cursor-pointer">
          <input
            type="checkbox"
            checked={byCommunity}
            onChange={(e) => setByCommunity(e.target.checked)}
            aria-label="community colors"
            className="accent-accent"
          />
          community colors
        </label>
        {activeQuery && (
          <button
            className="label text-accent hover:underline"
            onClick={() => { setActiveQuery(null); setQ(""); }}
          >
            clear overlay
          </button>
        )}
        {activeQuery && (
          <span className="label text-text-muted">PPR: {activeQuery}</span>
        )}
      </div>

      {/* Canvas region */}
      <div className="relative min-h-0 flex-1 bg-bg">
        {isLoading && (
          <div className="p-6">
            <Skeleton className="h-full w-full min-h-64" />
          </div>
        )}
        {isError && (
          <div className="p-8 text-center">
            <p className="display text-lg text-tier-procedural mb-2">Graph unavailable</p>
            <p className="text-sm text-text-muted">Could not load the entity graph. Is the daemon running?</p>
          </div>
        )}
        {data && data.nodes.length === 0 && (
          <div className="flex h-full items-center justify-center p-8 text-center">
            <div className="max-w-sm">
              <p className="display text-xl mb-3">No entities yet</p>
              <p className="text-text-muted text-sm">
                Entities form as the learning pipeline links memories.
                Add some memories and run the pipeline to populate the graph.
              </p>
            </div>
          </div>
        )}
        {data && data.nodes.length > 0 && (
          <GraphCanvas
            data={data}
            pprScores={pprScores}
            colorByCommunity={byCommunity}
            onSelect={handleSelect}
          />
        )}
      </div>
    </div>
  );
}
