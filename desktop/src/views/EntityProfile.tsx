import { useState } from "react";
import { useParams } from "@tanstack/react-router";
import { useUiStore } from "../store/ui";
import { useEntity } from "../api/queries";
import { EntityNeighborhood } from "../components/EntityNeighborhood";
import { MergeDialog } from "../components/MergeDialog";
import { Button, Card, Skeleton } from "../design/primitives";
import type { EnrichedEdge, CoMentionedEntity, EntityMemoryPreview } from "../api/types";

/* ── Tier color mapping ── */
const TIER_COLORS: Record<string, string> = {
  working: "var(--tier-working)",
  episodic: "var(--tier-episodic)",
  semantic: "var(--tier-semantic)",
  procedural: "var(--tier-procedural)",
  reflection: "var(--tier-reflection)",
};

function TierBadge({ tier }: { tier: string }) {
  const color = TIER_COLORS[tier] ?? "var(--text-muted)";
  return (
    <span
      className="inline-flex items-center gap-1 text-xs font-body px-1.5 py-0.5 rounded"
      style={{ color, border: `1px solid ${color}33`, background: `${color}0D` }}
    >
      <span
        className="inline-block w-1.5 h-1.5 rounded-full"
        style={{ background: color }}
      />
      {tier}
    </span>
  );
}

function formatDate(iso: string | undefined): string {
  if (!iso) return "";
  try {
    return new Date(iso).toLocaleDateString("en-US", {
      month: "short",
      day: "numeric",
      year: "numeric",
    });
  } catch {
    return iso;
  }
}

/* ── Relationships section ── */
function RelationshipsSection({
  edges,
  entityId,
  onNavigate,
}: {
  edges: EnrichedEdge[];
  entityId: string;
  onNavigate: (id: string) => void;
}) {
  if (!edges.length) return null;

  const outgoing = edges.filter((e) => e.source === entityId);
  const incoming = edges.filter((e) => e.target === entityId);

  return (
    <Card className="p-3">
      <div className="label mb-3">Relationships</div>
      {outgoing.length > 0 && (
        <div className="mb-3">
          <div className="text-xs text-text-muted mb-1.5 font-body">Outgoing</div>
          <ul className="space-y-1.5">
            {outgoing.map((e) => (
              <li key={e.id} className="flex items-center gap-2 text-sm">
                <span className="text-accent shrink-0">→</span>
                <button
                  className="text-accent hover:underline font-body truncate"
                  onClick={() => onNavigate(e.target)}
                  title={e.target_name}
                >
                  {e.target_name}
                </button>
                <span className="text-text-muted text-xs truncate flex-1">
                  {e.relation}
                </span>
                <span
                  className="text-text-muted text-xs shrink-0 mono"
                  title="Edge weight"
                >
                  ×{e.weight.toFixed(0)}
                </span>
              </li>
            ))}
          </ul>
        </div>
      )}
      {incoming.length > 0 && (
        <div>
          <div className="text-xs text-text-muted mb-1.5 font-body">Incoming</div>
          <ul className="space-y-1.5">
            {incoming.map((e) => (
              <li key={e.id} className="flex items-center gap-2 text-sm">
                <span className="text-tier-working shrink-0">←</span>
                <button
                  className="text-accent hover:underline font-body truncate"
                  onClick={() => onNavigate(e.source)}
                  title={e.source_name}
                >
                  {e.source_name}
                </button>
                <span className="text-text-muted text-xs truncate flex-1">
                  {e.relation}
                </span>
                <span
                  className="text-text-muted text-xs shrink-0 mono"
                  title="Edge weight"
                >
                  ×{e.weight.toFixed(0)}
                </span>
              </li>
            ))}
          </ul>
        </div>
      )}
    </Card>
  );
}

/* ── Memories section ── */
function MemoriesSection({
  memories,
  onSelectMemory,
}: {
  memories: EntityMemoryPreview[];
  onSelectMemory: (id: string) => void;
}) {
  if (!memories.length) return null;

  return (
    <Card className="p-3">
      <div className="label mb-3">Mentioned in {memories.length} {memories.length === 1 ? "memory" : "memories"}</div>
      <ul className="space-y-2">
        {memories.map((m) => (
          <li key={m.id}>
            <button
              className="text-left w-full rounded-md p-2 -mx-0.5 transition-colors hover:bg-surface-raised/60"
              onClick={() => onSelectMemory(m.id)}
              style={{
                borderLeft: `3px solid ${TIER_COLORS[m.tier ?? "semantic"] ?? "var(--accent)"}`,
              }}
            >
              <div className="flex items-center gap-2 mb-0.5">
                <span className="font-body text-accent text-sm truncate flex-1">
                  {m.title}
                </span>
                {m.tier && <TierBadge tier={m.tier} />}
              </div>
              <div className="text-text-muted text-xs line-clamp-2 font-body">
                {m.body_preview}
              </div>
              {m.created_at && (
                <div className="text-text-muted text-xs mt-1 mono">
                  {formatDate(m.created_at)}
                </div>
              )}
            </button>
          </li>
        ))}
      </ul>
    </Card>
  );
}

/* ── Co-mentioned entities section ── */
function CoMentionedSection({
  entities,
  onNavigate,
}: {
  entities: CoMentionedEntity[];
  onNavigate: (id: string) => void;
}) {
  if (!entities.length) return null;

  return (
    <Card className="p-3">
      <div className="label mb-3">Frequently co-mentioned</div>
      <ul className="space-y-1">
        {entities.map((e) => (
          <li key={e.id} className="flex items-center gap-2">
            <button
              className="text-accent hover:underline text-sm font-body truncate flex-1 text-left"
              onClick={() => onNavigate(e.id)}
              title={`${e.name} (${e.kind})`}
            >
              {e.name}
            </button>
            <span className="text-text-muted text-xs shrink-0">
              {e.kind}
            </span>
            <span
              className="inline-flex items-center justify-center text-xs mono rounded-full px-1.5 py-0.5 bg-accent/10 text-accent shrink-0"
              title={`${e.shared_memory_count} shared memories`}
            >
              {e.shared_memory_count}
            </span>
          </li>
        ))}
      </ul>
    </Card>
  );
}

/* ── Main component ── */
export function EntityProfile({
  id: idProp,
  onNavigateEntity,
}: {
  id?: string;
  onNavigateEntity?: (id: string) => void;
}) {
  const params = useParams({ strict: false }) as { id?: string };
  const id = idProp ?? params.id ?? null;
  const { data, isLoading, isError } = useEntity(id);
  const select = useUiStore((s) => s.select);
  const [mergeOpen, setMergeOpen] = useState(false);

  const handleNavigate = (targetId: string) => {
    if (onNavigateEntity) {
      onNavigateEntity(targetId);
    }
  };

  if (!id || isLoading) {
    return (
      <div className="p-5 space-y-4">
        <Skeleton className="h-8 w-48" />
        <Skeleton className="h-4 w-32" />
        <Skeleton className="h-20 w-full" />
        <Skeleton className="h-32 w-full" />
        <Skeleton className="h-32 w-full" />
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
    <div className="p-5 space-y-4 overflow-y-auto">
      {/* Header */}
      <div>
        <h1 className="display text-xl leading-tight">{data.name}</h1>
        <div className="flex items-center gap-2 mt-1.5 flex-wrap">
          {data.kind && (
            <span className="inline-block px-2 py-0.5 text-xs rounded-full bg-accent/10 text-accent font-body">
              {data.kind}
            </span>
          )}
          <span className="text-text-muted text-xs mono">
            {data.mention_count} {data.mention_count === 1 ? "mention" : "mentions"}
          </span>
          {data.created_at && (
            <span className="text-text-muted text-xs mono">
              · {formatDate(data.created_at)}
            </span>
          )}
        </div>
      </div>

      {/* Description */}
      {data.description && (
        <p className="text-sm text-text-muted font-body leading-relaxed">
          {data.description}
        </p>
      )}

      {/* Aliases */}
      {!!data.aliases?.length && (
        <div className="flex items-center gap-1.5 flex-wrap">
          <span className="text-xs text-text-muted">aka</span>
          {data.aliases.map((a) => (
            <span
              key={a}
              className="inline-block px-1.5 py-0.5 text-xs rounded bg-surface-raised text-text-muted font-body"
            >
              {a}
            </span>
          ))}
        </div>
      )}

      {/* Community */}
      {data.community && (
        <div className="text-xs text-text-muted font-body p-2 rounded bg-surface-raised/50 border border-border/50">
          <span className="label">Community {data.community.id}</span>
          {data.community.summary && (
            <p className="mt-1 text-text-muted line-clamp-3">{data.community.summary}</p>
          )}
        </div>
      )}

      {/* Actions */}
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

      {/* Relationships */}
      <RelationshipsSection
        edges={data.edges}
        entityId={id}
        onNavigate={handleNavigate}
      />

      {/* Memories */}
      <MemoriesSection
        memories={data.memories ?? []}
        onSelectMemory={select}
      />

      {/* Co-mentioned entities */}
      <CoMentionedSection
        entities={data.co_mentioned_entities ?? []}
        onNavigate={handleNavigate}
      />

      {/* Local neighborhood graph */}
      <Card className="p-3">
        <div className="label mb-2">Neighborhood</div>
        <EntityNeighborhood id={id} />
      </Card>
    </div>
  );
}
