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

/** Enables the mnemos background service so hooks fire outside CLI sessions. */
export function enableService(): Promise<{ enabled: boolean } | null> {
  return invokeSafe<{ enabled: boolean }>("enable_service");
}
