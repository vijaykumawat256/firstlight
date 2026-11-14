import AxeBuilder from "@axe-core/playwright";
import { expect, test } from "@playwright/test";

test.use({ reducedMotion: "reduce" });

const at = (h: number, m: number) => new Date(new Date().setHours(h, m, 0, 0));

async function start(page: import("@playwright/test").Page) {
  await page.clock.install({ time: at(9, 30) });
  await page.goto("/");
  await expect(page.getByRole("heading", { name: /Good (morning|afternoon|evening)/ })).toBeVisible();
}

test("Cmd+K opens search, finds an event, and Enter opens it", async ({ page }) => {
  await start(page);
  await page.keyboard.press("Meta+k");
  const dialog = page.getByRole("dialog", { name: "Search" });
  await expect(dialog).toBeVisible();
  const box = dialog.getByRole("combobox", { name: "Search events" });
  await expect(box).toBeFocused();
  // Nothing typed: what is coming up.
  await expect(dialog.getByRole("option").first()).toBeVisible();

  await box.fill("standup");
  await expect(dialog.getByRole("option", { name: /Team standup/ })).toBeVisible();
  await expect(dialog.getByRole("option", { name: /Design review/ })).toHaveCount(0);
  await box.press("Enter");
  await expect(page.getByRole("dialog", { name: "Team standup" })).toBeVisible();
});

test("the same key closes it, and the toolbar button opens it", async ({ page }) => {
  await start(page);
  await page.getByRole("button", { name: "Search" }).click();
  await expect(page.getByRole("dialog", { name: "Search" })).toBeVisible();
  await page.keyboard.press("Control+k");
  await expect(page.getByRole("dialog", { name: "Search" })).toHaveCount(0);
  await page.keyboard.press("Meta+k");
  await page.keyboard.press("Escape");
  await expect(page.getByRole("dialog", { name: "Search" })).toHaveCount(0);
});

test("arrow keys move through the results; a place is searched too; no match says so", async ({ page }) => {
  await start(page);
  await page.keyboard.press("Meta+k");
  const dialog = page.getByRole("dialog", { name: "Search" });
  const box = dialog.getByRole("combobox", { name: "Search events" });
  await box.fill("review lunch cafe design");
  await expect(dialog.getByText("Nothing matches.")).toBeVisible();

  await box.fill("cafe"); // Lunch with Alex is at Café Nero; accents are ignored
  await expect(dialog.getByRole("option", { name: /Lunch with Alex/ })).toBeVisible();

  await box.fill("design");
  const options = dialog.getByRole("option");
  await expect(options.first()).toHaveAttribute("aria-selected", "true");
  const count = await options.count();
  if (count > 1) {
    await box.press("ArrowDown");
    await expect(options.nth(1)).toHaveAttribute("aria-selected", "true");
    await box.press("ArrowUp");
    await expect(options.first()).toHaveAttribute("aria-selected", "true");
  }
});

test("typing a date offers to jump to that day", async ({ page }) => {
  await start(page);
  await page.keyboard.press("Meta+k");
  const dialog = page.getByRole("dialog", { name: "Search" });
  const box = dialog.getByRole("combobox", { name: "Search events" });
  await box.fill("2026-12-24");
  const jump = dialog.getByRole("option", { name: /Go to/ });
  await expect(jump).toBeVisible();
  await box.press("Enter");
  await expect(page.getByRole("region", { name: "Day", exact: true })).toBeVisible();
  await expect(page.getByRole("heading", { level: 1 })).toContainText(/December 24|24 December/);
});

test("search has no accessibility violations, with results showing", async ({ page }) => {
  await start(page);
  await page.keyboard.press("Meta+k");
  const dialog = page.getByRole("dialog", { name: "Search" });
  await dialog.getByRole("combobox", { name: "Search events" }).fill("design");
  await expect(dialog.getByRole("option").first()).toBeVisible();
  const r = await new AxeBuilder({ page }).withTags(["wcag2a", "wcag2aa", "wcag21a", "wcag21aa", "wcag22aa"]).analyze();
  expect(r.violations.map((v) => `${v.id}: ${v.nodes[0]?.target.join(" ")}`)).toEqual([]);
});
