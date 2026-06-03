import { useEffect, useState } from "react";
import { client, type Connector, type ConnectorPreview } from "../api/client";
import { Button, Card } from "../design/primitives";

type PreviewState = { connectorId: string; preview: ConnectorPreview };

function statusLabel(c: Connector): string {
  if (c.connected === "full") return "Connected";
  if (c.connected === "partial") return "Partially connected";
  if (c.kind === "detectable" && !c.installed) return "Not installed";
  return "Installed";
}

export function Connections() {
  const [connectors, setConnectors] = useState<Connector[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [preview, setPreview] = useState<PreviewState | null>(null);
  const [busy, setBusy] = useState<string | null>(null);

  const load = () => {
    setError(null);
    client
      .listConnectors()
      .then(setConnectors)
      .catch(() => setError("Couldn't reach the daemon to list AI tools."));
  };

  useEffect(load, []);

  const handleConnect = async (id: string) => {
    const p = await client.previewConnector(id);
    setPreview({ connectorId: id, preview: p });
  };

  const handleApply = async () => {
    if (!preview) return;
    setBusy(preview.connectorId);
    try {
      await client.connectConnector(preview.connectorId);
      setPreview(null);
      load();
    } finally {
      setBusy(null);
    }
  };

  const handleDisconnect = async (id: string) => {
    setBusy(id);
    try {
      await client.disconnectConnector(id);
      load();
    } finally {
      setBusy(null);
    }
  };

  if (error) {
    return (
      <p role="alert" className="text-tier-procedural font-body text-sm">
        {error}
      </p>
    );
  }

  if (!connectors) {
    return <div className="label text-text-muted">Loading AI tool connections…</div>;
  }

  return (
    <div className="space-y-3">
      {connectors.map((c) => (
        <Card key={c.id} className="p-4 space-y-2">
          <div className="flex items-start justify-between gap-4">
            <div className="space-y-1">
              <div className="display text-base">{c.display_name}</div>
              <span className="label text-text-muted">{statusLabel(c)}</span>
              {c.deprecated && (
                <div className="label text-tier-procedural">
                  Deprecated: {c.deprecated}
                </div>
              )}
            </div>
            <div className="flex gap-2 shrink-0">
              {c.kind === "detectable" && c.installed && c.connected !== "full" && (
                <Button
                  variant="primary"
                  disabled={busy === c.id}
                  onClick={() => void handleConnect(c.id)}
                >
                  Connect
                </Button>
              )}
              {c.connected !== "none" && (
                <Button
                  variant="ghost"
                  disabled={busy === c.id}
                  onClick={() => void handleDisconnect(c.id)}
                >
                  Disconnect
                </Button>
              )}
            </div>
          </div>

          {c.kind === "manual" && c.manual_snippet && (
            <div className="space-y-1">
              <div className="label text-text-muted">{c.manual_snippet.target}</div>
              <pre className="mono text-xs bg-surface border border-border rounded-md p-3 overflow-x-auto">
                {c.manual_snippet.snippet}
              </pre>
            </div>
          )}

          {preview?.connectorId === c.id && (
            <div className="space-y-2 pt-2 border-t border-border">
              {preview.preview.edits.map((e) => (
                <div key={e.path} className="space-y-1">
                  <div className="mono text-xs text-text-muted">{e.path}</div>
                  <pre className="mono text-xs bg-surface border border-border rounded-md p-3 overflow-x-auto">
                    {e.after}
                  </pre>
                </div>
              ))}
              <div className="flex gap-2">
                <Button onClick={() => void handleApply()} disabled={busy === c.id}>
                  Apply changes
                </Button>
                <Button variant="ghost" onClick={() => setPreview(null)}>
                  Cancel
                </Button>
              </div>
            </div>
          )}
        </Card>
      ))}
    </div>
  );
}
