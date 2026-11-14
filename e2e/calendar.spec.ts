import { expect, test, type Page } from "@playwright/test";

const SHOTS = "e2e/screenshots";

function todayAt(h: number, m: number): Date {
  const d = new Date();
  d.setHours(h, m, 0, 0);
  return d;
}

test.use({ reducedMotion: "reduce" });

// A Wednesday, so "today +2" (Friday) falls in the same week whether weeks start on Sunday or Monday.
const WEDNESDAY = new Date(2026, 2, 11, 9, 30);

async function openCalendar(page: Page, at: Date = todayAt(9, 30)) {
  await page.clock.install({ time: at });
  await page.goto("/");
  await expect(page.getByRole("heading", { name: /Good (morning|afternoon|evening)/ })).toBeVisible();
  await page.getByRole("button", { name: "Calendar" }).click();
  await expect(page.getByRole("grid", { name: "Day" })).toBeVisible();
}

for (const scheme of ["light", "dark"] as const) {
  test.describe(`${scheme}`, () => {
    test.use({ colorScheme: scheme });

    test("day view shows today's events at their times, with overlaps side by side", async ({ page }) => {
      await page.setViewportSize({ width: 420, height: 760 });
      await openCalendar(page);
      await expect(page.getByRole("button", { name: "Design review" })).toBeVisible();
      await expect(page.getByRole("button", { name: "1:1 with Sam" })).toBeVisible();
      // Design review (10:30-11:30) and the 1:1 (11:00-11:30) overlap: they must not cover each other.
      const a = await page.getByRole("button", { name: "Design review" }).boundingBox();
      const b = await page.getByRole("button", { name: "1:1 with Sam" }).boundingBox();
      expect(a && b).toBeTruthy();
      expect(a!.x + a!.width).toBeLessThanOrEqual(b!.x + 1);
      await expect(page.getByLabel(/^Now,/)).toBeVisible();
      await page.screenshot({ path: `${SHOTS}/calendar-day-${scheme}.png` });
    });

    test("week view shows seven days with events and an all-day strip", async ({ page }) => {
      await page.setViewportSize({ width: 760, height: 760 });
      await openCalendar(page, WEDNESDAY);
      await page.getByRole("radio", { name: "Week" }).click();
      await expect(page.getByRole("grid", { name: "Week" })).toBeVisible();
      await expect(page.getByRole("columnheader")).toHaveCount(7);
      await expect(page.getByRole("button", { name: "Planning offsite" })).toBeVisible();
      await expect(page.getByRole("button", { name: "Maya's birthday" })).toBeVisible();
      await page.screenshot({ path: `${SHOTS}/calendar-week-${scheme}.png` });
    });
  });
}

test("navigation: next, previous and Today", async ({ page }) => {
  await openCalendar(page);
  const title = page.getByRole("heading", { level: 1 });
  const original = await title.textContent();
  await expect(page.getByRole("button", { name: "Today" })).toBeDisabled();
  await page.getByRole("button", { name: "Next day" }).click();
  await expect(title).not.toHaveText(original!);
  await expect(page.getByRole("button", { name: "Today" })).toBeEnabled();
  await expect(page.getByRole("button", { name: "Flight to Berlin" })).toBeVisible();
  await page.getByRole("button", { name: "Previous day" }).click();
  await page.getByRole("button", { name: "Previous day" }).click();
  await expect(page.getByRole("button", { name: "Sprint retro" })).toBeVisible();
  await page.getByRole("button", { name: "Today" }).click();
  await expect(title).toHaveText(original!);
});

test("clicking an event opens its details; hidden calendars disappear", async ({ page }) => {
  await openCalendar(page);
  await page.getByRole("button", { name: "Design review" }).click();
  await expect(page.getByRole("dialog", { name: "Design review" })).toBeVisible();
  await page.keyboard.press("Escape");

  // Cancelled and declined events never appear.
  await expect(page.getByRole("button", { name: /Vendor demo/ })).toHaveCount(0);
  await expect(page.getByRole("button", { name: "All-hands rehearsal" })).toHaveCount(0);

  await page.getByRole("button", { name: "Briefing" }).click();
  await page.getByRole("button", { name: "Settings" }).click();
  await page.getByRole("button", { name: /Accounts & calendars/ }).click();
  await page.getByRole("switch", { name: "Show Work" }).click();
  await page.getByRole("button", { name: "Briefing" }).click();
  await page.getByRole("button", { name: "Calendar" }).click();
  await expect(page.getByRole("button", { name: "Design review" })).toHaveCount(0);
  await expect(page.getByRole("button", { name: "Lunch with Alex" })).toBeVisible();
});

test("the view can be driven with the keyboard", async ({ page }) => {
  await openCalendar(page);
  await page.getByRole("button", { name: "Next day" }).focus();
  await page.keyboard.press("Enter");
  await expect(page.getByRole("button", { name: "Today" })).toBeEnabled();
});
