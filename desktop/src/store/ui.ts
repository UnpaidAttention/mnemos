import { create } from "zustand";

interface UiState {
  selectedMemoryId: string | null;
  inspectorOpen: boolean;
  asOf: string | null; // ISO date for time-travel mode; null = present
  select: (id: string | null) => void;
  toggleInspector: () => void;
  setAsOf: (d: string | null) => void;
}

export const useUiStore = create<UiState>((set) => ({
  selectedMemoryId: null,
  inspectorOpen: true,
  asOf: null,
  select: (selectedMemoryId) => set({ selectedMemoryId, inspectorOpen: true }),
  toggleInspector: () => set((s) => ({ inspectorOpen: !s.inspectorOpen })),
  setAsOf: (asOf) => set({ asOf }),
}));
