import { useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { useReflections } from "../api/queries";
import { client } from "../api/client";
import { useUiStore } from "../store/ui";
import { Button, Card, Skeleton } from "../design/primitives";

export function Reflections() {
  const { data, isLoading, isError } = useReflections();
  const qc = useQueryClient();
  const select = useUiStore((s) => s.select);
  const [busy, setBusy] = useState(false);

  const reflectNow = async () => {
    setBusy(true);
    try {
      await client.reflect();
      await qc.invalidateQueries({ queryKey: ["reflections"] });
    } finally {
      setBusy(false);
    }
  };

  if (isLoading) {
    return (
      <div className="p-6 space-y-2">
        {Array.from({ length: 4 }).map((_, i) => (
          <Skeleton key={i} className="h-16 w-full" />
        ))}
      </div>
    );
  }

  if (isError) {
    return (
      <div className="p-6 text-tier-procedural">
        Could not load reflections. Is the daemon running?
      </div>
    );
  }

  return (
    <div className="p-6 space-y-4">
      <div className="flex items-center justify-between">
        <h1 className="display text-xl">Reflections</h1>
        <Button onClick={reflectNow} disabled={busy}>
          {busy ? "Reflecting…" : "Reflect now"}
        </Button>
      </div>

      {!data?.length && (
        <p className="text-text-muted">
          No reflections yet. They form automatically as the system learns, or
          trigger one now.
        </p>
      )}

      <div className="space-y-2">
        {data?.map((r) => (
          <Card key={r.id} className="p-3 space-y-1">
            <button
              onClick={() => select(r.id)}
              className="block w-full text-left font-body"
            >
              {r.body}
            </button>
            <div className="flex items-center justify-between">
              <span className="label">{r.tags.join(" · ") || r.title}</span>
              <button
                className="label text-text-muted cursor-not-allowed"
                title="Re-tiering lands in Plan 7"
                disabled
              >
                Promote to procedural
              </button>
            </div>
          </Card>
        ))}
      </div>
    </div>
  );
}
