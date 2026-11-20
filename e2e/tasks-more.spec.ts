import AxeBuilder from "@axe-core/playwright";
import { expect, test, type Page } from "@playwright/test";

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
