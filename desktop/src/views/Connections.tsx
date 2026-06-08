import { useEffect, useRef, useState } from "react";
import { client, type Connector, type ConnectorPreview } from "../api/client";
import { Button, Card } from "../design/primitives";

type PreviewState = { connectorId: string; preview: ConnectorPreview };

// ── Status helpers ──────────────────────────────────────────────────────────────

type StatusInfo = { label: string; color: string; dot: string };

function connectorStatus(c: Connector): StatusInfo {
  if (c.connected === "full") {
    return { label: "Connected", color: "text-accent", dot: "bg-accent" };
  }
  if (c.connected === "partial") {
    return { label: "Partially connected", color: "text-tier-semantic", dot: "bg-tier-semantic" };
  }
  if (c.kind === "detectable" && c.installed) {
    return { label: "Installed · Not connected", color: "text-text-muted", dot: "bg-text-muted" };
  }
  if (c.kind === "detectable" && !c.installed) {
    return { label: "Not detected", color: "text-text-muted/50", dot: "bg-text-muted/50" };
  }
  // manual
  return { label: "Manual setup", color: "text-text-muted", dot: "bg-text-muted" };
}

function autonomyLabel(status?: string): { text: string; color: string } | null {
  switch (status) {
    case "autonomous":
      return { text: "Fully autonomous", color: "text-accent" };
    case "connected":
      return { text: "MCP connected", color: "text-tier-semantic" };
    case "not_installed":
      return { text: "Not installed", color: "text-text-muted/60" };
    default:
      return null;
  }
}

// ── Connector card ──────────────────────────────────────────────────────────────

function ConnectorCard({
  c,
  busy,
  preview,
  onConnect,
  onDisconnect,
  onApply,
  onCancelPreview,
}: {
  c: Connector;
  busy: string | null;
  preview: PreviewState | null;
  onConnect: (id: string) => void;
  onDisconnect: (id: string) => void;
  onApply: () => void;
  onCancelPreview: () => void;
}) {
  const status = connectorStatus(c);
  const isDeprecated = !!c.deprecated;
  const showPreview = preview?.connectorId === c.id;
  const autonomy = autonomyLabel(c.autonomy_status);

  return (
    <div
      className={`rounded-lg border transition-colors duration-150 ${
        c.connected === "full"
          ? "border-accent/30 bg-accent/[0.03]"
          : c.connected === "partial"
            ? "border-tier-semantic/30 bg-tier-semantic/[0.03]"
            : "border-border bg-surface/50"
      } ${isDeprecated ? "opacity-60" : ""}`}
    >
      <div className="flex items-center justify-between gap-4 px-4 py-3">
        {/* Left: name + status */}
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2">
            <span
              className={`inline-block h-2 w-2 rounded-full shrink-0 ${status.dot}`}
              aria-hidden
            />
            <span className="display text-sm truncate">{c.display_name}</span>
          </div>
          <div className={`text-xs mt-0.5 ml-4 ${status.color}`}>{status.label}</div>
          {autonomy && (
            <div className={`text-xs mt-0.5 ml-4 ${autonomy.color}`}>{autonomy.text}</div>
          )}
          {isDeprecated && (
            <div className="text-xs mt-0.5 ml-4 text-tier-procedural">{c.deprecated}</div>
          )}
        </div>

        {/* Right: actions */}
        <div className="flex gap-2 shrink-0">
          {c.kind === "detectable" && c.installed && c.connected !== "full" && !isDeprecated && (
            <Button
              variant="primary"
              disabled={busy === c.id}
              onClick={() => onConnect(c.id)}
            >
              {busy === c.id ? "Connecting…" : "Connect"}
            </Button>
          )}
          {c.connected !== "none" && (
            <Button
              variant="ghost"
              disabled={busy === c.id}
              onClick={() => onDisconnect(c.id)}
            >
              Disconnect
            </Button>
          )}
        </div>
      </div>

      {/* Manual snippet */}
      {c.kind === "manual" && c.manual_snippet && (
        <div className="px-4 pb-3 pt-0">
          <div className="text-xs text-text-muted mb-1">{c.manual_snippet.target}</div>
          <pre className="mono text-xs bg-base/60 border border-border rounded-md px-3 py-2 overflow-x-auto select-all">
            {c.manual_snippet.snippet}
          </pre>
        </div>
      )}

      {/* Preview diff */}
      {showPreview && (
        <div className="px-4 pb-3 space-y-2 border-t border-border/50 pt-3">
          <div className="label text-xs text-text-muted">Config changes to apply:</div>
          {preview.preview.edits.map((e) => (
            <div key={e.path}>
              <div className="mono text-xs text-text-muted mb-1">
                {e.path.replace(/^\/home\/[^/]+\//, "~/")}
              </div>
              <pre className="mono text-xs bg-base/60 border border-border rounded-md px-3 py-2 overflow-x-auto max-h-32 overflow-y-auto">
                {e.after}
              </pre>
            </div>
          ))}
          <div className="flex gap-2 pt-1">
            <Button onClick={onApply} disabled={busy === preview.connectorId}>
              Apply changes
            </Button>
            <Button variant="ghost" onClick={onCancelPreview}>
              Cancel
            </Button>
          </div>
        </div>
      )}
    </div>
  );
}

// ── Main component ──────────────────────────────────────────────────────────────

export function Connections() {
  const [connectors, setConnectors] = useState<Connector[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [preview, setPreview] = useState<PreviewState | null>(null);
  const [busy, setBusy] = useState<string | null>(null);

  const mounted = useRef(true);
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);

  const load = () => {
    setError(null);
    client
      .listConnectors()
      .then((cs) => {
        if (mounted.current) setConnectors(cs);
      })
      .catch(() => {
        if (mounted.current) setError("Couldn't reach the daemon to list AI tools.");
      });
  };

  useEffect(load, []);

  const handleConnect = async (id: string) => {
    setError(null);
    setBusy(id);
    try {
      const p = await client.previewConnector(id);
      if (mounted.current) setPreview({ connectorId: id, preview: p });
    } catch (err) {
      if (mounted.current) setError(err instanceof Error ? err.message : "Something went wrong");
    } finally {
      if (mounted.current) setBusy(null);
    }
  };

  const handleApply = async () => {
    if (!preview) return;
    setError(null);
    setBusy(preview.connectorId);
    try {
      await client.connectConnector(preview.connectorId);
      if (mounted.current) setPreview(null);
      if (mounted.current) load();
    } catch (err) {
      if (mounted.current) setError(err instanceof Error ? err.message : "Something went wrong");
    } finally {
      if (mounted.current) setBusy(null);
    }
  };

  const handleDisconnect = async (id: string) => {
    setError(null);
    setBusy(id);
    try {
      await client.disconnectConnector(id);
      if (mounted.current) load();
    } catch (err) {
      if (mounted.current) setError(err instanceof Error ? err.message : "Something went wrong");
    } finally {
      if (mounted.current) setBusy(null);
    }
  };

  if (error) {
    return (
      <div className="space-y-2">
        <p role="alert" className="text-tier-procedural font-body text-sm">
          {error}
        </p>
        <Button variant="ghost" onClick={load}>
          Retry
        </Button>
      </div>
    );
  }

  if (!connectors) {
    return <div className="label text-text-muted py-4">Loading AI tool connections…</div>;
  }

  // Split into sections: connected first, then installed-but-not-connected,
  // then deprecated detectable, then manual integrations
  const detected = connectors.filter((c) => c.kind === "detectable" && !c.deprecated);
  const deprecated = connectors.filter((c) => c.kind === "detectable" && c.deprecated);
  const manual = connectors.filter((c) => c.kind === "manual");

  // Sort detected: connected > installed > not detected
  const sortOrder = (c: Connector) =>
    c.connected === "full" ? 0 : c.connected === "partial" ? 1 : c.installed ? 2 : 3;
  detected.sort((a, b) => sortOrder(a) - sortOrder(b));

  return (
    <div className="space-y-4">
      {/* Detected tools */}
      {detected.length > 0 && (
        <div className="space-y-2">
          <div className="label text-text-muted text-xs uppercase tracking-wider">
            Detected tools
          </div>
          {detected.map((c) => (
            <ConnectorCard
              key={c.id}
              c={c}
              busy={busy}
              preview={preview}
              onConnect={handleConnect}
              onDisconnect={handleDisconnect}
              onApply={handleApply}
              onCancelPreview={() => setPreview(null)}
            />
          ))}
        </div>
      )}

      {/* Deprecated tools */}
      {deprecated.length > 0 && (
        <div className="space-y-2">
          <div className="label text-text-muted text-xs uppercase tracking-wider">
            Deprecated
          </div>
          {deprecated.map((c) => (
            <ConnectorCard
              key={c.id}
              c={c}
              busy={busy}
              preview={preview}
              onConnect={handleConnect}
              onDisconnect={handleDisconnect}
              onApply={handleApply}
              onCancelPreview={() => setPreview(null)}
            />
          ))}
        </div>
      )}

      {/* Manual integrations */}
      {manual.length > 0 && (
        <div className="space-y-2">
          <div className="label text-text-muted text-xs uppercase tracking-wider">
            Manual integrations
          </div>
          <p className="font-body text-text-muted text-xs">
            Copy the snippet into the tool's config file. No auto-connect is needed.
          </p>
          {manual.map((c) => (
            <ConnectorCard
              key={c.id}
              c={c}
              busy={busy}
              preview={preview}
              onConnect={handleConnect}
              onDisconnect={handleDisconnect}
              onApply={handleApply}
              onCancelPreview={() => setPreview(null)}
            />
          ))}
        </div>
      )}

      {detected.length === 0 && manual.length === 0 && (
        <Card className="p-4">
          <p className="font-body text-text-muted text-sm">
            No AI tools detected. Install Claude Code, Codex, or Antigravity CLI and reopen this
            page — or use a manual integration below.
          </p>
        </Card>
      )}
    </div>
  );
}
