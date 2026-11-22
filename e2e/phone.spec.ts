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

async function openComputers(page: import("@playwright/test").Page, query: string) {
  await page.clock.install({ time: todayAt(9, 30) });
  await page.goto(`/${query}`);
  await expect(page.getByRole("heading", { name: /Good (morning|afternoon|evening)/ })).toBeVisible();
  await page.getByRole("button", { name: "Settings" }).click();
  await page.getByRole("main").getByRole("button", { name: /^Your computers/ }).click();
}

test("a computer shows a pairing code for the phone, and turns on sharing on the network", async ({ page }) => {
  await openComputers(page, "");
  await page.getByRole("button", { name: "Show code" }).click();
  const dialog = page.getByRole("dialog", { name: "Scan this with your phone" });
  await expect(dialog.getByRole("img", { name: "Pairing code for your phone" })).toBeVisible();
  await expect(dialog.getByText(/192\.168\.1\.20/)).toBeVisible();
  await settle(page);
  expect((await new AxeBuilder({ page }).analyze()).violations).toEqual([]);
  await dialog.getByRole("button", { name: "Done" }).click();
  // Sharing on the network is now on, so the page shows the running state.
  await expect(page.getByText("This network only")).toBeVisible();
});

test("a phone pairs by scanning, or by pasting the text, and refuses anything else", async ({ page }) => {
  await openComputers(page, "?phone");
  await expect(page.getByRole("radio", { name: "Synced folder" })).toHaveCount(0);
  // Something that isn't a pairing code.
  await page.getByLabel("Or paste the pairing text").fill("FL1-NOPE");
  await page.getByRole("button", { name: "Pair", exact: true }).click();
  await expect(page.getByRole("alert")).toHaveText("That isn't a Firstlight pairing code.");
  // Scanning the computer's code.
  await page.evaluate(() => {
    (window as unknown as { __scanText: string }).__scanText = "firstlight:pair?v=1&code=FL1-ABCD-EFGH&device=a1b2c3d4&at=192.168.1.20";
  });
  await page.getByRole("button", { name: "Scan the code" }).click();
  await expect(page.getByText("Paired with your computer").first()).toBeVisible();
  await expect(page.getByText(/Looking for your computer at 192\.168\.1\.20/)).toBeVisible();
  await expect(page.getByRole("button", { name: "Scan again" })).toBeVisible();
  await settle(page);
  expect((await new AxeBuilder({ page }).analyze()).violations).toEqual([]);
});

test("a phone without camera permission is told how to pair instead", async ({ page }) => {
  await openComputers(page, "?phone&scanfail");
  await page.getByRole("button", { name: "Scan the code" }).click();
  await expect(page.getByRole("alert")).toContainText("paste the pairing text instead");
});
