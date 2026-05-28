import { useEffect, useState } from "react";
import { check } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { Button } from "../design/primitives";

type State =
  | { kind: "idle" }
  | { kind: "available"; version: string; download: () => Promise<void> }
  | { kind: "downloading" }
  | { kind: "ready_to_relaunch" }
  | { kind: "error"; message: string };

export function UpdateBanner() {
  const [state, setState] = useState<State>({ kind: "idle" });

  useEffect(() => {
    let cancelled = false;
    void (async () => {
      try {
        const update = await check();
        if (cancelled || !update) return;
        setState({
          kind: "available",
          version: update.version,
          download: async () => {
            setState({ kind: "downloading" });
            try {
              await update.downloadAndInstall();
              setState({ kind: "ready_to_relaunch" });
            } catch (e) {
              setState({
                kind: "error",
                message: e instanceof Error ? e.message : "update failed",
              });
            }
          },
        });
      } catch {
        // Not running under Tauri (e.g., vitest jsdom), or no network —
        // leave the banner idle.
      }
    })();
    return () => { cancelled = true; };
  }, []);

  if (state.kind === "idle") return null;

  if (state.kind === "available") {
    return (
      <div
        role="status"
        className="flex items-center justify-between gap-3 border-b border-border bg-surface-raised px-4 py-2 text-sm"
      >
        <span className="font-body">
          A new version is available — <span className="mono">{state.version}</span>
        </span>
        <div className="flex items-center gap-2">
          <Button variant="ghost" onClick={() => setState({ kind: "idle" })}>
            Later
          </Button>
          <Button onClick={() => void state.download()}>Install</Button>
        </div>
      </div>
    );
  }

  if (state.kind === "downloading") {
    return (
      <div
        role="status"
        className="border-b border-border bg-surface-raised px-4 py-2 text-sm text-text-muted"
      >
        Downloading update…
      </div>
    );
  }

  if (state.kind === "ready_to_relaunch") {
    return (
      <div
        role="status"
        className="flex items-center justify-between gap-3 border-b border-border bg-surface-raised px-4 py-2 text-sm"
      >
        <span className="font-body">Update installed. Relaunch to apply.</span>
        <Button onClick={() => void relaunch()}>Relaunch now</Button>
      </div>
    );
  }

  return (
    <div
      role="alert"
      className="border-b border-border bg-surface-raised px-4 py-2 text-sm text-tier-procedural"
      title={state.message}
    >
      Update failed: {state.message}
    </div>
  );
}
