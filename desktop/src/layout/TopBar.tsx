import { Plus, Moon, Sun, Activity } from "lucide-react";
import { useTheme } from "../design/ThemeProvider";
import { useUiStore } from "../store/ui";
import { SyncStatusPill } from "../components/SyncStatusPill";
import { useQuery } from "@tanstack/react-query";
import { client } from "../api/client";

export function TopBar({ onCommand, onAdd }: { onCommand: () => void; onAdd?: () => void }) {
  const asOf = useUiStore((s) => s.asOf);
  const { mode, toggle } = useTheme();
  const { data: health } = useQuery({
    queryKey: ["health"],
    queryFn: () => client.getHealth(),
    refetchInterval: 30_000,
  });

  return (
    <header
      role="banner"
      className="flex items-center gap-3 border-b border-border bg-surface px-4 h-12 shrink-0"
    >
      {/* Brand — compact serif lockup */}
      <span className="flex items-center gap-2 shrink-0">
        <span
          aria-hidden
          className="h-2 w-2 rounded-full"
          style={{ background: "var(--accent)", boxShadow: "var(--glow-teal)" }}
        />
        <span className="display text-lg leading-none">mnemos</span>
      </span>

      {/* ⌘K trigger — compact */}
      <button
        onClick={onCommand}
        className="label ml-1 rounded-md border border-border px-2 py-0.5 hover:bg-surface-raised transition-colors duration-[var(--dur-micro)] focus-visible:outline focus-visible:outline-2 focus-visible:outline-accent text-[0.65rem]"
        aria-label="Open command palette (⌘K)"
      >
        ⌘K
      </button>

      {/* Bi-temporal as-of pill */}
      {asOf && (
        <span
          className="rounded-full px-2 py-0.5 label text-[0.6rem]"
          style={{ background: "var(--tier-episodic)", color: "#fff" }}
        >
          viewing {asOf.slice(0, 10)}
        </span>
      )}

      {/* Spacer */}
      <span className="ml-auto" />

      {/* System status indicator */}
      <span className="flex items-center gap-1.5 text-xs" title="Daemon status">
        <Activity size={13} strokeWidth={2} className="status-ok" />
        <span className="mono text-text-dim text-[0.65rem]">
          {health?.version ?? "—"}
        </span>
      </span>

      {/* Divider */}
      <span aria-hidden className="self-stretch w-px my-2.5 bg-border" />

      {/* Quick-add */}
      {onAdd && (
        <button
          onClick={onAdd}
          className="flex items-center justify-center h-7 w-7 rounded-md border border-border hover:bg-surface-raised hover:border-accent/30 transition-all duration-[var(--dur-micro)] focus-visible:outline focus-visible:outline-2 focus-visible:outline-accent"
          aria-label="Quick add memory"
          title="New memory"
        >
          <Plus size={15} strokeWidth={2.2} aria-hidden />
        </button>
      )}

      {/* Theme toggle */}
      <button
        onClick={toggle}
        className="flex items-center justify-center h-7 w-7 rounded-md border border-border hover:bg-surface-raised hover:border-accent/30 transition-all duration-[var(--dur-micro)] focus-visible:outline focus-visible:outline-2 focus-visible:outline-accent"
        aria-label="Toggle theme"
        title={mode === "light" ? "Switch to dark theme" : "Switch to light theme"}
      >
        {mode === "light" ? (
          <Moon size={14} strokeWidth={2} aria-hidden />
        ) : (
          <Sun size={14} strokeWidth={2} aria-hidden />
        )}
      </button>

      {/* Sync status pill */}
      <SyncStatusPill />
    </header>
  );
}
