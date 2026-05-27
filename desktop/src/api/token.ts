// Reads the bearer token via the Tauri command (secret stays in the Rust shell).
// In a plain browser (vitest / `vite dev` without Tauri) falls back to a dev token.
export async function getToken(): Promise<string> {
  try {
    const { invoke } = await import("@tauri-apps/api/core");
    return await invoke<string>("read_token");
  } catch {
    return import.meta.env.VITE_MNEMOS_TOKEN ?? "dev-token";
  }
}
