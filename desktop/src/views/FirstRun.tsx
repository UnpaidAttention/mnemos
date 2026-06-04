import { useState } from "react";
import { client } from "../api/client";
import { Button, Card } from "../design/primitives";
import { Connections } from "./Connections";

type Step = 0 | 1 | 2;

export function FirstRun({ onClose }: { onClose: () => void }) {
  const [step, setStep] = useState<Step>(0);

  const finish = async () => {
    await client.completeFirstRun();
    onClose();
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40">
      <Card className="w-[40rem] p-6 space-y-4">
        <div className="label">Welcome · step {step + 1} of 3</div>
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
            <h1 className="display text-xl">Connect your AI tools</h1>
            <p className="text-text-muted font-body">
              The Mnemos daemon is running with the bundled embedder. Connect your AI tools below to
              give them persistent memory.
            </p>
            <Connections />
            <div className="flex justify-between">
              <button className="label text-text-muted" onClick={() => setStep(1)}>
                Back
              </button>
              <Button onClick={finish}>Finish setup</Button>
            </div>
          </>
        )}
      </Card>
    </div>
  );
}
