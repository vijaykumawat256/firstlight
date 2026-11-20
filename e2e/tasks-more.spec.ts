import AxeBuilder from "@axe-core/playwright";
import { expect, test, type Page } from "@playwright/test";
import { settle } from "./settle";

test.use({ reducedMotion: "reduce" });

const at = (h: number, m: number) => new Date(new Date().setHours(h, m, 0, 0));
const openTasks = async (page: Page, query = "?tasks") => {
  await page.goto(`/${query}`);
  await page.getByRole("button", { name: "Tasks", exact: true }).click();
  await expect(page.getByRole("heading", { name: "Tasks", level: 1 })).toBeVisible();
};

test("a repeating task comes back on its next day once ticked, and only once", async ({ page }) => {
  await page.clock.install({ time: at(9, 30) });
  await openTasks(page);
  await page.getByRole("button", { name: "Change Review Sam's slides" }).click();
  const form = page.getByRole("dialog", { name: "Change task" });
  await form.getByRole("combobox", { name: "Repeats" }).click();
  await page.getByRole("option", { name: "Every week", exact: true }).click();
  await form.getByRole("button", { name: "Save" }).click();
  await expect(page.getByText("Every week", { exact: false }).first()).toBeVisible();

  await page.getByRole("checkbox", { name: "Done: Review Sam's slides" }).click();
  await page.getByRole("radio", { name: "Upcoming" }).click();
  const upcoming = page.getByRole("list", { name: "upcoming tasks" });
  await expect(upcoming.getByText("Review Sam's slides")).toBeVisible();
  const inAWeek = new Date(at(12, 0).getTime() + 7 * 86_400_000).toLocaleDateString("en-US", { month: "short", day: "numeric" });
  await expect(upcoming).toContainText(inAWeek.split(" ")[1]!);

  // Unticking and ticking again does not make a second copy.
  await page.getByRole("radio", { name: "Done" }).click();
  await page.getByRole("checkbox", { name: "Not done: Review Sam's slides" }).click();
  await page.getByRole("radio", { name: /^Today/ }).click();
  await page.getByRole("checkbox", { name: "Done: Review Sam's slides" }).click();
  await page.getByRole("radio", { name: "Upcoming" }).click();
  await expect(upcoming.getByText("Review Sam's slides")).toHaveCount(1);
});

test("a task waiting on someone moves to Waiting, shows how long, and the nudge is shown in full before it goes", async ({ page }) => {
  await openTasks(page, "?tasks&waiting");
  await page.getByRole("radio", { name: /^Waiting/ }).click();
  const waiting = page.getByRole("list", { name: "waiting tasks" });
  await expect(waiting).toContainText("Waiting on priya@example.com · 4 days");
  await settle(page);
  expect((await new AxeBuilder({ page }).analyze()).violations).toEqual([]);
  await waiting.getByRole("button", { name: "Nudge priya@example.com about Budget numbers" }).click();
  const nudge = page.getByRole("dialog", { name: "Nudge" });
  await expect(nudge).toContainText("Just checking in on “Budget numbers” from 4 days ago");
  await nudge.getByRole("button", { name: "Open in mail app" }).click();
  const drafts = await page.evaluate(() => (window as unknown as { __drafts?: { to: string; subject: string }[] }).__drafts ?? []);
  expect(drafts.at(-1)).toMatchObject({ to: "priya@example.com", subject: "Checking in: Budget numbers" });

  // Setting someone in the form moves a task there; clearing it brings it back.
  await page.getByRole("radio", { name: "Upcoming" }).click();
  await page.getByRole("button", { name: "Change Book flights" }).click();
  const form = page.getByRole("dialog", { name: "Change task" });
  await form.getByLabel("Waiting on").fill("Sam");
  await form.getByRole("button", { name: "Save" }).click();
  await expect(page.getByText("Book flights")).toHaveCount(0);
  await page.getByRole("radio", { name: /^Waiting/ }).click();
  await expect(waiting.getByText("Book flights")).toBeVisible();
  await waiting.getByRole("button", { name: "Nudge Sam about Book flights" }).click();
  await expect(page.getByRole("dialog", { name: "Nudge" })).toContainText("Hi Sam,");
  await expect(page.getByRole("button", { name: "Copy message" })).toBeVisible();
});

test("month-end reminders add tasks that repeat on the last working day", async ({ page }) => {
  await openTasks(page);
  await page.getByRole("button", { name: "Month-end" }).click();
  const dialog = page.getByRole("dialog", { name: "Month-end reminders" });
  await expect(dialog.getByRole("checkbox", { name: "Submit your timesheet" })).toBeChecked();
  await expect(dialog.getByRole("checkbox", { name: "File your expenses" })).toBeChecked();
  await dialog.getByLabel("Another one").fill("Pay the software bills");
  await settle(page);
  expect((await new AxeBuilder({ page }).include('[role="dialog"]').analyze()).violations).toEqual([]);
  await dialog.getByRole("button", { name: "Add 3 reminders" }).click();
  await expect(page.getByText("3 month-end reminders added").first()).toBeVisible();
  const todayOrLater = page.getByText("Submit your timesheet");
  await page.getByRole("radio", { name: "Upcoming" }).click();
  if ((await todayOrLater.count()) === 0) await page.getByRole("radio", { name: /^Today/ }).click();
  await expect(todayOrLater).toBeVisible();
  await expect(page.getByText("Last working day of the month").first()).toBeVisible();
  // Offered again, the ones already there are not.
  await page.getByRole("button", { name: "Month-end" }).click();
  await expect(page.getByRole("dialog", { name: "Month-end reminders" }).getByRole("checkbox", { name: "Submit your timesheet" })).toHaveCount(0);
});

test("an email becomes a task that opens the email again", async ({ page }) => {
  await page.clock.install({ time: at(9, 30) });
  await page.goto("/?mailon&tasks");
  const inbox = page.getByRole("region", { name: "Inbox" });
  await inbox.getByRole("button", { name: "Make a task from Agenda for Tuesday" }).click();
  await expect(page.getByText("Added to your tasks").first()).toBeVisible();
  await page.getByRole("button", { name: "Tasks", exact: true }).click();
  await page.getByRole("radio", { name: "Someday" }).click();
  const row = page.getByRole("list", { name: "someday tasks" });
  await expect(row).toContainText("Agenda for Tuesday");
  await expect(row).toContainText("Email from Priya Shah");
  await row.getByRole("button", { name: "Open the email for Agenda for Tuesday" }).click();
  expect(await page.evaluate(() => (window as unknown as { __opened?: unknown[] }).__opened?.length ?? 0)).toBeGreaterThan(0);
});

test("a task typed without a day goes on today's list, and every add says where it went", async ({ page }) => {
  await page.clock.install({ time: at(9, 30) });
  await page.goto("/?tasks");
  // From the briefing: no day = today, shown straight away, lit up, with a confirmation.
  const tasks = page.getByRole("region", { name: "Tasks" });
  await expect(tasks).toContainText("Goes on today's list unless you add a day");
  await tasks.getByLabel("Add a task").fill("Water the plants");
  await expect(tasks).toContainText("Adds “Water the plants” to Today");
  await tasks.getByLabel("Add a task").press("Enter");
  await expect(page.getByText("Added to Today").first()).toBeVisible();
  await expect(tasks.locator("li[data-fresh]")).toContainText("Water the plants");

  // A later day goes to Upcoming, and the Tasks page opens that list and lights the row. ("tomorrow", not a weekday
  // name: on that weekday it would be today.)
  await page.getByRole("button", { name: "All tasks" }).click();
  await page.getByLabel("Add a task").fill("Renew passport tomorrow");
  await page.getByLabel("Add a task").press("Enter");
  await expect(page.getByText(/^Added to Upcoming, /).first()).toBeVisible();
  await expect(page.getByRole("radio", { name: "Upcoming" })).toHaveAttribute("aria-checked", "true");
  await expect(page.locator("li[data-fresh]")).toContainText("Renew passport");

  // On Someday, no day means no day.
  await page.getByRole("radio", { name: "Someday" }).click();
  await page.getByLabel("Add a task").fill("Learn Spanish");
  await page.getByLabel("Add a task").press("Enter");
  await expect(page.getByText("Added to Someday").first()).toBeVisible();
  await expect(page.getByRole("list", { name: "someday tasks" })).toContainText("Learn Spanish");

  // The briefing links straight to the tasks with no day.
  await page.getByRole("button", { name: "Briefing" }).click();
  await page.getByRole("button", { name: /^No day \(/ }).click();
  await expect(page.getByRole("radio", { name: "Someday" })).toHaveAttribute("aria-checked", "true");
  await expect(page.getByRole("list", { name: "someday tasks" }).getByText("Learn Spanish")).toBeVisible();
});

test("a focus timer runs on a task, counts its minutes when stopped or finished, and shows them", async ({ page }) => {
  await page.clock.install({ time: at(9, 30) });
  await openTasks(page);
  await page.getByRole("button", { name: "Start a focus timer on Review Sam's slides" }).click();
  const dialog = page.getByRole("dialog", { name: "Focus timer" });
  await expect(dialog.getByRole("radio", { name: "25 min" })).toHaveAttribute("aria-checked", "true");
  await dialog.getByRole("button", { name: "Start" }).click();
  const bar = page.getByRole("region", { name: "Focus timer" });
  await expect(bar).toContainText("Review Sam's slides");
  await expect(bar).toContainText(/2[45]:\d\d/);
  await settle(page);
  expect((await new AxeBuilder({ page }).analyze()).violations).toEqual([]);

  await page.clock.fastForward("10:00");
  await expect(bar).toContainText(/1[45]:\d\d/);
  await bar.getByRole("button", { name: "Stop" }).click();
  await expect(page.getByText("10 min counted on the task").first()).toBeVisible();
  await expect(bar).toHaveCount(0);
  await expect(page.getByText("10 min spent")).toBeVisible();

  // Left to run out, the full length is counted and the bar goes.
  await page.getByRole("button", { name: "Start a focus timer on Review Sam's slides" }).click();
  await page.getByRole("dialog", { name: "Focus timer" }).getByRole("radio", { name: "15 min" }).click();
  await page.getByRole("dialog", { name: "Focus timer" }).getByRole("button", { name: "Start" }).click();
  await expect(bar).toBeVisible();
  await page.clock.fastForward("16:00");
  await expect(bar).toHaveCount(0);
  await expect(page.getByText("25 min spent")).toBeVisible();
  // Once it has ended the app is asked once, not every second.
  const asks = await page.evaluate(() => (window as unknown as { __timerAsks: number }).__timerAsks);
  await page.clock.fastForward("00:10");
  expect(await page.evaluate(() => (window as unknown as { __timerAsks: number }).__timerAsks)).toBeLessThanOrEqual(asks + 1);
});

test("tasks can be kept in step with Apple Reminders, and stopped", async ({ page }) => {
  await page.goto("/?tasks");
  await page.getByRole("button", { name: "Settings" }).click();
  await page.getByRole("main").getByRole("button", { name: /^General/ }).click();
  const group = page.getByRole("region", { name: "Task sync" });
  await expect(group).toContainText("Your Mac asks for permission once");
  await group.getByRole("button", { name: "Sync tasks with Reminders" }).click();
  await expect(page.getByText("Tasks are kept in step with Reminders").first()).toBeVisible();
  await expect(group).toContainText("Kept in step with your default Reminders list");
  await settle(page);
  expect((await new AxeBuilder({ page }).analyze()).violations).toEqual([]);
  await page.getByRole("button", { name: "Settings", exact: true }).click();
  await page.getByRole("button", { name: "Briefing", exact: true }).click();
  await page.getByRole("button", { name: /^No day/ }).click();
  await expect(page.getByText("Buy stamps")).toBeVisible();

  await page.goto("/?tasks&remindersdenied");
  await page.getByRole("button", { name: "Settings" }).click();
  await page.getByRole("main").getByRole("button", { name: /^General/ }).click();
  await page.getByRole("button", { name: "Sync tasks with Reminders" }).click();
  await expect(page.getByText("Couldn't turn on Reminders").first()).toBeVisible();
});

test("Reminders is not offered on Windows", async ({ page }) => {
  await page.goto("/?windows");
  await page.getByRole("button", { name: "Settings" }).click();
  await page.getByRole("main").getByRole("button", { name: /^General/ }).click();
  await expect(page.getByRole("region", { name: "Task sync" })).toBeVisible();
  await expect(page.getByRole("button", { name: "Sync tasks with Reminders" })).toHaveCount(0);
});
