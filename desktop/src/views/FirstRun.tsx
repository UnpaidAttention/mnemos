import { useState } from "react";
import { client } from "../api/client";
import { enableService } from "../api/tauri";
import { Button, Card } from "../design/primitives";
import { Connections } from "./Connections";

// 0: vault intro, 1: embedder, 2: background service, 3: connect tools
type Step = 0 | 1 | 2 | 3;

export function FirstRun({ onClose }: { onClose: () => void }) {
  const [step, setStep] = useState<Step>(0);
  const [serviceState, setServiceState] = useState<"idle" | "enabling" | "done" | "skipped">(
    "idle",
  );
  const [finishError, setFinishError] = useState<string | null>(null);

  const finish = async () => {
    setFinishError(null);
    try {
      await client.completeFirstRun();
      onClose();
    } catch {
      // Non-fatal: allow the user to retry; wizard stays visible
      setFinishError("Could not reach the daemon. Please try again.");
    }
  };

  const handleEnableService = async () => {
    setServiceState("enabling");
    try {
      await enableService();
      setServiceState("done");
    } catch {
      // Non-fatal: user can do it manually; proceed anyway
      setServiceState("skipped");
    }
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40">
      <Card className="w-[40rem] max-h-[80vh] p-6 flex flex-col">
        <div className="label shrink-0">Welcome · step {step + 1} of 4</div>
        <div className="space-y-4 overflow-y-auto min-h-0 flex-1 mt-4">
        {step === 0 && (
          <>
            <h1 className="display text-2xl">Set up your memory vault</h1>
            <p className="text-text-muted font-body">
              mnemos keeps a local-first vault of your AI conversations. Memories live as markdown
              files in <span className="mono">~/.local/share/mnemos/</span> (you can move it
              anytime in <strong>Settings &rarr; Storage</strong>).
            </p>
            <div className="flex justify-end">
              <Button onClick={() => setStep(1)}>Continue</Button>
            </div>
          </>
        )}
        {step === 1 && (
          <>
            <h1 className="display text-xl">Embedder</h1>
            <p className="text-text-muted font-body">
              Mnemos ships with a local 22 MB embedder (all-MiniLM-L6-v2). Semantic recall works
              immediately — no setup needed.
            </p>
            <p className="text-text-muted font-body text-sm">
              To use Ollama or OpenAI for embeddings instead, switch in{" "}
              <strong>Settings &rarr; Embedder</strong> after the wizard.
            </p>
            <div className="flex items-center gap-3">
              <span className="label">✓ Bundled embedder ready</span>
            </div>
            <div className="flex justify-between">
              <button className="label text-text-muted" onClick={() => setStep(0)}>
                Back
              </button>
              <Button onClick={() => setStep(2)}>Continue</Button>
            </div>
          </>
        )}
        {step === 2 && (
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
              <button className="label text-text-muted" onClick={() => setStep(1)}>
                Back
              </button>
              <div className="flex gap-2">
                {serviceState === "idle" && (
                  <button className="label text-text-muted" onClick={() => setStep(3)}>
                    Skip
                  </button>
                )}
                {(serviceState === "done" || serviceState === "skipped") && (
                  <Button onClick={() => setStep(3)}>Continue</Button>
                )}
              </div>
            </div>
          </>
        )}
        {step === 3 && (
          <>
            <h1 className="display text-xl">Connect your AI tools</h1>
            <p className="text-text-muted font-body">
              The Mnemos daemon is running with the bundled embedder. Connect your AI tools below to
              give them persistent memory.
            </p>
            <Connections />
            {finishError && (
              <p role="alert" className="label text-tier-procedural">{finishError}</p>
            )}
            <div className="flex justify-between pt-2">
              <button className="label text-text-muted" onClick={() => setStep(2)}>
                Back
              </button>
              <Button onClick={() => void finish()}>Finish setup</Button>
            </div>
          </>
        )}
        </div>
      </Card>
    </div>
  );
}
