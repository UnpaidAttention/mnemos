import { useSyncStatus } from "../api/queries";

function relativeTime(iso?: string | null): string {
  if (!iso) return "never";
  const then = Date.parse(iso);
  if (Number.isNaN(then)) return "never";
  const secs = Math.max(0, Math.floor((Date.now() - then) / 1000));
  if (secs < 60) return "just now";
  const mins = Math.floor(secs / 60);
  if (mins < 60) return `${mins}m ago`;
  const hours = Math.floor(mins / 60);
  if (hours < 24) return `${hours}h ago`;
  const days = Math.floor(hours / 24);
  return `${days}d ago`;
}

function dispatchPull() {
  window.dispatchEvent(new CustomEvent("mnemos:sync-pull"));
}

export function SyncStatusPill() {
  const { data } = useSyncStatus();

  // No status loaded yet — render a quiet placeholder so layout doesn't shift.
  if (!data) {
    return (
      <span className="label text-text-muted" aria-label="Sync status: loading">
        sync · …
      </span>
    );
  }

  // Error state: brick-red, click to retry pull.
  if (data.last_error) {
    return (
      <button
        onClick={dispatchPull}
        title={data.last_error}
        className="label text-tier-procedural rounded-md border border-border px-2 py-1 hover:bg-surface-raised transition-colors duration-[120ms] focus-visible:outline focus-visible:outline-2 focus-visible:outline-accent"
        aria-label={`Sync error: ${data.last_error}. Click to retry pull.`}
      >
        sync · error
      </button>
    );
  }

  // Off / disabled / not configured.
  if (data.backend === "none" || !data.ready) {
    return (
      <span className="label text-text-muted" aria-label="Sync status: off">
        sync · off
      </span>
    );
  }

  // Healthy: show backend + last activity time. Click pulls.
  const last = data.last_pulled_at ?? data.last_pushed_at;
  return (
    <button
      onClick={dispatchPull}
      title={`Last activity: ${last ?? "never"}. Click to pull now.`}
      className="label rounded-md border border-border px-2 py-1 hover:bg-surface-raised transition-colors duration-[120ms] focus-visible:outline focus-visible:outline-2 focus-visible:outline-accent"
      aria-label={`Sync backend ${data.backend}, last ${relativeTime(last)}`}
    >
      {data.backend} · {relativeTime(last)}
    </button>
  );
}
