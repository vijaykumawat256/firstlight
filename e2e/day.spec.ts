import { expect, test } from "@playwright/test";

test.use({ reducedMotion: "reduce" });

// A weekday, so the lunch check applies: Wednesday 18 November 2026.
const on = (h: number, m: number) => new Date(2026, 10, 18, h, m);

test("after the wrap-up time the briefing offers to move what is left to tomorrow", async ({ page }) => {
  const { default: AxeBuilder } = await import("@axe-core/playwright");
  await page.clock.install({ time: on(17, 0) });
  await page.goto("/?tasks");
  await expect(page.getByRole("region", { name: "Tasks" })).toBeVisible();
  await expect(page.getByRole("region", { name: "Wrap up" })).toHaveCount(0);

  await page.clock.install({ time: on(17, 45) });
  await page.goto("/?tasks");
  const card = page.getByRole("region", { name: "Wrap up" });
  await expect(card).toContainText("0 tasks done, 2 left.");
  expect((await new AxeBuilder({ page }).analyze()).violations).toEqual([]);
  await card.getByRole("button", { name: "Move Review Sam's slides to tomorrow" }).click();
  await expect(page.getByText("Moved to tomorrow").first()).toBeVisible();
  await expect(card).toContainText("1 left.");
  await page.getByRole("region", { name: "Tasks" }).getByRole("checkbox", { name: "Done: Send the budget to Priya" }).click();
  await expect(card).toContainText("1 task done. Nothing left for today.");
});

test("when meetings squeeze out lunch the briefing says so and can keep what is left", async ({ page }) => {
  await page.clock.install({ time: on(9, 30) });
  await page.goto("/?busylunch");
  const card = page.getByRole("region", { name: "Lunch" });
  await expect(card).toContainText("Only 20 free minutes between 12:00 PM and 2:00 PM.");
  await card.getByRole("button", { name: /^Keep 12:45 PM to 1:05 PM for lunch$/ }).click();
  await expect(page.getByText("Lunch is in your calendar").first()).toBeVisible();
  await expect(card).toHaveCount(0);
});

test("with lunch in the calendar there is no warning", async ({ page }) => {
  await page.clock.install({ time: on(9, 30) });
  await page.goto("/");
  await expect(page.getByRole("region", { name: "Today" }).or(page.getByRole("heading", { name: "Today" })).first()).toBeVisible();
  await expect(page.getByRole("region", { name: "Lunch" })).toHaveCount(0);
});

test("the day's notes can be set up in Settings", async ({ page }) => {
  await page.goto("/");
  await page.getByRole("button", { name: "Settings" }).click();
  await page.getByRole("main").getByRole("button", { name: /^Notifications/ }).click();
  await expect(page.getByRole("switch", { name: "Wrap up your day" })).toBeChecked();
  await expect(page.getByRole("switch", { name: "Break after long meetings" })).not.toBeChecked();
  await expect(page.getByRole("switch", { name: "Stop-work reminder" })).not.toBeChecked();
  await page.getByLabel("Wrap-up time").fill("18:15");
  await page.getByRole("switch", { name: "Break after long meetings" }).click();
  await expect(page.getByRole("combobox", { name: "Break after" }).or(page.getByLabel("Break after")).first()).toBeVisible();
  await page.getByRole("switch", { name: "Room for lunch" }).click();
  await expect(page.getByLabel("Lunch from")).toHaveCount(0);
});
