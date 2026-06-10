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

// Obsidian exact vibrant palette for the starfield
const COMMUNITY_COLORS = ["#5EEAD4", "#38BDF8", "#818CF8", "#C084FC", "#F472B6", "#FB923C", "#FBBF24", "#34D399"];
const KIND_COLOR = "#818CF8"; 

// Simple hash for assigning consistent colors to nodes without a community
function hashString(str: string): number {
  let hash = 0;
  for (let i = 0; i < str.length; i++) {
    hash = str.charCodeAt(i) + ((hash << 5) - hash);
  }
  return Math.abs(hash);
}

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

  // Compute graph data format expected by react-force-graph
  const graphData = useMemo(() => {
    return {
      nodes: data.nodes.map(n => ({ ...n })), // force graph mutates objects
      links: data.edges.map(e => ({ source: e.source, target: e.target, weight: e.weight }))
    };
  }, [data]);

  // Compute neighbors map for fast lookup
  const neighbors = useMemo(() => {
    const map = new Map<string, Set<string>>();
    graphData.nodes.forEach(n => map.set(n.id, new Set()));
    graphData.links.forEach(link => {
      const src = typeof link.source === 'object' ? (link.source as Node).id : link.source;
      const tgt = typeof link.target === 'object' ? (link.target as Node).id : link.target;
      map.get(src)?.add(tgt);
    });
    return map;
  }, [graphData]);

  // Configure forces to match Obsidian's tight center clustering
  useEffect(() => {
    if (fgRef.current) {
      const chargeForce = fgRef.current.d3Force('charge');
      if (chargeForce) {
        // Map 0-100 to -10 to -300
        const repelStrength = -10 - ((forceConfig?.repel ?? 50) / 100) * 290;
        chargeForce.strength(repelStrength);
        // We remove distanceMax so the charge force applies globally, balancing with center gravity.
      }
      
      const linkForce = fgRef.current.d3Force('link');
      if (linkForce) {
        // Map 0-100 to 10 to 150
        const linkDistance = 10 + ((forceConfig?.link ?? 50) / 100) * 140;
        linkForce.distance(linkDistance);
      }
      
      // Custom center gravity to pull nodes towards the origin
      fgRef.current.d3Force('centerGravity', (alpha: number) => {
        // Map 0-100 to 0.0 to 0.3
        const centerStrength = ((forceConfig?.center ?? 50) / 100) * 0.3;
        const strength = centerStrength * alpha;
        const nodes = graphData.nodes as any[];
        for (let i = 0; i < nodes.length; i++) {
          const node = nodes[i];
          node.vx -= (node.x || 0) * strength;
          node.vy -= (node.y || 0) * strength;
        }
      });
      
      // Reheat the simulation so changes take effect immediately
      fgRef.current.d3ReheatSimulation();
    }
  }, [graphData, forceConfig]);

  // Responsive container observer
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
      // Smoothly center and zoom to the clicked node
      if (fgRef.current) {
        fgRef.current.centerAt(node.x, node.y, 1000);
        fgRef.current.zoom(4, 2000);
      }
      if (onSelect) onSelect(node.id);
    },
    [onSelect]
  );

  const maxPpr = Math.max(0.0001, ...Object.values(pprScores ?? {}));

  // Render nodes with glowing effect and text labels
  const paintNode = useCallback(
    (node: Node, ctx: CanvasRenderingContext2D, globalScale: number) => {
      const isHovered = node.id === hoverNode;
      const isNeighbor = hoverNode ? neighbors.get(hoverNode)?.has(node.id) : false;
      const isFocused = isHovered || isNeighbor;
      const dimOthers = hoverNode !== null && !isFocused;

      const ppr = pprScores?.[node.id] ?? 0;
      const communityIdx = node.community_id != null && node.community_id >= 0 ? node.community_id : hashString(node.id);
      const color = COMMUNITY_COLORS[communityIdx % COMMUNITY_COLORS.length];
      
      // If colorByCommunity is off, use the single brand color (vibrant indigo)
      const nodeColor = colorByCommunity ? color : KIND_COLOR;
      
      const baseRadius = 4 + (node.mentions ?? 0) * 0.5 + (ppr / maxPpr) * 6;
      const radius = isHovered ? baseRadius * 1.3 : baseRadius;

      // Opacity handling for "dim others"
      const opacity = dimOthers ? 0.15 : 1;
      
      ctx.globalAlpha = opacity;
      
      // Node body (solid crisp circle)
      ctx.beginPath();
      ctx.arc(node.x || 0, node.y || 0, radius, 0, 2 * Math.PI, false);
      ctx.fillStyle = nodeColor;
      ctx.fill();
      
      // Node labels - only show if large enough, hovered, or a neighbor of hovered
      const showLabel = isFocused || (globalScale > 1.5 && baseRadius > 5) || (!hoverNode && globalScale > 2);
      
      if (showLabel) {
        const fontSize = isHovered ? 14 / globalScale : 12 / globalScale;
        ctx.font = `${fontSize}px "Source Serif 4 Variable", Georgia, serif`;
        ctx.textAlign = 'center';
        ctx.textBaseline = 'middle';
        
        // Match token colors
        const isDark = document.documentElement.getAttribute("data-theme") === "dark";
        ctx.fillStyle = isDark ? `rgba(232, 232, 240, ${opacity})` : `rgba(28, 27, 24, ${opacity})`;
        
        // Draw text slightly below the node
        ctx.fillText(node.name, node.x || 0, (node.y || 0) + radius + (fontSize/2) + 2);
      }
      
      ctx.globalAlpha = 1;
    },
    [hoverNode, neighbors, pprScores, colorByCommunity, maxPpr]
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
            const radius = 4 + (node.mentions ?? 0) * 0.5 + ((pprScores?.[node.id] ?? 0) / maxPpr) * 6;
            ctx.fillStyle = color;
            ctx.beginPath();
            ctx.arc(node.x || 0, node.y || 0, radius * 1.5, 0, 2 * Math.PI, false);
            ctx.fill();
          }}
          onNodeClick={handleNodeClick}
          onNodeHover={(node: Node | null) => setHoverNode(node ? node.id : null)}
          linkColor={(link: Link) => {
            const srcId = typeof link.source === 'object' ? (link.source as Node).id : link.source as string;
            const tgtId = typeof link.target === 'object' ? (link.target as Node).id : link.target as string;
            const dimOthers = hoverNode && srcId !== hoverNode && tgtId !== hoverNode;
            const isDark = document.documentElement.getAttribute("data-theme") === "dark";
            
            if (dimOthers) return isDark ? 'rgba(255, 255, 255, 0.02)' : 'rgba(0, 0, 0, 0.02)';
            
            const isFocused = hoverNode && (srcId === hoverNode || tgtId === hoverNode);
            const baseAlpha = isDark ? 0.2 : 0.15; 
            const focusAlpha = isDark ? 0.6 : 0.4;
            
            return isDark 
              ? `rgba(255, 255, 255, ${isFocused ? focusAlpha : baseAlpha})`
              : `rgba(0, 0, 0, ${isFocused ? focusAlpha : baseAlpha})`;
          }}
          linkWidth={(link: Link) => {
            const srcId = typeof link.source === 'object' ? (link.source as Node).id : link.source as string;
            const tgtId = typeof link.target === 'object' ? (link.target as Node).id : link.target as string;
            const isFocused = hoverNode && (srcId === hoverNode || tgtId === hoverNode);
            // Crisp thin lines
            return isFocused ? 1.5 : 0.5;
          }}
        />
      )}
    </div>
  );
}
