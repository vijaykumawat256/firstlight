import { expect, test } from "@playwright/test";

test.use({ reducedMotion: "reduce" });

// Wednesday 18 November 2026.
const on = (h: number, m: number, day = 18) => new Date(2026, 10, day, h, m);

test("other time zones label the hours in the day view and show in Find a time", async ({ page }) => {
  await page.clock.install({ time: on(9, 0) });
  await page.goto("/?zones");
  await page.getByRole("button", { name: "Calendar" }).click();
  const day = page.getByRole("region", { name: "Day" });
  await expect(day.getByText("London", { exact: true })).toBeVisible();
  await expect(day.getByText("Kolkata", { exact: true })).toBeVisible();
  await expect(day.getByText("Here", { exact: true })).toBeVisible();
  await page.getByRole("button", { name: "Briefing" }).click();
  await page.getByRole("button", { name: "Find a time" }).click();
  await expect(page.getByRole("dialog", { name: "Find a time" }).getByText(/ in London$/).first()).toBeVisible();
});

test("search answers '3pm in Tokyo' in your own time", async ({ page }) => {
  await page.clock.install({ time: on(9, 0) });
  await page.goto("/");
  await page.getByRole("button", { name: "Search" }).click();
  const search = page.getByRole("dialog", { name: "Search" });
  await search.getByRole("combobox", { name: "Search events" }).fill("3pm in Tokyo");
  await expect(search.getByRole("option").first()).toHaveText(/^3\sPM in Tokyo is .* here/);
});

test("travel time warns when the meeting before leaves too little time", async ({ page }) => {
  await page.clock.install({ time: on(9, 0) });
  await page.goto("/?travel");
  const card = page.getByRole("region", { name: "Travel" });
  await expect(card).toContainText(/Lunch with Alex at 12:30\sPM: 60 min after /);
  await page.goto("/");
  await expect(page.getByRole("region", { name: "Travel" })).toHaveCount(0);
});

test("the week's review shows on Friday afternoon, not before", async ({ page }) => {
  const { default: AxeBuilder } = await import("@axe-core/playwright");
  await page.clock.install({ time: on(11, 0, 20) });
  await page.goto("/");
  await expect(page.getByRole("heading", { level: 1 })).toBeVisible();
  await expect(page.getByRole("region", { name: "Your week" })).toHaveCount(0);
  await page.clock.install({ time: on(15, 0, 20) });
  await page.goto("/");
  const card = page.getByRole("region", { name: "Your week" });
  await expect(card).toContainText("Meetings");
  await expect(card).toContainText("than last week");
  expect((await new AxeBuilder({ page }).analyze()).violations).toEqual([]);
});

test("zones, travel time and the weekly review are in Settings", async ({ page }) => {
  await page.goto("/");
  await page.getByRole("button", { name: "Settings" }).click();
  await page.getByRole("main").getByRole("button", { name: /^Appearance/ }).click();
  await expect(page.getByLabel("First other time zone")).toBeVisible();
  await page.getByRole("button", { name: "Settings", exact: true }).click();
  await page.getByRole("main").getByRole("button", { name: /^Notifications/ }).click();
  await expect(page.getByLabel("Travel time")).toBeVisible();
  await expect(page.getByRole("switch", { name: "Weekly review" })).toBeChecked();
});
