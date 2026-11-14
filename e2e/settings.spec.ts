import { expect, test, type Page } from "@playwright/test";

const SHOTS = "e2e/screenshots";

function todayAt(h: number, m: number): Date {
  const d = new Date();
  d.setHours(h, m, 0, 0);
  return d;
}

test.use({ reducedMotion: "reduce" });

async function openSettings(page: Page, query = "") {
  await page.clock.install({ time: todayAt(9, 30) });
  await page.goto(`/${query}`);
  await expect(page.getByRole("heading", { name: /Good (morning|afternoon|evening)/ })).toBeVisible();
  await page.getByRole("button", { name: "Settings" }).click();
  await expect(page.getByRole("heading", { name: "Settings", level: 1 })).toBeVisible();
}

const back = (page: Page) => page.getByRole("button", { name: "Briefing" }).click();

test("every section from the plan is there", async ({ page }) => {
  await page.setViewportSize({ width: 420, height: 900 });
  await openSettings(page);
  for (const name of ["General", "Briefing", "When Daybrief appears", "Notifications", "Calendars", "Appearance", "Privacy"]) {
    await expect(page.getByRole("region", { name, exact: true })).toBeVisible();
  }
});

for (const scheme of ["light", "dark"] as const) {
  test(`full settings page (${scheme})`, async ({ page }) => {
    await page.emulateMedia({ colorScheme: scheme });
    await page.setViewportSize({ width: 420, height: 2300 });
    await openSettings(page);
    await page.screenshot({ path: `${SHOTS}/settings-full-${scheme}.png` });
  });
}

test("time format changes how times read everywhere, immediately", async ({ page }) => {
  await openSettings(page);
  await page.getByRole("radio", { name: "24-hour" }).click();
  await back(page);
  await expect(page.getByLabel("Up next")).toContainText("10:30 – 11:30");
  await expect(page.getByLabel("Up next")).not.toContainText("AM");
  await page.getByRole("button", { name: "Settings" }).click();
  await page.getByRole("radio", { name: "12-hour" }).click();
  await back(page);
  await expect(page.getByLabel("Up next")).toContainText("10:30 AM – 11:30 AM");
});

test("the briefing sections can be switched off and on", async ({ page }) => {
  await openSettings(page);
  await page.getByRole("switch", { name: "Show tomorrow" }).click();
  await page.getByRole("switch", { name: "Show needs attention" }).click();
  await page.getByRole("switch", { name: "Show free time" }).click();
  await back(page);
  await expect(page.getByLabel("Tomorrow")).toHaveCount(0);
  await expect(page.getByLabel("Needs attention")).toHaveCount(0);
  await expect(page.getByLabel("Today")).not.toContainText("Free:");
  await expect(page.getByLabel("Today")).toBeVisible();

  await page.getByRole("button", { name: "Settings" }).click();
  await page.getByRole("switch", { name: "Show tomorrow" }).click();
  await back(page);
  await expect(page.getByLabel("Tomorrow")).toBeVisible();
});

test("declined events can be shown", async ({ page }) => {
  await openSettings(page);
  await page.getByRole("switch", { name: "Show declined events" }).click();
  await back(page);
  await expect(page.getByText("All-hands rehearsal")).toBeVisible();
});

test("theme and accent apply at once", async ({ page }) => {
  await openSettings(page);
  await page.getByRole("radio", { name: "Dark" }).click();
  await expect(page.locator("html")).toHaveAttribute("data-theme", "dark");
  await page.getByRole("radio", { name: "Light" }).click();
  await expect(page.locator("html")).toHaveAttribute("data-theme", "light");

  await page.getByRole("radio", { name: "purple" }).click();
  await expect(page.getByRole("radio", { name: "purple" })).toHaveAttribute("aria-checked", "true");
  await expect.poll(() => page.evaluate(() => document.documentElement.style.getPropertyValue("--accent"))).toBe("var(--cal-purple)");
  await page.getByRole("radio", { name: "Default" }).click();
  await expect.poll(() => page.evaluate(() => document.documentElement.style.getPropertyValue("--accent"))).toBe("");
});

test("quiet hours reveal their times; days toggle", async ({ page }) => {
  await openSettings(page);
  await expect(page.getByRole("combobox", { name: "Quiet hours from" })).toBeVisible();
  await page.getByRole("switch", { name: "Quiet hours" }).click();
  await expect(page.getByRole("combobox", { name: "Quiet hours from" })).toHaveCount(0);
  await page.getByRole("switch", { name: "Quiet hours" }).click();
  await expect(page.getByRole("combobox", { name: "Quiet hours until" })).toBeVisible();

  const saturday = page.getByRole("button", { name: "Saturday" });
  await expect(saturday).toHaveAttribute("data-state", "on");
  await saturday.click();
  await expect(saturday).toHaveAttribute("data-state", "off");
  await saturday.click();
  await expect(saturday).toHaveAttribute("data-state", "on");
});

test("notification options appear only when their switch is on", async ({ page }) => {
  await openSettings(page);
  await expect(page.getByRole("combobox", { name: "Notify how early" })).toBeVisible();
  await page.getByRole("switch", { name: "Notify before events" }).click();
  await expect(page.getByRole("combobox", { name: "Notify how early" })).toHaveCount(0);
  await page.getByRole("switch", { name: "Morning briefing" }).click();
  await expect(page.getByRole("combobox", { name: "Morning briefing time" })).toHaveCount(0);
  await page.getByRole("switch", { name: "Evening look-ahead" }).click();
  await expect(page.getByRole("combobox", { name: "Evening look-ahead time" })).toHaveCount(0);
});

test("a values dropdown saves its choice", async ({ page }) => {
  await openSettings(page);
  await page.getByRole("combobox", { name: "Refresh interval" }).click();
  await page.getByRole("option", { name: "30 min" }).click();
  await expect(page.getByRole("combobox", { name: "Refresh interval" })).toContainText("30 min");
  await back(page);
  await page.getByRole("button", { name: "Settings" }).click();
  await expect(page.getByRole("combobox", { name: "Refresh interval" })).toContainText("30 min");
});

test("recording a shortcut, and a refused one is put back with a reason", async ({ page }) => {
  await openSettings(page);
  const button = page.getByRole("button", { name: "Global shortcut" });
  await expect(button).toHaveText("Not set");
  await button.click();
  await expect(button).toHaveText("Press keys…");
  await page.keyboard.press("k"); // no modifier
  await expect(page.getByRole("status")).toContainText("Include Cmd, Ctrl or Alt");
  await page.keyboard.press("Meta+Shift+K");
  await expect(button).toHaveText("⌘⇧ K");

  // The system refuses Cmd+Q: the previous shortcut returns and the user is told.
  await button.click();
  await page.keyboard.press("Meta+Q");
  await expect(page.getByText("Couldn't save that setting")).toBeVisible();
  await expect(button).toHaveText("⌘⇧ K");

  await page.getByRole("button", { name: "Clear" }).click();
  await expect(button).toHaveText("Not set");

  // Escape cancels recording.
  await button.click();
  await page.keyboard.press("Escape");
  await expect(button).toHaveText("Not set");
});

test("backing up and restoring settings", async ({ page }) => {
  await openSettings(page);
  await page.getByRole("button", { name: "Save to Downloads" }).click();
  await expect(page.getByText("Settings saved")).toBeVisible();

  await page.getByRole("button", { name: "Import…" }).click();
  const dialog = page.getByRole("dialog", { name: "Import settings" });
  await dialog.getByLabel("Settings file contents").fill("this is not a settings file");
  await dialog.getByRole("button", { name: "Import" }).click();
  await expect(dialog.getByRole("alert")).toContainText("isn't a valid settings file");

  await dialog.getByLabel("Settings file contents").fill(JSON.stringify({ format: "daybrief-settings", settings: { general: { default_view: "week" } } }));
  await dialog.getByRole("button", { name: "Import" }).click();
  await expect(page.getByText("Settings imported")).toBeVisible();
  await expect(page.getByRole("combobox", { name: "Opens to" })).toContainText("Week view");
});

test("erase all data asks first, then returns to the welcome screen", async ({ page }) => {
  await openSettings(page);
  await page.getByRole("button", { name: "Erase…" }).click();
  const dialog = page.getByRole("dialog", { name: "Erase all data?" });
  await expect(dialog).toContainText("cannot be undone");
  await page.screenshot({ path: `${SHOTS}/erase-confirm.png` });

  // Cancel changes nothing.
  await dialog.getByRole("button", { name: "Cancel" }).click();
  await expect(page.getByRole("dialog")).toBeHidden();
  await expect(page.getByRole("heading", { name: "Settings", level: 1 })).toBeVisible();

  await page.getByRole("button", { name: "Erase…" }).click();
  await page.getByRole("dialog").getByRole("button", { name: "Erase everything" }).click();
  await expect(page.getByRole("heading", { name: "Welcome to Daybrief" })).toBeVisible();
});

test("the app can open straight to the week view", async ({ page }) => {
  await page.clock.install({ time: todayAt(9, 30) });
  await page.goto("/?defaultview=week");
  await expect(page.getByRole("grid", { name: "Week" })).toBeVisible();
});

test("changing 'Opens to' does not move you off the settings screen", async ({ page }) => {
  await openSettings(page);
  await page.getByRole("combobox", { name: "Opens to" }).click();
  await page.getByRole("option", { name: "Week view" }).click();
  await expect(page.getByRole("heading", { name: "Settings", level: 1 })).toBeVisible();
});

test("the calendars section leads to accounts", async ({ page }) => {
  await openSettings(page);
  await page.getByRole("button", { name: /Accounts & calendars/ }).click();
  await expect(page.getByRole("heading", { name: "Accounts", level: 1 })).toBeVisible();
});
