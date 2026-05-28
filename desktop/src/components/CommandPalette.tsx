import { useEffect, useMemo, useRef, useState } from "react";
import { useNavigate } from "@tanstack/react-router";
import { useUiStore } from "../store/ui";
import { client } from "../api/client";

interface Cmd {
  label: string;
  category: string;
  run: () => void;
}

export function CommandPalette({ open, onClose }: { open: boolean; onClose: () => void }) {
  const [q, setQ] = useState("");
  const [activeIdx, setActiveIdx] = useState(0);
  const navigate = useNavigate();
  const toggleInspector = useUiStore((s) => s.toggleInspector);
  const inputRef = useRef<HTMLInputElement>(null);

  // Reset query + focus on open
  useEffect(() => {
    if (open) {
      setQ("");
      setActiveIdx(0);
      // autoFocus on the input via a tiny defer so the DOM is ready
      requestAnimationFrame(() => inputRef.current?.focus());
    }
  }, [open]);

  const go = (to: string) => () => {
    // string-typed navigation avoids router type-tree constraints
    void navigate({ to: to as "/" });
    onClose();
  };

  const commands = useMemo<Cmd[]>(
    () => [
      {
        label: "New memory",
        category: "Create",
        run: () => {
          document.dispatchEvent(new CustomEvent("mnemos:quick-add"));
          onClose();
        },
      },
      { label: "Open Graph", category: "Navigate", run: go("/graph") },
      { label: "Open Timeline", category: "Navigate", run: go("/timeline") },
      { label: "Open Pipelines", category: "Navigate", run: go("/pipelines") },
      { label: "Open Reflections", category: "Navigate", run: go("/reflections") },
      { label: "Open Audit", category: "Navigate", run: go("/audit") },
      {
        label: "Reflect now",
        category: "Action",
        run: () => {
          void client.reflect();
          onClose();
        },
      },
      {
        label: "Toggle inspector",
        category: "Action",
        run: () => {
          toggleInspector();
          onClose();
        },
      },
    ],
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [onClose, toggleInspector],
  );

  const filtered = useMemo(
    () =>
      q.trim()
        ? commands.filter((c) => c.label.toLowerCase().includes(q.toLowerCase()))
        : commands,
    [commands, q],
  );

  // Clamp activeIdx when filtered list shrinks
  const safeIdx = Math.min(activeIdx, Math.max(0, filtered.length - 1));

  const onKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Escape") {
      onClose();
      return;
    }
    if (e.key === "ArrowDown") {
      e.preventDefault();
      setActiveIdx((i) => Math.min(i + 1, filtered.length - 1));
      return;
    }
    if (e.key === "ArrowUp") {
      e.preventDefault();
      setActiveIdx((i) => Math.max(i - 1, 0));
      return;
    }
    if (e.key === "Enter") {
      if (filtered.length > 0) {
        filtered[safeIdx].run();
      } else if (q.trim()) {
        void navigate({ to: "/search" as "/" });
        onClose();
      }
    }
  };

  if (!open) return null;

  return (
    <div
      className="fixed inset-0 z-50 flex items-start justify-center pt-32"
      style={{ background: "rgba(15,18,24,0.45)", backdropFilter: "blur(2px)" }}
      onClick={onClose}
    >
      <div
        role="dialog"
        aria-label="Command palette"
        aria-modal="true"
        className="w-[32rem] rounded-xl border border-border shadow-floating overflow-hidden"
        style={{ background: "var(--surface-raised)" }}
        onClick={(e) => e.stopPropagation()}
      >
        {/* Search input */}
        <div className="flex items-center gap-2 px-4 py-3 border-b border-border">
          <svg
            aria-hidden
            width="14"
            height="14"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            strokeWidth="2.2"
            strokeLinecap="round"
            strokeLinejoin="round"
            className="shrink-0"
            style={{ color: "var(--text-muted)" }}
          >
            <circle cx="11" cy="11" r="8" />
            <line x1="21" y1="21" x2="16.65" y2="16.65" />
          </svg>
          <input
            ref={inputRef}
            className="flex-1 bg-transparent font-body text-[0.9rem] outline-none placeholder:text-text-muted"
            placeholder="Type a command or search…"
            value={q}
            onChange={(e) => {
              setQ(e.target.value);
              setActiveIdx(0);
            }}
            onKeyDown={onKeyDown}
            aria-label="Command or search input"
            autoComplete="off"
            spellCheck={false}
          />
          <kbd
            className="mono text-[0.65rem] border border-border rounded px-1.5 py-0.5"
            style={{ color: "var(--text-muted)" }}
          >
            esc
          </kbd>
        </div>

        {/* Results */}
        <ul
          className="max-h-72 overflow-y-auto py-1"
          role="listbox"
          aria-label="Commands"
        >
          {filtered.map((c, i) => (
            <li key={c.label} role="option" aria-selected={i === safeIdx}>
              <button
                className="w-full flex items-center justify-between px-4 py-2.5 text-left transition-colors duration-[80ms]"
                style={{
                  background: i === safeIdx ? "var(--surface)" : "transparent",
                  color: "var(--text)",
                }}
                onMouseEnter={() => setActiveIdx(i)}
                onClick={c.run}
              >
                <span className="font-body text-sm">{c.label}</span>
                <span
                  className="label text-[0.6rem] tracking-widest"
                  style={{ color: "var(--text-muted)" }}
                >
                  {c.category}
                </span>
              </button>
            </li>
          ))}
          {filtered.length === 0 && q.trim() && (
            <li className="px-4 py-3 font-body text-sm" style={{ color: "var(--text-muted)" }}>
              <span style={{ color: "var(--accent)" }}>↵</span> Search memories for &ldquo;{q}&rdquo;
            </li>
          )}
          {filtered.length === 0 && !q.trim() && (
            <li className="px-4 py-3 font-body text-sm" style={{ color: "var(--text-muted)" }}>
              No commands found.
            </li>
          )}
        </ul>

        {/* Footer hint */}
        <div
          className="flex items-center gap-3 px-4 py-2 border-t border-border label text-[0.6rem]"
          style={{ color: "var(--text-muted)" }}
        >
          <span><kbd className="mono border border-border rounded px-1">↑↓</kbd> navigate</span>
          <span><kbd className="mono border border-border rounded px-1">↵</kbd> select</span>
          <span><kbd className="mono border border-border rounded px-1">esc</kbd> close</span>
        </div>
      </div>
    </div>
  );
}
