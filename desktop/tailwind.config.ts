import type { Config } from "tailwindcss";

export default {
  content: ["./index.html", "./src/**/*.{ts,tsx}"],
  darkMode: ["selector", '[data-theme="dark"]'],
  theme: {
    colors: {
      transparent: "transparent",
      current: "currentColor",
      bg: "var(--bg)",
      surface: "var(--surface)",
      "surface-raised": "var(--surface-raised)",
      "surface-sunken": "var(--surface-sunken)",
      text: "var(--text)",
      "text-muted": "var(--text-muted)",
      "text-dim": "var(--text-dim)",
      border: "var(--border)",
      "border-subtle": "var(--border-subtle)",
      accent: "var(--accent)",
      "accent-light": "var(--accent-light)",
      "accent-contrast": "var(--accent-contrast)",
      status: {
        ok: "var(--status-ok)",
        warn: "var(--status-warn)",
        crit: "var(--status-crit)",
        info: "var(--status-info)",
      },
      tier: {
        working: "var(--tier-working)",
        episodic: "var(--tier-episodic)",
        semantic: "var(--tier-semantic)",
        procedural: "var(--tier-procedural)",
        reflection: "var(--tier-reflection)",
      },
    },
    extend: {
      fontFamily: {
        display: ['"Fraunces Variable"', "Georgia", "serif"],
        body: ['"Source Serif 4 Variable"', "Georgia", "serif"],
        mono: ['"JetBrains Mono Variable"', "ui-monospace", "monospace"],
      },
      boxShadow: {
        subtle: "var(--shadow-subtle)",
        raised: "var(--shadow-raised)",
        floating: "var(--shadow-floating)",
      },
      transitionTimingFunction: { brand: "var(--ease)" },
    },
  },
  plugins: [],
} satisfies Config;
