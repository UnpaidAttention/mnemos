import { useEffect, useState, type ReactNode } from "react";
import { TopBar } from "./TopBar";
import { LeftSidebar } from "./LeftSidebar";
import { Inspector } from "./Inspector";
import { CommandPalette } from "../components/CommandPalette";
import { QuickAdd } from "../components/QuickAdd";

export function Shell({ children }: { children: ReactNode }) {
  const [paletteOpen, setPaletteOpen] = useState(false);
  const [addOpen, setAddOpen] = useState(false);

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

  return (
    <div className="flex h-full flex-col">
      <TopBar onCommand={() => setPaletteOpen(true)} onAdd={() => setAddOpen(true)} />
      <div className="flex min-h-0 flex-1">
        <LeftSidebar />
        <main className="min-w-0 flex-1 overflow-y-auto">{children}</main>
        <Inspector />
      </div>
      <CommandPalette open={paletteOpen} onClose={() => setPaletteOpen(false)} />
      <QuickAdd open={addOpen} onClose={() => setAddOpen(false)} />
    </div>
  );
}
