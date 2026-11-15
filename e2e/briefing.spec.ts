import { expect, test } from "@playwright/test";

const SHOTS = "e2e/screenshots";

function todayAt(h: number, m: number): Date {
  const d = new Date();
  d.setHours(h, m, 0, 0);
  return d;
}

for (const scheme of ["light", "dark"] as const) {
  test.describe(`${scheme} theme`, () => {
    // Reduced motion makes screenshots deterministic (no animation caught mid-way).
    test.use({ colorScheme: scheme, reducedMotion: "reduce" });

    test("morning briefing", async ({ page }) => {
      await page.clock.install({ time: todayAt(9, 30) });
      await page.goto("/");
      await expect(page.getByRole("heading", { name: "Good morning" })).toBeVisible();
      await expect(page.getByLabel("Up next")).toContainText("Design review");
      await expect(page.getByRole("button", { name: "Join", exact: true })).toBeVisible();
      await expect(page.getByLabel("Needs attention")).toContainText("overlaps");
      await page.screenshot({ path: `${SHOTS}/briefing-morning-${scheme}.png` });
      await page.setViewportSize({ width: 420, height: 1100 });
      await page.screenshot({ path: `${SHOTS}/briefing-morning-full-${scheme}.png` });
    });

    test("afternoon briefing shows past events and a now marker", async ({ page }) => {
      await page.clock.install({ time: todayAt(13, 10) });
      await page.goto("/");
      await expect(page.getByRole("heading", { name: "Good afternoon" })).toBeVisible();
      await expect(page.getByLabel(/^Now,/)).toBeVisible();
      await expect(page.getByLabel("Tomorrow")).toContainText("Early start");
      await page.screenshot({ path: `${SHOTS}/briefing-afternoon-${scheme}.png` });
    });

    test("settings screen", async ({ page }) => {
      await page.clock.install({ time: todayAt(9, 30) });
      await page.goto("/");
      await page.getByRole("button", { name: "Settings" }).click();
      await expect(page.getByRole("heading", { name: "Settings", level: 1 })).toBeVisible();
      await expect(page.getByRole("button", { name: /^Privacy & data/ })).toBeVisible();
      await page.screenshot({ path: `${SHOTS}/settings-${scheme}.png` });
    });

    test("gallery", async ({ page }) => {
      await page.setViewportSize({ width: 900, height: 1500 });
      await page.goto("/#/gallery");
      await expect(page.getByRole("heading", { name: "Design gallery" })).toBeVisible();
      await page.screenshot({ path: `${SHOTS}/gallery-${scheme}.png` });
    });
  });
}

test("keyboard: tab reaches search, new event, calendar, refresh, then settings, and Enter opens settings", async ({ page }) => {
  await page.goto("/");
  await expect(page.getByRole("heading", { name: /Good (morning|afternoon|evening)/ })).toBeVisible();
  await page.keyboard.press("Tab");
  await expect(page.getByRole("button", { name: "Search" })).toBeFocused();
  await page.keyboard.press("Tab");
  await expect(page.getByRole("button", { name: "New event" })).toBeFocused();
  await page.keyboard.press("Tab");
  await expect(page.getByRole("button", { name: "Calendar" })).toBeFocused();
  await page.keyboard.press("Tab");
  await expect(page.getByRole("button", { name: "Refresh" })).toBeFocused();
  await page.keyboard.press("Tab");
  await expect(page.getByRole("button", { name: "Settings" })).toBeFocused();
  await page.keyboard.press("Enter");
  await expect(page.getByRole("heading", { name: "Settings", level: 1 })).toBeVisible();
});

test("only https join links are accepted", async ({ page }) => {
  await page.goto("/");
  const result = await page.evaluate(async () => {
    const { assertHttps } = await import("/src/api/backend.ts");
    const out: Record<string, boolean> = {};
    for (const u of ["https://zoom.example.com/j/1", "http://x.test", "javascript:alert(1)", "file:///etc/passwd", "nope"]) {
      try { assertHttps(u); out[u] = true; } catch { out[u] = false; }
    }
    return out;
  });
  expect(result).toEqual({
    "https://zoom.example.com/j/1": true,
    "http://x.test": false,
    "javascript:alert(1)": false,
    "file:///etc/passwd": false,
    nope: false,
  });
});

test("a meeting link can be joined or copied", async ({ page, context }) => {
  await context.grantPermissions(["clipboard-read", "clipboard-write"]);
  await page.clock.install({ time: todayAt(9, 30) });
  await page.goto("/");
  const card = page.getByLabel("Up next");
  await expect(card.getByRole("button", { name: "Join" })).toBeVisible();
  await card.getByRole("button", { name: "Copy link" }).click();
  await expect(page.getByText("Meeting link copied").first()).toBeVisible();
  const copied = await page.evaluate(() => navigator.clipboard.readText());
  expect(copied).toMatch(/^https:\/\//);
});

test("every upcoming meeting with a link has its own Join and Copy buttons in the list", async ({ page, context }) => {
  await context.grantPermissions(["clipboard-read", "clipboard-write"]);
  await page.clock.install({ time: todayAt(8, 15) });
  await page.goto("/");
  const today = page.getByRole("region", { name: "Today" });
  // At 8:15 three of the sample meetings have links (one running, two coming); each row offers both actions.
  const joins = today.getByRole("button", { name: /^Join / });
  await expect(joins).toHaveCount(3);
  await expect(today.getByRole("button", { name: "Join Team standup" })).toBeVisible();
  await expect(today.getByRole("button", { name: "Join Design review" })).toBeVisible();
  await expect(today.getByRole("button", { name: /^Copy meeting link for /})).toHaveCount(3);

  await today.getByRole("button", { name: "Copy meeting link for Team standup" }).click();
  await expect(page.getByText("Meeting link copied").first()).toBeVisible();
  expect(await page.evaluate(() => navigator.clipboard.readText())).toBe("https://meet.example.com/standup");

  // A meeting with no link has no buttons, and clicking the row itself still opens its details.
  await expect(today.getByRole("button", { name: /Join Lunch with Alex/ })).toHaveCount(0);
  await today.getByRole("button", { name: /Lunch with Alex/ }).click();
  await expect(page.getByRole("dialog", { name: "Lunch with Alex" })).toBeVisible();
});

test("meetings that are already over have no Join button", async ({ page }) => {
  await page.clock.install({ time: todayAt(9, 30) });
  await page.goto("/");
  const today = page.getByRole("region", { name: "Today" });
  await expect(today.getByRole("button", { name: "Join Design review" })).toBeVisible();
  await expect(today.getByRole("button", { name: /Join Team standup/ })).toHaveCount(0);
  await expect(today.getByRole("button", { name: /Join Roadmap planning/ })).toHaveCount(0);
});
