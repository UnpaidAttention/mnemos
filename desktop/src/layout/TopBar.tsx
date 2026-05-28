import { useEventStore } from "../store/events";
import { useUiStore } from "../store/ui";

export function TopBar({ onCommand }: { onCommand: () => void }) {
  const status = useEventStore((s) => s.status);
  const asOf = useUiStore((s) => s.asOf);
  const dot = status === "open" ? "var(--accent)" : status === "connecting" ? "var(--tier-working)" : "var(--tier-procedural)";
  return (
    <header role="banner" className="flex items-center gap-3 border-b border-border bg-surface px-4 h-12 shrink-0">
      <span className="display text-lg">mnemos</span>
      <button onClick={onCommand} className="label ml-2 rounded-md border border-border px-2 py-1 hover:bg-surface-raised">
        ⌘K  Search / commands
      </button>
      {asOf && (
        <span className="ml-2 rounded-full px-2 py-0.5 text-xs" style={{ background: "var(--tier-episodic)", color: "#fff" }}>
          viewing {asOf.slice(0, 10)}
        </span>
      )}
      <span className="ml-auto flex items-center gap-1.5 label" title={`daemon ${status}`}>
        <span className="h-2 w-2 rounded-full" style={{ background: dot }} /> {status}
      </span>
    </header>
  );
}
