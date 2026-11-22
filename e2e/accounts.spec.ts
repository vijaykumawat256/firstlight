import { expect, test, type Page } from "@playwright/test";
import { settle } from "./settle";

const SHOTS = "e2e/screenshots";

test.use({ reducedMotion: "reduce" });

async function openAdd(page: Page) {
  await page.getByRole("button", { name: "Add a calendar" }).click();
  await expect(page.getByRole("dialog", { name: "Add a calendar" })).toBeVisible();
}

test("first run shows onboarding and sample data fills the briefing", async ({ page }) => {
  await page.goto("/?fresh");
  await expect(page.getByRole("heading", { name: "Welcome to Firstlight" })).toBeVisible();
  await page.screenshot({ path: `${SHOTS}/onboarding.png` });

  await openAdd(page);
  await expect(page.getByText("Setup needed")).toHaveCount(0);
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
  await expect(page.getByRole("heading", { name: "Welcome to Firstlight" })).toBeVisible();

  // A good link
  await openAdd(page);
  await page.getByRole("button", { name: /Calendar link/ }).click();
  await page.getByLabel("Name", { exact: true }).fill("Team holidays");
  await page.getByLabel("Calendar address").fill("webcal://example.com/holidays.ics");
  await page.getByRole("button", { name: "Add calendar" }).click();
  await expect(page.getByRole("heading", { name: "Your day is clear" })).toBeVisible();
});

test("accounts screen: hide a calendar, rename, remove", async ({ page }) => {
  // The sample day is fixed-hour; pin the clock so "Team standup" is always on now, whatever time this runs.
  await page.clock.install({ time: new Date(new Date().setHours(9, 5, 0, 0)) });
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
  await expect(page.getByRole("region", { name: "Today" }).getByRole("button", { name: "Team standup", exact: true })).toBeVisible();

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
  await expect(page.getByRole("heading", { name: "Welcome to Firstlight" })).toBeVisible();
});

test("an account that needs attention says so", async ({ page }) => {
  await page.goto("/?attention");
  await page.getByRole("button", { name: "Settings" }).click();
  await page.getByRole("button", { name: /Accounts & calendars/ }).click();
  await expect(page.getByText(/Needs attention. Sign in again/)).toBeVisible(); // OAuth: reconnect
  await expect(page.getByText(/Needs attention. Add the calendar again/)).toBeVisible(); // link: re-add
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

test("CalDAV: preset, validation, wrong password explained, then connected", async ({ page }) => {
  await page.goto("/?fresh");
  await openAdd(page);
  await page.getByRole("button", { name: /iCloud, Yahoo and others/ }).click();
  await expect(page.getByRole("link", { name: /app-specific password/ }).or(page.getByRole("button", { name: /app-specific password/ }))).toBeVisible();
  await page.screenshot({ path: `${SHOTS}/add-caldav.png` });

  // Empty form: field-level messages, nothing sent
  await page.getByRole("button", { name: "Add calendar" }).click();
  await expect(page.getByText("Enter your username.")).toBeVisible();
  await expect(page.getByText("Enter the app password.")).toBeVisible();

  // Wrong password: plain-language error and the sheet stays open for a retry
  await page.getByLabel("Username").fill("me@icloud.com");
  await page.getByLabel("App password").fill("wrong");
  await page.getByRole("button", { name: "Add calendar" }).click();
  await expect(page.getByRole("alert")).toContainText("rejected that username or password");
  await page.screenshot({ path: `${SHOTS}/add-caldav-error.png` });

  // The password field is masked
  await expect(page.getByLabel("App password")).toHaveAttribute("type", "password");

  // Correct password connects and shows both calendars on the Accounts screen
  await page.getByLabel("App password").fill("abcd-efgh-ijkl-mnop");
  await page.getByRole("button", { name: "Add calendar" }).click();
  await expect(page.getByRole("heading", { name: "Your day is clear" })).toBeVisible();
  await page.getByRole("button", { name: "Settings" }).click();
  await page.getByRole("button", { name: /Accounts & calendars/ }).click();
  await expect(page.getByRole("switch", { name: "Show Home" })).toBeVisible();
  await expect(page.getByRole("switch", { name: "Show Family" })).toBeVisible();
});

test("CalDAV: a custom server needs a full https address", async ({ page }) => {
  await page.goto("/?fresh");
  await openAdd(page);
  await page.getByRole("button", { name: /iCloud, Yahoo and others/ }).click();
  await page.getByRole("combobox", { name: "Provider" }).click();
  await page.getByRole("option", { name: "Other server…" }).click();
  await page.getByLabel("Server address").fill("http://insecure.example.com");
  await page.getByLabel("Username").fill("me");
  await page.getByLabel("App password").fill("pw");
  await page.getByRole("button", { name: "Add calendar" }).click();
  await expect(page.getByText("Enter the full https:// address.")).toBeVisible();
});

test("Google sign-in: waiting state, then connected; signing in again does not duplicate", async ({ page }) => {
  await page.goto("/?fresh&slowoauth");
  await openAdd(page);
  await page.getByRole("button", { name: /^Google/ }).click();
  await expect(page.getByText("Finish signing in in your browser")).toBeVisible();
  await expect(page.getByRole("button", { name: "Cancel" })).toBeVisible();
  await page.screenshot({ path: `${SHOTS}/oauth-waiting.png` });

  await expect(page.getByRole("heading", { name: "Your day is clear" })).toBeVisible({ timeout: 10_000 });

  // Second sign-in with the same account reconnects instead of adding another.
  await page.getByRole("button", { name: "Settings" }).click();
  await page.getByRole("button", { name: /Accounts & calendars/ }).click();
  await page.getByRole("button", { name: "Add a calendar" }).click();
  await page.getByRole("button", { name: /^Google/ }).click();
  await expect(page.getByRole("heading", { name: "Accounts" })).toBeVisible({ timeout: 10_000 });
  await expect(page.getByRole("heading", { name: "me@gmail.com" })).toHaveCount(1);
});

test("Google sign-in can be cancelled while waiting", async ({ page }) => {
  await page.goto("/?fresh&slowoauth");
  await openAdd(page);
  await page.getByRole("button", { name: /^Microsoft/ }).click();
  await expect(page.getByText("Finish signing in in your browser")).toBeVisible();
  await page.getByRole("button", { name: "Cancel" }).click();
  await expect(page.getByRole("button", { name: /^Microsoft/ })).toBeVisible();
  await expect(page.getByRole("button", { name: /Calendar link/ })).toBeVisible();
});

test("a failed sign-in explains and offers to try again", async ({ page }) => {
  await page.goto("/?fresh&oauthfail");
  await openAdd(page);
  await page.getByRole("button", { name: /^Google/ }).click();
  await expect(page.getByRole("alert")).toContainText("Couldn't complete sign-in");
  await expect(page.getByRole("button", { name: "Try again" })).toBeVisible();
  await expect(page.getByRole("button", { name: "Back" })).toBeVisible();
  await page.screenshot({ path: `${SHOTS}/oauth-error.png` });
});

test("a provider with no sign-in app built in can be given your own in the app, with no .env or rebuild", async ({ page }) => {
  const { default: AxeBuilder } = await import("@axe-core/playwright");
  await page.goto("/?fresh&nomicrosoft");
  await openAdd(page);
  const tile = page.getByRole("button", { name: /^Microsoft/ });
  await expect(tile).toContainText("Set up");
  await expect(page.getByRole("button", { name: /^Google/ })).not.toContainText("Set up");
  await tile.click();

  const dialog = page.getByRole("dialog", { name: "Your own sign-in app" });
  await expect(dialog.getByLabel("Application (client) ID")).toBeFocused();
  await expect(dialog).toContainText("This copy of Firstlight has none built in");
  await settle(page);
  expect((await new AxeBuilder({ page }).include('[role="dialog"]').analyze()).violations).toEqual([]);
  await dialog.getByLabel("Application (client) ID").fill("not-an-id");
  await dialog.getByRole("button", { name: "Save" }).click();
  await expect(dialog.getByRole("alert")).toContainText("isn't a Microsoft application ID");
  await dialog.getByLabel("Application (client) ID").fill("1A2B3C4D-1234-5678-9ABC-1A2B3C4D5E6F");
  await dialog.getByRole("button", { name: "Save" }).click();
  await expect(page.getByText("Sign-in app saved").first()).toBeVisible();
  // Microsoft now signs in like Google.
  await expect(tile).not.toContainText("Set up");
  await tile.click();
  await expect(page.getByText(/browser/i).first()).toBeVisible();
});

test("your own sign-in app can also be set from the Accounts screen, and is kept", async ({ page }) => {
  await page.goto("/");
  await page.getByRole("button", { name: "Settings" }).click();
  await page.getByRole("button", { name: /Accounts & calendars/ }).click();
  await page.getByRole("button", { name: "Your own Google or Microsoft sign-in app…" }).click();
  const dialog = page.getByRole("dialog", { name: "Your own sign-in app" });
  await expect(dialog).toContainText("Leave empty to use the one built into Firstlight");
  await dialog.getByLabel("Client ID").fill("1234-abc.apps.googleusercontent.com");
  await dialog.getByLabel("Client secret").fill("GOCSPX-abcdefghijkl");
  await dialog.getByRole("button", { name: "Save" }).click();
  await expect(dialog).toHaveCount(0);
  await page.getByRole("button", { name: "Your own Google or Microsoft sign-in app…" }).click();
  await expect(dialog.getByLabel("Client ID")).toHaveValue("1234-abc.apps.googleusercontent.com");
  await expect(dialog.getByLabel("Client secret")).toHaveAttribute("placeholder", "Saved (type to replace)");
});

test("Reconnect restores an account that needs attention", async ({ page }) => {
  await page.goto("/?attention");
  await page.getByRole("button", { name: "Settings" }).click();
  await page.getByRole("button", { name: /Accounts & calendars/ }).click();
  await expect(page.getByText(/Sign in again to keep it up to date/)).toBeVisible();
  await page.getByRole("button", { name: "Reconnect" }).click();
  await expect(page.getByText(/Sign in again to keep it up to date/)).toHaveCount(0, { timeout: 10_000 });
  await expect(page.getByRole("button", { name: "Reconnect" })).toHaveCount(0);
});

test("Calendar app: explains the permission, connects, birthdays start hidden", async ({ page }) => {
  await page.goto("/?fresh");
  await openAdd(page);
  await page.getByRole("button", { name: /Calendar app on this computer/ }).click();
  await expect(page.getByText("Allow Full Access")).toBeVisible();
  await page.screenshot({ path: `${SHOTS}/system-calendar.png` });
  await page.getByRole("button", { name: "Allow access" }).click();
  await expect(page.getByRole("heading", { name: "Your day is clear" })).toBeVisible();

  await page.getByRole("button", { name: "Settings" }).click();
  await page.getByRole("button", { name: /Accounts & calendars/ }).click();
  await expect(page.getByRole("heading", { name: "Calendar app" })).toBeVisible();
  await expect(page.getByRole("switch", { name: "Show Home (iCloud)" })).toHaveAttribute("aria-checked", "true");
  await expect(page.getByRole("switch", { name: "Show Birthdays (Other)" })).toHaveAttribute("aria-checked", "false");
});

test("Calendar app: denied access shows the way to System Settings", async ({ page }) => {
  await page.goto("/?fresh&denied");
  await openAdd(page);
  await page.getByRole("button", { name: /Calendar app on this computer/ }).click();
  await expect(page.getByRole("button", { name: "Check again" })).toBeVisible();
  await expect(page.getByRole("button", { name: "Open System Settings" })).toBeVisible();
  await page.getByRole("button", { name: "Check again" }).click();
  await expect(page.getByRole("alert")).toContainText("turned off for Firstlight");
});

test("Calendar app option is hidden where it is unsupported", async ({ page }) => {
  await page.goto("/?fresh&nosystem");
  await openAdd(page);
  await expect(page.getByRole("button", { name: /Calendar link/ })).toBeVisible();
  await expect(page.getByRole("button", { name: /Calendar app on this computer/ })).toHaveCount(0);
});

test("public holidays are added with one tap and are an ordinary read-only calendar", async ({ page }) => {
  await page.goto("/?fresh");
  await openAdd(page);
  await page.getByRole("button", { name: /Public holidays/ }).click();
  const sheet = page.getByRole("dialog", { name: "Public holidays" });
  await expect(sheet).toContainText("published by Google");
  await expect(sheet.getByRole("button", { name: "United Kingdom" })).toBeVisible();
  await sheet.getByRole("button", { name: "Back" }).click();
  await expect(page.getByRole("dialog", { name: "Add a calendar" })).toBeVisible();

  await page.getByRole("button", { name: /Public holidays/ }).click();
  await page.getByRole("dialog", { name: "Public holidays" }).getByRole("button", { name: "United Kingdom" }).click();
  await expect(page.getByRole("heading", { name: "Your day is clear" })).toBeVisible();

  await page.getByRole("button", { name: "Settings" }).click();
  await page.getByRole("main").getByRole("button", { name: /^Accounts & calendars/ }).click();
  await expect(page.getByRole("region", { name: "Holidays: United Kingdom" })).toBeVisible();
});

test("on Windows the computer's own calendars are offered as Outlook, read-only", async ({ page }) => {
  await page.goto("/?fresh&outlook");
  await page.getByRole("button", { name: "Add a calendar" }).click();
  const dialog = page.getByRole("dialog", { name: "Add a calendar" });
  await expect(dialog.getByText("Outlook on this computer")).toBeVisible();
  await expect(dialog.getByText("Calendar app on this computer")).toHaveCount(0);
  await dialog.getByText("Outlook on this computer").click();
  const outlook = page.getByRole("dialog", { name: "Outlook" });
  await expect(outlook.getByText(/classic Outlook/)).toBeVisible();
  await expect(outlook.getByText(/new Outlook can't be read/)).toBeVisible();
  await expect(outlook.getByRole("button", { name: "Connect Outlook" })).toBeVisible();
  await expect(outlook.getByText(/Open System Settings/)).toHaveCount(0);
});

test("with Outlook the Calendar-app write switch and the Mail-app source are not offered", async ({ page }) => {
  await page.goto("/?outlook");
  await page.getByRole("button", { name: "Settings" }).click();
  await page.getByRole("main").getByRole("button", { name: /^Privacy & data/ }).click();
  await expect(page.getByText("Let Firstlight add events to the Calendar app")).toHaveCount(0);
});
