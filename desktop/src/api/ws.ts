import type { QueryClient } from "@tanstack/react-query";
import { getToken } from "./token";
import { useEventStore, type DaemonEvent } from "../store/events";

const INVALIDATE: Record<string, string[][]> = {
  memory_created: [["memories"], ["graph"]],
  memory_updated: [["memories"], ["memory"]],
  memory_invalidated: [["memories"], ["memory"]],
  pipeline_completed: [["pipelines"], ["memories"], ["graph"]],
  pipeline_failed: [["pipelines"]],
  reflection_completed: [["reflections"], ["memories"]],
  community_detected: [["communities"], ["graph"]],
  sync_started: [["sync", "status"]],
  sync_completed: [["sync", "status"]],
  sync_failed: [["sync", "status"]],
  sync_conflict: [["sync", "status"]],
  embed_rebuild_started: [["embed-rebuild", "status"]],
  embed_rebuild_progress: [["embed-rebuild", "status"]],
  embed_rebuild_completed: [["embed-rebuild", "status"], ["doctor"], ["memories"]],
  embed_rebuild_failed: [["embed-rebuild", "status"]],
  session_started: [], session_ended: [],
};

export function connectEvents(queryClient: QueryClient, baseUrl = "localhost:7423"): () => void {
  let ws: WebSocket | null = null;
  let closed = false;
  let backoff = 500;

  const open = async () => {
    if (closed) return;
    const token = await getToken();
    useEventStore.getState().setStatus("connecting");
    ws = new WebSocket(`ws://${baseUrl}/v1/events?token=${encodeURIComponent(token)}`);
    ws.onopen = () => { backoff = 500; useEventStore.getState().setStatus("open"); };
    ws.onmessage = (msg) => {
      try {
        const e = JSON.parse(msg.data) as DaemonEvent;
        useEventStore.getState().push(e);
        for (const key of INVALIDATE[e.type] ?? []) queryClient.invalidateQueries({ queryKey: key });
      } catch { /* ignore malformed */ }
    };
    ws.onclose = () => {
      useEventStore.getState().setStatus("closed");
      if (!closed) { setTimeout(open, backoff); backoff = Math.min(backoff * 2, 8000); }
    };
  };
  void open();
  return () => { closed = true; ws?.close(); };
}
