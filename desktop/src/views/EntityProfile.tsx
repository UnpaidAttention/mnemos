import { useState } from "react";
import { useParams, useNavigate } from "@tanstack/react-router";
import { useUiStore } from "../store/ui";
import { useEntity } from "../api/queries";
import { EntityNeighborhood } from "../components/EntityNeighborhood";
import { MergeDialog } from "../components/MergeDialog";
import { Button, Card, Skeleton } from "../design/primitives";

export function EntityProfile({ id: idProp }: { id?: string }) {
  const params = useParams({ strict: false }) as { id?: string };
  const id = idProp ?? params.id ?? null;
  const { data, isLoading, isError } = useEntity(id);
  const select = useUiStore((s) => s.select);
  const [mergeOpen, setMergeOpen] = useState(false);
  const navigate = useNavigate();

  if (!id || isLoading) {
    return (
      <div className="p-6">
        <Skeleton className="h-64 w-full" />
      </div>
    );
  }

  if (isError) {
    return (
      <div className="p-6 text-tier-procedural">
        Could not load entity.
      </div>
    );
  }

  if (!data) {
    return (
      <div className="p-6 text-text-muted">Entity not found.</div>
    );
  }

  return (
    <div className="p-6 space-y-4 max-w-3xl">
      {/* Breadcrumb navigation */}
      <nav className="flex items-center gap-2 text-sm text-text-muted" aria-label="Breadcrumb">
        <button
          onClick={() => void navigate({ to: "/graph" as "/" })}
          className="hover:text-accent transition-colors duration-100"
          aria-label="Back to Graph"
        >
          ← Graph
        </button>
        <span aria-hidden>/</span>
        <span className="truncate text-text">{data.name}</span>
      </nav>

      <h1 className="display text-2xl">{data.name}</h1>
      {data.description && (
        <p className="text-text-muted">{data.description}</p>
      )}
      {!!data.aliases?.length && (
        <p className="label">aka {data.aliases.join(", ")}</p>
      )}
      <p className="label">{data.mention_count} mentions</p>

      <div className="flex items-center gap-2">
        <Button variant="ghost" onClick={() => setMergeOpen(true)}>
          Merge into…
        </Button>
      </div>
      <MergeDialog
        open={mergeOpen}
        source={{ id, name: data.name }}
        onClose={() => setMergeOpen(false)}
      />

      <EntityNeighborhood id={id} />

      <Card className="p-3">
        <div className="label mb-1">relationships</div>
        <ul className="text-sm mono space-y-0.5">
          {data.edges.map((e) => (
            <li key={e.id}>
              {e.source === id ? "→" : "←"} {e.relation} (w{e.weight.toFixed(0)})
            </li>
          ))}
          {!data.edges.length && (
            <li className="text-text-muted">none</li>
          )}
        </ul>
      </Card>

      <Card className="p-3">
        <div className="label mb-1">mentioned in</div>
        <ul className="text-sm space-y-0.5">
          {data.memory_ids.map((m) => (
            <li key={m}>
              <button className="text-accent" onClick={() => select(m)}>
                {m}
              </button>
            </li>
          ))}
          {!data.memory_ids.length && (
            <li className="text-text-muted">No memories reference this entity.</li>
          )}
        </ul>
      </Card>
    </div>
  );
}
