import { Card, Button } from "../design/primitives";

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
  /** Short description */
  description: string;
  /** Whether this is the recommended option */
  recommended?: boolean;
  /** Direct download URL (for bundled-download models) */
  downloadUrl?: string;
  /** Filename for the downloaded GGUF file */
  downloadFilename?: string;
}

export const EMBEDDER_MODELS: ModelEntry[] = [
  {
    name: "MiniLM-L6-v2",
    tag: "bundled",
    provider: "bundled",
    size: "22 MB",
    dim: 384,
    description: "Ships with Mnemos. Fast, lightweight, works offline with zero setup.",
  },
  {
    name: "nomic-embed-text",
    tag: "nomic-embed-text",
    provider: "ollama",
    size: "274 MB",
    dim: 768,
    ram: "1 GB",
    description: "Best open embedder for its size. Great all-around retrieval quality.",
    recommended: true,
  },
  {
    name: "mxbai-embed-large",
    tag: "mxbai-embed-large",
    provider: "ollama",
    size: "670 MB",
    dim: 1024,
    ram: "2 GB",
    description: "Top-tier retrieval quality. Best for large knowledge bases.",
  },
  {
    name: "snowflake-arctic-embed",
    tag: "snowflake-arctic-embed",
    provider: "ollama",
    size: "670 MB",
    dim: 1024,
    ram: "2 GB",
    description: "Excellent for code and technical content.",
  },
];

export const LLM_MODELS: ModelEntry[] = [
  {
    name: "Qwen3 0.6B",
    tag: "qwen3-0.6b",
    provider: "bundled-download",
    size: "462 MB",
    ram: "1 GB",
    description: "Minimal footprint, works on any hardware. Uses the built-in inference engine.",
    downloadUrl:
      "https://huggingface.co/bartowski/Qwen_Qwen3-0.6B-GGUF/resolve/main/Qwen_Qwen3-0.6B-Q4_K_M.gguf",
    downloadFilename: "Qwen3-0.6B-Q4_K_M.gguf",
  },
  {
    name: "Gemma 4 E4B",
    tag: "gemma4:e4b",
    provider: "ollama",
    size: "~3 GB",
    ram: "4 GB",
    description: "Google's edge-optimized model. Fast inference on modest hardware.",
  },
  {
    name: "Phi-4 Mini",
    tag: "phi4-mini",
    provider: "ollama",
    size: "~2.5 GB",
    ram: "4 GB",
    description: "Strong structured output and reasoning in a compact package.",
  },
  {
    name: "Qwen3 4B",
    tag: "qwen3:4b",
    provider: "ollama",
    size: "~3 GB",
    ram: "4 GB",
    description: "Excellent quality-to-size ratio. Great at JSON extraction.",
  },
  {
    name: "Gemma 4 12B",
    tag: "gemma4:12b",
    provider: "ollama",
    size: "~8 GB",
    ram: "10 GB",
    description: "Best quality for laptops with 16 GB RAM. Top-tier fact extraction.",
    recommended: true,
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
  return (
    <div className="space-y-2">
      {label && <div className="label text-text-muted text-xs mb-1">{label}</div>}
      {catalog.map((m) => {
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
                ? "ring-2 ring-accent bg-surface-hover"
                : "hover:bg-surface-hover"
            }`}
            onClick={() => {
              onSelect(m.tag);
            }}
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
                    <span className="text-[10px] font-bold px-1.5 py-0.5 rounded bg-green-500/20 text-green-400 uppercase tracking-wider">
                      ✓ Bundled
                    </span>
                  )}
                  {m.provider === "bundled-download" && isDownloaded && (
                    <span className="text-[10px] font-bold px-1.5 py-0.5 rounded bg-green-500/20 text-green-400 uppercase tracking-wider">
                      ✓ Downloaded
                    </span>
                  )}
                  {m.provider === "bundled-download" && !isDownloaded && !isDownloading && (
                    <span className="text-[10px] font-bold px-1.5 py-0.5 rounded bg-blue-500/20 text-blue-400 uppercase tracking-wider">
                      Direct Download
                    </span>
                  )}
                  {m.provider === "ollama" && isOllamaInstalled && (
                    <span className="text-[10px] font-bold px-1.5 py-0.5 rounded bg-green-500/20 text-green-400 uppercase tracking-wider">
                      ✓ Downloaded
                    </span>
                  )}
                </div>
                <p className="text-text-muted text-xs mt-0.5">{m.description}</p>
                <div className="flex items-center gap-3 mt-1.5 text-[11px] text-text-muted">
                  <span>{m.size}</span>
                  {m.ram && <span>• {m.ram} RAM</span>}
                  {m.dim && <span>• {m.dim}d</span>}
                </div>

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
                    <div className="w-full h-1.5 rounded-full bg-surface-hover mt-1 overflow-hidden">
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
                    <div className="w-full h-1.5 rounded-full bg-surface-hover mt-1 overflow-hidden">
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
      })}
    </div>
  );
}
