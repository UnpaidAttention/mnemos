import { create } from "zustand";

interface UiState {
  selectedMemoryId: string | null;
  inspectorOpen: boolean;
  sidebarCollapsed: boolean;
  asOf: string | null; // ISO date for time-travel mode; null = present
  select: (id: string | null) => void;
  toggleInspector: () => void;
  toggleSidebar: () => void;
  setAsOf: (d: string | null) => void;
}

export const useUiStore = create<UiState>((set) => ({
  selectedMemoryId: null,
  inspectorOpen: true,
  sidebarCollapsed: true, // default: collapsed (icon rail) to maximize canvas
  asOf: null,
  select: (selectedMemoryId) => set({ selectedMemoryId, inspectorOpen: true }),
  toggleInspector: () => set((s) => ({ inspectorOpen: !s.inspectorOpen })),
  toggleSidebar: () => set((s) => ({ sidebarCollapsed: !s.sidebarCollapsed })),
  setAsOf: (asOf) => set({ asOf }),
}));
