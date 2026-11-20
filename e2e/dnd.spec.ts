import AxeBuilder from "@axe-core/playwright";
import { expect, test, type Page } from "@playwright/test";
import { settle } from "./settle";

test.use({ reducedMotion: "reduce" });

const openNotifications = async (page: Page, query: string) => {
  await page.goto(`/${query}`);
  await page.getByRole("button", { name: "Settings" }).click();
  await page.getByRole("main").getByRole("button", { name: /^Notifications/ }).click();
};

test("Do Not Disturb walks through the two shortcuts, then can be tried and turned on", async ({ page }) => {
  await openNotifications(page, "?dndhalf");
  const group = page.getByRole("region", { name: "Do Not Disturb" });
  await expect(group).toContainText("One-time setup in the Shortcuts app");
  await expect(group.getByRole("switch", { name: "Do Not Disturb in meetings" })).toBeDisabled();
  await expect(group.getByLabel("Done")).toHaveCount(1);
  await expect(group).toContainText("Optional: Firstlight Focus Status");
  await group.getByRole("button", { name: "Open Shortcuts" }).click();
  expect(await page.evaluate(() => (window as unknown as { __shortcutsOpened?: number }).__shortcutsOpened)).toBe(1);
  await settle(page);
  expect((await new AxeBuilder({ page }).analyze()).violations).toEqual([]);

  await openNotifications(page, "?dnd");
  await expect(group).toContainText("Set up: both shortcuts found.");
  await expect(group).toContainText("Without the third shortcut");
  await openNotifications(page, "?dnd&dndstatus");
  await expect(group).not.toContainText("Without the third shortcut");
  await group.getByRole("button", { name: "Try: turn on" }).click();
  await expect(page.getByText("Do Not Disturb is on").first()).toBeVisible();
  await group.getByRole("switch", { name: "Do Not Disturb in meetings" }).click();
  await expect(group.getByRole("switch", { name: "Do Not Disturb in meetings" })).toBeChecked();
});

test("Do Not Disturb is not offered where it can't work", async ({ page }) => {
  await openNotifications(page, "?windows");
  await expect(page.getByRole("heading", { name: "Notifications", level: 1 })).toBeVisible();
  await expect(page.getByRole("region", { name: "Do Not Disturb" })).toHaveCount(0);
});
