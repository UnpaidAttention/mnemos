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
  | { type: "community_detected"; communities: number };

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
