import AxeBuilder from "@axe-core/playwright";
import { expect, test } from "@playwright/test";
import { settle } from "./settle";

test.use({ reducedMotion: "reduce" });

test("automatic backups are listed and one can be restored after saying what happens", async ({ page }) => {
  await page.goto("/");
  await page.getByRole("button", { name: "Settings" }).click();
  await page.getByRole("main").getByRole("button", { name: /^Privacy & data/ }).click();
  await expect(page.getByText("Automatic backups")).toBeVisible();
  await expect(page.getByText("Daily copy")).toBeVisible();
  await expect(page.getByText("Before an update")).toBeVisible();

  await page.getByRole("button", { name: /^Restore the copy from / }).first().click();
  const dialog = page.getByRole("dialog", { name: "Restore this backup?" });
  await expect(dialog).toContainText("What you have now is kept as a backup first");
  await settle(page);
  expect((await new AxeBuilder({ page }).include('[role="dialog"]').analyze()).violations).toEqual([]);
  expect(await page.evaluate(() => (window as unknown as { __restored?: string }).__restored)).toBeUndefined();
  await dialog.getByRole("button", { name: "Restore and restart" }).click();
  await expect(dialog.getByRole("status")).toContainText("Restarting");
  expect(await page.evaluate(() => (window as unknown as { __restored?: string }).__restored)).toMatch(/^firstlight-\d{8}-\d{6}-daily\.db$/);
});
