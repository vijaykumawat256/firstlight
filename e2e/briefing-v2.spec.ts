import { expect, test } from "@playwright/test";

const SHOTS = "e2e/screenshots";

function todayAt(h: number, m: number): Date {
  const d = new Date();
  d.setHours(h, m, 0, 0);
  return d;
}

test.use({ reducedMotion: "reduce" });

for (const scheme of ["light", "dark"] as const) {
  test.describe(`${scheme}`, () => {
    test.use({ colorScheme: scheme });

    test("event detail sheet shows the facts and a Join button", async ({ page }) => {
      await page.clock.install({ time: todayAt(9, 30) });
      await page.goto("/");
      await page.getByRole("button", { name: /Design review/ }).first().click();
      const sheet = page.getByRole("dialog", { name: "Design review" });
      await expect(sheet).toBeVisible();
      await expect(sheet.getByText("Room 4B")).toBeVisible();
      await expect(sheet.getByText("Priya", { exact: true })).toBeVisible();
      await expect(sheet.getByText("Work")).toBeVisible();
      await expect(sheet.getByRole("button", { name: "Join" })).toBeVisible();
      await page.screenshot({ path: `${SHOTS}/event-detail-${scheme}.png` });
      await page.keyboard.press("Escape");
      await expect(page.getByRole("dialog")).toBeHidden();
    });

    test("the compact up-next panel", async ({ page }) => {
      await page.setViewportSize({ width: 360, height: 148 });
      await page.clock.install({ time: todayAt(9, 30) });
      await page.goto("/#/upnext");
      await expect(page.getByText(/Up next · in/)).toBeVisible();
      await expect(page.getByText("Design review")).toBeVisible();
      await expect(page.getByRole("button", { name: "Open briefing" })).toBeVisible();
      await expect(page.getByRole("button", { name: "Join" })).toBeVisible();
      await expect(page.getByRole("button", { name: "Dismiss" })).toBeVisible();
      await page.screenshot({ path: `${SHOTS}/up-next-panel-${scheme}.png` });
    });
  });
}

test("free time is called out between meetings", async ({ page }) => {
  await page.clock.install({ time: todayAt(9, 30) });
  await page.goto("/");
  await expect(page.getByLabel("Today")).toContainText("Free:");
});

test("the summary mentions overlaps", async ({ page }) => {
  await page.clock.install({ time: todayAt(9, 30) });
  await page.goto("/");
  await expect(page.getByText(/overlap(s)? to sort out/)).toBeVisible();
});

test("the panel says so when nothing is coming up", async ({ page }) => {
  await page.setViewportSize({ width: 360, height: 148 });
  await page.clock.install({ time: todayAt(23, 0) });
  await page.goto("/#/upnext");
  await expect(page.getByText("Nothing coming up.")).toBeVisible();
});

// ---- meeting prep ------------------------------------------------------------------------

test.describe("meeting prep", () => {
  test.use({ reducedMotion: "reduce" });
  const at = (h: number, m: number) => new Date(new Date().setHours(h, m, 0, 0));

  async function open(page: import("@playwright/test").Page, title: RegExp, flags = "") {
    await page.clock.install({ time: at(9, 30) });
    await page.goto(`/?${flags}`);
    await expect(page.getByRole("heading", { name: /Good (morning|afternoon|evening)/ })).toBeVisible();
    await page.getByRole("button", { name: title }).first().click();
  }

  test("shows what happened last time (links removed), what is around it, and an overlap", async ({ page }) => {
    await open(page, /^Design review/);
    const prep = page.getByRole("region", { name: "Prepare" });
    await expect(prep).toBeVisible();
    await expect(prep.getByText(/Last time/)).toBeVisible();
    await expect(prep.getByText(/Agree the navigation pattern/)).toBeVisible();
    await expect(prep.getByText(/docs\.example\.com|pw=secret/)).toHaveCount(0);
    await expect(prep.getByText(/Overlaps with 1:1 with Sam for 30 minutes/)).toBeVisible();
    await expect(prep.getByText(/before Lunch with Alex/)).toBeVisible();
  });

  test("related unread mail appears when there is some, and has no accessibility problems", async ({ page }) => {
    const { default: AxeBuilder } = await import("@axe-core/playwright");
    await open(page, /^Design review/, "mailon&prepmail");
    const prep = page.getByRole("region", { name: "Prepare" });
    await expect(prep.getByRole("button", { name: "Design review: slides attached" })).toBeVisible();
    const r = await new AxeBuilder({ page }).analyze();
    expect(r.violations).toEqual([]);
  });

  test("a meeting with nothing to prepare shows no Prepare section", async ({ page }) => {
    await open(page, /^Dinner at Nopi/);
    await expect(page.getByRole("dialog", { name: "Dinner at Nopi" })).toBeVisible();
    await expect(page.getByRole("region", { name: "Prepare" })).toHaveCount(0);
  });
});
