import AxeBuilder from "@axe-core/playwright";
import { expect, test } from "@playwright/test";
import { settle } from "./settle";

// The iPhone app's page, imitated in the browser with ?phone (see src/app/platform.ts).
test.use({ reducedMotion: "reduce", viewport: { width: 390, height: 844 } });

function todayAt(h: number, m: number): Date {
  const d = new Date();
  d.setHours(h, m, 0, 0);
  return d;
}

test("on a phone the page is solid, and computer-only settings are not offered", async ({ page }) => {
  await page.clock.install({ time: todayAt(9, 30) });
  await page.goto("/?phone");
  await expect(page.getByRole("heading", { name: /Good (morning|afternoon|evening)/ })).toBeVisible();
  await expect(page.locator("html")).toHaveAttribute("data-platform", "ios");
  // No sideways scrolling at iPhone width.
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);

  await page.getByRole("button", { name: "Settings" }).click();
  const main = page.getByRole("main");
  await expect(main.getByRole("button", { name: /^General/ })).toBeVisible();
  await expect(main.getByRole("button", { name: /^When it appears/ })).toHaveCount(0);
  await main.getByRole("button", { name: /^General/ }).click();
  await expect(page.getByText("Opens to", { exact: true })).toBeVisible();
  for (const gone of ["Start at login", "Show in the Dock or taskbar", "Quick capture", "Next meeting in the menu bar", "Check for updates"]) {
    await expect(page.getByText(gone, { exact: true })).toHaveCount(0);
  }
  await settle(page);
  const axe = await new AxeBuilder({ page }).analyze();
  expect(axe.violations).toEqual([]);
});

test("on a computer the same settings are all there", async ({ page }) => {
  await page.clock.install({ time: todayAt(9, 30) });
  await page.goto("/");
  await expect(page.getByRole("heading", { name: /Good (morning|afternoon|evening)/ })).toBeVisible();
  await expect(page.locator("html")).not.toHaveAttribute("data-platform", "ios");
  await page.getByRole("button", { name: "Settings" }).click();
  await expect(page.getByRole("main").getByRole("button", { name: /^When it appears/ })).toBeVisible();
  await page.getByRole("main").getByRole("button", { name: /^General/ }).click();
  await expect(page.getByText("Start at login", { exact: true })).toBeVisible();
});
