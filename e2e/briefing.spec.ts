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
      await expect(page.getByRole("button", { name: "Join" })).toBeVisible();
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

    test("settings sheet", async ({ page }) => {
      await page.clock.install({ time: todayAt(9, 30) });
      await page.goto("/");
      await page.getByRole("button", { name: "Settings" }).click();
      await expect(page.getByRole("dialog", { name: "Settings" })).toBeVisible();
      await expect(page.getByRole("switch", { name: "Start at login" })).toBeVisible();
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

test("keyboard: tab reaches refresh then settings, and Enter opens the sheet", async ({ page }) => {
  await page.goto("/");
  await expect(page.getByRole("heading", { name: /Good (morning|afternoon|evening)/ })).toBeVisible();
  await page.keyboard.press("Tab");
  await expect(page.getByRole("button", { name: "Refresh" })).toBeFocused();
  await page.keyboard.press("Tab");
  await expect(page.getByRole("button", { name: "Settings" })).toBeFocused();
  await page.keyboard.press("Enter");
  await expect(page.getByRole("dialog", { name: "Settings" })).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(page.getByRole("dialog")).toBeHidden();
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
