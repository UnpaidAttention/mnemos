import { useEffect, useState } from "react";
import { client } from "../api/client";
import { Button, Card } from "../design/primitives";

type Step = 0 | 1 | 2;

export function FirstRun({ onClose }: { onClose: () => void }) {
  const [step, setStep] = useState<Step>(0);
  const [ollamaModels, setOllamaModels] = useState<string[] | null>(null);
  const [ollamaError, setOllamaError] = useState<string | null>(null);
  const [pulling, setPulling] = useState(false);

  useEffect(() => {
    if (step !== 1) return;
    void (async () => {
      try {
        const cfg = (await client.getConfig()) as { embedder: { url: string } };
        const res = await fetch(`${cfg.embedder.url}/api/tags`);
        if (!res.ok) throw new Error(`Ollama responded ${res.status}`);
        const j = (await res.json()) as { models?: { name: string }[] };
        setOllamaModels((j.models ?? []).map((m) => m.name));
      } catch (e) {
        setOllamaError(e instanceof Error ? e.message : "unreachable");
      }
    })();
  }, [step]);

  const pullEmbed = async () => {
    setPulling(true);
    try {
      const cfg = (await client.getConfig()) as { embedder: { url: string } };
      await fetch(`${cfg.embedder.url}/api/pull`, {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ name: "nomic-embed-text" }),
      });
      setOllamaModels((m) => (m ?? []).concat("nomic-embed-text"));
    } finally {
      setPulling(false);
    }
  };

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
              files in <span className="mono">~/.local/share/mnemos/</span> (you can change this in
              Settings).
            </p>
            <div className="flex justify-end">
              <Button onClick={() => setStep(1)}>Continue</Button>
            </div>
          </>
        )}
        {step === 1 && (
          <>
            <h1 className="display text-xl">Embedder · Ollama</h1>
            {ollamaModels === null && !ollamaError && (
              <p className="text-text-muted">Checking Ollama…</p>
            )}
            {ollamaError && (
              <p className="text-tier-procedural">
                Ollama isn&apos;t running. Install from ollama.com and start it, then click Retry.
              </p>
            )}
            {ollamaModels && (
              <div>
                <p className="text-text-muted">
                  Found {ollamaModels.length} installed model
                  {ollamaModels.length === 1 ? "" : "s"}.
                </p>
                {!ollamaModels.includes("nomic-embed-text") ? (
                  <Button onClick={pullEmbed} disabled={pulling}>
                    {pulling ? "Pulling nomic-embed-text…" : "Pull nomic-embed-text"}
                  </Button>
                ) : (
                  <p className="label">✓ nomic-embed-text installed</p>
                )}
              </div>
            )}
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
              Copy a snippet into each tool&apos;s config to use mnemos as its memory provider.
            </p>
            <details open>
              <summary className="display text-base cursor-pointer">Claude Code</summary>
              <pre className="mono text-xs bg-surface border border-border rounded-md p-3 overflow-x-auto">
                {`{"mcpServers":{"mnemos":{"command":"mnemos-mcp-stdio"}}}`}
              </pre>
            </details>
            <details>
              <summary className="display text-base cursor-pointer">
                Codex / OpenAI function-calling
              </summary>
              <pre className="mono text-xs bg-surface border border-border rounded-md p-3 overflow-x-auto">
                {`see adapters/openai-functions/schema.json`}
              </pre>
            </details>
            <details>
              <summary className="display text-base cursor-pointer">Generic MCP</summary>
              <pre className="mono text-xs bg-surface border border-border rounded-md p-3 overflow-x-auto">
                {`see adapters/generic-mcp/example.json`}
              </pre>
            </details>
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
