import { Moon, Plus, Sun } from "lucide-react";
import { useTheme } from "../design/ThemeProvider";
import { useEventStore } from "../store/events";
import { useUiStore } from "../store/ui";

export function TopBar({ onCommand, onAdd }: { onCommand: () => void; onAdd?: () => void }) {
  const status = useEventStore((s) => s.status);
  const asOf = useUiStore((s) => s.asOf);
  const { mode, toggle } = useTheme();
  const dot =
    status === "open"
      ? "var(--accent)"
      : status === "connecting"
        ? "var(--tier-working)"
        : "var(--tier-procedural)";

  return (
    <header
      role="banner"
      className="flex items-center gap-3 border-b border-border bg-surface px-4 h-12 shrink-0"
    >
      <span className="display text-lg">mnemos</span>

      {/* ⌘K trigger */}
      <button
        onClick={onCommand}
        className="label ml-2 rounded-md border border-border px-2 py-1 hover:bg-surface-raised transition-colors duration-[120ms] focus-visible:outline focus-visible:outline-2 focus-visible:outline-accent"
        aria-label="Open command palette (⌘K)"
      >
        ⌘K&nbsp; Search / commands
      </button>

      {/* Bi-temporal as-of pill */}
      {asOf && (
        <span
          className="ml-2 rounded-full px-2 py-0.5 label text-[0.65rem]"
          style={{ background: "var(--tier-episodic)", color: "#fff" }}
        >
          viewing {asOf.slice(0, 10)}
        </span>
      )}

      {/* Spacer */}
      <span className="ml-auto" />

      {/* Quick-add */}
      {onAdd && (
        <button
          onClick={onAdd}
          className="flex items-center justify-center h-7 w-7 rounded-md border border-border hover:bg-surface-raised transition-colors duration-[120ms] focus-visible:outline focus-visible:outline-2 focus-visible:outline-accent"
          aria-label="Quick add memory"
          title="Quick add (mnemos:quick-add)"
        >
          <Plus size={14} strokeWidth={2.2} aria-hidden />
        </button>
      )}

      {/* Theme toggle */}
      <button
        onClick={toggle}
        className="flex items-center justify-center h-7 w-7 rounded-md border border-border hover:bg-surface-raised transition-colors duration-[120ms] focus-visible:outline focus-visible:outline-2 focus-visible:outline-accent"
        aria-label="Toggle theme"
        title={mode === "light" ? "Switch to dark theme" : "Switch to light theme"}
      >
        {mode === "light" ? (
          <Moon size={14} strokeWidth={2} aria-hidden />
        ) : (
          <Sun size={14} strokeWidth={2} aria-hidden />
        )}
      </button>

      {/* Daemon status */}
      <span
        className="flex items-center gap-1.5 label"
        title={`daemon ${status}`}
        aria-label={`Daemon status: ${status}`}
      >
        <span
          className="h-2 w-2 rounded-full"
          style={{ background: dot }}
          aria-hidden
        />
        {status}
      </span>
    </header>
  );
}
