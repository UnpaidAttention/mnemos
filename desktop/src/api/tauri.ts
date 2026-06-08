// Thin wrappers over the Rust shell commands. In a plain browser (vite dev /
// vitest) Tauri isn't present, so these degrade to no-ops / nulls.

export interface DaemonStatus {
  running: boolean;
  pid: number | null;
  detail: string;
}

async function invokeSafe<T>(cmd: string, args?: Record<string, unknown>): Promise<T | null> {
  let core: typeof import("@tauri-apps/api/core");
  try {
    core = await import("@tauri-apps/api/core");
  } catch {
    return null; // Tauri runtime not present
  }
  const present =
    typeof core.isTauri === "function"
      ? core.isTauri()
      : typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
  if (!present) return null;
  return await core.invoke<T>(cmd, args); // real command errors propagate
}

export function pickVaultDir(): Promise<string | null> {
  return invokeSafe<string | null>("pick_vault_dir").then((r) => r ?? null);
}

export function daemonStatus(): Promise<DaemonStatus | null> {
  return invokeSafe<DaemonStatus>("daemon_status");
}

export function moveVault(newPath: string): Promise<{ moved_to: string } | null> {
  return invokeSafe<{ moved_to: string }>("move_vault", { newPath });
}

/**
 * Enables the mnemos background service so hooks fire outside CLI sessions.
 * Returns null when the Tauri runtime is absent (browser/test environment),
 * NOT as a signal that the service was enabled.
 */
export function enableService(): Promise<{ enabled: boolean } | null> {
  return invokeSafe<{ enabled: boolean }>("enable_service");
}

// ─── Ollama + model management ─────────────────────────────────────────

export interface OllamaStatus {
  installed: boolean;
  running: boolean;
  version: string | null;
  models: string[];
}

/** Detect if Ollama is installed + running, list downloaded models. */
export function checkOllama(): Promise<OllamaStatus | null> {
  return invokeSafe<OllamaStatus>("check_ollama");
}

/** Download and install Ollama. Emits 'ollama-install-progress' events. */
export function installOllama(): Promise<null> {
  return invokeSafe("install_ollama");
}

/** Pull (download) an Ollama model. Emits 'model-pull-progress' events. */
export function pullModel(model: string): Promise<null> {
  return invokeSafe("pull_model", { model });
}

/** Write [llm] config and restart daemon. */
export function applyLlmConfig(kind: string, model: string): Promise<null> {
  return invokeSafe("apply_llm_config", { kind, model });
}

/** Write [embedder] config and restart daemon. */
export function applyEmbedderConfig(kind: string, model: string, dim: number): Promise<null> {
  return invokeSafe("apply_embedder_config", { kind, model, dim });
}

// ─── In-app updates ────────────────────────────────────────────────────

export interface UpdateInfo {
  current_version: string;
  latest_version: string;
  update_available: boolean;
  release_url: string;
  release_notes: string;
  asset_name: string | null;
  asset_url: string | null;
}

/** Check GitHub releases for a newer version. */
export function checkForUpdates(): Promise<UpdateInfo | null> {
  return invokeSafe<UpdateInfo>("check_for_updates");
}

/** Download and install an update package. */
export function installUpdate(assetUrl: string, assetName: string): Promise<null> {
  return invokeSafe("install_update", { assetUrl, assetName });
}
