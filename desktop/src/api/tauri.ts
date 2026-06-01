// Thin wrappers over the Rust shell commands. In a plain browser (vite dev /
// vitest) Tauri isn't present, so these degrade to no-ops / nulls.

export interface DaemonStatus {
  running: boolean;
  pid: number | null;
  detail: string;
}

async function invokeSafe<T>(cmd: string, args?: Record<string, unknown>): Promise<T | null> {
  try {
    const { invoke } = await import("@tauri-apps/api/core");
    return await invoke<T>(cmd, args);
  } catch {
    return null;
  }
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
