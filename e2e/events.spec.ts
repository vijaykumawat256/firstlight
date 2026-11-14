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
  const row = page.getByRole("button", { name: /Pick up the car/ });
  await expect(row).toBeVisible();

  // Edit
  await row.click();
  const detail = page.getByRole("dialog", { name: "Pick up the car" });
  await expect(detail.getByRole("button", { name: "Join" })).toBeVisible();
  await detail.getByRole("button", { name: "Edit" }).click();
  const edit = page.getByRole("dialog", { name: "Edit event" });
  await edit.getByLabel("Title").fill("Pick up the car (moved)");
  await edit.getByRole("button", { name: "Save" }).click();
  await expect(page.getByRole("button", { name: /Pick up the car \(moved\)/ })).toBeVisible();
  await expect(page.getByRole("button", { name: /^Pick up the car,? / })).toHaveCount(0);

  // Delete asks first
  await page.getByRole("button", { name: /Pick up the car \(moved\)/ }).click();
  const d2 = page.getByRole("dialog", { name: "Pick up the car (moved)" });
  await d2.getByRole("button", { name: "Delete" }).click();
  await expect(d2.getByRole("button", { name: "Yes, delete it" })).toBeVisible();
  await d2.getByRole("button", { name: "Keep" }).click();
  await expect(d2.getByRole("button", { name: "Delete" })).toBeVisible();
  await d2.getByRole("button", { name: "Delete" }).click();
  await d2.getByRole("button", { name: "Yes, delete it" }).click();
  await expect(page.getByRole("button", { name: /Pick up the car/ })).toHaveCount(0);
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

  await page.getByRole("button", { name: /Team lunch/ }).click();
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
