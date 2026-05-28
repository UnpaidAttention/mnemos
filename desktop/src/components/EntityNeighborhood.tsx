import { useQuery } from "@tanstack/react-query";
import ForceGraph2D from "react-force-graph-2d";
import { client } from "../api/client";

export function EntityNeighborhood({ id }: { id: string }) {
  const { data } = useQuery({
    queryKey: ["entity-graph", id],
    queryFn: () => client.entityGraph(id),
  });
  if (!data) return null;

  const graphData = {
    nodes: data.nodes.map((n) => ({ id: n.id, name: n.name })),
    links: data.edges.map((e) => ({ source: e.source, target: e.target })),
  };

  return (
    <div
      className="border border-border rounded-lg overflow-hidden"
      style={{ height: 280 }}
    >
      <ForceGraph2D
        graphData={graphData}
        nodeLabel="name"
        height={280}
        width={520}
        nodeColor={() => "#1F6F6B"}
        linkColor={() => "#5B6168"}
      />
    </div>
  );
}
