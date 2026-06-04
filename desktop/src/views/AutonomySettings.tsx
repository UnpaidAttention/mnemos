import { useEffect, useRef, useState } from "react";
import { client, type AutonomyConfig } from "../api/client";
import { Button, Card, Skeleton } from "../design/primitives";

type SaveState = "idle" | "saving" | "saved" | "error";

export function AutonomySettings() {
  const [cfg, setCfg] = useState<AutonomyConfig | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [saveState, setSaveState] = useState<SaveState>("idle");
  const [errorMsg, setErrorMsg] = useState<string | null>(null);
  const mounted = useRef(true);

  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);

  useEffect(() => {
    client
      .getAutonomyConfig()
      .then((c) => {
        if (mounted.current) setCfg(c);
      })
      .catch((e: unknown) => {
        if (mounted.current) {
          setLoadError(e instanceof Error ? e.message : "Failed to load autonomy settings");
        }
      });
  }, []);

  if (loadError) {
    return (
      <Card className="p-4">
        <p role="alert" className="label text-tier-procedural">
          {loadError}
        </p>
      </Card>
    );
  }

  if (!cfg) {
    return (
      <Card className="p-4 space-y-3">
        <Skeleton className="h-6 w-40" />
        <Skeleton className="h-32 w-full" />
      </Card>
    );
  }

  const save = async () => {
    setSaveState("saving");
    setErrorMsg(null);
    try {
      await client.putAutonomyConfig(cfg);
      setSaveState("saved");
    } catch (e) {
      setSaveState("error");
      setErrorMsg(e instanceof Error ? e.message : "Save failed");
    }
  };

  return (
    <Card className="p-4 space-y-4">
      <div className="flex items-center justify-between">
        <h2 className="display text-lg">Autonomy</h2>
        <div className="flex items-center gap-3">
          {saveState === "saved" && (
            <span className="label text-accent">Saved</span>
          )}
          <Button onClick={save} disabled={saveState === "saving"}>
            {saveState === "saving" ? "Saving…" : "Save"}
          </Button>
        </div>
      </div>

      {saveState === "error" && errorMsg && (
        <p role="alert" className="label text-tier-procedural">
          {errorMsg}
        </p>
      )}

      <div className="space-y-3">
        {/* Capture toggle */}
        <label className="flex items-center gap-3 cursor-pointer">
          <input
            type="checkbox"
            className="accent-accent w-4 h-4"
            checked={cfg.capture}
            onChange={(e) => {
              setCfg({ ...cfg, capture: e.target.checked });
              setSaveState("idle");
            }}
            aria-label="Capture"
          />
          <div>
            <div className="font-body text-sm">Capture sessions</div>
            <div className="label">
              When off, no new memories are captured from connected tools.
            </div>
          </div>
        </label>

        {/* Retention select */}
        <label className="flex flex-col gap-1">
          <span className="label">Retention</span>
          <select
            className="bg-surface border border-border rounded-md px-2 py-1 text-sm w-full max-w-xs"
            value={cfg.retention}
            onChange={(e) => {
              setCfg({
                ...cfg,
                retention: e.target.value as AutonomyConfig["retention"],
              });
              setSaveState("idle");
            }}
            aria-label="Retention"
          >
            <option value="distill-and-prune">Distill and prune (recommended)</option>
            <option value="keep-raw">Keep raw chunks</option>
          </select>
          <span className="label">
            {cfg.retention === "distill-and-prune"
              ? "Raw session chunks are removed after distillation saves space."
              : "Raw chunks are kept indefinitely alongside distilled memories."}
          </span>
        </label>

        {/* Recall budget */}
        <label className="flex flex-col gap-1">
          <span className="label">Recall budget (chars)</span>
          <input
            type="number"
            className="bg-surface border border-border rounded-md px-2 py-1 mono text-sm w-32"
            min={200}
            max={8000}
            step={100}
            value={cfg.recall_budget_chars}
            onChange={(e) => {
              setCfg({ ...cfg, recall_budget_chars: Number(e.target.value) });
              setSaveState("idle");
            }}
            aria-label="Recall budget (chars)"
          />
          <span className="label">
            Max characters of recall context injected per prompt (~{Math.round(cfg.recall_budget_chars / 4)} tokens).
          </span>
        </label>
      </div>
    </Card>
  );
}
