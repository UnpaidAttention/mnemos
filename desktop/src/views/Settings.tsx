import { useEffect, useState } from "react";
import { client } from "../api/client";
import { VaultIO } from "../components/VaultIO";
import { Button, Card, Skeleton } from "../design/primitives";
import { Connections } from "./Connections";

type Field =
  | { key: string; label: string; kind: "text" | "password" }
  | { key: string; label: string; kind: "number"; min?: number; max?: number; step?: number }
  | { key: string; label: string; kind: "range"; min: number; max: number; step: number }
  | { key: string; label: string; kind: "boolean" }
  | { key: string; label: string; kind: "select"; options: string[] };

type Section = { title: string; path: string[]; fields: Field[] };

const SCHEMA: Section[] = [
  {
    title: "Daemon",
    path: ["daemon"],
    fields: [
      { key: "host", label: "Host", kind: "text" },
      { key: "port", label: "Port", kind: "number", min: 1024, max: 65535 },
    ],
  },
  {
    title: "Embedder",
    path: ["embedder"],
    fields: [
      {
        key: "kind",
        label: "Backend",
        kind: "select",
        options: ["bundled", "ollama", "openai", "mock", "none"],
      },
      { key: "url", label: "URL (Ollama)", kind: "text" },
      { key: "model", label: "Model", kind: "text" },
      { key: "dim", label: "Dim", kind: "number" },
      { key: "timeout_secs", label: "Timeout (s)", kind: "number" },
    ],
  },
  {
    title: "LLM",
    path: ["llm"],
    fields: [
      {
        key: "kind",
        label: "Backend",
        kind: "select",
        options: ["ollama", "openai", "mock", "none"],
      },
      { key: "url", label: "URL", kind: "text" },
      { key: "model", label: "Model", kind: "text" },
      { key: "timeout_secs", label: "Timeout (s)", kind: "number" },
    ],
  },
  {
    title: "OpenAI",
    path: ["openai"],
    fields: [
      { key: "base_url", label: "Base URL", kind: "text" },
      { key: "api_key", label: "API Key", kind: "password" },
    ],
  },
  {
    title: "Retrieval",
    path: ["retrieval"],
    fields: [
      { key: "default_k", label: "Default k", kind: "number" },
      { key: "rrf_k", label: "RRF k", kind: "number" },
      { key: "ppr_alpha", label: "PPR α", kind: "range", min: 0.5, max: 0.95, step: 0.05 },
      { key: "ppr_iterations", label: "PPR iters", kind: "number", min: 1, max: 200 },
    ],
  },
  {
    title: "Decay (reweight)",
    path: ["retrieval", "reweight"],
    fields: [
      { key: "recency_decay", label: "Recency decay/day", kind: "range", min: 0, max: 0.2, step: 0.005 },
      { key: "importance_weight", label: "Importance weight", kind: "range", min: 0, max: 3, step: 0.05 },
    ],
  },
  {
    title: "Reflection",
    path: ["reflection"],
    fields: [
      { key: "salience_threshold", label: "Salience threshold", kind: "range", min: 0, max: 50, step: 0.5 },
      { key: "max_sources", label: "Max sources", kind: "number", min: 1, max: 100 },
    ],
  },
  {
    title: "Community",
    path: ["community"],
    fields: [{ key: "min_community_size", label: "Min community size", kind: "number", min: 2, max: 50 }],
  },
  {
    title: "Sync",
    path: ["sync"],
    fields: [
      { key: "kind", label: "Backend", kind: "select", options: ["none", "filesystem", "git", "s3"] },
      { key: "interval_secs", label: "Interval (s)", kind: "number", min: 0, max: 86400 },
    ],
  },
  {
    title: "Sync · Git",
    path: ["sync", "git"],
    fields: [
      { key: "remote", label: "Remote URL", kind: "text" },
      { key: "branch", label: "Branch", kind: "text" },
    ],
  },
  {
    title: "Sync · S3 (rclone)",
    path: ["sync", "s3"],
    fields: [{ key: "remote", label: "Remote (rclone target)", kind: "text" }],
  },
];

type Cfg = Record<string, unknown>;

function getAt(obj: unknown, path: string[]): unknown {
  return path.reduce<unknown>((acc, k) => {
    if (acc && typeof acc === "object" && !Array.isArray(acc) && k in (acc as Record<string, unknown>)) {
      return (acc as Record<string, unknown>)[k];
    }
    return undefined;
  }, obj);
}

function setAt(obj: Cfg, path: string[], value: unknown): Cfg {
  if (path.length === 0) return obj;
  const out: Cfg = { ...obj };
  let cur: Record<string, unknown> = out;
  for (let i = 0; i < path.length - 1; i++) {
    const existing = cur[path[i]];
    const next: Record<string, unknown> =
      existing && typeof existing === "object" && !Array.isArray(existing)
        ? { ...(existing as Record<string, unknown>) }
        : {};
    cur[path[i]] = next;
    cur = next;
  }
  cur[path[path.length - 1]] = value;
  return out;
}

export function Settings() {
  const [cfg, setCfg] = useState<Cfg | null>(null);
  const [saving, setSaving] = useState(false);
  const [savedAt, setSavedAt] = useState<string | null>(null);

  useEffect(() => {
    void client.getConfig().then(setCfg);
  }, []);

  if (!cfg) {
    return (
      <div className="p-6">
        <Skeleton className="h-64 w-full" />
      </div>
    );
  }

  const save = async () => {
    setSaving(true);
    try {
      await client.putConfig(cfg);
      setSavedAt(new Date().toISOString());
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="p-6 max-w-3xl space-y-4">
      <div className="flex items-center justify-between">
        <h1 className="display text-xl">Settings</h1>
        <div className="flex items-center gap-3">
          {savedAt && <span className="label text-text-muted">saved {savedAt.slice(11, 16)}</span>}
          <Button onClick={save} disabled={saving}>
            {saving ? "Saving…" : "Save settings"}
          </Button>
        </div>
      </div>
      <Card className="p-4">
        <details open className="space-y-3">
          <summary className="display text-base cursor-pointer">AI Tool Connections</summary>
          <div className="pt-2">
            <Connections />
          </div>
        </details>
      </Card>
      {SCHEMA.map((section) => (
        <Card key={section.title} className="p-4">
          <details open className="space-y-3">
            <summary className="display text-base cursor-pointer">{section.title}</summary>
            <div className="grid grid-cols-2 gap-3 pt-2">
              {section.fields.map((f) => {
                const path = [...section.path, f.key];
                const v = getAt(cfg, path);
                const onChange = (val: unknown) => setCfg(setAt(cfg, path, val));
                return (
                  <label key={f.key} className="flex flex-col gap-1">
                    <span className="label">{f.label}</span>
                    {f.kind === "text" && (
                      <input
                        className="bg-surface border border-border rounded-md px-2 py-1 mono text-sm"
                        value={String(v ?? "")}
                        onChange={(e) => onChange(e.target.value)}
                      />
                    )}
                    {f.kind === "password" && (
                      <input
                        type="password"
                        className="bg-surface border border-border rounded-md px-2 py-1 mono text-sm"
                        value={String(v ?? "")}
                        onChange={(e) => onChange(e.target.value)}
                      />
                    )}
                    {f.kind === "number" && (
                      <input
                        type="number"
                        className="bg-surface border border-border rounded-md px-2 py-1 mono text-sm"
                        value={Number(v ?? 0)}
                        min={f.min}
                        max={f.max}
                        step={f.step}
                        onChange={(e) => onChange(Number(e.target.value))}
                      />
                    )}
                    {f.kind === "range" && (
                      <>
                        <input
                          type="range"
                          min={f.min}
                          max={f.max}
                          step={f.step}
                          value={Number(v ?? f.min)}
                          onChange={(e) => onChange(Number(e.target.value))}
                          className="accent-accent"
                        />
                        <span className="mono text-xs text-text-muted">
                          {Number(v ?? f.min).toFixed(2)}
                        </span>
                      </>
                    )}
                    {f.kind === "boolean" && (
                      <input
                        type="checkbox"
                        checked={Boolean(v)}
                        onChange={(e) => onChange(e.target.checked)}
                      />
                    )}
                    {f.kind === "select" && (
                      <select
                        className="bg-surface border border-border rounded-md px-2 py-1 text-sm"
                        value={String(v ?? "")}
                        onChange={(e) => onChange(e.target.value)}
                      >
                        {f.options.map((o) => (
                          <option key={o} value={o}>
                            {o}
                          </option>
                        ))}
                      </select>
                    )}
                  </label>
                );
              })}
            </div>
          </details>
        </Card>
      ))}
      <VaultIO />
    </div>
  );
}
