import { Moon, Plus, Sun } from "lucide-react";
import { useTheme } from "../design/ThemeProvider";
import { useUiStore } from "../store/ui";
import { SyncStatusPill } from "../components/SyncStatusPill";

export function TopBar({ onCommand, onAdd }: { onCommand: () => void; onAdd?: () => void }) {
  const asOf = useUiStore((s) => s.asOf);
  const { mode, toggle } = useTheme();

  return (
    <header
      role="banner"
      className="flex items-center gap-3 border-b border-border bg-surface px-4 h-14 shrink-0"
    >
      {/* Brand: serif lockup with a single tier-teal accent dot for character. */}
      <span className="flex items-center gap-2">
        <span
          aria-hidden
          className="h-1.5 w-1.5 rounded-full"
          style={{ background: "var(--tier-semantic)" }}
        />
        <span className="display text-xl leading-none">mnemos</span>
      </span>

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

      {/* Subtle divider between brand/command area and the trailing controls. */}
      <span
        aria-hidden
        className="self-stretch w-px my-2 bg-border"
      />

      {/* Quick-add */}
      {onAdd && (
        <button
          onClick={onAdd}
          className="flex items-center justify-center h-8 w-8 rounded-md border border-border hover:bg-surface-raised transition-colors duration-[120ms] focus-visible:outline focus-visible:outline-2 focus-visible:outline-accent"
          aria-label="Quick add memory"
          title="New memory"
        >
          <Plus size={17} strokeWidth={2.2} aria-hidden />
        </button>
      )}

      {/* Theme toggle */}
      <button
        onClick={toggle}
        className="flex items-center justify-center h-8 w-8 rounded-md border border-border hover:bg-surface-raised transition-colors duration-[120ms] focus-visible:outline focus-visible:outline-2 focus-visible:outline-accent"
        aria-label="Toggle theme"
        title={mode === "light" ? "Switch to dark theme" : "Switch to light theme"}
      >
        {mode === "light" ? (
          <Moon size={16} strokeWidth={2} aria-hidden />
        ) : (
          <Sun size={16} strokeWidth={2} aria-hidden />
        )}
      </button>

      {/* Sync status pill — live backend + last activity, click-to-pull. */}
      <SyncStatusPill />
    </header>
  );
}
