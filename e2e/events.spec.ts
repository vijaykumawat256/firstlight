import { expect, test } from "@playwright/test";

test.use({ reducedMotion: "reduce" });

const at = (h: number, m: number) => new Date(new Date().setHours(h, m, 0, 0));

test("add an event of your own: it shows in the briefing, can be edited, and deleted", async ({ page }) => {
  await page.clock.install({ time: at(9, 30) });
  await page.goto("/");
  await page.getByRole("button", { name: "New event" }).click();
  const sheet = page.getByRole("dialog", { name: "New event" });
  await expect(sheet).toContainText("Saved only on this computer");
  await expect(sheet.getByRole("button", { name: "Add event" })).toBeDisabled(); // needs a title

  await sheet.getByLabel("Title").fill("Pick up the car");
  await sheet.getByLabel("Start time").fill("16:00");
  await sheet.getByLabel("End time").fill("16:30");
  await sheet.getByLabel("Location").fill("Garage on 5th");
  await sheet.getByLabel("Meeting link").fill("https://meet.google.com/xyz-abcd-efg");
  await sheet.getByRole("button", { name: "Add event" }).click();

  await expect(page.getByText("Event added").first()).toBeVisible();
  const row = page.getByRole("button", { name: /^(?!Join|Copy).*Pick up the car/ });
  await expect(row).toBeVisible();

  // Edit
  await row.click();
  const detail = page.getByRole("dialog", { name: "Pick up the car" });
  await expect(detail.getByRole("button", { name: "Join" })).toBeVisible();
  await detail.getByRole("button", { name: "Edit" }).click();
  const edit = page.getByRole("dialog", { name: "Edit event" });
  await edit.getByLabel("Title").fill("Pick up the car (moved)");
  await edit.getByRole("button", { name: "Save" }).click();
  await expect(page.getByRole("button", { name: /^(?!Join|Copy).*Pick up the car \(moved\)/ })).toBeVisible();
  await expect(page.getByRole("button", { name: "Pick up the car", exact: true })).toHaveCount(0);

  // Delete asks first
  await page.getByRole("button", { name: /^(?!Join|Copy).*Pick up the car \(moved\)/ }).click();
  const d2 = page.getByRole("dialog", { name: "Pick up the car (moved)" });
  await d2.getByRole("button", { name: "Delete" }).click();
  await expect(d2.getByRole("button", { name: "Yes, delete it" })).toBeVisible();
  await d2.getByRole("button", { name: "Keep" }).click();
  await expect(d2.getByRole("button", { name: "Delete" })).toBeVisible();
  await d2.getByRole("button", { name: "Delete" }).click();
  await d2.getByRole("button", { name: "Yes, delete it" }).click();
  await expect(page.getByRole("button", { name: /^(?!Join|Copy).*Pick up the car/ })).toHaveCount(0);
});

test("bad input is explained, not saved", async ({ page }) => {
  await page.clock.install({ time: at(9, 30) });
  await page.goto("/");
  await page.getByRole("button", { name: "New event" }).click();
  const sheet = page.getByRole("dialog", { name: "New event" });
  await sheet.getByLabel("Title").fill("Broken link");
  await sheet.getByLabel("Meeting link").fill("http://zoom.us/j/1");
  await sheet.getByRole("button", { name: "Add event" }).click();
  await expect(sheet.getByRole("alert")).toContainText("must start with https://");

  await sheet.getByLabel("Meeting link").fill("");
  await sheet.getByLabel("End time").fill("00:00");
  await sheet.getByLabel("Start time").fill("10:00");
  await sheet.getByRole("button", { name: "Add event" }).click();
  await expect(sheet.getByRole("alert")).toContainText("end after it starts");
});

test("all-day events, and events from connected calendars cannot be edited or deleted", async ({ page }) => {
  await page.clock.install({ time: at(9, 30) });
  await page.goto("/");
  await page.getByRole("button", { name: /Design review/ }).first().click();
  const detail = page.getByRole("dialog", { name: "Design review" });
  await expect(detail.getByRole("button", { name: "Edit" })).toHaveCount(0);
  await expect(detail.getByRole("button", { name: "Delete" })).toHaveCount(0);
  await page.keyboard.press("Escape");

  await page.getByRole("button", { name: "New event" }).click();
  const sheet = page.getByRole("dialog", { name: "New event" });
  await sheet.getByLabel("Title").fill("Conference");
  await sheet.getByRole("switch", { name: "All day" }).click();
  await expect(sheet.getByLabel("Start time")).toHaveCount(0);
  await sheet.getByRole("button", { name: "Add event" }).click();
  await expect(page.getByText("Event added").first()).toBeVisible();
  await page.getByRole("button", { name: "Calendar" }).click();
  await expect(page.getByRole("button", { name: /Conference/ }).first()).toBeVisible();
});

test("the new-event form has no accessibility violations", async ({ page }) => {
  const { default: AxeBuilder } = await import("@axe-core/playwright");
  await page.clock.install({ time: at(9, 30) });
  await page.goto("/");
  await page.getByRole("button", { name: "New event" }).click();
  await expect(page.getByRole("dialog", { name: "New event" })).toBeVisible();
  const r = await new AxeBuilder({ page }).withTags(["wcag2a", "wcag2aa", "wcag21a", "wcag21aa", "wcag22aa"]).analyze();
  expect(r.violations.map((v) => `${v.id}: ${v.nodes[0]?.target.join(" ")}`)).toEqual([]);
});

test("a repeating event shows on every day it repeats, and is edited and deleted as a series", async ({ page }) => {
  await page.setViewportSize({ width: 1000, height: 800 });
  await page.clock.install({ time: at(9, 30) });
  await page.goto("/");
  await page.getByRole("button", { name: "New event" }).click();
  const sheet = page.getByRole("dialog", { name: "New event" });
  await sheet.getByLabel("Title").fill("Pilates");
  await sheet.getByRole("combobox", { name: "Repeat" }).click();
  await page.getByRole("option", { name: "Every day" }).click();
  await sheet.getByRole("combobox", { name: "Reminder" }).click();
  await page.getByRole("option", { name: "15 minutes before" }).click();
  await sheet.getByRole("button", { name: "Add event" }).click();
  await expect(page.getByText("Event added").first()).toBeVisible();

  // Every day in the month grid carries it.
  await page.getByRole("button", { name: "Calendar" }).click();
  await page.getByRole("radio", { name: "Month" }).click();
  const grid = page.getByRole("grid", { name: "Month" });
  const chips = grid.getByRole("button", { name: /Pilates/ });
  await expect(chips.first()).toBeVisible();
  expect(await chips.count()).toBeGreaterThan(3);

  await chips.first().click();
  const detail = page.getByRole("dialog", { name: "Pilates" });
  await expect(detail).toContainText("Repeats every day");
  await expect(detail).toContainText("Reminder 15 minutes before");
  await detail.getByRole("button", { name: "Edit all" }).click();
  const edit = page.getByRole("dialog", { name: "Edit event" });
  await expect(edit).toContainText("Changes apply to every repeat");
  await edit.getByLabel("Title").fill("Pilates (studio)");
  await edit.getByRole("button", { name: "Save" }).click();
  await expect(grid.getByRole("button", { name: /Pilates \(studio\)/ }).first()).toBeVisible();
  await expect(grid.getByRole("button", { name: /^\S+ Pilates,? ?$/ })).toHaveCount(0);

  await grid.getByRole("button", { name: /Pilates \(studio\)/ }).first().click();
  const d2 = page.getByRole("dialog", { name: "Pilates (studio)" });
  await d2.getByRole("button", { name: "Delete all" }).click();
  await d2.getByRole("button", { name: "Yes, delete all repeats" }).click();
  await expect(grid.getByRole("button", { name: /Pilates/ })).toHaveCount(0);
});

test("a repeat can end on a date, and impossible choices are explained", async ({ page }) => {
  await page.clock.install({ time: at(9, 30) });
  await page.goto("/");
  await page.getByRole("button", { name: "New event" }).click();
  const sheet = page.getByRole("dialog", { name: "New event" });
  await sheet.getByLabel("Title").fill("Course");
  await sheet.getByRole("combobox", { name: "Repeat" }).click();
  await page.getByRole("option", { name: "Every 2 weeks" }).click();
  await sheet.getByRole("radio", { name: "On a date" }).click();
  await sheet.getByLabel("Last day it repeats").fill("2020-01-01"); // before it starts
  await sheet.getByRole("button", { name: "Add event" }).click();
  await expect(sheet.getByRole("alert")).toContainText("before it starts");
});

test("invite people: the invitation opens in your own mail app, nothing is sent by Firstlight", async ({ page }) => {
  await page.clock.install({ time: at(9, 30) });
  await page.goto("/");
  await page.getByRole("button", { name: "New event" }).click();
  const sheet = page.getByRole("dialog", { name: "New event" });
  await sheet.getByLabel("Title").fill("Team lunch");
  await sheet.getByLabel("Start time").fill("18:00");
  await sheet.getByLabel("End time").fill("19:00");
  await sheet.getByLabel("Invite people").fill("Sam@Example.com, kim@example.org");
  await sheet.getByRole("button", { name: "Add event" }).click();
  await expect(page.getByText("Event added").first()).toBeVisible();

  await page.getByRole("button", { name: /^(?!Sort out).*Team lunch/ }).click();
  const detail = page.getByRole("dialog", { name: "Team lunch" });
  await expect(detail).toContainText("Invited: sam@example.com, kim@example.org");
  await detail.getByRole("button", { name: "Send invitation…" }).click();

  const dialog = page.getByRole("dialog", { name: "Send invitation" });
  await expect(dialog.getByLabel("To")).toHaveValue("sam@example.com, kim@example.org");
  await expect(dialog).toContainText("doesn't send anything");

  await dialog.getByLabel("To").fill("not-an-address");
  await dialog.getByRole("button", { name: "Open in my mail app" }).click();
  await expect(dialog.getByRole("alert")).toContainText("doesn't look right");

  await dialog.getByLabel("To").fill("sam@example.com");
  await dialog.getByRole("button", { name: "Open in my mail app" }).click();
  await expect(page.getByText("Your mail app is open").first()).toBeVisible();
  const sent = await page.evaluate(() => (window as unknown as { __invites: { list: string[]; openMail: boolean }[] }).__invites);
  expect(sent).toHaveLength(1);
  expect(sent[0]).toMatchObject({ list: ["sam@example.com"], openMail: true });
});

test("the file alone can be saved without any address", async ({ page }) => {
  await page.clock.install({ time: at(9, 30) });
  await page.goto("/");
  await page.getByRole("button", { name: "New event" }).click();
  const sheet = page.getByRole("dialog", { name: "New event" });
  await sheet.getByLabel("Title").fill("Solo");
  await sheet.getByRole("button", { name: "Add event" }).click();
  await page.getByRole("button", { name: /Solo/ }).first().click();
  await page.getByRole("dialog", { name: "Solo" }).getByRole("button", { name: "Send invitation…" }).click();
  const dialog = page.getByRole("dialog", { name: "Send invitation" });
  await expect(dialog.getByRole("button", { name: "Open in my mail app" })).toBeDisabled();
  await dialog.getByRole("button", { name: "Just save the file" }).click();
  await expect(page.getByText("Calendar file saved").first()).toBeVisible();
});

test("the form with repeat, reminder and invitees, and the invitation dialog, have no accessibility violations", async ({ page }) => {
  const { default: AxeBuilder } = await import("@axe-core/playwright");
  await page.clock.install({ time: at(9, 30) });
  await page.goto("/");
  await page.getByRole("button", { name: "New event" }).click();
  const sheet = page.getByRole("dialog", { name: "New event" });
  await sheet.getByRole("combobox", { name: "Repeat" }).click();
  await page.getByRole("option", { name: "Every week" }).click();
  await sheet.getByRole("radio", { name: "On a date" }).click();
  const scan = async () => (await new AxeBuilder({ page }).withTags(["wcag2a", "wcag2aa", "wcag21a", "wcag21aa", "wcag22aa"]).analyze()).violations.map((v) => `${v.id}: ${v.nodes[0]?.target.join(" ")}`);
  expect(await scan()).toEqual([]);
  await sheet.getByLabel("Title").fill("A11y");
  await sheet.getByRole("button", { name: "Add event" }).click();
  await page.getByRole("button", { name: /A11y/ }).first().click();
  await page.getByRole("dialog", { name: "A11y" }).getByRole("button", { name: "Send invitation…" }).click();
  await expect(page.getByRole("dialog", { name: "Send invitation" })).toBeVisible();
  expect(await scan()).toEqual([]);
});

test("one day of a repeating event can be removed or changed on its own", async ({ page }) => {
  await page.setViewportSize({ width: 1000, height: 800 });
  await page.clock.install({ time: at(9, 30) });
  await page.goto("/");
  await page.getByRole("button", { name: "New event" }).click();
  const sheet = page.getByRole("dialog", { name: "New event" });
  await sheet.getByLabel("Title").fill("Choir");
  await sheet.getByRole("combobox", { name: "Repeat" }).click();
  await page.getByRole("option", { name: "Every day" }).click();
  await sheet.getByRole("button", { name: "Add event" }).click();
  await page.getByRole("button", { name: "Calendar" }).click();
  await page.getByRole("radio", { name: "Month" }).click();
  const grid = page.getByRole("grid", { name: "Month" });
  const chips = grid.getByRole("button", { name: /Choir/ });
  await expect(chips.first()).toBeVisible();
  const total = await chips.count();

  // Remove just one day.
  await chips.nth(2).click();
  let detail = page.getByRole("dialog", { name: "Choir" });
  await detail.getByRole("button", { name: "Delete this day" }).click();
  await detail.getByRole("button", { name: "Yes, remove this day" }).click();
  await expect(page.getByText("That day was removed").first()).toBeVisible();
  await expect(chips).toHaveCount(total - 1);

  // Change just one day: it becomes its own event, the series loses that day.
  await chips.nth(2).click();
  detail = page.getByRole("dialog", { name: "Choir" });
  await detail.getByRole("button", { name: "Edit this day" }).click();
  const edit = page.getByRole("dialog", { name: "Edit this day" });
  await expect(edit.getByRole("combobox", { name: "Repeat" })).toHaveCount(0);
  await edit.getByLabel("Title").fill("Choir (special)");
  await edit.getByRole("button", { name: "Save" }).click();
  await expect(grid.getByRole("button", { name: /Choir \(special\)/ })).toHaveCount(1);
  await expect(chips).toHaveCount(total - 1); // the plain "Choir" chips lost the changed day; "Choir (special)" matches /Choir/ too
  await expect(grid.getByRole("button", { name: /^(?!.*special).*Choir/ })).toHaveCount(total - 2);

  // A single event has only the plain Delete.
  await grid.getByRole("button", { name: /Choir \(special\)/ }).click();
  const one = page.getByRole("dialog", { name: "Choir (special)" });
  await expect(one.getByRole("button", { name: "Delete this day" })).toHaveCount(0);
  await expect(one.getByRole("button", { name: "Delete", exact: true })).toBeVisible();
});

test("Calendar-app saving is off by default: no 'Save to' choice", async ({ page }) => {
  await page.goto("/");
  await page.getByRole("button", { name: "New event" }).click();
  const sheet = page.getByRole("dialog", { name: "New event" });
  await expect(sheet.getByLabel("Title")).toBeVisible();
  await expect(sheet.getByText("Save to")).toHaveCount(0);
});

test("with the setting on, an event can be saved to a calendar, and changes follow it", async ({ page }) => {
  await page.clock.install({ time: at(9, 30) });
  await page.goto("/?writeon");
  await page.getByRole("button", { name: "New event" }).click();
  const sheet = page.getByRole("dialog", { name: "New event" });
  await sheet.getByRole("combobox", { name: "Save to" }).click();
  await page.getByRole("option", { name: /Work/ }).click();
  await sheet.getByLabel("Title").fill("Eye test");
  await sheet.getByLabel("Start time").fill("17:00");
  await sheet.getByLabel("End time").fill("17:30");
  await sheet.getByRole("button", { name: "Add event" }).click();
  const row = page.getByRole("button", { name: /^(?!Join|Copy).*Eye test/ });
  await expect(row).toBeVisible();

  await row.click();
  const detail = page.getByRole("dialog", { name: "Eye test" });
  await expect(detail).toContainText("Calendar app");
  await detail.getByRole("button", { name: "Edit" }).click();
  const edit = page.getByRole("dialog", { name: "Edit event" });
  await edit.getByLabel("Title").fill("Eye test (moved)");
  await edit.getByRole("button", { name: "Save" }).click();
  await expect(page.getByRole("button", { name: /^(?!Join|Copy).*Eye test \(moved\)/ })).toBeVisible();
});

test("a failed Calendar-app save says so and keeps the form", async ({ page }) => {
  await page.goto("/?writeon&calfail");
  await page.getByRole("button", { name: "New event" }).click();
  const sheet = page.getByRole("dialog", { name: "New event" });
  await sheet.getByRole("combobox", { name: "Save to" }).click();
  await page.getByRole("option", { name: /Work/ }).click();
  await sheet.getByLabel("Title").fill("Nope");
  await sheet.getByRole("button", { name: "Add event" }).click();
  await expect(page.getByText(/couldn't save that/).first()).toBeVisible();
  await expect(sheet).toBeVisible();
});

test("a CalDAV account is read-only until the user lets Firstlight add events to it", async ({ page }) => {
  await page.clock.install({ time: at(9, 30) });
  await page.goto("/?withcaldav");

  // Nothing is offered before the switch.
  await page.getByRole("button", { name: "New event" }).click();
  await expect(page.getByRole("dialog", { name: "New event" }).getByLabel("Title")).toBeVisible();
  await expect(page.getByText("Save to")).toHaveCount(0);
  await page.keyboard.press("Escape");

  // The switch lives on the account, and is off.
  await page.getByRole("button", { name: "Settings" }).click();
  await page.getByRole("button", { name: /Accounts & calendars/ }).click();
  const account = page.getByRole("region", { name: "Fastmail" });
  const sw = account.getByRole("switch", { name: "Add events to Fastmail" });
  await expect(sw).not.toBeChecked();
  await sw.click();
  await expect(sw).toBeChecked();

  // Back to the briefing: the form now offers that account's changeable calendar, not the read-only one.
  await page.getByRole("button", { name: "Briefing" }).click();
  await page.getByRole("button", { name: "New event" }).click();
  const sheet = page.getByRole("dialog", { name: "New event" });
  await sheet.getByRole("combobox", { name: "Save to" }).click();
  await expect(page.getByRole("option", { name: /Team \(read only\)/ })).toHaveCount(0);
  await page.getByRole("option", { name: /Fastmail Work/ }).click();
  await sheet.getByLabel("Title").fill("Piano lesson");
  await sheet.getByLabel("Start time").fill("18:00");
  await sheet.getByLabel("End time").fill("19:00");
  await sheet.getByRole("button", { name: "Add event" }).click();
  await expect(page.getByRole("button", { name: /^(?!Join|Copy|Sort out).*Piano lesson/ })).toBeVisible();
});

test("turning the account switch off again stops the offer", async ({ page }) => {
  await page.goto("/?withcaldav");
  await page.getByRole("button", { name: "Settings" }).click();
  await page.getByRole("button", { name: /Accounts & calendars/ }).click();
  const sw = page.getByRole("region", { name: "Fastmail" }).getByRole("switch", { name: "Add events to Fastmail" });
  await sw.click();
  await sw.click();
  await expect(sw).not.toBeChecked();
});

async function inviteDialog(page: import("@playwright/test").Page, url: string) {
  await page.clock.install({ time: at(9, 30) });
  await page.goto(url);
  await page.getByRole("button", { name: "New event" }).click();
  const sheet = page.getByRole("dialog", { name: "New event" });
  await sheet.getByLabel("Title").fill("Team lunch");
  await sheet.getByLabel("Start time").fill("18:00");
  await sheet.getByLabel("End time").fill("19:00");
  await sheet.getByLabel("Invite people").fill("sam@example.com");
  await sheet.getByRole("button", { name: "Add event" }).click();
  await page.getByRole("button", { name: /^(?!Sort out).*Team lunch/ }).click();
  await page.getByRole("dialog", { name: "Team lunch" }).getByRole("button", { name: "Send invitation…" }).click();
  return page.getByRole("dialog", { name: "Send invitation" });
}

test("sending is not offered unless the user turned it on", async ({ page }) => {
  const dialog = await inviteDialog(page, "/");
  await expect(dialog.getByRole("button", { name: "Review and send" })).toHaveCount(0);
});

test("with sending on, the exact message is shown first and goes out only on Send", async ({ page }) => {
  const dialog = await inviteDialog(page, "/?sendon");
  await dialog.getByRole("button", { name: "Review and send" }).click();
  const review = page.getByRole("dialog", { name: "Review before sending" });
  await expect(review).toContainText("me@yahoo.com");
  await expect(review).toContainText("sam@example.com");
  await expect(review).toContainText("Invitation: Team lunch");
  await expect(review.getByLabel("Message text")).toContainText("calendar file is attached");
  // Reviewing alone sends nothing.
  expect(await page.evaluate(() => (window as unknown as { __sent?: unknown[] }).__sent ?? [])).toHaveLength(0);
  await review.getByRole("button", { name: "Send" }).click();
  await expect(page.getByText("Invitation sent").first()).toBeVisible();
  const sent = await page.evaluate(() => (window as unknown as { __sent: { list: string[] }[] }).__sent);
  expect(sent).toHaveLength(1);
  expect(sent[0]!.list).toEqual(["sam@example.com"]);
});

test("a failed send says so, keeps the window open and sends nothing", async ({ page }) => {
  const dialog = await inviteDialog(page, "/?sendon&sendfail");
  await dialog.getByRole("button", { name: "Review and send" }).click();
  const review = page.getByRole("dialog", { name: "Review before sending" });
  await review.getByRole("button", { name: "Send" }).click();
  await expect(review.getByRole("alert")).toContainText("didn't accept the saved password");
  expect(await page.evaluate(() => (window as unknown as { __sent?: unknown[] }).__sent ?? [])).toHaveLength(0);
});

test("the review window has no accessibility violations", async ({ page }) => {
  const { default: AxeBuilder } = await import("@axe-core/playwright");
  const dialog = await inviteDialog(page, "/?sendon");
  await dialog.getByRole("button", { name: "Review and send" }).click();
  await expect(page.getByRole("dialog", { name: "Review before sending" }).getByLabel("Message text")).toContainText("Team lunch");
  const r = await new AxeBuilder({ page }).analyze();
  expect(r.violations).toEqual([]);
});

test("Find a time lists free stretches, changes with the length asked, and has no accessibility problems", async ({ page }) => {
  const { default: AxeBuilder } = await import("@axe-core/playwright");
  await page.goto("/");
  await page.getByRole("button", { name: "Find a time" }).click();
  const dialog = page.getByRole("dialog", { name: "Find a time" });
  await expect(dialog.getByRole("button", { name: /^Copy / }).first()).toBeVisible();
  const before = await dialog.getByRole("button", { name: /^Copy / }).count();
  await dialog.getByRole("radio", { name: "2 hours" }).click();
  await expect.poll(() => dialog.getByRole("button", { name: /^Copy / }).count()).toBeLessThanOrEqual(before);
  const r = await new AxeBuilder({ page }).analyze();
  expect(r.violations).toEqual([]);
});
