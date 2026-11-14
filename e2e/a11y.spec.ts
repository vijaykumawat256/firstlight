import AxeBuilder from "@axe-core/playwright";
import { expect, test, type Page } from "@playwright/test";

test.use({ reducedMotion: "reduce" });

const WCAG = ["wcag2a", "wcag2aa", "wcag21a", "wcag21aa", "wcag22aa"];

async function scan(page: Page, label: string) {
  const results = await new AxeBuilder({ page }).withTags(WCAG).analyze();
  const summary = results.violations.map((v) => `${v.id} (${v.impact}): ${v.help}\n   ${v.nodes.slice(0, 3).map((n) => n.target.join(" ")).join("\n   ")}`);
  expect(summary, `${label}: accessibility violations`).toEqual([]);
}

/** Opens one page of settings from the settings list. */
async function section(page: Page, name: string) {
  await page.getByRole("main").getByRole("button", { name: new RegExp(`^${name}`) }).click();
  await expect(page.getByRole("heading", { name, level: 1 })).toBeVisible();
}
const toList = (page: Page) => page.getByRole("button", { name: "Settings", exact: true }).click();

const at = (h: number, m: number) => new Date(new Date().setHours(h, m, 0, 0));

for (const scheme of ["light", "dark"] as const) {
  test.describe(`${scheme} theme`, () => {
    test.use({ colorScheme: scheme });

    test("briefing, with email and AI summary", async ({ page }) => {
      await page.clock.install({ time: at(9, 30) });
      await page.goto("/?mailon&aion");
      await expect(page.getByRole("region", { name: "Inbox" })).toBeVisible();
      await expect(page.getByText("Summary by AI")).toBeVisible();
      await scan(page, "briefing");
    });

    test("onboarding and the add-account sheet", async ({ page }) => {
      await page.goto("/?fresh");
      await expect(page.getByRole("heading", { name: "Welcome to Firstlight" })).toBeVisible();
      await scan(page, "onboarding");
      await page.getByRole("button", { name: "Add a calendar" }).click();
      await expect(page.getByRole("dialog", { name: "Add a calendar" })).toBeVisible();
      await scan(page, "add-account sheet");
    });

    test("settings", async ({ page }) => {
      await page.setViewportSize({ width: 420, height: 2400 });
      await page.goto("/?mailon&withgoogle");
      await page.getByRole("button", { name: "Settings" }).click();
      await expect(page.getByRole("heading", { name: "Settings", level: 1 })).toBeVisible();
      await scan(page, "settings list");
      for (const name of ["General", "Appearance", "Briefing", "When it appears", "Notifications", "Email", "AI summaries", "Privacy & data"]) {
        await section(page, name);
        await scan(page, `settings: ${name}`);
        await toList(page);
      }
    });

    test("accounts", async ({ page }) => {
      await page.goto("/?attention");
      await page.getByRole("button", { name: "Settings" }).click();
      await page.getByRole("button", { name: /Accounts & calendars/ }).click();
      await expect(page.getByRole("heading", { name: "Accounts", level: 1 })).toBeVisible();
      await scan(page, "accounts");
    });

    test("day and week views", async ({ page }) => {
      await page.clock.install({ time: new Date(2026, 2, 11, 9, 30) });
      await page.goto("/");
      await page.getByRole("button", { name: "Calendar" }).click();
      await expect(page.getByRole("region", { name: "Day", exact: true })).toBeVisible(); // lazy screen has arrived
      await scan(page, "day view");
      await page.getByRole("radio", { name: "Week" }).click();
      await expect(page.getByRole("region", { name: "Week", exact: true })).toBeVisible();
      await scan(page, "week view");
    });

    test("dialogs: event detail, AI consent, erase", async ({ page }) => {
      await page.clock.install({ time: at(9, 30) });
      await page.goto("/");
      await page.getByRole("button", { name: /Design review/ }).first().click();
      await expect(page.getByRole("dialog", { name: "Design review" })).toBeVisible();
      await scan(page, "event detail");
      await page.keyboard.press("Escape");
      await page.getByRole("button", { name: "Settings" }).click();
      await section(page, "AI summaries");
      await page.getByRole("switch", { name: "AI summaries" }).click();
      await expect(page.getByRole("dialog", { name: "Turn on AI summaries?" })).toBeVisible();
      await scan(page, "AI consent");
      await page.getByRole("dialog").getByRole("button", { name: "Cancel" }).click();
      await toList(page);
      await section(page, "Privacy & data");
      await page.getByRole("button", { name: "Erase…" }).click();
      await scan(page, "erase confirmation");
    });

    test("the add-mailbox dialog, with an error showing", async ({ page }) => {
      await page.goto("/?mailon&imapbad");
      await page.getByRole("button", { name: "Settings" }).click();
      await section(page, "Email");
      await page.getByRole("button", { name: "Add mailbox…" }).click();
      const dialog = page.getByRole("dialog", { name: "Add a mailbox" });
      await scan(page, "add mailbox");
      await dialog.getByLabel("Email address").fill("sam@yahoo.com");
      await dialog.getByLabel("App-specific password").fill("abcd");
      await dialog.getByRole("button", { name: "Connect" }).click();
      await expect(dialog.getByRole("alert")).toBeVisible();
      await scan(page, "add mailbox with error");
    });

    test("month view and the empty-day screen", async ({ page }) => {
      await page.clock.install({ time: at(9, 30) });
      await page.goto("/?quietdays");
      await expect(page.getByRole("region", { name: "Coming up" })).toBeVisible();
      await scan(page, "empty day");
      await page.getByRole("button", { name: "See the month" }).click();
      await expect(page.getByRole("grid", { name: "Month" })).toBeVisible();
      await scan(page, "month view");
    });

    test("the up-next panel", async ({ page }) => {
      await page.setViewportSize({ width: 360, height: 148 });
      await page.clock.install({ time: at(9, 30) });
      await page.goto("/#/upnext");
      await expect(page.getByText(/Up next/)).toBeVisible();
      await scan(page, "up-next panel");
    });
  });
}

test("every interactive control can be reached with the keyboard on the briefing", async ({ page }) => {
  await page.clock.install({ time: at(9, 30) });
  await page.goto("/");
  await expect(page.getByLabel("Up next")).toBeVisible();
  const reached = new Set<string>();
  for (let i = 0; i < 40; i++) {
    await page.keyboard.press("Tab");
    const label = await page.evaluate(() => {
      const el = document.activeElement as HTMLElement | null;
      return el ? (el.getAttribute("aria-label") ?? el.textContent ?? el.tagName).trim().slice(0, 40) : "";
    });
    if (label) reached.add(label);
  }
  for (const expected of ["Calendar", "Refresh", "Settings", "Join"]) {
    expect([...reached].some((l) => l.includes(expected)), `Tab never reached "${expected}"; reached: ${[...reached].join(" | ")}`).toBe(true);
  }
});

// A custom accent colour must stay readable in both themes: every one is derived from the palette in CSS.
for (const scheme of ["light", "dark"] as const) {
  test(`every accent colour keeps text readable (${scheme})`, async ({ page }) => {
    test.setTimeout(90_000);
    await page.emulateMedia({ colorScheme: scheme });
    await page.clock.install({ time: at(9, 30) });
    await page.goto("/?mailon");
    await page.getByRole("button", { name: "Settings" }).click();
    await section(page, "Appearance");
    for (const accent of ["red", "orange", "green", "teal", "indigo", "purple", "pink"]) {
      await page.getByRole("radio", { name: accent }).click();
      await expect(page.locator("html")).toHaveAttribute("data-accent", accent);
      await scan(page, `settings with ${accent} accent`);
    }
    await toList(page);
    await scan(page, "settings list with a custom accent");
    await page.getByRole("button", { name: "Briefing", exact: true }).click();
    await expect(page.getByRole("heading", { name: /Good (morning|afternoon|evening)/ })).toBeVisible();
    await scan(page, "briefing with a custom accent");
  });
}
