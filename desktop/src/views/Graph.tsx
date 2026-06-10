import { useState, useCallback, useMemo } from "react";
import { useQuery } from "@tanstack/react-query";
import { useGraph } from "../api/queries";
import { client } from "../api/client";
import { GraphCanvas } from "../components/GraphCanvas";
import { Skeleton } from "../design/primitives";
import { EntityProfile } from "./EntityProfile";
import { Network, GitBranch, Users, Sliders, X } from "lucide-react";

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

  // Graph stats
  const stats = useMemo(() => {
    if (!data) return null;
    const communities = new Set(data.nodes.map((n) => n.community_id).filter((c) => c != null));
    return {
      nodes: data.nodes.length,
      edges: data.edges.length,
      communities: communities.size,
    };
  }, [data]);

  // Community distribution for legend
  const communityGroups = useMemo(() => {
    if (!data) return [];
    const groups = new Map<number, { count: number; sample: string }>();
    for (const node of data.nodes) {
      const cid = node.community_id ?? -1;
      const existing = groups.get(cid);
      if (existing) {
        existing.count++;
      } else {
        groups.set(cid, { count: 1, sample: node.name });
      }
    }
    return Array.from(groups.entries())
      .sort((a, b) => b[1].count - a[1].count)
      .slice(0, 8);
  }, [data]);

  const COMMUNITY_COLORS = [
    "#5EEAD4", "#38BDF8", "#818CF8", "#FBBF24",
    "#34D399", "#FB923C", "#F472B6", "#A78BFA",
  ];

  const getEntityName = useCallback(
    (id: string): string => {
      const node = data?.nodes.find((n) => n.id === id);
      return node?.name ?? id.slice(0, 8);
    },
    [data],
  );

  const handleSelect = useCallback(
    (id: string | null) => {
      if (id === null) {
        setSelectedNode(null);
        setBreadcrumbs([]);
        return;
      }
      setSelectedNode(id);
      setBreadcrumbs([{ id, name: getEntityName(id) }]);
    },
    [getEntityName],
  );

  const handleNavigateEntity = useCallback(
    (id: string) => {
      setSelectedNode(id);
      setBreadcrumbs((prev) => {
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
      {/* ── Main Canvas Region (~70% height) ──────────────────────── */}
      <div className="relative flex-[7] min-h-0 bg-bg overflow-hidden flex">
        <div className="flex-1 relative min-w-0 h-full">
          {isLoading && (
            <div className="p-6 h-full flex flex-col">
              <Skeleton className="flex-1 w-full min-h-64" />
            </div>
          )}
          {isError && (
            <div className="p-8 text-center">
              <p className="display text-lg text-tier-procedural mb-2">Graph unavailable</p>
              <p className="text-sm text-text-muted">
                Could not load the entity graph. Is the daemon running?
              </p>
            </div>
          )}
          {data && data.nodes.length === 0 && (
            <div className="flex h-full items-center justify-center p-8 text-center">
              <div className="max-w-sm">
                <p className="display text-xl mb-3">No entities yet</p>
                <p className="text-text-muted text-sm">
                  The knowledge graph builds as entities and relationships are extracted from
                  your memories. Go to <strong>Pipelines</strong> and click{" "}
                  <strong>Backfill entities</strong> to populate the graph.
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

        {/* ── Entity Inspector Overlay Panel ────────────────────────── */}
        <div
          className={`absolute top-0 right-0 bottom-0 w-[480px] max-w-full glass-panel z-10 flex flex-col shadow-floating transition-transform duration-[240ms] ease-[cubic-bezier(0.22,1,0.36,1)] ${
            selectedNode ? "translate-x-0" : "translate-x-full"
          }`}
        >
          {selectedNode && (
            <>
              <div className="flex flex-col border-b border-border/50 shrink-0">
                <div className="flex items-center justify-between px-4 py-2.5">
                  <span className="label text-text-muted">Entity Inspector</span>
                  <button
                    onClick={handleClose}
                    className="flex items-center gap-1 label hover:text-text transition-colors px-2 py-1 rounded-md hover:bg-surface-raised"
                    aria-label="Close Inspector"
                  >
                    <X size={14} strokeWidth={2} />
                  </button>
                </div>
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
                <EntityProfile id={selectedNode} onNavigateEntity={handleNavigateEntity} />
              </div>
            </>
          )}
        </div>
      </div>

      {/* ── Bottom Panel — Glassmorphic Cards ────────────────────────── */}
      <div className="flex-[3] min-h-0 border-t border-border bg-surface-sunken p-3 overflow-hidden">
        <div className="h-full flex gap-3">
          {/* Card 1: Graph Stats */}
          <div className="card-glass flex-1 p-4 flex flex-col min-w-0">
            <div className="flex items-center gap-2 mb-3">
              <Network size={14} strokeWidth={2} className="text-accent shrink-0" />
              <span className="label">Graph Overview</span>
            </div>
            {stats ? (
              <div className="flex-1 flex flex-col justify-center gap-3">
                <div className="flex items-baseline justify-between">
                  <span className="text-xs text-text-muted">Entities</span>
                  <span className="display text-2xl text-text">{stats.nodes}</span>
                </div>
                <div className="flex items-baseline justify-between">
                  <span className="text-xs text-text-muted">Edges</span>
                  <span className="display text-2xl text-text">{stats.edges}</span>
                </div>
                <div className="flex items-baseline justify-between">
                  <span className="text-xs text-text-muted">Communities</span>
                  <span className="display text-2xl text-accent">{stats.communities}</span>
                </div>
              </div>
            ) : (
              <div className="flex-1 flex items-center justify-center text-text-dim text-xs">
                Loading…
              </div>
            )}
          </div>

          {/* Card 2: Community Legend */}
          <div className="card-glass flex-1 p-4 flex flex-col min-w-0">
            <div className="flex items-center gap-2 mb-3">
              <Users size={14} strokeWidth={2} className="text-accent shrink-0" />
              <span className="label">Communities</span>
            </div>
            <div className="flex-1 overflow-y-auto space-y-1.5">
              {communityGroups.map(([cid, info]) => (
                <div key={cid} className="flex items-center gap-2 text-xs">
                  <span
                    className="w-2.5 h-2.5 rounded-full shrink-0"
                    style={{
                      background:
                        cid >= 0
                          ? COMMUNITY_COLORS[cid % COMMUNITY_COLORS.length]
                          : "#4a5264",
                    }}
                  />
                  <span className="text-text truncate flex-1">{info.sample}</span>
                  <span className="text-text-dim mono shrink-0">{info.count}</span>
                </div>
              ))}
              {communityGroups.length === 0 && (
                <div className="text-text-dim text-xs text-center py-4">
                  No communities detected
                </div>
              )}
            </div>
          </div>

          {/* Card 3: PPR Query */}
          <div className="card-glass flex-1 p-4 flex flex-col min-w-0">
            <div className="flex items-center gap-2 mb-3">
              <GitBranch size={14} strokeWidth={2} className="text-accent shrink-0" />
              <span className="label">Query Highlight (PPR)</span>
            </div>
            <div className="flex-1 flex flex-col gap-2">
              <input
                className="bg-bg border border-border rounded-lg px-3 py-1.5 text-sm w-full
                           focus-visible:outline focus-visible:outline-2 focus-visible:outline-accent"
                placeholder="Highlight by query…"
                value={q}
                aria-label="highlight by query"
                onChange={(e) => setQ(e.target.value)}
                onKeyDown={(e) => e.key === "Enter" && setActiveQuery(q.trim() || null)}
              />
              <label className="flex items-center gap-1.5 text-xs text-text-muted cursor-pointer">
                <input
                  type="checkbox"
                  checked={byCommunity}
                  onChange={(e) => setByCommunity(e.target.checked)}
                  className="accent-accent"
                />
                Community colors
              </label>
              {activeQuery && (
                <div className="flex items-center gap-2">
                  <span className="text-xs text-accent mono truncate flex-1">
                    PPR: {activeQuery}
                  </span>
                  <button
                    className="text-xs text-text-muted hover:text-text"
                    onClick={() => {
                      setActiveQuery(null);
                      setQ("");
                    }}
                  >
                    Clear
                  </button>
                </div>
              )}
            </div>
          </div>

          {/* Card 4: Force Controls */}
          <div className="card-glass flex-1 p-4 flex flex-col min-w-0">
            <div className="flex items-center gap-2 mb-3">
              <Sliders size={14} strokeWidth={2} className="text-accent shrink-0" />
              <span className="label">Display / Forces</span>
            </div>
            <div className="flex-1 flex flex-col justify-center gap-3">
              <div className="space-y-1">
                <div className="flex justify-between text-xs">
                  <span className="text-text-muted">Center</span>
                  <span className="mono text-text-dim">{forceConfig.center}</span>
                </div>
                <input
                  type="range" min="0" max="100" step="1"
                  value={forceConfig.center}
                  onChange={(e) => setForceConfig((p) => ({ ...p, center: +e.target.value }))}
                  className="w-full accent-accent"
                />
              </div>
              <div className="space-y-1">
                <div className="flex justify-between text-xs">
                  <span className="text-text-muted">Repel</span>
                  <span className="mono text-text-dim">{forceConfig.repel}</span>
                </div>
                <input
                  type="range" min="0" max="100" step="1"
                  value={forceConfig.repel}
                  onChange={(e) => setForceConfig((p) => ({ ...p, repel: +e.target.value }))}
                  className="w-full accent-accent"
                />
              </div>
              <div className="space-y-1">
                <div className="flex justify-between text-xs">
                  <span className="text-text-muted">Link distance</span>
                  <span className="mono text-text-dim">{forceConfig.link}</span>
                </div>
                <input
                  type="range" min="0" max="100" step="1"
                  value={forceConfig.link}
                  onChange={(e) => setForceConfig((p) => ({ ...p, link: +e.target.value }))}
                  className="w-full accent-accent"
                />
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}
