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

// ---- conflict help -----------------------------------------------------------------------

test.describe("conflict help", () => {
  test.use({ reducedMotion: "reduce" });
  const at = (h: number, m: number) => new Date(new Date().setHours(h, m, 0, 0));

  async function start(page: import("@playwright/test").Page, flags = "") {
    await page.clock.install({ time: at(9, 30) });
    await page.goto(`/?${flags}`);
    await expect(page.getByRole("heading", { name: /Good (morning|afternoon|evening)/ })).toBeVisible();
  }
  const drafts = (page: import("@playwright/test").Page) =>
    page.evaluate(() => (window as unknown as { __drafts?: Array<{ to: string | null; subject: string; body: string }> }).__drafts ?? []);

  test("suggests the unanswered shorter meeting, says why, lists free times, and drafts a message that names no other meeting", async ({ page }) => {
    await start(page);
    await page.getByRole("button", { name: /^Sort out: .*1:1 with Sam/ }).first().click();
    const dialog = page.getByRole("dialog", { name: "Sort out an overlap" });
    await expect(dialog.getByRole("status")).toContainText("Move 1:1 with Sam");
    await expect(dialog.getByText("You haven't answered the invitation yet")).toBeVisible();
    await expect(dialog.getByText("It's the shorter of the two")).toBeVisible();
    await expect(dialog.getByRole("list", { name: "Free times" }).getByRole("listitem").first()).toBeVisible();
    await expect(dialog.getByRole("button", { name: /^Move here/ })).toHaveCount(0); // not yours to move
    await dialog.getByRole("button", { name: "Write the message" }).click();
    await expect.poll(async () => (await drafts(page)).length).toBe(1);
    const [d] = await drafts(page);
    expect(d!.subject).toBe("Can we move “1:1 with Sam”?");
    expect(d!.body).not.toContain("Design review");
    expect(d!.body).toContain("Could we move it to one of these instead?");
    // Overrule the suggestion.
    await dialog.getByRole("button", { name: "Move Design review instead" }).click();
    await expect(dialog.getByRole("status")).toContainText("Move Design review");
    await expect(dialog.getByText("You chose this one")).toBeVisible();
  });

  test("an event you created can be moved to a free time in one click", async ({ page }) => {
    await start(page);
    await page.getByRole("button", { name: "New event" }).click();
    const sheet = page.getByRole("dialog", { name: "New event" });
    await sheet.getByLabel("Title").fill("Eye test");
    await sheet.getByLabel("Start time").fill("10:45");
    await sheet.getByLabel("End time").fill("11:15");
    await sheet.getByRole("button", { name: "Add event" }).click();
    await page.getByRole("button", { name: /^Sort out: .*Eye test/ }).first().click();
    const dialog = page.getByRole("dialog", { name: "Sort out an overlap" });
    await expect(dialog.getByRole("status")).toContainText("Move Eye test");
    await expect(dialog.getByText("You created it, so you can change it yourself")).toBeVisible();
    await dialog.getByRole("button", { name: /^Move here/ }).first().click();
    await expect(page.getByText("Moved").first()).toBeVisible();
    await expect(page.getByRole("dialog", { name: "Sort out an overlap" })).toHaveCount(0);
  });

  test("the sheet has no accessibility problems", async ({ page }) => {
    const { default: AxeBuilder } = await import("@axe-core/playwright");
    await start(page);
    await page.getByRole("button", { name: /^Sort out: .*1:1 with Sam/ }).first().click();
    await expect(page.getByRole("dialog", { name: "Sort out an overlap" }).getByRole("status")).toBeVisible();
    const r = await new AxeBuilder({ page }).analyze();
    expect(r.violations).toEqual([]);
  });
});
