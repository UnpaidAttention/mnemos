import { useEffect, useState, type ReactNode } from "react";
import { TopBar } from "./TopBar";
import { LeftSidebar } from "./LeftSidebar";
import { Inspector } from "./Inspector";
import { CommandPalette } from "../components/CommandPalette";

export function Shell({ children }: { children: ReactNode }) {
  const [paletteOpen, setPaletteOpen] = useState(false);

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

  return (
    <div className="flex h-full flex-col">
      <TopBar onCommand={() => setPaletteOpen(true)} />
      <div className="flex min-h-0 flex-1">
        <LeftSidebar />
        <main className="min-w-0 flex-1 overflow-y-auto">{children}</main>
        <Inspector />
      </div>
      <CommandPalette open={paletteOpen} onClose={() => setPaletteOpen(false)} />
    </div>
  );
}
