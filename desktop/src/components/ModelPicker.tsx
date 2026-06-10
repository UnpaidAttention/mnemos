import { Card, Button } from "../design/primitives";
import { Cpu, MonitorCog, AlertTriangle } from "lucide-react";

// ─── Model catalogs ────────────────────────────────────────────────────

export interface ModelEntry {
  /** Display name */
  name: string;
  /** Ollama model tag, or "bundled" for built-in models, or filename for direct-download */
  tag: string;
  /**
   * Provider:
   * - "bundled"          ships with the app (embedder MiniLM only)
   * - "bundled-download" uses the bundled llama-server but model downloads on first use
   * - "ollama"           needs Ollama installed
   */
  provider: "bundled" | "bundled-download" | "ollama";
  /** Human-readable download size */
  size: string;
  /** Embedding dimension (embedders only) */
  dim?: number;
  /** RAM requirement for running */
  ram?: string;
  /** VRAM requirement (GPU models only) */
  vram?: string;
  /** Short description */
  description: string;
  /** One-liner "ideal for" guidance */
  idealFor?: string;
  /** Hardware tier: CPU-friendly or GPU-accelerated */
  hardware: "cpu" | "gpu" | "any";
  /** Whether this is the recommended option */
  recommended?: boolean;
  /** Direct download URL (for bundled-download models) */
  downloadUrl?: string;
  /** Filename for the downloaded GGUF file */
  downloadFilename?: string;
}

// ─── Embedder Models ───────────────────────────────────────────────────

export const EMBEDDER_MODELS: ModelEntry[] = [
  {
    name: "MiniLM-L6-v2",
    tag: "bundled",
    provider: "bundled",
    size: "22 MB",
    dim: 384,
    hardware: "cpu",
    description: "Ships with Mnemos. Fast, lightweight, works offline with zero setup.",
    idealFor: "Quick start — no downloads needed",
  },
  {
    name: "nomic-embed-text",
    tag: "nomic-embed-text",
    provider: "ollama",
    size: "274 MB",
    dim: 768,
    ram: "1 GB",
    hardware: "cpu",
    description: "Best open embedder for its size. Excellent retrieval quality on any hardware.",
    idealFor: "Any machine with 8+ GB RAM",
    recommended: true,
  },
  {
    name: "mxbai-embed-large",
    tag: "mxbai-embed-large",
    provider: "ollama",
    size: "670 MB",
    dim: 1024,
    ram: "2 GB",
    hardware: "any",
    description: "Top-tier retrieval quality. Best for large knowledge bases.",
    idealFor: "Machines with 16+ GB RAM or a dedicated GPU",
  },
  {
    name: "snowflake-arctic-embed",
    tag: "snowflake-arctic-embed",
    provider: "ollama",
    size: "670 MB",
    dim: 1024,
    ram: "2 GB",
    hardware: "any",
    description: "Excellent for code and technical content retrieval.",
    idealFor: "Developers working with codebases and technical docs",
  },
];

// ─── LLM Models (Chat / Extraction) ───────────────────────────────────

export const LLM_MODELS: ModelEntry[] = [
  // ── CPU-Friendly Tier ──────────────────────────────────────────────
  {
    name: "Phi-4 Mini",
    tag: "phi4-mini",
    provider: "ollama",
    size: "~2.5 GB",
    ram: "4 GB",
    hardware: "cpu",
    description:
      "Best reasoning-to-size ratio available. Near-8B quality in a 3.8B package. " +
      "Strong structured output and JSON extraction.",
    idealFor: "CPU-only machines, laptops with 8+ GB RAM",
    recommended: true,
  },
  {
    name: "Gemma 4 E4B",
    tag: "gemma4:e4b",
    provider: "ollama",
    size: "~3 GB",
    ram: "4 GB",
    hardware: "cpu",
    description:
      "Google's edge-optimized model. Fast inference on modest hardware with " +
      "reliable tool calling and JSON generation.",
    idealFor: "Laptops, NUCs, any machine without a dedicated GPU",
  },
  {
    name: "Qwen3 4B",
    tag: "qwen3:4b",
    provider: "ollama",
    size: "~3 GB",
    ram: "4 GB",
    hardware: "cpu",
    description:
      "Strong multilingual support and coding tasks. Excellent at " +
      "JSON extraction and entity recognition.",
    idealFor: "Developers, multilingual workflows, 8+ GB RAM",
  },
  // ── GPU-Accelerated Tier ───────────────────────────────────────────
  {
    name: "Qwen3 8B",
    tag: "qwen3:8b",
    provider: "ollama",
    size: "~5 GB",
    ram: "10 GB",
    vram: "8 GB",
    hardware: "gpu",
    description:
      "Excellent structured output and multi-step reasoning. Great balance " +
      "of speed and quality for entity extraction.",
    idealFor: "Machines with 8+ GB VRAM (RTX 3060, RTX 4060)",
  },
  {
    name: "Gemma 4 12B",
    tag: "gemma4:12b",
    provider: "ollama",
    size: "~8 GB",
    ram: "12 GB",
    vram: "10 GB",
    hardware: "gpu",
    description:
      "Best quality for consumer GPUs. Top-tier entity extraction, " +
      "fact distillation, and reflective reasoning.",
    idealFor: "Machines with 12–16 GB VRAM (RTX 3080, RTX 4070+)",
    recommended: true,
  },
  {
    name: "Mistral Small 3.1",
    tag: "mistral-small3.1",
    provider: "ollama",
    size: "~14 GB",
    ram: "18 GB",
    vram: "16 GB",
    hardware: "gpu",
    description:
      "Best quality-per-RAM at this tier. Highly reliable instruction " +
      "following and complex structured outputs.",
    idealFor: "Workstations with 16+ GB VRAM (RTX 4080, A4000)",
  },
  {
    name: "Qwen3 27B",
    tag: "qwen3:27b",
    provider: "ollama",
    size: "~16 GB",
    ram: "22 GB",
    vram: "20 GB",
    hardware: "gpu",
    description:
      "Top overall quality for consumer hardware. Best for complex " +
      "multi-step reasoning and knowledge graph construction.",
    idealFor: "Workstations with 24 GB VRAM (RTX 3090, RTX 4090)",
  },
];

// ─── Component ─────────────────────────────────────────────────────────

interface ModelPickerProps {
  catalog: ModelEntry[];
  selectedTag: string;
  onSelect: (tag: string) => void;
  installedModels: string[];
  /** List of downloaded GGUF filenames (for bundled-download provider) */
  downloadedFiles?: string[];
  pullingModel: string | null;
  pullProgress: number; // 0–100
  onPull: (tag: string) => void;
  /** Called when a bundled-download model needs downloading */
  onDownload?: (model: ModelEntry) => void;
  /** Progress for an active bundled-download (0–100) */
  downloadingModel?: string | null;
  downloadProgress?: number;
  label?: string;
}

export function ModelPicker({
  catalog,
  selectedTag,
  onSelect,
  installedModels,
  downloadedFiles = [],
  pullingModel,
  pullProgress,
  onPull,
  onDownload,
  downloadingModel,
  downloadProgress = 0,
  label,
}: ModelPickerProps) {
  const cpuModels = catalog.filter((m) => m.hardware === "cpu" || m.hardware === "any");
  const gpuModels = catalog.filter((m) => m.hardware === "gpu");
  const selectedModel = catalog.find((m) => m.tag === selectedTag);
  const showGpuWarning = selectedModel?.hardware === "gpu";

  const renderModel = (m: ModelEntry) => {
    const isSelected = selectedTag === m.tag;
    const isOllamaInstalled =
      m.provider === "ollama" &&
      installedModels.some((im) => im.startsWith(m.tag.split(":")[0]));
    const isDownloaded =
      m.provider === "bundled" ||
      (m.provider === "bundled-download" &&
        m.downloadFilename != null &&
        downloadedFiles.includes(m.downloadFilename)) ||
      isOllamaInstalled;
    const isPulling = pullingModel === m.tag;
    const isDownloading = downloadingModel === m.tag;

    return (
      <Card
        key={m.tag}
        className={`p-3 cursor-pointer transition-all ${
          isSelected
            ? "ring-2 ring-accent bg-surface-raised"
            : "hover:bg-surface-raised"
        }`}
        onClick={() => onSelect(m.tag)}
        id={`model-${m.tag.replace(/[:.]/g, "-")}`}
      >
        <div className="flex items-start gap-3">
          {/* Radio indicator */}
          <div className="mt-1 shrink-0">
            <div
              className={`w-4 h-4 rounded-full border-2 flex items-center justify-center ${
                isSelected ? "border-accent" : "border-border"
              }`}
            >
              {isSelected && (
                <div className="w-2 h-2 rounded-full bg-accent" />
              )}
            </div>
          </div>

          {/* Model info */}
          <div className="flex-1 min-w-0">
            <div className="flex items-center gap-2 flex-wrap">
              <span className="font-semibold text-sm">{m.name}</span>
              {m.recommended && (
                <span className="text-[10px] font-bold px-1.5 py-0.5 rounded bg-accent/20 text-accent uppercase tracking-wider">
                  ★ Recommended
                </span>
              )}
              {m.provider === "bundled" && (
                <span className="text-[10px] font-bold px-1.5 py-0.5 rounded bg-status-ok/20 text-status-ok uppercase tracking-wider">
                  ✓ Bundled
                </span>
              )}
              {(m.provider === "bundled-download" || m.provider === "ollama") && isDownloaded && (
                <span className="text-[10px] font-bold px-1.5 py-0.5 rounded bg-status-ok/20 text-status-ok uppercase tracking-wider">
                  ✓ Downloaded
                </span>
              )}
              {m.provider === "bundled-download" && !isDownloaded && !isDownloading && (
                <span className="text-[10px] font-bold px-1.5 py-0.5 rounded bg-status-info/20 text-status-info uppercase tracking-wider">
                  Direct Download
                </span>
              )}
            </div>
            <p className="text-text-muted text-xs mt-0.5">{m.description}</p>

            {/* Hardware / resource badges */}
            <div className="flex items-center gap-3 mt-1.5 text-[11px] text-text-muted flex-wrap">
              <span>{m.size}</span>
              {m.ram && <span>• {m.ram} RAM</span>}
              {m.vram && <span>• {m.vram} VRAM</span>}
              {m.dim && <span>• {m.dim}d</span>}
            </div>

            {/* "Ideal for" guidance */}
            {m.idealFor && (
              <p className="text-[10px] text-text-dim mt-1 italic">
                Ideal for: {m.idealFor}
              </p>
            )}

            {/* Download button for bundled-download models */}
            {m.provider === "bundled-download" && !isDownloaded && !isDownloading && (
              <Button
                className="mt-2 text-xs"
                onClick={(e) => {
                  e.stopPropagation();
                  onDownload?.(m);
                }}
              >
                Download ({m.size})
              </Button>
            )}
            {isDownloading && (
              <div className="mt-2">
                <div className="flex items-center gap-2 text-xs text-accent">
                  <span aria-busy="true">Downloading…</span>
                  <span>{downloadProgress}%</span>
                </div>
                <div className="w-full h-1.5 rounded-full bg-surface-sunken mt-1 overflow-hidden">
                  <div
                    className="h-full rounded-full bg-accent transition-all duration-300"
                    style={{ width: `${downloadProgress}%` }}
                  />
                </div>
              </div>
            )}

            {/* Download button for Ollama models */}
            {m.provider === "ollama" && !isOllamaInstalled && !isPulling && (
              <Button
                className="mt-2 text-xs"
                onClick={(e) => {
                  e.stopPropagation();
                  onPull(m.tag);
                }}
              >
                Download ({m.size})
              </Button>
            )}
            {isPulling && (
              <div className="mt-2">
                <div className="flex items-center gap-2 text-xs text-accent">
                  <span aria-busy="true">Downloading…</span>
                  <span>{pullProgress}%</span>
                </div>
                <div className="w-full h-1.5 rounded-full bg-surface-sunken mt-1 overflow-hidden">
                  <div
                    className="h-full rounded-full bg-accent transition-all duration-300"
                    style={{ width: `${pullProgress}%` }}
                  />
                </div>
              </div>
            )}
          </div>
        </div>
      </Card>
    );
  };

  return (
    <div className="space-y-3">
      {label && <div className="label text-text-muted text-xs mb-1">{label}</div>}

      {/* GPU warning banner */}
      {showGpuWarning && (
        <div className="flex items-start gap-2.5 p-3 rounded-lg border border-status-warn/30 bg-status-warn/5">
          <AlertTriangle size={16} className="text-status-warn shrink-0 mt-0.5" />
          <div className="text-xs text-text-muted">
            <span className="font-semibold text-text">Dedicated GPU recommended.</span>{" "}
            This model requires {selectedModel?.vram ?? "significant VRAM"} to run at full speed.
            Running on CPU-only hardware will be very slow.
          </div>
        </div>
      )}

      {/* CPU-Friendly tier */}
      {cpuModels.length > 0 && (
        <div className="space-y-2">
          <div className="flex items-center gap-2 text-xs text-text-muted">
            <Cpu size={13} strokeWidth={2} />
            <span className="label">CPU-Friendly — No GPU Required</span>
          </div>
          {cpuModels.map(renderModel)}
        </div>
      )}

      {/* GPU-Accelerated tier */}
      {gpuModels.length > 0 && (
        <div className="space-y-2 mt-4">
          <div className="flex items-center gap-2 text-xs text-text-muted">
            <MonitorCog size={13} strokeWidth={2} />
            <span className="label">GPU-Accelerated — Dedicated GPU</span>
          </div>
          {gpuModels.map(renderModel)}
        </div>
      )}
    </div>
  );
}
