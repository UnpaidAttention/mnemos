import { defineConfig } from "@playwright/test";

export default defineConfig({
  testDir: "./tests-e2e",
  timeout: 30_000,
  use: { baseURL: "http://localhost:1420" },
  webServer: {
    command: "VITE_MSW=1 pnpm dev",
    url: "http://localhost:1420",
    reuseExistingServer: !process.env.CI,
    timeout: 60_000,
  },
});
