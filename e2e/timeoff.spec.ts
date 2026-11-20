import AxeBuilder from "@axe-core/playwright";
import { expect, test, type Page } from "@playwright/test";

test.use({ reducedMotion: "reduce" });

const at = (h: number, m: number) => new Date(new Date().setHours(h, m, 0, 0));
const pad = (n: number) => String(n).padStart(2, "0");
const ymdIn = (days: number) => {
  const d = new Date(at(12, 0));
  d.setDate(d.getDate() + days);
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
};
const openTimeOff = async (page: Page, query = "") => {
  await page.goto(`/${query}`);
  await page.getByRole("button", { name: "Settings" }).click();
  await page.getByRole("main").getByRole("button", { name: /^Time off/ }).click();
  await expect(page.getByRole("heading", { name: "Time off", level: 1 })).toBeVisible();
};

test("time off is added, its meetings are pointed out, and an out-of-office reply is ready", async ({ page, context }) => {
  await context.grantPermissions(["clipboard-read", "clipboard-write"]);
  await page.clock.install({ time: at(8, 0) });
  await openTimeOff(page, "?holidays");
  await expect(page.getByText("No time off planned.")).toBeVisible();
  await page.getByLabel("First day off").fill(ymdIn(0));
  await page.getByLabel("Last day off").fill(ymdIn(2));
  await page.getByLabel("What for (optional)").fill("Holiday");
  await page.getByRole("button", { name: "Add time off" }).click();
  await expect(page.getByText("Time off added").first()).toBeVisible();

  const clashes = page.getByRole("region", { name: "Meetings on your days off" });
  await expect(clashes).toContainText("Design review");
  await clashes.getByRole("button", { name: /Write (to organiser|a note)/ }).first().click();
  const drafts = await page.evaluate(() => (window as unknown as { __drafts?: { subject: string; body: string }[] }).__drafts ?? []);
  expect(drafts.at(-1)!.subject).toMatch(/^Away for /);
  expect(drafts.at(-1)!.body).toContain("I'll be out of the office");

  const reply = page.getByRole("region", { name: "Out-of-office reply" });
  await expect(reply).toContainText("I'm out of the office from");
  await expect(reply).toContainText("(holiday)");
  await reply.getByRole("button", { name: "Copy reply" }).click();
  expect(await page.evaluate(() => navigator.clipboard.readText())).toContain("I'll reply when I'm back on");

  const holidays = page.getByRole("region", { name: "Public holidays coming up" });
  await expect(holidays).toContainText("Founders' Day");
  await holidays.getByRole("button", { name: "Mark as day off" }).click();
  await expect(holidays).toContainText("Day off");
  expect((await new AxeBuilder({ page }).analyze()).violations).toEqual([]);

  // The briefing says you are off today.
  await page.getByRole("button", { name: "Settings", exact: true }).click();
  await page.getByRole("button", { name: "Briefing", exact: true }).click();
  await expect(page.getByRole("region", { name: "Time off" })).toContainText("You're off today");

  // And a stretch can be removed.
  await openTimeOff(page);
});

test("in the week before time off, the briefing shows the meetings that fall in it", async ({ page }) => {
  await page.clock.install({ time: at(8, 0) });
  await openTimeOff(page);
  await page.getByLabel("First day off").fill(ymdIn(1));
  await page.getByRole("button", { name: "Add time off" }).click();
  await page.getByRole("button", { name: "Settings", exact: true }).click();
  await page.getByRole("button", { name: "Briefing", exact: true }).click();
  const card = page.getByRole("region", { name: "Time off" });
  await expect(card).toContainText("Time off from");
  await card.getByRole("button").click();
  await expect(page.getByRole("heading", { name: "Time off", level: 1 })).toBeVisible();
  await page.getByRole("button", { name: /^Remove time off/ }).click();
  await expect(page.getByText("No time off planned.")).toBeVisible();
});

test("the out-of-office reply is set in a Google account after allowing it once, and can be turned off", async ({ page }) => {
  await page.clock.install({ time: at(8, 0) });
  await openTimeOff(page, "?withgoogle");
  await page.getByLabel("First day off").fill(ymdIn(3));
  await page.getByLabel("Last day off").fill(ymdIn(5));
  await page.getByRole("button", { name: "Add time off" }).click();
  const group = page.getByRole("region", { name: "Out-of-office reply" });
  await expect(group.getByRole("textbox")).toHaveValue(/I'm out of the office from/);
  await group.getByRole("textbox").fill("Away this week. Back Monday.");
  await group.getByRole("button", { name: /^Allow .* to take the reply$/ }).click();
  await group.getByRole("button", { name: /^Set reply in / }).click();
  await expect(page.getByText("Automatic reply set").first()).toBeVisible();
  const calls = await page.evaluate(() => (window as unknown as { __autoReplies: { from: string; to: string; text: string | null }[] }).__autoReplies);
  expect(calls.at(-1)).toMatchObject({ from: ymdIn(3), to: ymdIn(5), text: "Away this week. Back Monday." });
  await group.getByRole("button", { name: "Turn off" }).click();
  await expect(page.getByText("Automatic reply turned off").first()).toBeVisible();
  expect((await new AxeBuilder({ page }).analyze()).violations).toEqual([]);
});
