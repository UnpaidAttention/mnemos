import type { ButtonHTMLAttributes, HTMLAttributes, ReactNode } from "react";
import { TIER_COLOR_VAR, type Tier } from "./theme";

export function TierChip({ tier }: { tier: Tier }) {
  return (
    <span
      data-tier={tier}
      className="label inline-flex items-center gap-1.5 rounded-sm px-1.5 py-0.5"
      style={{ color: TIER_COLOR_VAR[tier] }}
    >
      <span aria-hidden className="h-2 w-2 rounded-full" style={{ background: TIER_COLOR_VAR[tier] }} />
      {tier}
    </span>
  );
}

type ButtonProps = ButtonHTMLAttributes<HTMLButtonElement> & { variant?: "primary" | "ghost" };
export function Button({ variant = "primary", className = "", children, ...rest }: ButtonProps) {
  const base =
    "font-body text-sm rounded-md px-3 py-1.5 transition-[transform,box-shadow,background] duration-[120ms] ease-brand focus-visible:outline focus-visible:outline-2 focus-visible:outline-accent active:scale-[0.97] disabled:opacity-50 disabled:pointer-events-none";
  const styles =
    variant === "primary"
      ? "bg-accent text-accent-contrast shadow-subtle hover:shadow-raised"
      : "bg-transparent text-text hover:bg-surface-raised";
  return (
    <button className={`${base} ${styles} ${className}`} {...rest}>
      {children}
    </button>
  );
}

type CardProps = HTMLAttributes<HTMLDivElement> & { children: ReactNode; className?: string };
export function Card({ children, className = "", ...rest }: CardProps) {
  return (
    <div className={`bg-surface border border-border rounded-lg shadow-subtle ${className}`} {...rest}>
      {children}
    </div>
  );
}

export function Skeleton({ className = "" }: { className?: string }) {
  return <div aria-hidden className={`animate-pulse bg-border/60 rounded ${className}`} />;
}
