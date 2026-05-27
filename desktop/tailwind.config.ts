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
      text: "var(--text)",
      "text-muted": "var(--text-muted)",
      border: "var(--border)",
      accent: "var(--accent)",
      "accent-contrast": "var(--accent-contrast)",
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
