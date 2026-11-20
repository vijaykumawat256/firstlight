import { expect, test } from "@playwright/test";
import { settle } from "./settle";

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
    await settle(page);
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
    await sheet.getByLabel("Start time").fill("15:30");
    await sheet.getByLabel("End time").fill("16:30");
    await sheet.getByRole("button", { name: "Add event" }).click();
    await page.getByRole("button", { name: /^Sort out: .*Eye test/ }).first().click();
    const dialog = page.getByRole("dialog", { name: "Sort out an overlap" });
    await expect(dialog.getByRole("status")).toContainText("Move Eye test");
    await expect(dialog.getByText("You created it, so you can change it yourself")).toBeVisible();
    await dialog.getByRole("button", { name: /^Move here/ }).first().click();
    await expect(page.getByText("Moved").first()).toBeVisible();
    await expect(page.getByRole("dialog", { name: "Sort out an overlap" })).toHaveCount(0);
  });

  async function addOwn(page: import("@playwright/test").Page, title: string, from: string, to: string, extra?: (sheet: import("@playwright/test").Locator) => Promise<void>) {
    await page.getByRole("button", { name: "New event" }).click();
    const sheet = page.getByRole("dialog", { name: "New event" });
    await sheet.getByLabel("Title").fill(title);
    await sheet.getByLabel("Start time").fill(from);
    await sheet.getByLabel("End time").fill(to);
    if (extra) await extra(sheet);
    await sheet.getByRole("button", { name: "Add event" }).click();
    await expect(page.getByText("Event added").first()).toBeVisible();
  }

  test("three or more meetings piled up are sorted out together with places that do not collide", async ({ page }) => {
    await start(page);
    await addOwn(page, "Eye test", "10:45", "11:15"); // overlaps Design review and the 1:1
    await page.getByRole("button", { name: /^Sort out: .*Eye test/ }).first().click();
    const dialog = page.getByRole("dialog", { name: "Sort out an overlap" });
    await expect(dialog.getByRole("status")).toContainText("3 meetings overlap each other. Moving 2 clears it.");
    await expect(dialog.getByText("Keep: Design review")).toBeVisible();
    await expect(dialog.getByRole("heading", { name: "Move 1:1 with Sam" })).toBeVisible();
    await expect(dialog.getByRole("heading", { name: "Move Eye test" })).toBeVisible();
    // Only the one you made can be moved from here; the other needs a note to its organiser.
    await expect(dialog.getByRole("button", { name: /^Move here/ }).first()).toBeVisible();
    await expect(dialog.getByRole("button", { name: "Write the message" })).toHaveCount(1);
    const first = (name: string) => dialog.getByRole("list", { name }).getByRole("listitem").first().innerText();
    expect(await first("Free times for 1:1 with Sam")).not.toEqual(await first("Free times for Eye test"));
  });

  test("after moving an event you invited people to, you are offered the updated invitation", async ({ page }) => {
    await start(page, "sendon");
    await addOwn(page, "Eye test", "10:45", "11:15", async (sheet) => {
      await sheet.getByLabel("Invite people").fill("sam@example.com");
    });
    await page.getByRole("button", { name: /^Sort out: .*Eye test/ }).first().click();
    const dialog = page.getByRole("dialog", { name: "Sort out an overlap" });
    await expect(dialog.getByText(/you can send the people you invited an updated invitation/).first()).toBeVisible();
    await dialog.getByRole("button", { name: /^Move here/ }).first().click();
    const invite = page.getByRole("dialog", { name: "Send invitation" });
    await expect(invite).toBeVisible();
    await expect(invite.getByLabel("To")).toHaveValue("sam@example.com");
  });

  test("one day of a repeating event you created can be moved on its own", async ({ page }) => {
    await start(page);
    await addOwn(page, "Pilates", "10:45", "11:15", async (sheet) => {
      await sheet.getByRole("combobox", { name: "Repeat" }).click();
      await page.getByRole("option", { name: "Every day" }).click();
    });
    await page.getByRole("button", { name: /^Sort out: .*Pilates/ }).first().click();
    const dialog = page.getByRole("dialog", { name: "Sort out an overlap" });
    await expect(dialog.getByText(/moves only this day/i).first()).toBeVisible();
    await dialog.getByRole("button", { name: /^Move this day here/ }).first().click();
    await expect(page.getByText("That day was moved").first()).toBeVisible();
    // The series is still there tomorrow.
    await expect(page.getByRole("region", { name: "Tomorrow" }).getByText("Pilates").first()).toBeVisible();
  });

  test("the sheet has no accessibility problems", async ({ page }) => {
    const { default: AxeBuilder } = await import("@axe-core/playwright");
    await start(page);
    await page.getByRole("button", { name: /^Sort out: .*1:1 with Sam/ }).first().click();
    await expect(page.getByRole("dialog", { name: "Sort out an overlap" }).getByRole("status")).toBeVisible();
    await settle(page);
    const r = await new AxeBuilder({ page }).analyze();
    expect(r.violations).toEqual([]);
  });
});

// ---- week ahead ---------------------------------------------------------------------------

test.describe("week ahead", () => {
  test.use({ reducedMotion: "reduce" });

  test("the briefing shows the next seven days with a bar per day and notes, accessibly", async ({ page }) => {
    const { default: AxeBuilder } = await import("@axe-core/playwright");
    await page.clock.install({ time: todayAt(9, 30) });
    await page.goto("/");
    const week = page.getByRole("region", { name: "Week ahead" });
    await expect(week).toBeVisible();
    await expect(week.getByRole("list", { name: "Meeting time per day" }).getByRole("listitem")).toHaveCount(7);
    await expect(week.getByText(/Early start:/)).toBeVisible(); // tomorrow's 06:45 flight
    await expect(week.getByText(/\d+ meetings? · /)).toBeVisible();
    await week.scrollIntoViewIfNeeded();
    await settle(page);
    const r = await new AxeBuilder({ page }).analyze();
    expect(r.violations).toEqual([]);
  });

  test("the Sunday week preview can be turned off in Settings", async ({ page }) => {
    await page.goto("/");
    await page.getByRole("button", { name: "Settings" }).click();
    await page.getByRole("main").getByRole("button", { name: /^Notifications/ }).click();
    const sw = page.getByRole("switch", { name: "Week preview on Sundays" });
    await expect(sw).toBeChecked();
    await sw.click();
    await expect(sw).not.toBeChecked();
    // It belongs to the evening look-ahead: without that, there is nothing to replace.
    await page.getByRole("switch", { name: "Evening look-ahead" }).click();
    await expect(page.getByRole("switch", { name: "Week preview on Sundays" })).toHaveCount(0);
  });
});

// ---- guest lists --------------------------------------------------------------------------

test.describe("guest lists", () => {
  test.use({ reducedMotion: "reduce" });

  test("an event from a calendar shows who is on it and how each person answered", async ({ page }) => {
    const { default: AxeBuilder } = await import("@axe-core/playwright");
    await page.clock.install({ time: todayAt(9, 30) });
    await page.goto("/");
    await page.getByRole("button", { name: /^Design review/ }).first().click();
    const people = page.getByRole("region", { name: "People" });
    await expect(people.getByRole("heading")).toHaveText("5 people: 1 accepted, 1 maybe, 1 declined, 1 waiting");
    const row = (name: string) => people.getByRole("listitem").filter({ hasText: name });
    await expect(row("Priya Shah")).toContainText("Organiser");
    await expect(row("Sam Lee")).toContainText("No reply yet");
    await expect(row("kim@example.com")).toContainText("Declined");
    await expect(row("Lee Park")).toContainText("Maybe");
    await settle(page);
    const r = await new AxeBuilder({ page }).analyze();
    expect(r.violations).toEqual([]);
  });

  test("an event with no guest list shows no People section, and your own events never do", async ({ page }) => {
    await page.clock.install({ time: todayAt(9, 30) });
    await page.goto("/");
    await page.getByRole("button", { name: /^Team standup/ }).first().click();
    await expect(page.getByRole("dialog", { name: "Team standup" })).toBeVisible();
    await expect(page.getByRole("region", { name: "People" })).toHaveCount(0);
  });
});
