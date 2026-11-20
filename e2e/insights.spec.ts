import AxeBuilder from "@axe-core/playwright";
import { expect, test, type Page } from "@playwright/test";
import { settle } from "./settle";

test.use({ reducedMotion: "reduce" });

const at = (h: number, m: number) => new Date(new Date().setHours(h, m, 0, 0));
const openTasks = async (page: Page, query = "?tasks&waiting") => {
  await page.clock.install({ time: at(15, 0) });
  await page.goto(`/${query}`);
  await page.getByRole("button", { name: "Tasks", exact: true }).click();
  await expect(page.getByRole("heading", { name: "Tasks", level: 1 })).toBeVisible();
};

test("a standup update is written from tasks and can be changed, copied or emailed", async ({ page, context }) => {
  await context.grantPermissions(["clipboard-read", "clipboard-write"]);
  await openTasks(page);
  await page.getByRole("button", { name: "Write update" }).click();
  const dialog = page.getByRole("dialog", { name: "Write an update" });
  const text = dialog.getByRole("textbox", { name: /Update for/ });
  await expect(text).toHaveValue(/Today\n- Send the budget to Priya/);
  await expect(text).toHaveValue(/Blockers\n- Budget numbers: waiting on priya@example.com/);
  await settle(page);
  expect((await new AxeBuilder({ page }).include('[role="dialog"]').analyze()).violations).toEqual([]);

  await text.fill("Yesterday\n- Shipped it");
  await dialog.getByRole("button", { name: "Copy" }).click();
  await expect(page.getByText("Update copied").first()).toBeVisible();
  expect(await page.evaluate(() => navigator.clipboard.readText())).toBe("Yesterday\n- Shipped it");

  // Weekly keeps its own text; switching back keeps the change.
  await dialog.getByRole("radio", { name: "Weekly" }).click();
  await expect(dialog.getByRole("textbox", { name: "Weekly update" })).toHaveValue(/Done this week/);
  await dialog.getByRole("radio", { name: "Standup" }).click();
  await expect(text).toHaveValue("Yesterday\n- Shipped it");

  // With an address, it goes through the usual send bar (here: open in the mail app).
  await dialog.getByLabel("Send to (optional)").fill("lead@example.com");
  await dialog.getByRole("button", { name: "Open in mail app" }).click();
  const drafts = await page.evaluate(() => (window as unknown as { __drafts?: { to: string; subject: string; body: string }[] }).__drafts ?? []);
  expect(drafts.at(-1)).toMatchObject({ to: "lead@example.com", body: "Yesterday\n- Shipped it" });
});

test("where my time went shows totals by calendar and saves a spreadsheet", async ({ page }) => {
  await openTasks(page, "?tasks");
  await page.getByRole("button", { name: "My time" }).click();
  const sheet = page.getByRole("dialog", { name: "Where my time went" });
  await expect(sheet.getByRole("region", { name: "By calendar" })).toBeVisible();
  await expect(sheet.getByText("Meetings", { exact: true })).toBeVisible();
  await expect(sheet.getByRole("img", { name: /Meeting time per day/ })).toBeVisible();
  await settle(page);
  expect((await new AxeBuilder({ page }).include('[role="dialog"]').analyze()).violations).toEqual([]);
  await sheet.getByRole("radio", { name: "Last month" }).click();
  await expect.poll(() => page.evaluate(() => (window as unknown as { __timeReports?: [string, string][] }).__timeReports?.length ?? 0)).toBeGreaterThan(1);
  const [from, to] = await page.evaluate(() => (window as unknown as { __timeReports: [string, string][] }).__timeReports.at(-1)!);
  expect(from.endsWith("-01")).toBe(true);
  expect(new Date(`${to}T12:00:00`).getDate()).toBeGreaterThanOrEqual(28);
  await sheet.getByRole("radio", { name: "This week" }).click();
  await sheet.getByRole("button", { name: "Save as spreadsheet" }).click();
  await expect(page.getByText("Saved to Downloads").first()).toBeVisible();
});
