import { test, expect } from "@playwright/test";

test("browse → search golden path", async ({ page }) => {
  await page.goto("/");
  await expect(page.getByText("Rust note")).toBeVisible();

  await page.getByRole("link", { name: "Search" }).click();
  await page.getByPlaceholder(/search memories/i).fill("rust");
  await page.getByRole("button", { name: /search/i }).click();
  await expect(page.getByText("Rust note")).toBeVisible();
  // PPR rank bar is rendered by RankBars; use title attribute to avoid matching the checkbox label
  await expect(page.locator("[title^='rank']").filter({ hasText: /PPR/i }).first()).toBeVisible();
});

test("command palette opens with ⌘K and lists commands", async ({ page }) => {
  await page.goto("/");
  // Wait for the app to fully settle before sending keyboard events
  await expect(page.getByText("Rust note")).toBeVisible();
  await page.locator("body").press("Control+k");
  await expect(page.getByRole("dialog", { name: /command palette/i })).toBeVisible();
  await page.getByPlaceholder(/type a command/i).fill("graph");
  await expect(page.getByText(/open graph/i)).toBeVisible();
});
