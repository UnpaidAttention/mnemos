import { useState, useCallback } from "react";
import { useQuery } from "@tanstack/react-query";
import { useGraph } from "../api/queries";
import { client } from "../api/client";
import { GraphCanvas } from "../components/GraphCanvas";
import { Skeleton } from "../design/primitives";
import { EntityProfile } from "./EntityProfile";

interface BreadcrumbEntry {
  id: string;
  name: string;
}

export function Graph() {
  const { data, isLoading, isError } = useGraph();
  const [q, setQ] = useState("");
  const [activeQuery, setActiveQuery] = useState<string | null>(null);
  const [byCommunity, setByCommunity] = useState(true);
  const [selectedNode, setSelectedNode] = useState<string | null>(null);
  const [breadcrumbs, setBreadcrumbs] = useState<BreadcrumbEntry[]>([]);
  
  const [forceConfig, setForceConfig] = useState({
    center: 50,
    repel: 50,
    link: 50,
  });

  const { data: pprScores } = useQuery({
    queryKey: ["graph-ppr", activeQuery],
    queryFn: () => client.graphPpr(activeQuery!),
    enabled: !!activeQuery,
  });

  // Find entity name from graph data for breadcrumbs
  const getEntityName = useCallback(
    (id: string): string => {
      const node = data?.nodes.find((n) => n.id === id);
      return node?.name ?? id.slice(0, 8);
    },
    [data],
  );

  const handleSelect = useCallback(
    (id: string) => {
      setSelectedNode(id);
      setBreadcrumbs([{ id, name: getEntityName(id) }]);
    },
    [getEntityName],
  );

  const handleNavigateEntity = useCallback(
    (id: string) => {
      setSelectedNode(id);
      setBreadcrumbs((prev) => {
        // If navigating back to an entity already in the trail, truncate
        const idx = prev.findIndex((b) => b.id === id);
        if (idx >= 0) return prev.slice(0, idx + 1);
        return [...prev, { id, name: getEntityName(id) }];
      });
    },
    [getEntityName],
  );

  const handleClose = useCallback(() => {
    setSelectedNode(null);
    setBreadcrumbs([]);
  }, []);

  return (
    <div className="flex h-full flex-col overflow-hidden relative">
      {/* Controls bar */}
      <div className="flex items-center gap-3 border-b border-border bg-surface px-4 py-3 shrink-0 z-20 relative">
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
      <div className="relative min-h-0 flex-1 bg-bg overflow-hidden flex">
        <div className="flex-1 relative min-w-0 h-full">
          {isLoading && (
            <div className="p-6 h-full flex flex-col">
              <Skeleton className="flex-1 w-full min-h-64" />
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
                  The knowledge graph builds as entities and relationships are
                  extracted from your memories. Go to the{" "}
                  <strong>Pipelines</strong> tab and click{" "}
                  <strong>Backfill entities</strong> to populate the graph from
                  existing memories, or use MCP-connected tools to create new
                  conversations.
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
              forceConfig={forceConfig}
            />
          )}
        </div>

        {/* Settings Overlay - Matches Obsidian controls */}
        {!selectedNode && (
          <div className="absolute top-4 right-4 w-64 glass-panel border border-border/50 shadow-floating z-10 flex flex-col p-4 space-y-6">
            <div>
              <h3 className="label text-text-muted mb-4 border-b border-border/50 pb-2">Display / Forces</h3>
              
              <div className="space-y-4">
                <div className="space-y-1.5">
                  <div className="flex justify-between">
                    <label className="text-xs text-text">Center force</label>
                  </div>
                  <input 
                    type="range" min="0" max="100" step="1" 
                    value={forceConfig.center} 
                    onChange={e => setForceConfig(p => ({...p, center: parseFloat(e.target.value)}))}
                    className="w-full accent-accent" 
                  />
                </div>
                
                <div className="space-y-1.5">
                  <div className="flex justify-between">
                    <label className="text-xs text-text">Repel force</label>
                  </div>
                  <input 
                    type="range" min="0" max="100" step="1" 
                    value={forceConfig.repel} 
                    onChange={e => setForceConfig(p => ({...p, repel: parseFloat(e.target.value)}))}
                    className="w-full accent-accent" 
                  />
                </div>
                
                <div className="space-y-1.5">
                  <div className="flex justify-between">
                    <label className="text-xs text-text">Link distance</label>
                  </div>
                  <input 
                    type="range" min="0" max="100" step="1" 
                    value={forceConfig.link} 
                    onChange={e => setForceConfig(p => ({...p, link: parseFloat(e.target.value)}))}
                    className="w-full accent-accent" 
                  />
                </div>
              </div>
            </div>
          </div>
        )}

        {/* Overlay Side Panel */}
        <div 
          className={`absolute top-0 right-0 bottom-0 w-[480px] max-w-full glass-panel z-10 flex flex-col shadow-floating transition-transform duration-[240ms] ease-[cubic-bezier(0.22,1,0.36,1)] ${
            selectedNode ? "translate-x-0" : "translate-x-full"
          }`}
        >
          {selectedNode && (
            <>
              {/* Panel header with breadcrumbs */}
              <div className="flex flex-col border-b border-border/50 shrink-0">
                <div className="flex items-center justify-between px-4 py-2.5">
                  <span className="label text-text-muted">Entity Inspector</span>
                  <button 
                    onClick={handleClose} 
                    className="label hover:text-text transition-colors px-2 py-1"
                    aria-label="Close Inspector"
                  >
                    Close ✕
                  </button>
                </div>
                {/* Breadcrumb trail */}
                {breadcrumbs.length > 1 && (
                  <div className="flex items-center gap-1 px-4 pb-2 overflow-x-auto text-xs">
                    {breadcrumbs.map((b, i) => (
                      <span key={b.id} className="flex items-center gap-1 shrink-0">
                        {i > 0 && <span className="text-text-muted">›</span>}
                        {i < breadcrumbs.length - 1 ? (
                          <button
                            className="text-accent hover:underline font-body"
                            onClick={() => handleNavigateEntity(b.id)}
                          >
                            {b.name}
                          </button>
                        ) : (
                          <span className="text-text font-body font-medium">{b.name}</span>
                        )}
                      </span>
                    ))}
                  </div>
                )}
              </div>
              <div className="flex-1 overflow-y-auto">
                <EntityProfile
                  id={selectedNode}
                  onNavigateEntity={handleNavigateEntity}
                />
              </div>
            </>
          )}
        </div>
      </div>
    </div>
  );
}
