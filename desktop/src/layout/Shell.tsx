import { useEffect, useState, type ReactNode } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { useRouterState } from "@tanstack/react-router";
import { TopBar } from "./TopBar";
import { LeftSidebar } from "./LeftSidebar";
import { Inspector } from "./Inspector";
import { CommandPalette } from "../components/CommandPalette";
import { QuickAdd } from "../components/QuickAdd";
import { client } from "../api/client";

export function Shell({ children }: { children: ReactNode }) {
  const [paletteOpen, setPaletteOpen] = useState(false);
  const [addOpen, setAddOpen] = useState(false);
  const queryClient = useQueryClient();
  const routerState = useRouterState();
  const currentPath = routerState.location.pathname;

  // Views that manage their own context panels — hide the global Inspector
  const hideInspector = currentPath === "/graph" || currentPath === "/knowledge";
  // Views that manage their own scroll/canvas — suppress page-level scroll
  const suppressScroll = currentPath === "/graph";

  // ⌘K / Ctrl+K global shortcut
  useEffect(() => {
    const h = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "k") {
        e.preventDefault();
        setPaletteOpen(true);
      }
    };
    window.addEventListener("keydown", h);
    return () => window.removeEventListener("keydown", h);
  }, []);

  // mnemos:quick-add custom event (fired by command palette "New memory" or TopBar "+")
  useEffect(() => {
    const h = () => setAddOpen(true);
    document.addEventListener("mnemos:quick-add", h);
    return () => document.removeEventListener("mnemos:quick-add", h);
  }, []);

  // mnemos:sync-pull custom event (fired by the top-bar SyncStatusPill).
  // Invokes a manual pull, then invalidates the sync status query whether the
  // pull succeeded or failed so the pill reflects the new state immediately.
  useEffect(() => {
    const h = () => {
      void client
        .runSyncPull()
        .catch(() => {
          /* swallow — pill will show the daemon-side error on next refetch */
        })
        .finally(() => {
          queryClient.invalidateQueries({ queryKey: ["sync", "status"] });
        });
    };
    window.addEventListener("mnemos:sync-pull", h);
    return () => window.removeEventListener("mnemos:sync-pull", h);
  }, [queryClient]);

  return (
    <div className="flex h-full flex-col">
      <TopBar onCommand={() => setPaletteOpen(true)} onAdd={() => setAddOpen(true)} />
      <div className="flex min-h-0 flex-1">
        <LeftSidebar />
        <main className={`min-w-0 flex-1 ${suppressScroll ? "overflow-hidden" : "overflow-y-auto"}`}>{children}</main>
        {!hideInspector && <Inspector />}
      </div>
      <CommandPalette open={paletteOpen} onClose={() => setPaletteOpen(false)} />
      <QuickAdd open={addOpen} onClose={() => setAddOpen(false)} />
    </div>
  );
}
