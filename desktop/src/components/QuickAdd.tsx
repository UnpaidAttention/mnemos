import { useRef, useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { X } from "lucide-react";
import { client } from "../api/client";
import { Button } from "../design/primitives";
import { TIERS, type Tier } from "../design/theme";

export function QuickAdd({ open, onClose }: { open: boolean; onClose: () => void }) {
  const qc = useQueryClient();
  const [body, setBody] = useState("");
  const [tier, setTier] = useState<Tier>("semantic");
  const [tags, setTags] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  if (!open) return null;

  const submit = async () => {
    if (!body.trim()) return;
    setBusy(true);
    setError(null);
    try {
      await client.createMemory({
        body,
        tier,
        tags: tags
          .split(",")
          .map((t) => t.trim())
          .filter(Boolean),
      });
      await qc.invalidateQueries({ queryKey: ["memories"] });
      setBody("");
      setTags("");
      setBusy(false);
      onClose();
    } catch (err) {
      setBusy(false);
      setError(err instanceof Error ? err.message : "Failed to add memory");
    }
  };

  const onKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Escape") {
      onClose();
    }
    // Ctrl/Cmd+Enter submits
    if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
      void submit();
    }
  };

  return (
    <div
      className="fixed inset-0 z-50 flex items-start justify-center pt-28"
      style={{ background: "rgba(15,18,24,0.45)", backdropFilter: "blur(2px)" }}
      onClick={onClose}
    >
      <div
        role="dialog"
        aria-label="Quick add memory"
        aria-modal="true"
        className="w-[34rem] rounded-xl border border-border shadow-floating overflow-hidden"
        style={{ background: "var(--surface-raised)" }}
        onClick={(e) => e.stopPropagation()}
      >
        {/* Header */}
        <div className="flex items-center justify-between px-4 pt-3 pb-2 border-b border-border">
          <span className="display text-base" style={{ color: "var(--text)" }}>
            Quick add
          </span>
          <button
            onClick={onClose}
            className="flex items-center justify-center h-6 w-6 rounded hover:bg-surface transition-colors duration-[80ms] focus-visible:outline focus-visible:outline-2 focus-visible:outline-accent"
            aria-label="Close quick add"
          >
            <X size={14} strokeWidth={2} aria-hidden />
          </button>
        </div>

        {/* Body */}
        <div className="p-4 space-y-3">
          <textarea
            ref={textareaRef}
            autoFocus
            className="w-full h-28 bg-surface border border-border rounded-lg p-3 font-body text-sm resize-none transition-[border-color] duration-[120ms] focus:outline-none focus:border-accent"
            placeholder="What should mnemos remember?"
            value={body}
            onChange={(e) => setBody(e.target.value)}
            onKeyDown={onKeyDown}
            aria-label="Memory content"
          />

          <div className="flex items-center gap-2">
            {/* Tier selector */}
            <select
              className="bg-surface border border-border rounded-md px-2 py-1.5 font-body text-sm focus:outline-none focus:border-accent transition-[border-color] duration-[120ms]"
              value={tier}
              onChange={(e) => setTier(e.target.value as Tier)}
              aria-label="Memory tier"
            >
              {TIERS.map((t) => (
                <option key={t} value={t}>
                  {t}
                </option>
              ))}
            </select>

            {/* Tags input */}
            <input
              className="flex-1 bg-surface border border-border rounded-md px-2 py-1.5 mono text-sm focus:outline-none focus:border-accent transition-[border-color] duration-[120ms]"
              placeholder="tags (comma separated)"
              value={tags}
              onChange={(e) => setTags(e.target.value)}
              aria-label="Memory tags"
            />

            <Button
              onClick={() => void submit()}
              disabled={busy || !body.trim()}
              aria-label="Add memory"
            >
              {busy ? "Saving…" : "Add memory"}
            </Button>
          </div>

          {error && (
            <p className="font-body text-xs" style={{ color: "var(--tier-procedural)" }} role="alert">
              {error}
            </p>
          )}

          <p className="label text-[0.6rem]" style={{ color: "var(--text-muted)" }}>
            <kbd className="mono border border-border rounded px-1">⌘↵</kbd> to save &nbsp;·&nbsp;{" "}
            <kbd className="mono border border-border rounded px-1">esc</kbd> to close
          </p>
        </div>
      </div>
    </div>
  );
}
