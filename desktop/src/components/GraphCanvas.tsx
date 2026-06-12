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

// Helper functions for color conversion and manipulation
function hexToRgb(hex: string): { r: number; g: number; b: number } | null {
  const result = /^#?([a-f\d]{2})([a-f\d]{2})([a-f\d]{2})$/i.exec(hex);
  return result
    ? {
        r: parseInt(result[1], 16),
        g: parseInt(result[2], 16),
        b: parseInt(result[3], 16),
      }
    : null;
}

function hexToRgba(hex: string, alpha: number): string {
  const rgb = hexToRgb(hex);
  if (!rgb) return `rgba(58, 157, 151, ${alpha})`;
  return `rgba(${rgb.r}, ${rgb.g}, ${rgb.b}, ${alpha})`;
}

// ── Procedural shape generation ────────────────────────────────────

interface Facet {
  p1: { x: number; y: number };
  p2: { x: number; y: number };
  p3: { x: number; y: number };
  brightness: number; // 0 to 1 shade multiplier
}

interface GlowingCrack {
  points: { x: number; y: number }[];
  brightness: number;
}

interface ObsidianShape {
  vertices: { x: number; y: number }[];
  facets: Facet[];
  cracks: GlowingCrack[];
}

function generateObsidianShape(seed: number, baseRadius: number): ObsidianShape {
  let s = seed;
  const rand = () => {
    s = (s * 16807 + 0) % 2147483647;
    return (s & 0x7fffffff) / 0x7fffffff;
  };

  // Sharp, angular shard look: 6 to 9 vertices
  const vertexCount = 6 + Math.floor(rand() * 4);
  const outerVertices: { x: number; y: number }[] = [];
  const lightAngle = -Math.PI * 0.75; // light source from top-left

  for (let i = 0; i < vertexCount; i++) {
    const angle = (i / vertexCount) * Math.PI * 2;
    // High variation (0.55 to 1.1) to create highly irregular crystal shapes
    const r = baseRadius * (0.55 + rand() * 0.55);
    outerVertices.push({
      x: Math.cos(angle) * r,
      y: Math.sin(angle) * r,
    });
  }

  // Offset center hub for asymmetric 3D crystal look
  const hubOffsetDist = baseRadius * 0.28 * rand();
  const hubOffsetAngle = rand() * Math.PI * 2;
  const hub = {
    x: Math.cos(hubOffsetAngle) * hubOffsetDist,
    y: Math.sin(hubOffsetAngle) * hubOffsetDist,
  };

  const facets: Facet[] = [];
  for (let i = 0; i < vertexCount; i++) {
    const p1 = hub;
    const p2 = outerVertices[i];
    const p3 = outerVertices[(i + 1) % vertexCount];

    // Midpoint of the facet triangle
    const midX = (p1.x + p2.x + p3.x) / 3;
    const midY = (p1.y + p2.y + p3.y) / 3;
    const angleToMid = Math.atan2(midY, midX);

    // Light alignment
    const diff = angleToMid - lightAngle;
    const alignment = Math.cos(diff);
    
    // Specular highlight facets
    const isSpecular = rand() > 0.6 && alignment > 0.25;
    let brightness = 0.2 + (alignment + 1) * 0.35; // range [0.2, 0.9]
    if (isSpecular) {
      brightness = 0.98;
    }

    facets.push({ p1, p2, p3, brightness });
  }

  const cracks: GlowingCrack[] = [];
  // Only add energy cracks for larger nodes (more importance/mentions)
  if (baseRadius > 7) {
    const crackCount = 1 + Math.floor(rand() * 2); // 1 or 2 cracks
    for (let c = 0; c < crackCount; c++) {
      const targetIdx = Math.floor(rand() * vertexCount);
      const target = outerVertices[targetIdx];
      const points = [hub];
      
      const segmentCount = 2 + Math.floor(rand() * 2);
      for (let sc = 1; sc < segmentCount; sc++) {
        const t = sc / segmentCount;
        const lx = hub.x + (target.x - hub.x) * t;
        const ly = hub.y + (target.y - hub.y) * t;
        const perpAngle = Math.atan2(target.y - hub.y, target.x - hub.x) + Math.PI / 2;
        const jitterDist = baseRadius * 0.16 * (rand() - 0.5);
        points.push({
          x: lx + Math.cos(perpAngle) * jitterDist,
          y: ly + Math.sin(perpAngle) * jitterDist,
        });
      }
      points.push(target);
      cracks.push({ points, brightness: 0.7 + rand() * 0.3 });
    }
  }

  return {
    vertices: outerVertices,
    facets,
    cracks,
  };
}

// Cache shapes per node to avoid regeneration
const shapeCache = new Map<string, ObsidianShape>();

function getObsidianShape(nodeId: string, baseRadius: number): ObsidianShape {
  const key = `${nodeId}-${Math.round(baseRadius)}`;
  if (!shapeCache.has(key)) {
    shapeCache.set(key, generateObsidianShape(hashString(nodeId), baseRadius));
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
  onSelect?: (id: string | null) => void;
  forceConfig?: ForceConfig;
}

export function GraphCanvas({ data, pprScores, colorByCommunity, onSelect, forceConfig }: GraphCanvasProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const fgRef = useRef<ForceGraphMethods>();
  const [dimensions, setDimensions] = useState({ width: 0, height: 0 });
  const [hoverNode, setHoverNode] = useState<string | null>(null);
  const [hoverPos, setHoverPos] = useState<{ x: number; y: number } | null>(null);

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

  const [focusedNode, setFocusedNode] = useState<string | null>(null);

  const handleNodeClick = useCallback(
    (node: Node) => {
      if (focusedNode === node.id) {
        // Second click on same node → zoom back out to fit all
        if (fgRef.current) {
          fgRef.current.zoomToFit(600, 60);
        }
        setFocusedNode(null);
        onSelect?.(null);
      } else {
        // First click → zoom in
        if (fgRef.current) {
          fgRef.current.centerAt(node.x, node.y, 1000);
          fgRef.current.zoom(4, 2000);
        }
        setFocusedNode(node.id);
        onSelect?.(node.id);
      }
    },
    [onSelect, focusedNode],
  );

  const maxPpr = Math.max(0.0001, ...Object.values(pprScores ?? {}));

  // ── High-Fidelity Crystalline Obsidian Node Renderer ─────────────────
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

      const shape = getObsidianShape(node.id, radius);
      const isDark = document.documentElement.getAttribute("data-theme") === "dark";

      // ── Render Faceted Obsidian Shard ──
      ctx.save();
      
      // 1. Draw outer boundary and clip
      ctx.beginPath();
      for (let i = 0; i < shape.vertices.length; i++) {
        const v = shape.vertices[i];
        if (i === 0) ctx.moveTo(cx + v.x, cy + v.y);
        else ctx.lineTo(cx + v.x, cy + v.y);
      }
      ctx.closePath();
      ctx.clip();

      // 2. Draw glossy, linear-shaded facets
      for (const facet of shape.facets) {
        ctx.beginPath();
        ctx.moveTo(cx + facet.p1.x, cy + facet.p1.y);
        ctx.lineTo(cx + facet.p2.x, cy + facet.p2.y);
        ctx.lineTo(cx + facet.p3.x, cy + facet.p3.y);
        ctx.closePath();

        const baseColor = hexToRgb(nodeColor) || { r: 58, g: 157, b: 151 };
        
        // Solid dark blend factor for Obsidian base stone
        const blendFactor = isDark ? 0.82 : 0.65;
        const bgR = isDark ? 14 : 200;
        const bgG = isDark ? 16 : 204;
        const bgB = isDark ? 20 : 210;

        const rBase = bgR * blendFactor + baseColor.r * (1 - blendFactor);
        const gBase = bgG * blendFactor + baseColor.g * (1 - blendFactor);
        const bBase = bgB * blendFactor + baseColor.b * (1 - blendFactor);

        const fGrad = ctx.createLinearGradient(
          cx + facet.p1.x, cy + facet.p1.y,
          cx + (facet.p2.x + facet.p3.x) / 2, cy + (facet.p2.y + facet.p3.y) / 2
        );
        
        const bStart = Math.min(1.0, facet.brightness * 1.35);
        const bEnd = Math.max(0.1, facet.brightness * 0.55);

        fGrad.addColorStop(0, `rgb(${Math.round(rBase * bStart)}, ${Math.round(gBase * bStart)}, ${Math.round(bBase * bStart)})`);
        fGrad.addColorStop(1, `rgb(${Math.round(rBase * bEnd)}, ${Math.round(gBase * bEnd)}, ${Math.round(bBase * bEnd)})`);

        ctx.fillStyle = fGrad;
        ctx.fill();
      }

      // 3. Global Specular Shine Sweep
      const hlGrad = ctx.createLinearGradient(cx - radius * 0.8, cy - radius * 0.8, cx + radius * 0.5, cy + radius * 0.5);
      hlGrad.addColorStop(0, isDark ? "rgba(255, 255, 255, 0.35)" : "rgba(255, 255, 255, 0.45)");
      hlGrad.addColorStop(0.2, "rgba(255, 255, 255, 0.12)");
      hlGrad.addColorStop(0.5, "rgba(255, 255, 255, 0)");
      hlGrad.addColorStop(1, "rgba(0, 0, 0, 0.45)");
      ctx.fillStyle = hlGrad;
      ctx.beginPath();
      for (let i = 0; i < shape.vertices.length; i++) {
        const v = shape.vertices[i];
        if (i === 0) ctx.moveTo(cx + v.x, cy + v.y);
        else ctx.lineTo(cx + v.x, cy + v.y);
      }
      ctx.closePath();
      ctx.fill();

      // 4. Glowing magma/energy cracks (Neon core rendering)
      for (const crack of shape.cracks) {
        // Pass 1: Soft Outer Glow
        ctx.save();
        ctx.shadowColor = nodeColor;
        ctx.shadowBlur = 12 / globalScale;
        ctx.strokeStyle = hexToRgba(nodeColor, 0.85);
        ctx.lineWidth = 4 / globalScale;
        ctx.lineCap = "round";
        ctx.lineJoin = "round";
        ctx.beginPath();
        for (let i = 0; i < crack.points.length; i++) {
          const pt = crack.points[i];
          if (i === 0) ctx.moveTo(cx + pt.x, cy + pt.y);
          else ctx.lineTo(cx + pt.x, cy + pt.y);
        }
        ctx.stroke();
        ctx.restore();

        // Pass 2: Hot Inner Core (White core)
        ctx.save();
        ctx.strokeStyle = isDark ? "#ffffff" : "#fff8e1";
        ctx.lineWidth = 1.2 / globalScale;
        ctx.lineCap = "round";
        ctx.lineJoin = "round";
        ctx.beginPath();
        for (let i = 0; i < crack.points.length; i++) {
          const pt = crack.points[i];
          if (i === 0) ctx.moveTo(cx + pt.x, cy + pt.y);
          else ctx.lineTo(cx + pt.x, cy + pt.y);
        }
        ctx.stroke();
        ctx.restore();
      }

      ctx.restore(); // restore clipping

      // 5. Crisp edge highlights facing top-left light source
      const lightDir = { x: Math.cos(-Math.PI * 0.75), y: Math.sin(-Math.PI * 0.75) };
      for (let i = 0; i < shape.vertices.length; i++) {
        const v1 = shape.vertices[i];
        const v2 = shape.vertices[(i + 1) % shape.vertices.length];
        
        const dx = v2.x - v1.x;
        const dy = v2.y - v1.y;
        const len = Math.sqrt(dx*dx + dy*dy) || 1;
        const nx = -dy / len;
        const ny = dx / len;
        
        const dot = nx * lightDir.x + ny * lightDir.y;
        if (dot > 0.25) {
          ctx.beginPath();
          ctx.moveTo(cx + v1.x, cy + v1.y);
          ctx.lineTo(cx + v2.x, cy + v2.y);
          ctx.strokeStyle = `rgba(255, 255, 255, ${0.48 * dot})`;
          ctx.lineWidth = 1.6 / globalScale;
          ctx.stroke();
        }
      }

      // Outer outline matching brand/community color
      ctx.beginPath();
      for (let i = 0; i < shape.vertices.length; i++) {
        const v = shape.vertices[i];
        if (i === 0) ctx.moveTo(cx + v.x, cy + v.y);
        else ctx.lineTo(cx + v.x, cy + v.y);
      }
      ctx.closePath();
      ctx.strokeStyle = hexToRgba(nodeColor, isHovered ? 0.95 : 0.45);
      ctx.lineWidth = isHovered ? 2.2 / globalScale : 0.9 / globalScale;
      ctx.stroke();

      ctx.globalAlpha = dimOthers ? 0.12 : 1;

      // ── Outer glow aura (always visible in dark mode) ────────────
      if (isDark && !dimOthers) {
        const auraAlpha = isHovered ? 0.25 : isFocused ? 0.15 : 0.08;
        const aura = ctx.createRadialGradient(cx, cy, radius * 0.8, cx, cy, radius * 2.2);
        aura.addColorStop(0, nodeColor);
        aura.addColorStop(1, "rgba(0,0,0,0)");
        ctx.globalAlpha = auraAlpha;
        ctx.fillStyle = aura;
        ctx.beginPath();
        ctx.arc(cx, cy, radius * 2.2, 0, Math.PI * 2);
        ctx.fill();
      }

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
        isHovered ||
        (!hoverNode && globalScale > 2) ||
        (!hoverNode && globalScale > 1.5 && baseRadius > 5);

      if (showLabel) {
        const fontSize = isHovered ? 14 / globalScale : 11 / globalScale;
        ctx.font = `${isHovered ? "600" : "400"} ${fontSize}px "Source Serif 4 Variable", Georgia, serif`;
        ctx.textAlign = "center";
        ctx.textBaseline = "middle";
        ctx.fillStyle = isDark
          ? `rgba(235, 240, 250, ${dimOthers ? 0.12 : 1})`
          : `rgba(28, 27, 24, ${dimOthers ? 0.12 : 0.9})`;
        ctx.fillText(node.name, cx, cy + radius + fontSize / 2 + 3);
      }

      ctx.globalAlpha = 1;
    },
    [hoverNode, neighbors, pprScores, colorByCommunity, maxPpr],
  );

  // Build tooltip data when hovering
  const tooltipData = useMemo(() => {
    if (!hoverNode) return null;
    const node = graphData.nodes.find((n) => n.id === hoverNode);
    if (!node) return null;
    const neighborIds = neighbors.get(hoverNode);
    if (!neighborIds) return { name: node.name, connections: [] };
    const connections = Array.from(neighborIds)
      .map((nid) => {
        const n = graphData.nodes.find((nd) => nd.id === nid);
        return n ? n.name : nid;
      })
      .sort((a, b) => a.localeCompare(b));
    return { name: node.name, connections };
  }, [hoverNode, graphData.nodes, neighbors]);

  return (
    <div
      ref={containerRef}
      className="h-full w-full relative"
      data-testid="graph-canvas"
      onMouseMove={(e) => {
        if (!containerRef.current) return;
        const rect = containerRef.current.getBoundingClientRect();
        setHoverPos({ x: e.clientX - rect.left, y: e.clientY - rect.top });
      }}
    >
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
          onNodeHover={(node: Node | null) => {
            setHoverNode(node ? node.id : null);
            if (!node) setHoverPos(null);
          }}
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

      {/* ── Floating Hover Tooltip ─────────────────────────────────── */}
      {tooltipData && hoverPos && (
        <div
          className="absolute z-20 pointer-events-none"
          style={{
            left: Math.min(hoverPos.x + 16, dimensions.width - 280),
            top: Math.min(hoverPos.y - 12, dimensions.height - 200),
          }}
        >
          <div className="bg-surface/95 backdrop-blur-sm border border-border rounded-lg shadow-floating px-3 py-2.5 w-64">
            <p className="display text-sm text-accent truncate mb-1">
              {tooltipData.name}
            </p>
            {tooltipData.connections.length > 0 ? (
              <>
                <p className="label text-[0.6rem] text-text-muted mb-1">
                  {tooltipData.connections.length} connection{tooltipData.connections.length !== 1 ? "s" : ""}
                </p>
                <ul className="space-y-0.5">
                  {tooltipData.connections.slice(0, 8).map((name) => (
                    <li key={name} className="text-xs font-body text-text truncate">
                      <span className="text-text-muted mr-1">·</span>{name}
                    </li>
                  ))}
                  {tooltipData.connections.length > 8 && (
                    <li className="text-xs font-body text-text-muted italic">
                      +{tooltipData.connections.length - 8} more
                    </li>
                  )}
                </ul>
              </>
            ) : (
              <p className="text-xs text-text-muted font-body">No connections</p>
            )}
            <p className="text-[0.6rem] text-text-muted mt-1.5 border-t border-border/50 pt-1">
              Click node to inspect all
            </p>
          </div>
        </div>
      )}
    </div>
  );
}
