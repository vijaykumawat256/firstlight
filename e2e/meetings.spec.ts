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
