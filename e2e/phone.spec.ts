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

test("a computer shows a 6-digit code, and says Paired only once a phone really connected", async ({ page }) => {
  await openComputers(page, "");
  await page.getByRole("button", { name: "Add phone" }).click();
  const dialog = page.getByRole("dialog", { name: "Pair your phone" });
  await expect(dialog.getByLabel("Pairing code 4 8 2 9 1 3")).toHaveText("482 913");
  await expect(dialog.getByText(/Works for \d+:\d\d/)).toBeVisible();
  await settle(page);
  expect((await new AxeBuilder({ page }).analyze()).violations).toEqual([]);
  // The phone types the code.
  await page.evaluate(() => {
    (window as unknown as { __phonePaired: boolean }).__phonePaired = true;
  });
  const done = page.getByRole("dialog", { name: "Phone paired" });
  await expect(done.getByRole("status")).toContainText("Paired with iPhone");
  await done.getByRole("button", { name: "Done" }).click();
  await expect(page.getByText("This network only")).toBeVisible();
});

test("a phone pairs by typing the 6 digits, and is told clearly when the code is wrong", async ({ page }) => {
  await openComputers(page, "?phone");
  await expect(page.getByRole("radio", { name: "Synced folder" })).toHaveCount(0);
  const field = page.getByLabel("Pairing code");
  const pairButton = page.getByRole("button", { name: "Pair", exact: true });
  await expect(pairButton).toBeDisabled();
  await field.fill("111111");
  await expect(field).toHaveValue("111 111");
  await pairButton.click();
  await expect(page.getByRole("alert")).toHaveText("That code isn't right. Check the 6 digits on your computer and try again.");
  await field.fill("482913");
  await pairButton.click();
  await expect(page.getByRole("status").filter({ hasText: "Paired with Vijay's MacBook" }).first()).toBeVisible();
  await expect(page.getByRole("button", { name: "Pair again" })).toBeVisible();
  await settle(page);
  expect((await new AxeBuilder({ page }).analyze()).violations).toEqual([]);
});

test("a phone that can't find the computer is told what to check", async ({ page }) => {
  await openComputers(page, "?phone&pairnotfound");
  await page.getByLabel("Pairing code").fill("482913");
  await page.getByRole("button", { name: "Pair", exact: true }).click();
  await expect(page.getByRole("alert")).toContainText("same Wi-Fi");
});
