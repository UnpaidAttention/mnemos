import { Link } from "@tanstack/react-router";
import { useMemories } from "../api/queries";
import { useUiStore } from "../store/ui";
import { TierChip, Skeleton, Card } from "../design/primitives";

export function Browser() {
  const { data, isLoading, isError } = useMemories();
  const select = useUiStore((s) => s.select);

  if (isLoading) {
    return (
      <div className="p-6 space-y-2">
        {Array.from({ length: 6 }).map((_, i) => (
          <Skeleton key={i} className="h-10 w-full" />
        ))}
      </div>
    );
  }

  if (isError) {
    return (
      <div className="p-6">
        <p className="text-tier-procedural font-body">
          Could not load memories. Is the daemon running?
        </p>
      </div>
    );
  }

  if (!data?.length) {
    return (
      <div className="p-6 space-y-2">
        <h1 className="display text-xl mb-1">Memories</h1>
        <p className="text-text-muted font-body">
          Your vault is empty.{" "}
          <kbd className="mono text-xs border border-border rounded px-1 py-0.5">⌘K</kbd>{" "}
          → New memory to add one.
        </p>
      </div>
    );
  }

  return (
    <div className="p-6 space-y-2">
      <h1 className="display text-xl mb-3">Memories</h1>
      {data.map((m) => {
        const invalid = !!m.invalid_at;
        const editPath: string = `/editor/${m.id}`;
        return (
          <Card
            key={m.id}
            className={`p-3 hover:shadow-raised transition-shadow duration-[120ms] ease-brand ${invalid ? "opacity-60 border-dashed" : ""}`}
          >
            <button
              onClick={() => select(m.id)}
              className="block w-full text-left"
            >
              <div className="flex items-center justify-between gap-2">
                <span className={`font-body ${invalid ? "line-through" : ""}`}>
                  {m.title}
                </span>
                <TierChip tier={m.tier} />
              </div>
              {m.tags.length > 0 && (
                <div className="mt-1 flex flex-wrap gap-1">
                  {m.tags.map((tag) => (
                    <span
                      key={tag}
                      className="label mono text-[0.65rem] border border-border rounded-sm px-1"
                    >
                      {tag}
                    </span>
                  ))}
                </div>
              )}
            </button>
            <div className="mt-1 flex items-center gap-3">
              <Link
                to={editPath}
                className="label text-accent hover:underline"
              >
                edit
              </Link>
              <span className="label mono text-[0.65rem] text-text-muted">
                {m.valid_at.slice(0, 10)}
              </span>
            </div>
          </Card>
        );
      })}
    </div>
  );
}
