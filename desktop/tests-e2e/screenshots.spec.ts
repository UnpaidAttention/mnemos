import { test } from "@playwright/test";

const VIEWS: [string, string][] = [
  ["/", "01-browser"],
  ["/search", "02-search"],
  ["/graph", "03-graph"],
  ["/timeline", "04-timeline"],
  ["/pipelines", "05-pipelines"],
  ["/reflections", "06-reflections"],
  ["/audit", "07-audit"],
  ["/entity/ent_a", "08-entity"],
  ["/editor/mem_1", "09-editor"],
];

test.use({ viewport: { width: 1440, height: 900 } });

for (const [path, name] of VIEWS) {
  test(`screenshot ${name}`, async ({ page }) => {
    await page.goto(path);
    await page.waitForLoadState("networkidle");
    // settle a beat for font + token CSS
    await page.waitForTimeout(400);
    await page.screenshot({ path: `.screenshots/${name}-light.png`, fullPage: false });
    // dark
    await page.evaluate(() => document.documentElement.setAttribute("data-theme", "dark"));
    await page.waitForTimeout(150);
    await page.screenshot({ path: `.screenshots/${name}-dark.png`, fullPage: false });
  });
}

test("with palette open", async ({ page }) => {
  await page.goto("/");
  await page.waitForLoadState("networkidle");
  await page.locator("body").press("Control+k");
  await page.waitForTimeout(150);
  await page.screenshot({ path: ".screenshots/10-palette-light.png" });
});

test("with search results + explain", async ({ page }) => {
  await page.goto("/search");
  await page.getByPlaceholder(/search memories/i).fill("rust");
  await page.getByRole("button", { name: /^search$/i }).click();
  await page.waitForTimeout(300);
  await page.screenshot({ path: ".screenshots/11-search-results-light.png" });
});

test("with quick-add open", async ({ page }) => {
  await page.goto("/");
  await page.waitForLoadState("networkidle");
  await page.locator('[aria-label="Quick add"], button[title*="Add"], button:has-text("+")').first().click().catch(() => {});
  // fallback: dispatch the event our shell listens for
  await page.evaluate(() => document.dispatchEvent(new CustomEvent("mnemos:quick-add")));
  await page.waitForTimeout(150);
  await page.screenshot({ path: ".screenshots/12-quickadd-light.png" });
});
