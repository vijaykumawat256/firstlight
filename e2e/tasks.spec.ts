import { expect, test } from "@playwright/test";
import { settle } from "./settle";

test.use({ reducedMotion: "reduce" });

const at = (h: number, m: number) => new Date(new Date().setHours(h, m, 0, 0));
/** Today at that time if it is a weekday, else the coming Monday: planning only uses working days. */
const weekdayAt = (h: number, m: number) => {
  const d = at(h, m);
  while (d.getDay() === 0 || d.getDay() === 6) d.setDate(d.getDate() + 1);
  return d;
};

test("the briefing lists today's tasks with ones carried over, and a task can be added and ticked off", async ({ page }) => {
  await page.clock.install({ time: at(9, 30) });
  await page.goto("/?tasks");
  const tasks = page.getByRole("region", { name: "Tasks" });
  await expect(tasks.getByText("Send the budget to Priya")).toBeVisible();
  await expect(tasks.getByText("Waiting 2 days")).toBeVisible();
  await expect(tasks.getByText("Review Sam's slides")).toBeVisible();
  // Upcoming and someday tasks are not in the briefing.
  await expect(tasks.getByText("Book flights")).toHaveCount(0);

  const add = tasks.getByLabel("Add a task");
  await add.fill("Call the bank today 4pm");
  await expect(tasks).toContainText("Adds “Call the bank”");
  await add.press("Enter");
  await expect(tasks.getByText("Call the bank")).toBeVisible();
  await expect(add).toHaveValue("");

  await tasks.getByRole("checkbox", { name: "Done: Send the budget to Priya" }).click();
  await expect(page.getByText("Done: Send the budget to Priya").first()).toBeVisible();
  await expect(tasks.getByText("Send the budget to Priya")).toHaveCount(0);
  await expect(tasks).toContainText("1 done today");
});

test("the Tasks page shows each list, and a task can be changed and deleted", async ({ page }) => {
  const { default: AxeBuilder } = await import("@axe-core/playwright");
  await page.goto("/?tasks");
  await page.getByRole("button", { name: "Tasks", exact: true }).click();
  await expect(page.getByRole("heading", { name: "Tasks", level: 1 })).toBeVisible();
  await page.getByRole("radio", { name: "Upcoming" }).click();
  await expect(page.getByText("Book flights")).toBeVisible();
  await page.getByRole("radio", { name: "Someday" }).click();
  await expect(page.getByText("Read the security report")).toBeVisible();
  await settle(page);
  expect((await new AxeBuilder({ page }).analyze()).violations).toEqual([]);

  await page.getByRole("button", { name: "Change Read the security report" }).click();
  const form = page.getByRole("dialog", { name: "Change task" });
  await form.getByLabel("Task").fill("Read the security report, part 1");
  await form.getByRole("radio", { name: "1 hour" }).click();
  await form.getByRole("button", { name: "Save" }).click();
  await expect(page.getByText("Read the security report, part 1")).toBeVisible();
  await expect(page.getByText("1 h", { exact: true })).toBeVisible();

  await page.getByRole("button", { name: "Change Read the security report, part 1" }).click();
  await page.getByRole("dialog", { name: "Change task" }).getByRole("button", { name: "Delete" }).click();
  await expect(page.getByText("No tasks without a day.")).toBeVisible();
});

test("a time can be set aside for a task, and it appears in the calendar", async ({ page }) => {
  await page.goto("/?tasks");
  await page.getByRole("button", { name: "Tasks", exact: true }).click();
  await page.getByRole("button", { name: "Set time aside for Review Sam's slides" }).click();
  const dialog = page.getByRole("dialog", { name: "Set time aside" });
  await dialog.getByRole("button").filter({ hasText: /\d/ }).first().click();
  await expect(page.getByText("Time set aside in your calendar").first()).toBeVisible();
  await expect(page.getByText(/Time set aside$/).first()).toBeVisible();
});

test("lines of a meeting note become tasks marked with the meeting", async ({ page }) => {
  await page.clock.install({ time: at(8, 0) });
  await page.goto("/");
  await page.getByRole("button", { name: "Design review", exact: true }).first().click();
  const detail = page.getByRole("dialog", { name: "Design review" });
  await detail.getByLabel("Your notes").fill("- Send slides to Sam\n- Book the big room\n[x] Already done");
  await detail.getByRole("button", { name: "Make tasks" }).click();
  const pick = page.getByRole("dialog", { name: "Make tasks" });
  await expect(pick.getByRole("checkbox")).toHaveCount(2);
  await pick.getByRole("checkbox", { name: "Book the big room" }).uncheck();
  await pick.getByRole("button", { name: "Add 1 task" }).click();
  await expect(page.getByText("1 task added").first()).toBeVisible();
  await detail.getByRole("button", { name: "Close" }).click();
  await page.getByRole("button", { name: "Tasks", exact: true }).click();
  await page.getByRole("radio", { name: "Someday" }).click();
  await expect(page.getByText("Send slides to Sam")).toBeVisible();
  await expect(page.getByText("From Design review")).toBeVisible();
});

test("typing 'task …' in search adds a task", async ({ page }) => {
  await page.goto("/");
  await page.getByRole("button", { name: "Search" }).click();
  const search = page.getByRole("dialog", { name: "Search" });
  await search.getByRole("combobox", { name: "Search events" }).fill("task renew passport tomorrow");
  await expect(search.getByRole("option").first()).toHaveText(/^Add task: Renew passport · /);
  await search.getByRole("combobox", { name: "Search events" }).press("Enter");
  await expect(page.getByText(/^Added to Upcoming, /).first()).toBeVisible();
});

test("task reminders can be turned off in Settings", async ({ page }) => {
  await page.goto("/");
  await page.getByRole("button", { name: "Settings" }).click();
  await page.getByRole("main").getByRole("button", { name: /^Notifications/ }).click();
  const sw = page.getByRole("switch", { name: "Task reminders" });
  await expect(sw).toBeChecked();
  await sw.click();
  await expect(sw).not.toBeChecked();
});

test("plan my day suggests times for today's tasks, and sets time aside only for the ones kept", async ({ page }) => {
  await page.clock.install({ time: weekdayAt(9, 0) });
  await page.goto("/?tasks");
  await page.getByRole("region", { name: "Tasks" }).getByRole("button", { name: "Plan my day" }).click();
  const dialog = page.getByRole("dialog", { name: "Plan my day" });
  const list = dialog.getByRole("list", { name: "Suggestions" });
  await expect(list.getByRole("listitem").first()).toBeVisible();
  await expect(dialog).toContainText("Worked out on this computer");
  // AI is off, so it is not offered.
  await expect(dialog.getByRole("button", { name: "Ask AI instead…" })).toHaveCount(0);
  const count = await list.getByRole("checkbox").count();
  expect(count).toBeGreaterThan(1);
  await list.getByRole("checkbox").first().uncheck();
  await dialog.getByRole("button", { name: "Set time aside" }).click();
  await expect(page.getByText(count - 1 === 1 ? "Time set aside for 1 task" : `Time set aside for ${count - 1} tasks`).first()).toBeVisible();
});

test("planning with AI needs its own agreement, shows what is sent, and falls back when AI fails", async ({ page }) => {
  const { default: AxeBuilder } = await import("@axe-core/playwright");
  await page.clock.install({ time: weekdayAt(9, 0) });
  await page.goto("/?tasks&aion&planfail");
  await page.getByRole("region", { name: "Tasks" }).getByRole("button", { name: "Plan my day" }).click();
  await page.getByRole("dialog", { name: "Plan my day" }).getByRole("button", { name: "Ask AI instead…" }).click();
  const consent = page.getByRole("dialog", { name: "Plan with AI?" });
  await expect(consent.getByLabel("What would be sent")).toContainText("Review Sam's slides");
  expect(await page.evaluate(() => (window as unknown as { __planAsked?: number }).__planAsked ?? 0)).toBe(0);
  // Let the dialog finish fading in, or axe measures half-faded colours.
  await settle(page);
  expect((await new AxeBuilder({ page }).include('[role="dialog"]').analyze()).violations).toEqual([]);
  await consent.getByRole("button", { name: "Agree and ask AI" }).click();
  const dialog = page.getByRole("dialog", { name: "Plan my day" });
  await expect(dialog.getByRole("radio", { name: "AI" })).toBeChecked();
  await expect(dialog.getByRole("status")).toContainText("AI couldn't help this time");
  expect(await page.evaluate(() => (window as unknown as { __planAsked?: number }).__planAsked ?? 0)).toBe(1);
  await dialog.getByRole("button", { name: "Close" }).first().click();

  // The agreement can be withdrawn in Settings.
  await page.getByRole("button", { name: "Settings" }).click();
  await page.getByRole("main").getByRole("button", { name: /^AI/ }).click();
  await page.getByRole("button", { name: "Withdraw" }).click();
  await expect(page.getByText("Plan my day with AI")).toHaveCount(0);
});

test("task sync is turned on per account, brings in the phone's tasks, and can be stopped", async ({ page }) => {
  await page.goto("/?withgoogle&tasks");
  await page.getByRole("button", { name: "Settings" }).click();
  await page.getByRole("main").getByRole("button", { name: /^General/ }).click();
  const allow = page.getByRole("button", { name: /^Sync tasks with (?!Reminders)/ }).first();
  await allow.click();
  await expect(page.getByText(/Kept in step with Google Tasks; new tasks go here\./)).toBeVisible();
  await page.getByRole("button", { name: "Settings", exact: true }).click();
  await page.getByRole("button", { name: "Briefing", exact: true }).click();
  await page.getByRole("button", { name: "Tasks", exact: true }).click();
  await expect(page.getByText("Kept in step with your task list online")).toBeVisible();
  await page.getByRole("radio", { name: "Someday" }).click();
  await expect(page.getByText("Pick up dry cleaning")).toBeVisible();
  await page.getByRole("button", { name: "Sync now" }).click();
  await expect(page.getByText("Tasks are in step").first()).toBeVisible();
});

test("a failed task sync names the account and why", async ({ page }) => {
  await page.goto("/?withgoogle&tasks&taskfail");
  await page.getByRole("button", { name: "Settings" }).click();
  await page.getByRole("main").getByRole("button", { name: /^General/ }).click();
  await page.getByRole("button", { name: /^Sync tasks with / }).first().click();
  await page.getByRole("button", { name: "Settings", exact: true }).click();
  await page.getByRole("button", { name: "Briefing", exact: true }).click();
  await page.getByRole("button", { name: "Tasks", exact: true }).click();
  await page.getByRole("button", { name: "Sync now" }).click();
  await expect(page.getByText("Couldn't sync with Work Google (offline)").first()).toBeVisible();
});
