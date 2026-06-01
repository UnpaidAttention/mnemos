import { useEffect, useState } from "react";
import { client } from "../api/client";
import { pickVaultDir, moveVault } from "../api/tauri";
import { Button, Card } from "../design/primitives";

type Phase = "idle" | "picked" | "moving" | "done" | "error";

export function StorageSettings() {
  const [current, setCurrent] = useState<string | null>(null);
  const [target, setTarget] = useState<string | null>(null);
  const [phase, setPhase] = useState<Phase>("idle");
  const [message, setMessage] = useState<string>("");
  const [loading, setLoading] = useState<boolean>(true);
  const [loadError, setLoadError] = useState<string | null>(null);

  useEffect(() => {
    void client
      .getConfig()
      .then((c) => {
        const root = (c as { vault?: { root?: string } }).vault?.root ?? null;
        setCurrent(root);
      })
      .catch(() => {
        setLoadError("Couldn't reach the daemon to read your storage location.");
      })
      .finally(() => {
        setLoading(false);
      });
  }, []);

  const pick = async () => {
    const dir = await pickVaultDir();
    if (dir) {
      setTarget(dir);
      setPhase("picked");
    }
  };

  const confirmMove = async () => {
    if (!target) return;
    setPhase("moving");
    setMessage("Moving your vault and restarting the daemon…");
    try {
      const res = await moveVault(target);
      if (!res) throw new Error("Move is only available in the desktop app.");
      setCurrent(res.moved_to);
      setTarget(null);
      setPhase("done");
      setMessage(`Moved to ${res.moved_to}`);
    } catch (e) {
      setPhase("error");
      setMessage(e instanceof Error ? e.message : "Move failed");
    }
  };

  return (
    <Card className="p-4 space-y-3">
      <h2 className="display text-lg">Storage</h2>

      {loading && <p className="label" aria-busy="true">Loading…</p>}

      {loadError && (
        <p className="label text-tier-procedural" role="alert">{loadError}</p>
      )}

      {!loading && !loadError && (
        <div className="font-body text-text-muted">
          Current location: <span className="mono">{current ?? "unknown"}</span>
        </div>
      )}

      {!loading && !loadError && phase !== "picked" && phase !== "moving" && (
        <Button onClick={pick}>Change location…</Button>
      )}

      {phase === "picked" && target && (
        <div className="space-y-2">
          <p className="font-body">
            Move your vault from <span className="mono">{current}</span> to{" "}
            <span className="mono">{target}</span>? The daemon will restart.
          </p>
          <div className="flex gap-2">
            <Button onClick={confirmMove}>Move my data</Button>
            <Button variant="ghost" className="label text-text-muted" onClick={() => { setTarget(null); setPhase("idle"); }}>
              Cancel
            </Button>
          </div>
        </div>
      )}

      {phase === "moving" && <p className="label" aria-busy="true">{message}</p>}
      {phase === "done" && <p className="label text-accent">{message}</p>}
      {phase === "error" && <p className="label text-tier-procedural" role="alert">{message}</p>}
    </Card>
  );
}
