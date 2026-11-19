import { expect, test } from "@playwright/test";

test.use({ reducedMotion: "reduce" });

const on = (h: number, m: number) => new Date(2026, 10, 18, h, m);

test("after a meeting with notes, the briefing offers tasks and a recap, and can be dismissed", async ({ page }) => {
  const { default: AxeBuilder } = await import("@axe-core/playwright");
  await page.clock.install({ time: on(9, 30) });
  await page.goto("/?followup");
  const card = page.getByRole("region", { name: "Follow up" });
  await expect(card).toContainText("Follow up on Roadmap planning");
  expect((await new AxeBuilder({ page }).analyze()).violations).toEqual([]);
  await card.getByRole("button", { name: "Email a recap" }).click();
  const recap = page.getByRole("dialog", { name: "Email a recap" });
  await expect(recap).toContainText("No guest list is known");
  await recap.getByRole("button", { name: "Open in mail app" }).click();
  await expect(recap).toBeHidden();
  const drafts = await page.evaluate(() => (window as unknown as { __drafts?: Array<{ to: string | null; subject: string; body: string }> }).__drafts ?? []);
  expect(drafts[0]?.subject).toBe("Notes from Roadmap planning");
  expect(drafts[0]?.body).toContain("Jordan shares the Q1 list");

  await card.getByRole("button", { name: "Make tasks" }).click();
  await page.getByRole("dialog", { name: "Make tasks" }).getByRole("button", { name: "Add 2 tasks" }).click();
  await expect(page.getByText("2 tasks added").first()).toBeVisible();

  await card.getByRole("button", { name: "Done with Roadmap planning" }).click();
  await expect(card).toHaveCount(0);
  await page.reload();
  await expect(page.getByRole("region", { name: "Follow up" })).toHaveCount(0);
});

test("running late opens a note to the organiser in your mail app", async ({ page }) => {
  await page.clock.install({ time: on(9, 50) });
  await page.goto("/");
  await page.getByRole("button", { name: "Design review", exact: true }).first().click();
  const detail = page.getByRole("dialog", { name: "Design review" });
  await detail.getByRole("button", { name: "Running late" }).click();
  const late = page.getByRole("dialog", { name: "Running late" });
  await late.getByRole("radio", { name: "15 min" }).click();
  await expect(late.getByLabel("Message")).toContainText("running about 15 minutes late for Design review");
  await expect(late).toContainText("To: priya@example.com");
  await late.getByRole("checkbox", { name: /Tell everyone invited/ }).check();
  await expect(late).toContainText("lee@example.com");
  await late.getByRole("button", { name: "Open in mail app" }).click();
  await expect(late).toBeHidden();
  const drafts = await page.evaluate(() => (window as unknown as { __drafts?: Array<{ to: string | null; subject: string }> }).__drafts ?? []);
  expect(drafts[0]?.subject).toBe("Running late: Design review");
  expect(drafts[0]?.to).toContain("priya@example.com");
});

test("a template sets the length and starts the meeting's notes; quick add picks it up from the title", async ({ page }) => {
  await page.clock.install({ time: on(9, 30) });
  await page.goto("/");
  await page.getByRole("button", { name: "New event" }).click();
  const sheet = page.getByRole("dialog", { name: "New event" });
  await sheet.getByLabel("Quick add").fill("1:1 with Kim today 3pm");
  await expect(sheet.getByRole("combobox", { name: "Template" }).or(sheet.getByLabel("Template")).first()).toContainText("1:1");
  await expect(sheet.getByLabel("End time")).toHaveValue("15:30");
  await sheet.getByRole("button", { name: "Add event" }).click();
  await page.getByRole("button", { name: "1:1 with Kim", exact: true }).click();
  await expect(page.getByRole("dialog", { name: "1:1 with Kim" }).getByLabel("Your notes")).toHaveValue(/Blockers/);
});

test("your own template is saved from the form and listed in Settings", async ({ page }) => {
  await page.goto("/");
  await page.getByRole("button", { name: "New event" }).click();
  const sheet = page.getByRole("dialog", { name: "New event" });
  await sheet.getByLabel("Title").fill("Design crit");
  await sheet.getByLabel("Notes").fill("- Goals\n- Feedback");
  await sheet.getByRole("button", { name: "Keep as a template" }).click();
  await expect(page.getByText("Kept “Design crit” as a template").first()).toBeVisible();
  await sheet.getByRole("button", { name: "Cancel" }).click();
  await page.getByRole("button", { name: "Settings" }).click();
  await page.getByRole("main").getByRole("button", { name: /^General/ }).click();
  await expect(page.getByText("Design crit")).toBeVisible();
  await page.getByRole("button", { name: "Delete template Design crit" }).click();
  await expect(page.getByText("None yet.")).toBeVisible();
});

test("with sending on, a running-late note is sent with one press after it is shown", async ({ page }) => {
  await page.clock.install({ time: on(9, 50) });
  await page.goto("/?sendnotes");
  await page.getByRole("button", { name: "Design review", exact: true }).first().click();
  await page.getByRole("dialog", { name: "Design review" }).getByRole("button", { name: "Running late" }).click();
  const late = page.getByRole("dialog", { name: "Running late" });
  await expect(late).toContainText("From me@yahoo.com");
  await late.getByRole("button", { name: "Send", exact: true }).click();
  await expect(page.getByText("Sent to priya@example.com").first()).toBeVisible();
  const sent = await page.evaluate(() => (window as unknown as { __messages?: Array<{ senderId: string; to: string; subject: string }> }).__messages ?? []);
  expect(sent).toEqual([{ senderId: "imap:imap1", to: "priya@example.com", subject: "Running late: Design review", body: expect.stringContaining("10 minutes late") }]);
});

test("a failed send says so and keeps the note open", async ({ page }) => {
  await page.clock.install({ time: on(9, 50) });
  await page.goto("/?sendnotes&sendfail");
  await page.getByRole("button", { name: "Design review", exact: true }).first().click();
  await page.getByRole("dialog", { name: "Design review" }).getByRole("button", { name: "Running late" }).click();
  const dialog = page.getByRole("dialog", { name: "Running late" });
  await dialog.getByRole("button", { name: "Send", exact: true }).click();
  await expect(dialog.getByRole("alert")).toContainText("Nothing was sent");
  await expect(dialog).toBeVisible();
});

test("without sending on, notes only open as drafts", async ({ page }) => {
  await page.clock.install({ time: on(9, 50) });
  await page.goto("/");
  await page.getByRole("button", { name: "Design review", exact: true }).first().click();
  await page.getByRole("dialog", { name: "Design review" }).getByRole("button", { name: "Running late" }).click();
  const late = page.getByRole("dialog", { name: "Running late" });
  await expect(late.getByRole("button", { name: "Send", exact: true })).toHaveCount(0);
  await expect(late.getByRole("button", { name: "Open in mail app" })).toBeVisible();
});

test("sending is off by default and a Google account needs its own permission", async ({ page }) => {
  await page.goto("/?withgoogle");
  await page.getByRole("button", { name: "Settings" }).click();
  await page.getByRole("main").getByRole("button", { name: /^Email/ }).click();
  const sw = page.getByRole("switch", { name: "Send notes from Firstlight" });
  await expect(sw).not.toBeChecked();
  await sw.click();
  await expect(page.getByText(/Nothing to send from yet|Allow sending/).first()).toBeVisible();
});
