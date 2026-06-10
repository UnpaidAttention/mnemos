import { Link, useRouterState } from "@tanstack/react-router";
import {
  Globe, Search, Network, Clock, Workflow, Brain, ShieldCheck,
  BookOpen, Settings, Stethoscope, Database, ChevronLeft, ChevronRight,
} from "lucide-react";
import { useUiStore } from "../store/ui";

const NAV: { to: string; label: string; icon: typeof Globe }[] = [
  { to: "/", label: "Browser", icon: Globe },
  { to: "/search", label: "Search", icon: Search },
  { to: "/graph", label: "Graph", icon: Network },
  { to: "/timeline", label: "Timeline", icon: Clock },
  { to: "/pipelines", label: "Pipelines", icon: Workflow },
  { to: "/reflections", label: "Reflections", icon: Brain },
  { to: "/audit", label: "Audit", icon: ShieldCheck },
  { to: "/knowledge", label: "Knowledge", icon: BookOpen },
  { to: "/settings", label: "Settings", icon: Settings },
  { to: "/doctor", label: "Doctor", icon: Stethoscope },
  { to: "/embed-rebuild", label: "Migration", icon: Database },
];

export function LeftSidebar() {
  const collapsed = useUiStore((s) => s.sidebarCollapsed);
  const toggle = useUiStore((s) => s.toggleSidebar);

  // Get the current matched path for active highlighting
  const routerState = useRouterState();
  const currentPath = routerState.location.pathname;

  return (
    <nav
      role="navigation"
      className="sidebar-transition shrink-0 border-r border-border bg-surface flex flex-col overflow-hidden"
      style={{ width: collapsed ? "var(--sidebar-w-collapsed)" : "var(--sidebar-w-expanded)" }}
    >
      {/* Nav items */}
      <ul className="flex-1 space-y-0.5 p-1.5 overflow-y-auto overflow-x-hidden">
        {NAV.map(({ to, label, icon: Icon }) => {
          const isActive = to === "/" ? currentPath === "/" : currentPath.startsWith(to);
          return (
            <li key={to}>
              <Link
                to={to}
                aria-label={label}
                className={`
                  group relative flex items-center gap-2.5 rounded-lg transition-all duration-[var(--dur-micro)]
                  ${collapsed ? "justify-center px-0 py-2.5" : "px-3 py-2"}
                  ${isActive
                    ? "bg-accent/15 text-accent"
                    : "text-text-muted hover:bg-surface-raised hover:text-text"
                  }
                `}
                title={collapsed ? label : undefined}
              >
                {/* Active indicator bar */}
                {isActive && (
                  <span
                    className="absolute left-0 top-1/2 -translate-y-1/2 w-[3px] rounded-r-full bg-accent"
                    style={{ height: "60%" }}
                  />
                )}
                <Icon
                  size={18}
                  strokeWidth={isActive ? 2.2 : 1.8}
                  className="shrink-0"
                />
                {/* Label — fades and slides when collapsing */}
                <span
                  className={`text-sm font-body whitespace-nowrap transition-all duration-[var(--dur-layout)] ${
                    collapsed ? "opacity-0 w-0 overflow-hidden" : "opacity-100"
                  }`}
                >
                  {label}
                </span>

                {/* Tooltip for collapsed state */}
                {collapsed && (
                  <span className="
                    absolute left-full ml-2 px-2.5 py-1 rounded-md text-xs font-body
                    bg-surface-raised text-text border border-border shadow-floating
                    opacity-0 group-hover:opacity-100 pointer-events-none
                    transition-opacity duration-[var(--dur-micro)] z-50 whitespace-nowrap
                  ">
                    {label}
                  </span>
                )}
              </Link>
            </li>
          );
        })}
      </ul>

      {/* Collapse toggle */}
      <button
        onClick={toggle}
        className="flex items-center justify-center py-3 border-t border-border text-text-muted hover:text-text hover:bg-surface-raised transition-colors duration-[var(--dur-micro)]"
        aria-label={collapsed ? "Expand sidebar" : "Collapse sidebar"}
        title={collapsed ? "Expand sidebar" : "Collapse sidebar"}
      >
        {collapsed ? (
          <ChevronRight size={16} strokeWidth={2} />
        ) : (
          <ChevronLeft size={16} strokeWidth={2} />
        )}
      </button>
    </nav>
  );
}
