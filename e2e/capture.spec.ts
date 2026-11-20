import AxeBuilder from "@axe-core/playwright";
import { expect, test } from "@playwright/test";

test.use({ reducedMotion: "reduce" });

const at = (h: number, m: number) => new Date(new Date().setHours(h, m, 0, 0));

test("the capture box adds a task, or an event with Tab, and closes", async ({ page }) => {
  await page.clock.install({ time: at(9, 30) });
  await page.goto("/#/capture");
  const box = page.getByLabel("Quick capture");
  await expect(box).toBeFocused();
  expect((await new AxeBuilder({ page }).analyze()).violations).toEqual([]);

  await box.fill("Call the bank friday 3pm");
  await expect(page.getByRole("status")).toContainText("Task “Call the bank”, due");
  await box.press("Enter");
  await expect(page.getByRole("status")).toContainText("Added to Upcoming");
  await expect.poll(() => page.evaluate(() => (window as unknown as { __captureHidden?: number }).__captureHidden ?? 0)).toBe(1);

  // Tab makes it an event; a line with no day or time is not an event yet.
  await box.fill("Lunch with Riya");
  await box.press("Tab");
  await expect(page.getByRole("radio", { name: "Event" })).toHaveAttribute("aria-checked", "true");
  await expect(page.getByRole("status")).toContainText("Add a day or a time");
  await box.press("Enter");
  await expect(page.getByRole("status")).toContainText("Add a day or a time");
  await box.fill("Lunch with Riya tomorrow 1pm at Cafe Nero");
  await expect(page.getByRole("status")).toContainText("Event “Lunch with Riya”");
  await box.press("Enter");
  await expect(page.getByRole("status")).toContainText("Added to your calendar: Lunch with Riya");

  // "task …" is always a task; Escape closes.
  await box.fill("task renew passport");
  await expect(page.getByRole("radio", { name: "Event" })).toHaveCount(0);
  await box.press("Escape");
  await expect.poll(() => page.evaluate(() => (window as unknown as { __captureHidden?: number }).__captureHidden ?? 0)).toBe(3);
});

test("the capture shortcut is chosen in Settings, General", async ({ page }) => {
  await page.goto("/");
  await page.getByRole("button", { name: "Settings" }).click();
  await page.getByRole("main").getByRole("button", { name: /^General/ }).click();
  await expect(page.getByText("Quick capture", { exact: true })).toBeVisible();
  await expect(page.getByRole("button", { name: /Quick capture shortcut/ })).toBeVisible();
});
