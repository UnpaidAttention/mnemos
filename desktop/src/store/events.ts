import { create } from "zustand";

export type DaemonEvent =
  | { type: "memory_created"; id: string; title: string; tier: string }
  | { type: "memory_updated"; id: string }
  | { type: "memory_invalidated"; id: string; reason: string | null }
  | { type: "session_started"; id: string }
  | { type: "session_ended"; id: string }
  | { type: "pipeline_completed"; session_id: string; facts_added: number }
  | { type: "pipeline_failed"; session_id: string; error: string }
  | { type: "reflection_completed"; reflections_created: number }
  | { type: "community_detected"; communities: number }
  | { type: "sync_started"; backend: string; direction: string }
  | { type: "sync_completed"; backend: string; direction: string; files_changed: number }
  | { type: "sync_failed"; backend: string; direction: string; error: string }
  | { type: "sync_conflict"; path: string; detected_by: string }
  // P2-15: embed_rebuild_* variants so exhaustive narrowing over DaemonEvent
  // stays correct and ws.ts INVALIDATE table keys resolve to real event types.
  | { type: "embed_rebuild_started"; target_kind: string; target_model: string }
  | { type: "embed_rebuild_progress"; processed: number; total: number }
  | {
      type: "embed_rebuild_completed";
      processed: number;
      skipped: number;
      total: number;
      swapped: boolean;
    }
  | { type: "embed_rebuild_failed"; error: string; processed: number }
  | { type: "backfill_started"; total: number }
  | { type: "backfill_progress"; processed: number; total: number; entities_linked: number; errors: number }
  | { type: "backfill_completed"; total: number; entities_linked: number; edges_created: number; errors: number };

type Status = "connecting" | "open" | "closed";

interface EventState {
  status: Status;
  recent: DaemonEvent[];
  setStatus: (s: Status) => void;
  push: (e: DaemonEvent) => void;
}

export const useEventStore = create<EventState>((set) => ({
  status: "connecting",
  recent: [],
  setStatus: (status) => set({ status }),
  push: (e) => set((st) => ({ recent: [e, ...st.recent].slice(0, 50) })),
}));
