import { useEffect, useRef } from "react";
import Graph from "graphology";
import Sigma from "sigma";
import forceAtlas2 from "graphology-layout-forceatlas2";
import type { Graph as GraphData } from "../api/types";

// Community colors drawn from tier palette — never purple/rainbow
const COMMUNITY_COLORS = ["#1F6F6B", "#C77D33", "#A6432E", "#6E8B6A", "#5B6168"];
const KIND_COLOR = "#5B6168";

interface GraphCanvasProps {
  data: GraphData;
  pprScores?: Record<string, number>;
  colorByCommunity: boolean;
  onSelect?: (id: string) => void;
}

export function GraphCanvas({ data, pprScores, colorByCommunity, onSelect }: GraphCanvasProps) {
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!ref.current) return;
    const g = new Graph({ multi: true, type: "directed" });
    const maxPpr = Math.max(0.0001, ...Object.values(pprScores ?? {}));

    data.nodes.forEach((n, i) => {
      const ppr = pprScores?.[n.id] ?? 0;
      const communityIdx = n.community_id != null && n.community_id >= 0 ? n.community_id : -1;
      const color =
        colorByCommunity && communityIdx >= 0
          ? COMMUNITY_COLORS[communityIdx % COMMUNITY_COLORS.length]
          : KIND_COLOR;
      g.mergeNode(n.id, {
        label: n.name,
        x: Math.cos((i / Math.max(1, data.nodes.length)) * 2 * Math.PI),
        y: Math.sin((i / Math.max(1, data.nodes.length)) * 2 * Math.PI),
        size: 4 + (n.mentions ?? 0) * 1.5 + (ppr / maxPpr) * 14,
        color: pprScores && ppr > 0 ? "#1F6F6B" : color,
      });
    });

    data.edges.forEach((e) => {
      if (g.hasNode(e.source) && g.hasNode(e.target)) {
        g.addEdge(e.source, e.target, { size: Math.max(1, e.weight) });
      }
    });

    if (g.order > 1) {
      forceAtlas2.assign(g, {
        iterations: 120,
        settings: forceAtlas2.inferSettings(g),
      });
    }

    const sigma = new Sigma(g, ref.current, { renderEdgeLabels: false });
    if (onSelect) {
      sigma.on("clickNode", ({ node }: { node: string }) => onSelect(node));
    }
    return () => sigma.kill();
  }, [data, pprScores, colorByCommunity, onSelect]);

  return <div ref={ref} className="h-full w-full" data-testid="graph-canvas" />;
}
