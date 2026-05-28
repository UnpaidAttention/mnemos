import { Link } from "@tanstack/react-router";
import { TIERS } from "../design/theme";
import { TierChip } from "../design/primitives";

const NAV: [string, string][] = [
  ["/", "Browser"], ["/search", "Search"], ["/graph", "Graph"], ["/timeline", "Timeline"],
  ["/pipelines", "Pipelines"], ["/reflections", "Reflections"], ["/audit", "Audit"],
  ["/settings", "Settings"], ["/doctor", "Doctor"],
];

export function LeftSidebar() {
  return (
    <nav role="navigation" className="w-56 shrink-0 border-r border-border bg-surface p-3 overflow-y-auto">
      <div className="label mb-1">Views</div>
      <ul className="space-y-0.5">
        {NAV.map(([to, label]) => (
          <li key={to}>
            <Link to={to} className="block rounded-md px-2 py-1 text-sm hover:bg-surface-raised [&.active]:bg-surface-raised [&.active]:text-accent">
              {label}
            </Link>
          </li>
        ))}
      </ul>
      <div className="label mt-4 mb-1">Tiers</div>
      <ul className="space-y-0.5">
        {TIERS.map((t) => (
          <li key={t}>
            <Link to="/" search={{ tier: t }} className="block rounded-md px-2 py-1 hover:bg-surface-raised">
              <TierChip tier={t} />
            </Link>
          </li>
        ))}
      </ul>
    </nav>
  );
}
