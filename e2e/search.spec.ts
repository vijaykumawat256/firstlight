import AxeBuilder from "@axe-core/playwright";
import { expect, test } from "@playwright/test";
import { settle } from "./settle";

test.use({ reducedMotion: "reduce" });

const at = (h: number, m: number) => new Date(new Date().setHours(h, m, 0, 0));

async function start(page: import("@playwright/test").Page) {
  await page.clock.install({ time: at(9, 30) });
  await page.goto("/");
  await expect(page.getByRole("heading", { name: /Good (morning|afternoon|evening)/ })).toBeVisible();
}

test("Cmd+K opens search, finds an event, and Enter opens it", async ({ page }) => {
  await start(page);
  await page.keyboard.press("Meta+k");
  const dialog = page.getByRole("dialog", { name: "Search" });
  await expect(dialog).toBeVisible();
  const box = dialog.getByRole("combobox", { name: "Search events" });
  await expect(box).toBeFocused();
  // Nothing typed: what is coming up.
  await expect(dialog.getByRole("option").first()).toBeVisible();

  await box.fill("standup");
  await expect(dialog.getByRole("option", { name: /Team standup/ })).toBeVisible();
  await expect(dialog.getByRole("option", { name: /Design review/ })).toHaveCount(0);
  await box.press("Enter");
  await expect(page.getByRole("dialog", { name: "Team standup" })).toBeVisible();
});

test("the same key closes it, and the toolbar button opens it", async ({ page }) => {
  await start(page);
  await page.getByRole("button", { name: "Search" }).click();
  await expect(page.getByRole("dialog", { name: "Search" })).toBeVisible();
  await page.keyboard.press("Control+k");
  await expect(page.getByRole("dialog", { name: "Search" })).toHaveCount(0);
  await page.keyboard.press("Meta+k");
  await page.keyboard.press("Escape");
  await expect(page.getByRole("dialog", { name: "Search" })).toHaveCount(0);
});

test("arrow keys move through the results; a place is searched too; no match says so", async ({ page }) => {
  await start(page);
  await page.keyboard.press("Meta+k");
  const dialog = page.getByRole("dialog", { name: "Search" });
  const box = dialog.getByRole("combobox", { name: "Search events" });
  await box.fill("review lunch cafe design");
  await expect(dialog.getByText("Nothing matches.")).toBeVisible();

  await box.fill("cafe"); // Lunch with Alex is at Café Nero; accents are ignored
  await expect(dialog.getByRole("option", { name: /Lunch with Alex/ })).toBeVisible();

  await box.fill("design");
  const options = dialog.getByRole("option");
  await expect(options.first()).toHaveAttribute("aria-selected", "true");
  const count = await options.count();
  if (count > 1) {
    await box.press("ArrowDown");
    await expect(options.nth(1)).toHaveAttribute("aria-selected", "true");
    await box.press("ArrowUp");
    await expect(options.first()).toHaveAttribute("aria-selected", "true");
  }
});

test("typing a date offers to jump to that day", async ({ page }) => {
  await start(page);
  await page.keyboard.press("Meta+k");
  const dialog = page.getByRole("dialog", { name: "Search" });
  const box = dialog.getByRole("combobox", { name: "Search events" });
  await box.fill("2026-12-24");
  const jump = dialog.getByRole("option", { name: /Go to/ });
  await expect(jump).toBeVisible();
  await box.press("Enter");
  await expect(page.getByRole("region", { name: "Day", exact: true })).toBeVisible();
  await expect(page.getByRole("heading", { level: 1 })).toContainText(/December 24|24 December/);
});

test("search has no accessibility violations, with results showing", async ({ page }) => {
  await start(page);
  await page.keyboard.press("Meta+k");
  const dialog = page.getByRole("dialog", { name: "Search" });
  await dialog.getByRole("combobox", { name: "Search events" }).fill("design");
  await expect(dialog.getByRole("option").first()).toBeVisible();
  await settle(page);
  const r = await new AxeBuilder({ page }).withTags(["wcag2a", "wcag2aa", "wcag21a", "wcag21aa", "wcag22aa"]).analyze();
  expect(r.violations.map((v) => `${v.id}: ${v.nodes[0]?.target.join(" ")}`)).toEqual([]);
});

// ---- ask your calendar -------------------------------------------------------------------

async function asking(page: import("@playwright/test").Page, flags: string, question: string) {
  await page.clock.install({ time: at(9, 30) });
  await page.goto(`/?${flags}`);
  await expect(page.getByRole("heading", { name: /Good (morning|afternoon|evening)/ })).toBeVisible();
  await page.keyboard.press("Meta+k");
  const dialog = page.getByRole("dialog", { name: "Search" });
  await dialog.getByRole("combobox", { name: "Search events" }).fill(question);
  return dialog;
}

test("with AI off there is no Ask row, however the text looks", async ({ page }) => {
  const dialog = await asking(page, "", "when is my design review?");
  await expect(dialog.getByRole("option", { name: /^Ask:/ })).toHaveCount(0);
});

test("a question is answered from the calendar and only the question is sent", async ({ page }) => {
  const dialog = await asking(page, "aion", "when is my design review?");
  const row = dialog.getByRole("option", { name: /^Ask: when is my design review/ });
  await expect(row).toBeVisible();
  await row.click();
  await expect(dialog.getByRole("status")).toContainText(/1 event matching “design review”/);
  await expect(dialog.getByRole("button", { name: /Design review/ })).toBeVisible();
  await expect(dialog.getByText(/Only your question was sent to the AI/)).toBeVisible();
  const asked = await page.evaluate(() => (window as unknown as { __asked?: string[] }).__asked ?? []);
  expect(asked).toEqual(["when is my design review?"]);
  // An answer opens the event like any search result.
  await dialog.getByRole("button", { name: /Design review/ }).click();
  await expect(page.getByRole("dialog", { name: "Design review" })).toBeVisible();
});

test("a free-time question lists stretches that can be copied", async ({ page }) => {
  const dialog = await asking(page, "aion", "when am I free for an hour this week");
  await dialog.getByRole("option", { name: /^Ask:/ }).click();
  await expect(dialog.getByRole("status")).toContainText(/free stretch/);
  await expect(dialog.getByRole("button", { name: /^Copy / }).first()).toBeVisible();
});

test("a question that is not about the calendar, and a failing AI service, are said plainly", async ({ page }) => {
  const dialog = await asking(page, "aion", "what is the weather like today");
  await dialog.getByRole("option", { name: /^Ask:/ }).click();
  await expect(dialog.getByText(/couldn’t turn that into a search/)).toBeVisible();
  await dialog.getByRole("button", { name: "Back" }).click();
  await expect(dialog.getByRole("combobox", { name: "Search events" })).toBeVisible();
});

test("a failing AI service says so and shows nothing made up", async ({ page }) => {
  const dialog = await asking(page, "aion&askfail", "when is my design review?");
  await dialog.getByRole("option", { name: /^Ask:/ }).click();
  await expect(dialog.getByRole("alert")).toContainText("Couldn’t reach the AI service");
  await expect(dialog.getByRole("status")).toHaveCount(0);
});

test("the answer view has no accessibility problems", async ({ page }) => {
  const dialog = await asking(page, "aion", "when is my design review?");
  await dialog.getByRole("option", { name: /^Ask:/ }).click();
  await expect(dialog.getByRole("status")).toBeVisible();
  await settle(page);
  const r = await new AxeBuilder({ page }).analyze();
  expect(r.violations).toEqual([]);
});
