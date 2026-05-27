export const TIERS = ["working", "episodic", "semantic", "procedural", "reflection"] as const;
export type Tier = (typeof TIERS)[number];

export const TIER_COLOR_VAR: Record<Tier, string> = {
  working: "var(--tier-working)",
  episodic: "var(--tier-episodic)",
  semantic: "var(--tier-semantic)",
  procedural: "var(--tier-procedural)",
  reflection: "var(--tier-reflection)",
};

export type ThemeMode = "light" | "dark";
