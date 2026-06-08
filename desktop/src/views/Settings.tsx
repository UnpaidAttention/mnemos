import { useEffect, useState, useCallback } from "react";
import { client } from "../api/client";
import { VaultIO } from "../components/VaultIO";
import { StorageSettings } from "./StorageSettings";
import { AutonomySettings } from "./AutonomySettings";
import { Button, Card, Skeleton } from "../design/primitives";
import { Connections } from "./Connections";
import { ModelPicker, EMBEDDER_MODELS, LLM_MODELS } from "../components/ModelPicker";
import { checkOllama, installOllama, pullModel, applyLlmConfig, applyEmbedderConfig, checkForUpdates, installUpdate, OllamaStatus, UpdateInfo } from "../api/tauri";

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
  const [loadError, setLoadError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [savedAt, setSavedAt] = useState<string | null>(null);
  const [saveError, setSaveError] = useState<string | null>(null);
  const [versionInfo, setVersionInfo] = useState<{ version: string; git_hash: string } | null>(null);

  // Ollama + model state for inline pickers
  const [ollamaStatus, setOllamaStatus] = useState<OllamaStatus | null>(null);
  const [ollamaInstalling, setOllamaInstalling] = useState(false);
  const [selectedEmbedder, setSelectedEmbedder] = useState("bundled");
  const [selectedLlm, setSelectedLlm] = useState("bundled");
  const [pullingModel, setPullingModel] = useState<string | null>(null);
  const [pullProgress, setPullProgress] = useState(0);
  const [changingEmbedder, setChangingEmbedder] = useState(false);
  const [changingLlm, setChangingLlm] = useState(false);

  // Update state
  const [updateInfo, setUpdateInfo] = useState<UpdateInfo | null>(null);
  const [checkingUpdates, setCheckingUpdates] = useState(false);
  const [installingUpdate, setInstallingUpdate] = useState(false);
  const [updateError, setUpdateError] = useState<string | null>(null);

  const refreshOllama = useCallback(async () => {
    try {
      const st = await checkOllama();
      if (st) setOllamaStatus(st);
    } catch { /* not in Tauri */ }
  }, []);

  useEffect(() => {
    void client
      .getConfig()
      .then((c) => {
        setCfg(c);
        // Detect current embedder/llm from config
        const embKind = (c as Record<string, Record<string, string>>)?.embedder?.kind;
        const embModel = (c as Record<string, Record<string, string>>)?.embedder?.model;
        if (embKind === "ollama" && embModel) setSelectedEmbedder(embModel);
        const llmKind = (c as Record<string, Record<string, string>>)?.llm?.kind;
        const llmModel = (c as Record<string, Record<string, string>>)?.llm?.model;
        if (llmKind === "ollama" && llmModel) setSelectedLlm(llmModel);
      })
      .catch(() => setLoadError("Could not reach the daemon"));
    void client
      .getHealth()
      .then(setVersionInfo)
      .catch(() => {/* ignore */});
    void refreshOllama();
  }, [refreshOllama]);

  // Listen for pull progress
  useEffect(() => {
    let unlisten: (() => void) | null = null;
    (async () => {
      try {
        const { listen } = await import("@tauri-apps/api/event");
        unlisten = await listen<{ completed: number; total: number }>("model-pull-progress", (ev) => {
          if (ev.payload.total > 0) setPullProgress(Math.round((ev.payload.completed / ev.payload.total) * 100));
        });
      } catch { /* not in Tauri */ }
    })();
    return () => { unlisten?.(); };
  }, []);

  const [pullError, setPullError] = useState<string | null>(null);

  const handlePull = async (tag: string) => {
    setPullingModel(tag);
    setPullProgress(0);
    setPullError(null);
    try {
      // Check if Ollama is running first
      if (!ollamaStatus?.installed) {
        setPullError("Ollama is not installed. Install it first using the button above.");
        setPullingModel(null);
        return;
      }
      if (!ollamaStatus?.running) {
        setPullError("Ollama is not running. Start it with: ollama serve");
        setPullingModel(null);
        return;
      }
      await pullModel(tag);
      await refreshOllama();
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      setPullError(`Download failed: ${msg}`);
    }
    setPullingModel(null);
  };

  const handleInstallOllama = async () => {
    setOllamaInstalling(true);
    try { await installOllama(); await refreshOllama(); } catch (e) { console.error(e); }
    setOllamaInstalling(false);
  };

  if (loadError) {
    return (
      <div className="p-6">
        <p role="alert" className="label text-tier-procedural">{loadError}</p>
      </div>
    );
  }

  if (!cfg) {
    return (
      <div className="p-6">
        <Skeleton className="h-64 w-full" />
      </div>
    );
  }

  const save = async () => {
    setSaving(true);
    setSaveError(null);
    try {
      await client.putConfig(cfg);
      setSavedAt(new Date().toISOString());
    } catch (e) {
      setSaveError(e instanceof Error ? e.message : "Save failed");
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
      {saveError && (
        <p role="alert" className="label text-tier-procedural">{saveError}</p>
      )}
      <StorageSettings />
      <AutonomySettings />
      <Card className="p-4">
        <details open className="space-y-3">
          <summary className="display text-base cursor-pointer">AI Tool Connections</summary>
          <div className="pt-2">
            <Connections />
          </div>
        </details>
      </Card>

      {/* ── Embedder model picker ── */}
      <Card className="p-4">
        <details open className="space-y-3">
          <summary className="display text-base cursor-pointer">Search Model (Embedder)</summary>
          <div className="pt-2">
            <ModelPicker
              catalog={EMBEDDER_MODELS}
              selectedTag={selectedEmbedder}
              onSelect={setSelectedEmbedder}
              installedModels={ollamaStatus?.models ?? []}
              pullingModel={pullingModel}
              pullProgress={pullProgress}
              onPull={handlePull}
            />
            {pullError && (
              <p role="alert" className="label text-tier-procedural mt-2">{pullError}</p>
            )}
            {!ollamaStatus?.installed && (
              <div className="mt-2 p-2 rounded bg-tier-working/10 text-sm">
                <span className="font-semibold">Ollama not installed.</span> Required for non-bundled models.
                <Button className="ml-2 text-xs" onClick={() => void handleInstallOllama()} disabled={ollamaInstalling}>
                  {ollamaInstalling ? "Installing…" : "Install Ollama"}
                </Button>
              </div>
            )}
            {ollamaStatus?.installed && !ollamaStatus?.running && (
              <p className="label text-tier-working mt-2">Ollama is installed but not running. Start it with: <code className="mono">ollama serve</code></p>
            )}
            {!ollamaStatus?.running && selectedEmbedder !== "bundled" && !ollamaStatus?.installed && (
              <Button className="mt-2" onClick={() => void handleInstallOllama()} disabled={ollamaInstalling}>
                {ollamaInstalling ? "Installing Ollama…" : "Install Ollama"}
              </Button>
            )}
            <div className="flex justify-end mt-3">
              <Button
                disabled={changingEmbedder}
                onClick={async () => {
                  setChangingEmbedder(true);
                  try {
                    const m = EMBEDDER_MODELS.find((e) => e.tag === selectedEmbedder);
                    if (m && m.provider === "ollama") {
                      await applyEmbedderConfig("ollama", m.tag, m.dim ?? 768);
                    } else {
                      await applyEmbedderConfig("bundled", "", 384);
                    }
                  } catch (e) { console.error(e); }
                  setChangingEmbedder(false);
                }}
              >
                {changingEmbedder ? "Applying…" : "Apply embedder"}
              </Button>
            </div>
          </div>
        </details>
      </Card>

      {/* ── LLM model picker ── */}
      <Card className="p-4">
        <details open className="space-y-3">
          <summary className="display text-base cursor-pointer">Learning Model (Chat LLM)</summary>
          <div className="pt-2">
            <ModelPicker
              catalog={LLM_MODELS}
              selectedTag={selectedLlm}
              onSelect={setSelectedLlm}
              installedModels={ollamaStatus?.models ?? []}
              pullingModel={pullingModel}
              pullProgress={pullProgress}
              onPull={handlePull}
            />
            {pullError && (
              <p role="alert" className="label text-tier-procedural mt-2">{pullError}</p>
            )}
            {!ollamaStatus?.installed && (
              <div className="mt-2 p-2 rounded bg-tier-working/10 text-sm">
                <span className="font-semibold">Ollama not installed.</span> Required for non-bundled models.
                <Button className="ml-2 text-xs" onClick={() => void handleInstallOllama()} disabled={ollamaInstalling}>
                  {ollamaInstalling ? "Installing…" : "Install Ollama"}
                </Button>
              </div>
            )}
            {ollamaStatus?.installed && !ollamaStatus?.running && (
              <p className="label text-tier-working mt-2">Ollama is installed but not running. Start it with: <code className="mono">ollama serve</code></p>
            )}
            {!ollamaStatus?.running && selectedLlm !== "bundled" && !ollamaStatus?.installed && (
              <Button className="mt-2" onClick={() => void handleInstallOllama()} disabled={ollamaInstalling}>
                {ollamaInstalling ? "Installing Ollama…" : "Install Ollama"}
              </Button>
            )}
            <div className="flex justify-end mt-3">
              <Button
                disabled={changingLlm}
                onClick={async () => {
                  setChangingLlm(true);
                  try {
                    const m = LLM_MODELS.find((e) => e.tag === selectedLlm);
                    if (m && m.provider === "ollama") {
                      await applyLlmConfig("ollama", m.tag);
                    } else {
                      await applyLlmConfig("bundled", "Qwen3-0.6B");
                    }
                  } catch (e) { console.error(e); }
                  setChangingLlm(false);
                }}
              >
                {changingLlm ? "Applying…" : "Apply LLM"}
              </Button>
            </div>
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
                    className="bg-surface border border-border rounded-md px-2 py-1 mono text-sm text-text"
                        value={String(v ?? "")}
                        onChange={(e) => onChange(e.target.value)}
                      />
                    )}
                    {f.kind === "password" && (
                      <input
                        type="password"
                        className="bg-surface border border-border rounded-md px-2 py-1 mono text-sm text-text"
                        value={String(v ?? "")}
                        onChange={(e) => onChange(e.target.value)}
                      />
                    )}
                    {f.kind === "number" && (
                      <input
                        type="number"
                        className="bg-surface border border-border rounded-md px-2 py-1 mono text-sm text-text"
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
                        className="bg-surface border border-border rounded-md px-2 py-1 text-sm text-text"
                        value={String(v ?? "")}
                        onChange={(e) => onChange(e.target.value)}
                      >
                        {f.options.map((o) => (
                          <option key={o} value={o} className="bg-surface text-text">
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

      {/* ── Updates ── */}
      <Card className="p-4">
        <div className="space-y-3">
          <div className="display text-base">Updates</div>
          <div className="flex items-center gap-3">
            {versionInfo && (
              <span className="label text-text-muted text-xs">
                Current: v{versionInfo.version} · {versionInfo.git_hash}
              </span>
            )}
            <Button
              disabled={checkingUpdates}
              onClick={async () => {
                setCheckingUpdates(true);
                setUpdateError(null);
                try {
                  const info = await checkForUpdates();
                  if (info) setUpdateInfo(info);
                } catch (e) {
                  setUpdateError(e instanceof Error ? e.message : "Check failed");
                }
                setCheckingUpdates(false);
              }}
            >
              {checkingUpdates ? "Checking…" : "Check for updates"}
            </Button>
          </div>

          {updateError && (
            <p role="alert" className="label text-tier-procedural">{updateError}</p>
          )}

          {updateInfo && !updateInfo.update_available && (
            <p className="label text-accent">✓ You&apos;re on the latest version (v{updateInfo.current_version})</p>
          )}

          {updateInfo && updateInfo.update_available && (
            <div className="space-y-2">
              <div className="flex items-center gap-2">
                <span className="text-accent font-semibold text-sm">
                  Update available: v{updateInfo.latest_version}
                </span>
                {updateInfo.asset_name && (
                  <span className="label text-text-muted text-xs">
                    ({updateInfo.asset_name})
                  </span>
                )}
              </div>
              {updateInfo.release_notes && (
                <p className="text-text-muted text-xs font-body whitespace-pre-line max-h-24 overflow-y-auto">
                  {updateInfo.release_notes}
                </p>
              )}
              {updateInfo.asset_url && updateInfo.asset_name && (
                <Button
                  disabled={installingUpdate}
                  onClick={async () => {
                    setInstallingUpdate(true);
                    setUpdateError(null);
                    try {
                      await installUpdate(updateInfo.asset_url!, updateInfo.asset_name!);
                    } catch (e) {
                      setUpdateError(e instanceof Error ? e.message : "Install failed");
                    }
                    setInstallingUpdate(false);
                  }}
                >
                  {installingUpdate ? "Installing update…" : `Install v${updateInfo.latest_version}`}
                </Button>
              )}
              {!updateInfo.asset_url && (
                <p className="label text-text-muted text-xs">
                  No package found for your system. Visit the{" "}
                  <a href={updateInfo.release_url} target="_blank" rel="noreferrer" className="text-accent underline">
                    release page
                  </a>.
                </p>
              )}
            </div>
          )}
        </div>
      </Card>
    </div>
  );
}
