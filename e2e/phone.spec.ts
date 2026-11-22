import AxeBuilder from "@axe-core/playwright";
import { expect, test } from "@playwright/test";
import { settle } from "./settle";

// The iPhone app's page, imitated in the browser with ?phone (see src/app/platform.ts).
test.use({ reducedMotion: "reduce", viewport: { width: 390, height: 844 } });

function todayAt(h: number, m: number): Date {
  const d = new Date();
  d.setHours(h, m, 0, 0);
  return d;
}

test("on a phone the page is solid, and computer-only settings are not offered", async ({ page }) => {
  await page.clock.install({ time: todayAt(9, 30) });
  await page.goto("/?phone");
  await expect(page.getByRole("heading", { name: /Good (morning|afternoon|evening)/ })).toBeVisible();
  await expect(page.locator("html")).toHaveAttribute("data-platform", "ios");
  // No sideways scrolling at iPhone width.
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);

  await page.getByRole("button", { name: "Settings" }).click();
  const main = page.getByRole("main");
  await expect(main.getByRole("button", { name: /^General/ })).toBeVisible();
  await expect(main.getByRole("button", { name: /^When it appears/ })).toHaveCount(0);
  await main.getByRole("button", { name: /^General/ }).click();
  await expect(page.getByText("Opens to", { exact: true })).toBeVisible();
  for (const gone of ["Start at login", "Show in the Dock or taskbar", "Quick capture", "Next meeting in the menu bar", "Check for updates"]) {
    await expect(page.getByText(gone, { exact: true })).toHaveCount(0);
  }
  await settle(page);
  const axe = await new AxeBuilder({ page }).analyze();
  expect(axe.violations).toEqual([]);
});

test("on a computer the same settings are all there", async ({ page }) => {
  await page.clock.install({ time: todayAt(9, 30) });
  await page.goto("/");
  await expect(page.getByRole("heading", { name: /Good (morning|afternoon|evening)/ })).toBeVisible();
  await expect(page.locator("html")).not.toHaveAttribute("data-platform", "ios");
  await page.getByRole("button", { name: "Settings" }).click();
  await expect(page.getByRole("main").getByRole("button", { name: /^When it appears/ })).toBeVisible();
  await page.getByRole("main").getByRole("button", { name: /^General/ }).click();
  await expect(page.getByText("Start at login", { exact: true })).toBeVisible();
});

async function openComputers(page: import("@playwright/test").Page, query: string) {
  await page.clock.install({ time: todayAt(9, 30) });
  await page.goto(`/${query}`);
  await expect(page.getByRole("heading", { name: /Good (morning|afternoon|evening)/ })).toBeVisible();
  await page.getByRole("button", { name: "Settings" }).click();
  await page.getByRole("main").getByRole("button", { name: /^Your computers/ }).click();
}

test("a computer shows a 6-digit code, and says Paired only once a phone really connected", async ({ page }) => {
  await openComputers(page, "");
  await page.getByRole("button", { name: "Add phone" }).click();
  const dialog = page.getByRole("dialog", { name: "Pair your phone" });
  await expect(dialog.getByLabel("Pairing code 4 8 2 9 1 3")).toHaveText("482 913");
  await expect(dialog.getByText(/Works for \d+:\d\d/)).toBeVisible();
  await settle(page);
  expect((await new AxeBuilder({ page }).analyze()).violations).toEqual([]);
  // The phone types the code.
  await page.evaluate(() => {
    (window as unknown as { __phonePaired: boolean }).__phonePaired = true;
  });
  const done = page.getByRole("dialog", { name: "Phone paired" });
  await expect(done.getByRole("status")).toContainText("Paired with iPhone");
  await done.getByRole("button", { name: "Done" }).click();
  await expect(page.getByText("This network only")).toBeVisible();
});

test("a phone pairs by typing the 6 digits, and is told clearly when the code is wrong", async ({ page }) => {
  await openComputers(page, "?phone");
  await expect(page.getByRole("radio", { name: "Synced folder" })).toHaveCount(0);
  const field = page.getByLabel("Pairing code");
  const pairButton = page.getByRole("button", { name: "Pair", exact: true });
  await expect(pairButton).toBeDisabled();
  await field.fill("111111");
  await expect(field).toHaveValue("111 111");
  await pairButton.click();
  await expect(page.getByRole("alert")).toHaveText("That code isn't right. Check the 6 digits on your computer and try again.");
  await field.fill("482913");
  await pairButton.click();
  await expect(page.getByRole("status").filter({ hasText: "Paired with Vijay's MacBook" }).first()).toBeVisible();
  await expect(page.getByRole("button", { name: "Pair again" })).toBeVisible();
  await settle(page);
  expect((await new AxeBuilder({ page }).analyze()).violations).toEqual([]);
});

test("a phone that can't find the computer is told what to check", async ({ page }) => {
  await openComputers(page, "?phone&pairnotfound");
  await page.getByLabel("Pairing code").fill("482913");
  await page.getByRole("button", { name: "Pair", exact: true }).click();
  await expect(page.getByRole("alert")).toContainText("same Wi-Fi");
});

test("on a phone every main screen fits the width, with a tab bar and a big New button", async ({ page }) => {
  await page.clock.install({ time: todayAt(9, 30) });
  await page.goto("/?phone&tasks");
  await expect(page.getByRole("heading", { name: /Good (morning|afternoon|evening)/ })).toBeVisible();
  const nav = page.getByRole("navigation", { name: "Main" });
  const fits = () => page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth);
  expect(await fits()).toBe(true);
  // New event is the round button, and the form opens as a full page.
  await page.getByRole("button", { name: "New event" }).click();
  await expect(page.getByRole("dialog", { name: "New event" })).toBeVisible();
  await page.keyboard.press("Escape");
  for (const [tab, heading] of [
    ["Calendar", null],
    ["Tasks", "Tasks"],
    ["Settings", "Settings"],
  ] as const) {
    await nav.getByRole("button", { name: tab }).click();
    await expect(nav.getByRole("button", { name: tab })).toHaveAttribute("aria-current", "page");
    if (heading) await expect(page.getByRole("heading", { name: heading, level: 1 })).toBeVisible();
    expect(await fits(), tab).toBe(true);
  }
  // A task opens one sheet of big buttons instead of a row of small icons, and the Tasks tab doesn't hijack focus with the keyboard.
  await nav.getByRole("button", { name: "Tasks" }).click();
  expect(await page.evaluate(() => document.activeElement?.tagName !== "INPUT")).toBe(true);
  await page.getByRole("button", { name: /^Options for Review Sam's slides/ }).click();
  const menu = page.getByRole("dialog", { name: "Review Sam's slides" });
  await expect(menu.getByRole("button", { name: "Set time aside" })).toBeVisible();
  await menu.getByRole("button", { name: "Change", exact: true }).click();
  await expect(page.getByRole("dialog", { name: "Change task" })).toBeVisible();
  expect(await fits(), "Change task dialog").toBe(true);
  await page.keyboard.press("Escape");

  // Calendar Day, Week and Month views all fit without page-level horizontal overflow.
  await nav.getByRole("button", { name: "Calendar" }).click();
  for (const mode of ["Day", "Week", "Month"] as const) {
    await page.getByRole("radio", { name: mode, exact: true }).click();
    expect(await fits(), `Calendar ${mode}`).toBe(true);
  }

  // Every Settings sub-page, Accounts, Add a calendar, and About fit at phone width with no overflow.
  await nav.getByRole("button", { name: "Settings" }).click();
  const main = page.getByRole("main");
  for (const name of ["General", "Appearance", "Briefing", "Notifications", "Time off", "Your computers", "Email", "AI summaries", "Privacy & data"]) {
    await main.getByRole("button", { name: new RegExp(`^${name}`) }).click();
    await expect(page.getByRole("heading", { name, level: 1 })).toBeVisible();
    expect(await fits(), `Settings: ${name}`).toBe(true);
    await page.getByRole("button", { name: "Settings", exact: true }).first().click();
  }
  await main.getByRole("button", { name: /^Accounts & calendars/ }).click();
  await expect(page.getByRole("heading", { name: "Accounts", level: 1 })).toBeVisible();
  expect(await fits(), "Accounts").toBe(true);
  await page.getByRole("button", { name: "Add a calendar" }).click();
  await expect(page.getByRole("dialog", { name: "Add a calendar" })).toBeVisible();
  expect(await fits(), "Add a calendar sheet").toBe(true);
  await page.keyboard.press("Escape");
  await nav.getByRole("button", { name: "Settings" }).click();
  await main.getByRole("button", { name: /^About Firstlight/ }).click();
  await expect(page.getByRole("heading", { name: "About", level: 1 })).toBeVisible();
  expect(await fits(), "About").toBe(true);

  await settle(page);
  expect((await new AxeBuilder({ page }).analyze()).violations).toEqual([]);
});

test("coming back to the app asks the other devices for anything new", async ({ page }) => {
  await page.clock.install({ time: todayAt(9, 30) });
  await page.goto("/?phone");
  await expect(page.getByRole("heading", { name: /Good (morning|afternoon|evening)/ })).toBeVisible();
  await page.evaluate(() => {
    Object.defineProperty(document, "visibilityState", { value: "visible", configurable: true });
    document.dispatchEvent(new Event("visibilitychange"));
  });
  await expect.poll(() => page.evaluate(() => (window as unknown as { __shared?: number }).__shared ?? 0)).toBe(1);
});

for (const [query, text] of [
  ["?phone", /Synced with Vijay's MacBook · /],
  ["?phone&sharetrouble", /Can't reach your computer/],
] as const) {
  test(`Today says how sharing is going (${query})`, async ({ page }) => {
    await openComputers(page, query);
    await page.getByLabel("Pairing code").fill("482913");
    await page.getByRole("button", { name: "Pair", exact: true }).click();
    await expect(page.getByRole("button", { name: "Pair again" })).toBeVisible();
    await page.getByRole("navigation", { name: "Main" }).getByRole("button", { name: "Today" }).click();
    const line = page.getByRole("status").filter({ hasText: text });
    await expect(line).toBeVisible();
    // Tapping it opens where sharing is set up.
    await line.click();
    await expect(page.getByRole("heading", { name: "Your computers", level: 1 })).toBeVisible();
  });
}

test("on an iPhone, Add a calendar recommends iPhone Calendar and sends Google and Outlook there too", async ({ page }) => {
  await page.clock.install({ time: todayAt(9, 30) });
  await page.goto("/?phone&fresh");
  await page.getByRole("button", { name: "Add a calendar" }).click();
  const sheet = page.getByRole("dialog", { name: "Add a calendar" });
  const first = sheet.getByRole("button").filter({ hasText: /^iPhone Calendar/ });
  await expect(first).toContainText("Recommended");
  await expect(sheet.getByRole("button").filter({ hasText: /^Google/ })).toContainText("iPhone Settings");
  await sheet.getByRole("button").filter({ hasText: /^Google/ }).click();
  // It goes to iPhone Calendar instead of a sign-in that can't work inside the app yet.
  await expect(page.getByRole("dialog").getByText("Use the calendars already on this device.")).toBeVisible();
});

test.describe("swiping a task", () => {
  test.use({ hasTouch: true });
  const swipe = async (row: import("@playwright/test").Locator, dx: number) => {
    const box = (await row.boundingBox())!;
    const y = box.y + box.height / 2;
    const x = box.x + box.width / 2;
    const touch = (cx: number) => [{ identifier: 1, clientX: cx, clientY: y }];
    await row.dispatchEvent("touchstart", { touches: touch(x), changedTouches: touch(x) });
    for (const step of [0.25, 0.5, 0.75, 1]) await row.dispatchEvent("touchmove", { touches: touch(x + dx * step), changedTouches: touch(x + dx * step) });
    await row.dispatchEvent("touchend", { touches: [], changedTouches: touch(x + dx) });
  };
  test("left moves it to tomorrow, right ticks it off", async ({ page }) => {
    await page.clock.install({ time: todayAt(9, 30) });
    await page.goto("/?phone&tasks");
    await page.getByRole("navigation", { name: "Main" }).getByRole("button", { name: "Tasks" }).click();
    const row = (title: string) => page.getByRole("button", { name: `Options for ${title}` }).locator("..");
    await swipe(row("Review Sam's slides"), -130);
    await expect(page.getByText("Moved to tomorrow: Review Sam's slides").first()).toBeVisible();
    await swipe(row("Send the budget to Priya"), 130);
    await expect(page.getByText("Done: Send the budget to Priya").first()).toBeVisible();
  });
});
