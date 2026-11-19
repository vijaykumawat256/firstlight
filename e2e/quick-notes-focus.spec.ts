import { expect, test } from "@playwright/test";

test.use({ reducedMotion: "reduce" });

const at = (h: number, m: number) => new Date(new Date().setHours(h, m, 0, 0));

test("quick add: one typed line fills the form, and Return adds the event", async ({ page }) => {
  await page.clock.install({ time: at(9, 30) });
  await page.goto("/");
  await page.getByRole("button", { name: "New event" }).click();
  const sheet = page.getByRole("dialog", { name: "New event" });
  const quick = sheet.getByLabel("Quick add");
  await expect(quick).toBeFocused();
  await quick.fill("Pick up parcel today 4pm for 30 min at Post office");
  await expect(sheet.getByLabel("Title")).toHaveValue("Pick up parcel");
  await expect(sheet.getByLabel("Start time")).toHaveValue("16:00");
  await expect(sheet.getByLabel("End time")).toHaveValue("16:30");
  await expect(sheet.getByLabel("Location")).toHaveValue("Post office");
  await expect(sheet).toContainText("Understood: Pick up parcel");
  await quick.press("Enter");
  await expect(page.getByText("Event added").first()).toBeVisible();
  await expect(page.getByRole("button", { name: /^(?!Join|Copy).*Pick up parcel/ })).toBeVisible();
});

test("quick add asks for a day or time when the line has none, and adds nothing", async ({ page }) => {
  await page.goto("/");
  await page.getByRole("button", { name: "New event" }).click();
  const sheet = page.getByRole("dialog", { name: "New event" });
  await sheet.getByLabel("Quick add").fill("Dentist");
  await expect(sheet).toContainText("Add a day or a time");
  await expect(sheet.getByLabel("Title")).toHaveValue("");
});

test("search offers to add a typed event, but never for a question", async ({ page }) => {
  await page.clock.install({ time: at(9, 30) });
  await page.goto("/");
  await page.getByRole("button", { name: "Search" }).click();
  const search = page.getByRole("dialog", { name: "Search" });
  const box = search.getByRole("combobox", { name: "Search events" });
  await box.fill("what's on today?");
  await expect(search.getByRole("option", { name: /^Add:/ })).toHaveCount(0);
  await box.fill("Bike repair today 5pm");
  await expect(search.getByRole("option").first()).toHaveText(/^Add: Bike repair · .*5:00/);
  await box.press("Enter");
  const sheet = page.getByRole("dialog", { name: "New event" });
  await expect(sheet.getByLabel("Title")).toHaveValue("Bike repair");
  await expect(sheet.getByLabel("Start time")).toHaveValue("17:00");
  await sheet.getByRole("button", { name: "Add event" }).click();
  await expect(page.getByRole("button", { name: "Bike repair", exact: true })).toBeVisible();
});

test("meeting notes are saved as you type and are there when the meeting is opened again", async ({ page }) => {
  const { default: AxeBuilder } = await import("@axe-core/playwright");
  await page.clock.install({ time: at(8, 0) });
  await page.goto("/");
  await page.getByRole("button", { name: "Design review", exact: true }).first().click();
  const detail = page.getByRole("dialog", { name: "Design review" });
  const notes = detail.getByLabel("Your notes");
  await expect(detail).toContainText("It comes back every time this meeting does");
  await notes.fill("Ask about the budget");
  await page.clock.runFor(1000);
  await expect(detail.getByRole("status").filter({ hasText: /^Saved / })).toBeVisible();
  const r = await new AxeBuilder({ page }).include('[role="dialog"]').analyze();
  expect(r.violations).toEqual([]);

  // Closing straight after typing still keeps it.
  await notes.fill("Ask about the budget. Sam owes the slides.");
  await page.keyboard.press("Escape");
  await expect(detail).toBeHidden();
  await page.getByRole("button", { name: "Design review", exact: true }).first().click();
  await expect(page.getByRole("dialog", { name: "Design review" }).getByLabel("Your notes")).toHaveValue("Ask about the budget. Sam owes the slides.");

  // An empty note is removed.
  await page.getByRole("dialog", { name: "Design review" }).getByLabel("Your notes").fill("");
  await page.clock.runFor(1000);
  await expect(page.getByRole("dialog", { name: "Design review" })).toContainText("It comes back every time this meeting does");
});

test("focus time: blocked from Find a time, and offered in the form for timed events only", async ({ page }) => {
  await page.goto("/");
  await page.getByRole("button", { name: "Find a time" }).click();
  const dialog = page.getByRole("dialog", { name: "Find a time" });
  await dialog.getByRole("radio", { name: "1 hour" }).click();
  await dialog.getByRole("button", { name: /^Block 1 hour for focus from / }).first().click();
  await expect(page.getByText(/^Focus time added: /).first()).toBeVisible();
  await dialog.getByRole("button", { name: "Close" }).click();
  await expect(dialog).toBeHidden();

  await page.getByRole("button", { name: "New event" }).click();
  const sheet = page.getByRole("dialog", { name: "New event" });
  const focus = sheet.getByRole("switch", { name: "Focus time" });
  await expect(focus).toBeVisible();
  await expect(sheet).toContainText("Meeting heads-ups and reminders still come");
  await sheet.getByRole("switch", { name: "All day" }).click();
  await expect(focus).toHaveCount(0);
});
