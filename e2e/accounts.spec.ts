import { expect, test, type Page } from "@playwright/test";

const SHOTS = "e2e/screenshots";

test.use({ reducedMotion: "reduce" });

async function openAdd(page: Page) {
  await page.getByRole("button", { name: "Add a calendar" }).click();
  await expect(page.getByRole("dialog", { name: "Add a calendar" })).toBeVisible();
}

test("first run shows onboarding and sample data fills the briefing", async ({ page }) => {
  await page.goto("/?fresh");
  await expect(page.getByRole("heading", { name: "Welcome to Daybrief" })).toBeVisible();
  await page.screenshot({ path: `${SHOTS}/onboarding.png` });

  await openAdd(page);
  await expect(page.getByText("Soon")).toHaveCount(3);
  await page.screenshot({ path: `${SHOTS}/add-account.png` });
  await page.getByRole("button", { name: /Try with sample data/ }).click();

  await expect(page.getByRole("heading", { name: /Good (morning|afternoon|evening)/ })).toBeVisible();
  await expect(page.getByLabel("Today")).toBeVisible();
});

test("adding a calendar link validates, explains failures, and connects", async ({ page }) => {
  await page.goto("/?fresh");
  await openAdd(page);
  await page.getByRole("button", { name: /Calendar link/ }).click();

  // Client-side validation
  await page.getByLabel("Calendar address").fill("not a link");
  await page.getByRole("button", { name: "Add calendar" }).click();
  await expect(page.getByText("Enter an https:// or webcal:// link.")).toBeVisible();

  // A link the backend cannot read: plain-language error, nothing added
  await page.getByLabel("Calendar address").fill("https://example.com/fail.ics");
  await page.getByRole("button", { name: "Add calendar" }).click();
  await expect(page.getByRole("alert")).toContainText("Couldn't read a calendar from that link");
  await page.screenshot({ path: `${SHOTS}/add-ics-error.png` });
  // The modal keeps the page behind it hidden from assistive tech; closing it shows nothing was added.
  await page.keyboard.press("Escape");
  await expect(page.getByRole("heading", { name: "Welcome to Daybrief" })).toBeVisible();

  // A good link
  await openAdd(page);
  await page.getByRole("button", { name: /Calendar link/ }).click();
  await page.getByLabel("Name", { exact: true }).fill("Team holidays");
  await page.getByLabel("Calendar address").fill("webcal://example.com/holidays.ics");
  await page.getByRole("button", { name: "Add calendar" }).click();
  await expect(page.getByRole("heading", { name: "Your day is clear" })).toBeVisible();
});

test("accounts screen: hide a calendar, rename, remove", async ({ page }) => {
  await page.goto("/");
  await expect(page.getByLabel("Up next")).toBeVisible();

  await page.getByRole("button", { name: "Settings" }).click();
  await page.getByRole("button", { name: /Accounts & calendars/ }).click();
  await expect(page.getByRole("heading", { name: "Accounts" })).toBeVisible();
  await page.screenshot({ path: `${SHOTS}/accounts.png` });

  // Hiding Personal removes its events from the briefing.
  await page.getByRole("switch", { name: "Show Personal" }).click();
  await expect(page.getByRole("switch", { name: "Show Personal" })).toHaveAttribute("aria-checked", "false");
  await page.getByRole("button", { name: "Briefing" }).click();
  await expect(page.getByText("Lunch with Alex")).toHaveCount(0);
  await expect(page.getByText("Team standup")).toBeVisible();

  // Rename
  await page.getByRole("button", { name: "Settings" }).click();
  await page.getByRole("button", { name: /Accounts & calendars/ }).click();
  await page.getByRole("button", { name: "Rename" }).click();
  await page.getByLabel("Name", { exact: true }).fill("Work & life");
  await page.getByRole("button", { name: "Save" }).click();
  await expect(page.getByRole("heading", { name: "Work & life" })).toBeVisible();

  // Change colour
  await page.getByRole("button", { name: "Colour for Work" }).click();
  await page.getByRole("radio", { name: "purple" }).click();

  // Remove: needs confirmation, then returns to onboarding
  await page.getByRole("button", { name: "Remove" }).click();
  await page.screenshot({ path: `${SHOTS}/remove-confirm.png` });
  await page.getByRole("dialog").getByRole("button", { name: "Remove" }).click();
  await expect(page.getByRole("heading", { name: "Welcome to Daybrief" })).toBeVisible();
});

test("an account that needs attention says so", async ({ page }) => {
  await page.goto("/?attention");
  await page.getByRole("button", { name: "Settings" }).click();
  await page.getByRole("button", { name: /Accounts & calendars/ }).click();
  await expect(page.getByText(/Needs attention/)).toBeVisible();
  await page.screenshot({ path: `${SHOTS}/accounts-attention.png` });
});

test("accounts screen is keyboard reachable", async ({ page }) => {
  await page.goto("/");
  await page.getByRole("button", { name: "Settings" }).focus();
  await page.keyboard.press("Enter");
  await page.getByRole("button", { name: /Accounts & calendars/ }).focus();
  await page.keyboard.press("Enter");
  await expect(page.getByRole("heading", { name: "Accounts" })).toBeVisible();
  await page.getByRole("switch", { name: "Show Work" }).focus();
  await page.keyboard.press("Space");
  await expect(page.getByRole("switch", { name: "Show Work" })).toHaveAttribute("aria-checked", "false");
});
