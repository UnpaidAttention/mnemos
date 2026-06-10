import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import ForceGraph2D, { ForceGraphMethods } from "react-force-graph-2d";
import type { Graph as GraphData } from "../api/types";

interface Node {
  id: string;
  name: string;
  community_id?: number;
  mentions?: number;
  x?: number;
  y?: number;
}

interface Link {
  source: string | Node;
  target: string | Node;
  weight: number;
}

// Military-tactical palette: teals, cyans, ambers, warm accents
const COMMUNITY_COLORS = [
  "#5EEAD4", "#38BDF8", "#818CF8", "#FBBF24",
  "#34D399", "#FB923C", "#F472B6", "#A78BFA",
];
const BRAND_COLOR = "#3a9d97";

function hashString(str: string): number {
  let hash = 0;
  for (let i = 0; i < str.length; i++) {
    hash = str.charCodeAt(i) + ((hash << 5) - hash);
  }
  return Math.abs(hash);
}

// ── Procedural asteroid shape generation ────────────────────────────
// Generates a deterministic irregular polygon shape for each node,
// seeded by the node's ID hash so it doesn't change on rerender.

interface AsteroidShape {
  vertices: { angle: number; radius: number }[];
  craters: { cx: number; cy: number; r: number }[];
  highlightAngle: number; // angle of specular highlight
}

function generateAsteroidShape(seed: number, baseRadius: number): AsteroidShape {
  // Seeded random — deterministic per node
  let s = seed;
  const rand = () => {
    s = (s * 16807 + 0) % 2147483647;
    return (s & 0x7fffffff) / 0x7fffffff;
  };

  // Generate 10-14 vertices with irregular radial offsets
  const vertexCount = 10 + Math.floor(rand() * 5);
  const vertices: AsteroidShape["vertices"] = [];
  for (let i = 0; i < vertexCount; i++) {
    const angle = (i / vertexCount) * Math.PI * 2;
    // ±25% radial variation for rocky look
    const variation = 0.75 + rand() * 0.5;
    vertices.push({ angle, radius: baseRadius * variation });
  }

  // Generate 2-4 craters (relative to center, within radius)
  const craterCount = 2 + Math.floor(rand() * 3);
  const craters: AsteroidShape["craters"] = [];
  for (let i = 0; i < craterCount; i++) {
    const dist = rand() * baseRadius * 0.5;
    const angle = rand() * Math.PI * 2;
    craters.push({
      cx: Math.cos(angle) * dist,
      cy: Math.sin(angle) * dist,
      r: baseRadius * (0.08 + rand() * 0.12),
    });
  }

  return {
    vertices,
    craters,
    highlightAngle: rand() * Math.PI * 2,
  };
}

// Cache shapes per node to avoid regeneration
const shapeCache = new Map<string, AsteroidShape>();

function getAsteroidShape(nodeId: string, baseRadius: number): AsteroidShape {
  const key = `${nodeId}-${Math.round(baseRadius)}`;
  if (!shapeCache.has(key)) {
    shapeCache.set(key, generateAsteroidShape(hashString(nodeId), baseRadius));
  }
  return shapeCache.get(key)!;
}

// ── Component ────────────────────────────────────────────────────────

interface ForceConfig {
  center: number;
  repel: number;
  link: number;
}

interface GraphCanvasProps {
  data: GraphData;
  pprScores?: Record<string, number>;
  colorByCommunity: boolean;
  onSelect?: (id: string) => void;
  forceConfig?: ForceConfig;
}

export function GraphCanvas({ data, pprScores, colorByCommunity, onSelect, forceConfig }: GraphCanvasProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const fgRef = useRef<ForceGraphMethods>();
  const [dimensions, setDimensions] = useState({ width: 0, height: 0 });
  const [hoverNode, setHoverNode] = useState<string | null>(null);

  const graphData = useMemo(() => ({
    nodes: data.nodes.map(n => ({ ...n })),
    links: data.edges.map(e => ({ source: e.source, target: e.target, weight: e.weight })),
  }), [data]);

  const neighbors = useMemo(() => {
    const map = new Map<string, Set<string>>();
    graphData.nodes.forEach(n => map.set(n.id, new Set()));
    graphData.links.forEach(link => {
      const src = typeof link.source === "object" ? (link.source as Node).id : link.source;
      const tgt = typeof link.target === "object" ? (link.target as Node).id : link.target;
      map.get(src)?.add(tgt);
      map.get(tgt)?.add(src);
    });
    return map;
  }, [graphData]);

  // Configure forces
  useEffect(() => {
    if (!fgRef.current) return;
    const chargeForce = fgRef.current.d3Force("charge");
    if (chargeForce) {
      chargeForce.strength(-10 - ((forceConfig?.repel ?? 50) / 100) * 290);
    }
    const linkForce = fgRef.current.d3Force("link");
    if (linkForce) {
      linkForce.distance(10 + ((forceConfig?.link ?? 50) / 100) * 140);
    }
    fgRef.current.d3Force("centerGravity", (alpha: number) => {
      const strength = ((forceConfig?.center ?? 50) / 100) * 0.3 * alpha;
      const nodes = graphData.nodes as Array<{ x?: number; y?: number; vx?: number; vy?: number }>;
      for (const node of nodes) {
        node.vx = (node.vx ?? 0) - (node.x || 0) * strength;
        node.vy = (node.vy ?? 0) - (node.y || 0) * strength;
      }
    });
    fgRef.current.d3ReheatSimulation();
  }, [graphData, forceConfig]);

  // ResizeObserver
  useEffect(() => {
    if (!containerRef.current) return;
    const observer = new ResizeObserver((entries) => {
      if (entries[0]) {
        const { width, height } = entries[0].contentRect;
        setDimensions({ width, height });
      }
    });
    observer.observe(containerRef.current);
    return () => observer.disconnect();
  }, []);

  const handleNodeClick = useCallback(
    (node: Node) => {
      if (fgRef.current) {
        fgRef.current.centerAt(node.x, node.y, 1000);
        fgRef.current.zoom(4, 2000);
      }
      onSelect?.(node.id);
    },
    [onSelect],
  );

  const maxPpr = Math.max(0.0001, ...Object.values(pprScores ?? {}));

  // ── Asteroid node renderer ──────────────────────────────────────────
  const paintNode = useCallback(
    (node: Node, ctx: CanvasRenderingContext2D, globalScale: number) => {
      const isHovered = node.id === hoverNode;
      const isNeighbor = hoverNode ? neighbors.get(hoverNode)?.has(node.id) : false;
      const isFocused = isHovered || isNeighbor;
      const dimOthers = hoverNode !== null && !isFocused;

      const ppr = pprScores?.[node.id] ?? 0;
      const communityIdx =
        node.community_id != null && node.community_id >= 0
          ? node.community_id
          : hashString(node.id);
      const color = COMMUNITY_COLORS[communityIdx % COMMUNITY_COLORS.length];
      const nodeColor = colorByCommunity ? color : BRAND_COLOR;

      const baseRadius = 6 + (node.mentions ?? 0) * 0.6 + (ppr / maxPpr) * 8;
      const radius = isHovered ? baseRadius * 1.25 : baseRadius;
      const cx = node.x || 0;
      const cy = node.y || 0;

      ctx.globalAlpha = dimOthers ? 0.12 : 1;

      const shape = getAsteroidShape(node.id, radius);

      // ── Draw asteroid body ──────────────────────────────────────
      ctx.beginPath();
      for (let i = 0; i < shape.vertices.length; i++) {
        const v = shape.vertices[i];
        const x = cx + Math.cos(v.angle) * v.radius;
        const y = cy + Math.sin(v.angle) * v.radius;
        if (i === 0) ctx.moveTo(x, y);
        else ctx.lineTo(x, y);
      }
      ctx.closePath();

      // Dark charcoal gradient fill
      const isDark = document.documentElement.getAttribute("data-theme") === "dark";
      const grad = ctx.createRadialGradient(
        cx - radius * 0.3, cy - radius * 0.3, 0,
        cx, cy, radius * 1.2,
      );
      if (isDark) {
        grad.addColorStop(0, "#3a3d44");
        grad.addColorStop(0.5, "#252830");
        grad.addColorStop(1, "#16181e");
      } else {
        grad.addColorStop(0, "#8a8d94");
        grad.addColorStop(0.5, "#6a6d74");
        grad.addColorStop(1, "#4a4d54");
      }
      ctx.fillStyle = grad;
      ctx.fill();

      // ── Edge highlight (specular reflection) ────────────────────
      ctx.save();
      ctx.clip(); // clip to asteroid shape
      const hlX = cx + Math.cos(shape.highlightAngle) * radius * 0.5;
      const hlY = cy + Math.sin(shape.highlightAngle) * radius * 0.5;
      const hlGrad = ctx.createRadialGradient(hlX, hlY, 0, hlX, hlY, radius * 0.7);
      hlGrad.addColorStop(0, "rgba(255, 255, 255, 0.2)");
      hlGrad.addColorStop(1, "rgba(255, 255, 255, 0)");
      ctx.fillStyle = hlGrad;
      ctx.fill();

      // ── Craters ─────────────────────────────────────────────────
      for (const crater of shape.craters) {
        ctx.beginPath();
        ctx.arc(cx + crater.cx, cy + crater.cy, crater.r, 0, Math.PI * 2);
        ctx.fillStyle = isDark ? "rgba(0, 0, 0, 0.3)" : "rgba(0, 0, 0, 0.15)";
        ctx.fill();
      }
      ctx.restore();

      // ── Colored glow rim for community identification ───────────
      ctx.beginPath();
      for (let i = 0; i < shape.vertices.length; i++) {
        const v = shape.vertices[i];
        const x = cx + Math.cos(v.angle) * v.radius;
        const y = cy + Math.sin(v.angle) * v.radius;
        if (i === 0) ctx.moveTo(x, y);
        else ctx.lineTo(x, y);
      }
      ctx.closePath();

      const glowIntensity = isHovered ? 0.7 : isFocused ? 0.5 : 0.25;
      ctx.strokeStyle = nodeColor;
      ctx.lineWidth = isHovered ? 2 / globalScale : 1 / globalScale;
      ctx.globalAlpha = (dimOthers ? 0.12 : 1) * glowIntensity;
      ctx.stroke();

      // ── Inner glow for high-importance or hovered nodes ─────────
      if (isHovered || ppr > maxPpr * 0.5) {
        ctx.globalAlpha = dimOthers ? 0.05 : isHovered ? 0.3 : 0.15;
        const glow = ctx.createRadialGradient(cx, cy, 0, cx, cy, radius * 1.8);
        glow.addColorStop(0, nodeColor);
        glow.addColorStop(1, "rgba(0,0,0,0)");
        ctx.fillStyle = glow;
        ctx.beginPath();
        ctx.arc(cx, cy, radius * 1.8, 0, Math.PI * 2);
        ctx.fill();
      }

      // ── Labels ──────────────────────────────────────────────────
      ctx.globalAlpha = dimOthers ? 0.12 : 1;
      const showLabel =
        isFocused ||
        (globalScale > 1.5 && baseRadius > 5) ||
        (!hoverNode && globalScale > 2);

      if (showLabel) {
        const fontSize = isHovered ? 14 / globalScale : 11 / globalScale;
        ctx.font = `${isHovered ? "600" : "400"} ${fontSize}px "Source Serif 4 Variable", Georgia, serif`;
        ctx.textAlign = "center";
        ctx.textBaseline = "middle";
        ctx.fillStyle = isDark
          ? `rgba(216, 220, 230, ${dimOthers ? 0.12 : 0.9})`
          : `rgba(28, 27, 24, ${dimOthers ? 0.12 : 0.9})`;
        ctx.fillText(node.name, cx, cy + radius + fontSize / 2 + 3);
      }

      ctx.globalAlpha = 1;
    },
    [hoverNode, neighbors, pprScores, colorByCommunity, maxPpr],
  );

  return (
    <div ref={containerRef} className="h-full w-full relative" data-testid="graph-canvas">
      {dimensions.width > 0 && (
        <ForceGraph2D
          // eslint-disable-next-line @typescript-eslint/no-explicit-any
          ref={fgRef as any}
          width={dimensions.width}
          height={dimensions.height}
          graphData={graphData}
          nodeCanvasObject={paintNode}
          nodePointerAreaPaint={(node: Node, color, ctx) => {
            const radius = 6 + (node.mentions ?? 0) * 0.6 + ((pprScores?.[node.id] ?? 0) / maxPpr) * 8;
            ctx.fillStyle = color;
            ctx.beginPath();
            ctx.arc(node.x || 0, node.y || 0, radius * 1.5, 0, 2 * Math.PI, false);
            ctx.fill();
          }}
          onNodeClick={handleNodeClick}
          onNodeHover={(node: Node | null) => setHoverNode(node ? node.id : null)}
          linkColor={(link: Link) => {
            const srcId = typeof link.source === "object" ? (link.source as Node).id : (link.source as string);
            const tgtId = typeof link.target === "object" ? (link.target as Node).id : (link.target as string);
            const dimOthers = hoverNode && srcId !== hoverNode && tgtId !== hoverNode;
            const isDark = document.documentElement.getAttribute("data-theme") === "dark";

            if (dimOthers) return isDark ? "rgba(255,255,255,0.015)" : "rgba(0,0,0,0.02)";

            const isFocused = hoverNode && (srcId === hoverNode || tgtId === hoverNode);
            return isDark
              ? `rgba(94, 234, 212, ${isFocused ? 0.4 : 0.08})`
              : `rgba(31, 111, 107, ${isFocused ? 0.35 : 0.1})`;
          }}
          linkWidth={(link: Link) => {
            const srcId = typeof link.source === "object" ? (link.source as Node).id : (link.source as string);
            const tgtId = typeof link.target === "object" ? (link.target as Node).id : (link.target as string);
            const isFocused = hoverNode && (srcId === hoverNode || tgtId === hoverNode);
            return isFocused ? 1.5 : 0.4;
          }}
          backgroundColor="transparent"
        />
      )}
    </div>
  );
}
