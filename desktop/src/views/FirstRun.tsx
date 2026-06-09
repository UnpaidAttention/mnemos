import { useEffect, useState, useCallback } from "react";
import { client } from "../api/client";
import {
  checkOllama,
  enableService,
  installOllama,
  pullModel,
  applyLlmConfig,
  applyEmbedderConfig,
  checkDownloadedModels,
  downloadBundledModel,
  OllamaStatus,
} from "../api/tauri";
import { Button, Card } from "../design/primitives";
import { Connections } from "./Connections";
import { ModelPicker, EMBEDDER_MODELS, LLM_MODELS, ModelEntry } from "../components/ModelPicker";

type Step = 0 | 1 | 2 | 3 | 4 | 5;

export function FirstRun({ onClose }: { onClose: () => void }) {
  const [step, setStep] = useState<Step>(0);
  const [serviceState, setServiceState] = useState<"idle" | "enabling" | "done" | "skipped">("idle");
  const [finishError, setFinishError] = useState<string | null>(null);

  // Ollama state
  const [ollamaStatus, setOllamaStatus] = useState<OllamaStatus | null>(null);
  const [ollamaChecking, setOllamaChecking] = useState(false);
  const [ollamaInstalling, setOllamaInstalling] = useState(false);

  // Model selection
  const [selectedEmbedder, setSelectedEmbedder] = useState("bundled");
  const [selectedLlm, setSelectedLlm] = useState("qwen3-0.6b");
  const [pullingModel, setPullingModel] = useState<string | null>(null);
  const [pullProgress, setPullProgress] = useState(0);
  const [applyingConfig, setApplyingConfig] = useState(false);

  // Bundled model download state
  const [downloadedFiles, setDownloadedFiles] = useState<string[]>([]);
  const [downloadingModel, setDownloadingModel] = useState<string | null>(null);
  const [downloadProgress, setDownloadProgress] = useState(0);

  // Check Ollama status when entering steps that need it
  const refreshOllama = useCallback(async () => {
    setOllamaChecking(true);
    try {
      const status = await checkOllama();
      if (status) setOllamaStatus(status);
    } catch {
      // Tauri not available (dev mode)
    }
    setOllamaChecking(false);
  }, []);

  // Check which models are already downloaded
  const refreshDownloaded = useCallback(async () => {
    try {
      const result = await checkDownloadedModels();
      if (result) setDownloadedFiles(result.files);
    } catch {
      // Tauri not available
    }
  }, []);

  useEffect(() => {
    if (step === 1 || step === 2) {
      void refreshOllama();
      void refreshDownloaded();
    }
  }, [step, refreshOllama, refreshDownloaded]);

  // Listen for Ollama model pull progress events
  useEffect(() => {
    let unlisten: (() => void) | null = null;
    (async () => {
      try {
        const { listen } = await import("@tauri-apps/api/event");
        unlisten = await listen<{ model: string; status: string; completed: number; total: number }>(
          "model-pull-progress",
          (event) => {
            const { completed, total } = event.payload;
            if (total > 0) {
              setPullProgress(Math.round((completed / total) * 100));
            }
          },
        );
      } catch {
        // Not in Tauri environment
      }
    })();
    return () => { unlisten?.(); };
  }, []);

  // Listen for bundled model download progress events
  useEffect(() => {
    let unlisten: (() => void) | null = null;
    (async () => {
      try {
        const { listen } = await import("@tauri-apps/api/event");
        unlisten = await listen<{ filename: string; downloaded: number; total: number }>(
          "model-download-progress",
          (event) => {
            const { downloaded, total } = event.payload;
            if (total > 0) {
              setDownloadProgress(Math.round((downloaded / total) * 100));
            }
          },
        );
      } catch {
        // Not in Tauri environment
      }
    })();
    return () => { unlisten?.(); };
  }, []);

  const handleInstallOllama = async () => {
    setOllamaInstalling(true);
    try {
      await installOllama();
      await refreshOllama();
    } catch (e) {
      console.error("Ollama install failed:", e);
    }
    setOllamaInstalling(false);
  };

  const handlePullModel = async (tag: string) => {
    setPullingModel(tag);
    setPullProgress(0);
    try {
      await pullModel(tag);
      await refreshOllama();
      if (step === 1) setSelectedEmbedder(tag);
      if (step === 2) setSelectedLlm(tag);
    } catch (e) {
      console.error("Model pull failed:", e);
    }
    setPullingModel(null);
  };

  const handleDownloadModel = async (model: ModelEntry) => {
    if (!model.downloadUrl || !model.downloadFilename) return;
    setDownloadingModel(model.tag);
    setDownloadProgress(0);
    try {
      await downloadBundledModel(model.downloadUrl, model.downloadFilename);
      await refreshDownloaded();
      // Auto-select after download
      if (step === 1) setSelectedEmbedder(model.tag);
      if (step === 2) setSelectedLlm(model.tag);
    } catch (e) {
      console.error("Model download failed:", e);
    }
    setDownloadingModel(null);
  };

  const handleApplyAndContinue = async (fromStep: 1 | 2) => {
    setApplyingConfig(true);
    try {
      if (fromStep === 1) {
        const model = EMBEDDER_MODELS.find((m) => m.tag === selectedEmbedder);
        if (model && model.provider === "ollama") {
          await applyEmbedderConfig("ollama", model.tag, model.dim ?? 768);
        }
        // bundled = default config, no change needed
        setStep(2);
      } else if (fromStep === 2) {
        const model = LLM_MODELS.find((m) => m.tag === selectedLlm);
        if (model && model.provider === "ollama") {
          await applyLlmConfig("ollama", model.tag);
        }
        // bundled-download with "bundled" kind = default config (uses llama-server)
        setStep(3);
      }
    } catch (e) {
      console.error("Apply config failed:", e);
    }
    setApplyingConfig(false);
  };

  const handleEnableService = async () => {
    setServiceState("enabling");
    try {
      await enableService();
      setServiceState("done");
    } catch {
      setServiceState("skipped");
    }
  };

  const finish = async () => {
    setFinishError(null);
    try {
      await client.completeFirstRun();
      onClose();
    } catch {
      setFinishError("Could not reach the daemon. Please try again.");
    }
  };

  const installedModels = ollamaStatus?.models ?? [];
  const ollamaReady = ollamaStatus?.installed && ollamaStatus?.running;

  // Check if selected ollama model needs Ollama but it's not installed
  const needsOllama = (tag: string) => {
    const inEmbedder = EMBEDDER_MODELS.find((m) => m.tag === tag);
    const inLlm = LLM_MODELS.find((m) => m.tag === tag);
    const model = inEmbedder || inLlm;
    return model?.provider === "ollama" && !ollamaReady;
  };

  // Check if the selected LLM model needs downloading first
  const selectedLlmModel = LLM_MODELS.find((m) => m.tag === selectedLlm);
  const llmNeedsDownload =
    selectedLlmModel?.provider === "bundled-download" &&
    selectedLlmModel.downloadFilename != null &&
    !downloadedFiles.includes(selectedLlmModel.downloadFilename);

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40">
      <Card className="w-[42rem] max-h-[85vh] p-6 flex flex-col">
        <div className="label shrink-0">Welcome · step {step + 1} of 6</div>
        <div className="space-y-4 overflow-y-auto min-h-0 flex-1 mt-4">

        {/* ── Step 0: Welcome ── */}
        {step === 0 && (
          <>
            <h1 className="display text-2xl">Set up your memory vault</h1>
            <p className="text-text-muted font-body">
              Mnemos keeps a local-first vault of your AI conversations. Memories live as markdown
              files in <span className="mono">~/.local/share/mnemos/</span> (you can move it
              anytime in <strong>Settings &rarr; Storage</strong>).
            </p>
            <div className="flex justify-end">
              <Button onClick={() => setStep(1)}>Continue</Button>
            </div>
          </>
        )}

        {/* ── Step 1: Embedder ── */}
        {step === 1 && (
          <>
            <h1 className="display text-xl">Choose search model</h1>
            <p className="text-text-muted font-body text-sm">
              The embedder converts text into vectors for semantic search. A bundled model works
              immediately — or choose a higher-quality Ollama model.
            </p>

            {ollamaChecking && (
              <div className="label text-text-muted" aria-busy="true">Checking for Ollama…</div>
            )}

            <ModelPicker
              catalog={EMBEDDER_MODELS}
              selectedTag={selectedEmbedder}
              onSelect={(tag) => {
                if (!needsOllama(tag)) setSelectedEmbedder(tag);
              }}
              installedModels={installedModels}
              downloadedFiles={downloadedFiles}
              pullingModel={pullingModel}
              pullProgress={pullProgress}
              onPull={handlePullModel}
              onDownload={handleDownloadModel}
              downloadingModel={downloadingModel}
              downloadProgress={downloadProgress}
              label="Embedder model"
            />

            {/* Ollama install prompt if user selects an Ollama model */}
            {!ollamaReady && selectedEmbedder !== "bundled" && (
              <Card className="p-3 border-accent/30">
                <p className="text-sm text-text-muted">
                  Ollama is required for this model.
                </p>
                <Button
                  className="mt-2"
                  onClick={() => void handleInstallOllama()}
                  disabled={ollamaInstalling}
                >
                  {ollamaInstalling ? "Installing Ollama…" : "Install Ollama"}
                </Button>
              </Card>
            )}

            <div className="flex justify-between">
              <button className="label text-text-muted" onClick={() => setStep(0)}>Back</button>
              <Button
                onClick={() => void handleApplyAndContinue(1)}
                disabled={applyingConfig}
              >
                {applyingConfig ? "Applying…" : "Continue"}
              </Button>
            </div>
          </>
        )}

        {/* ── Step 2: LLM ── */}
        {step === 2 && (
          <>
            <h1 className="display text-xl">Choose learning model</h1>
            <p className="text-text-muted font-body text-sm">
              The learning pipeline extracts facts, builds reflections, and detects communities from
              your AI sessions. Choose a model below — it will be downloaded on first use if needed.
            </p>

            {ollamaChecking && (
              <div className="label text-text-muted" aria-busy="true">Checking for Ollama…</div>
            )}

            <ModelPicker
              catalog={LLM_MODELS}
              selectedTag={selectedLlm}
              onSelect={(tag) => {
                if (!needsOllama(tag)) setSelectedLlm(tag);
              }}
              installedModels={installedModels}
              downloadedFiles={downloadedFiles}
              pullingModel={pullingModel}
              pullProgress={pullProgress}
              onPull={handlePullModel}
              onDownload={handleDownloadModel}
              downloadingModel={downloadingModel}
              downloadProgress={downloadProgress}
              label="Chat LLM model"
            />

            {/* Ollama install prompt if not installed */}
            {!ollamaReady && selectedLlm !== "qwen3-0.6b" && LLM_MODELS.find((m) => m.tag === selectedLlm)?.provider === "ollama" && (
              <Card className="p-3 border-accent/30">
                <p className="text-sm text-text-muted">
                  Ollama is required for this model.
                </p>
                <Button
                  className="mt-2"
                  onClick={() => void handleInstallOllama()}
                  disabled={ollamaInstalling}
                >
                  {ollamaInstalling ? "Installing Ollama…" : "Install Ollama"}
                </Button>
              </Card>
            )}

            {/* Download prompt for bundled-download models */}
            {llmNeedsDownload && !downloadingModel && (
              <Card className="p-3 border-blue-500/30">
                <p className="text-sm text-text-muted">
                  This model needs to be downloaded ({selectedLlmModel?.size}). You can download it now
                  or proceed and it will be downloaded later when needed.
                </p>
              </Card>
            )}

            <div className="flex justify-between">
              <button className="label text-text-muted" onClick={() => setStep(1)}>Back</button>
              <Button
                onClick={() => void handleApplyAndContinue(2)}
                disabled={applyingConfig || downloadingModel != null}
              >
                {applyingConfig ? "Applying…" : "Continue"}
              </Button>
            </div>
          </>
        )}

        {/* ── Step 3: Background service ── */}
        {step === 3 && (
          <>
            <h1 className="display text-xl">Enable background memory</h1>
            <p className="text-text-muted font-body">
              Mnemos can run as a background service so every connected tool gets persistent memory
              automatically — even outside active terminal sessions.
            </p>
            <p className="text-text-muted font-body text-sm">
              This installs a systemd user unit via <span className="mono">mnemos service enable</span>.
              You can disable it anytime with <span className="mono">mnemos service disable</span>.
            </p>
            {serviceState === "idle" && (
              <Button onClick={() => void handleEnableService()}>
                Enable background service
              </Button>
            )}
            {serviceState === "enabling" && (
              <span className="label" aria-busy="true">Enabling…</span>
            )}
            {serviceState === "done" && (
              <span className="label text-accent">
                ✓ Background service enabled — Mnemos now runs in the background automatically.
              </span>
            )}
            {serviceState === "skipped" && (
              <span className="label text-text-muted">
                Could not enable the service automatically. You can run{" "}
                <span className="mono">mnemos service enable</span> in a terminal.
              </span>
            )}
            <div className="flex justify-between">
              <button className="label text-text-muted" onClick={() => setStep(2)}>Back</button>
              <div className="flex gap-2">
                {serviceState === "idle" && (
                  <button className="label text-text-muted" onClick={() => setStep(4)}>Skip</button>
                )}
                {(serviceState === "done" || serviceState === "skipped") && (
                  <Button onClick={() => setStep(4)}>Continue</Button>
                )}
              </div>
            </div>
          </>
        )}

        {/* ── Step 4: Connect tools ── */}
        {step === 4 && (
          <>
            <h1 className="display text-xl">Connect your AI tools</h1>
            <p className="text-text-muted font-body">
              The Mnemos daemon is running. Connect your AI tools below to give them persistent memory.
            </p>
            <Connections />
            <div className="flex justify-between pt-2">
              <button className="label text-text-muted" onClick={() => setStep(3)}>Back</button>
              <Button onClick={() => setStep(5)}>Continue</Button>
            </div>
          </>
        )}

        {/* ── Step 5: Done ── */}
        {step === 5 && (
          <>
            <h1 className="display text-xl">You&apos;re all set!</h1>
            <div className="space-y-2">
              <div className="flex items-center gap-2">
                <span className="text-green-400">✓</span>
                <span className="text-sm">
                  Search: <strong>{EMBEDDER_MODELS.find((m) => m.tag === selectedEmbedder)?.name ?? "Bundled"}</strong>
                </span>
              </div>
              <div className="flex items-center gap-2">
                <span className="text-green-400">✓</span>
                <span className="text-sm">
                  Learning: <strong>{LLM_MODELS.find((m) => m.tag === selectedLlm)?.name ?? "Qwen3 0.6B"}</strong>
                  {llmNeedsDownload && (
                    <span className="text-text-muted text-xs ml-1">(will download on first use)</span>
                  )}
                </span>
              </div>
              <div className="flex items-center gap-2">
                <span className={serviceState === "done" ? "text-green-400" : "text-text-muted"}>
                  {serviceState === "done" ? "✓" : "–"}
                </span>
                <span className="text-sm">
                  Background service {serviceState === "done" ? "enabled" : "skipped"}
                </span>
              </div>
            </div>
            <p className="text-text-muted font-body text-sm mt-3">
              You can change these anytime in <strong>Settings</strong>.
            </p>
            {finishError && (
              <p role="alert" className="label text-tier-procedural">{finishError}</p>
            )}
            <div className="flex justify-between pt-2">
              <button className="label text-text-muted" onClick={() => setStep(4)}>Back</button>
              <Button onClick={() => void finish()}>Finish setup</Button>
            </div>
          </>
        )}

        </div>
      </Card>
    </div>
  );
}
