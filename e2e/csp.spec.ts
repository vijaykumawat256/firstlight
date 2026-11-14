import { expect, test, type Page } from "@playwright/test";
import { readFileSync } from "node:fs";

// The strict policy the release build ships with (the dev server uses a relaxed one, so nothing else tests this).
const conf = JSON.parse(readFileSync("src-tauri/tauri.conf.json", "utf8"));
const CSP: string = conf.app.security.csp;

test.use({ baseURL: "http://localhost:4173", reducedMotion: "reduce" });

/** Serves the production build with the release CSP header and records every violation. */
async function strictPage(page: Page) {
  await page.addInitScript(() => {
    (window as unknown as { __csp: string[] }).__csp = [];
    document.addEventListener("securitypolicyviolation", (e) => (window as unknown as { __csp: string[] }).__csp.push(`${e.violatedDirective}: ${e.blockedURI || e.sample}`));
  });
  await page.route("**/*", async (route) => {
    if (route.request().resourceType() !== "document") return route.continue();
    const response = await route.fetch();
    await route.fulfill({ response, headers: { ...response.headers(), "content-security-policy": CSP } });
  });
}

const violations = (page: Page) => page.evaluate(() => (window as unknown as { __csp: string[] }).__csp);

test("the release CSP is strict", () => {
  expect(CSP).toContain("default-src 'self'");
  expect(CSP).toContain("script-src 'self'");
  expect(CSP).not.toMatch(/unsafe-eval/);
  expect(CSP).not.toMatch(/script-src[^;]*unsafe-inline/);
  expect(CSP).not.toMatch(/style-src[^;]*unsafe-inline/);
  expect(CSP).toContain("object-src 'none'");
  expect(CSP).toContain("frame-ancestors 'none'");
  expect(CSP).not.toMatch(/https?:\/\/(?!ipc\.localhost)/); // no remote origins allowed at all
});

test("the whole UI works under the strict release CSP with zero violations", async ({ page }) => {
  await strictPage(page);
  const errors: string[] = [];
  page.on("pageerror", (e) => errors.push(e.message));

  await page.clock.install({ time: new Date(new Date().setHours(9, 30, 0, 0)) });
  await page.goto("/?withgoogle&mailon");
  await expect(page.getByRole("heading", { name: /Good (morning|afternoon|evening)/ })).toBeVisible();
  await expect(page.getByLabel("Up next")).toBeVisible();
  await expect(page.getByRole("region", { name: "Inbox" })).toBeVisible();

  // Event detail sheet (Radix dialog portal)
  await page.getByRole("button", { name: /Design review/ }).first().click();
  await expect(page.getByRole("dialog", { name: "Design review" })).toBeVisible();
  await page.keyboard.press("Escape");

  // Calendar views (inline positioning styles, colour-mix backgrounds)
  await page.getByRole("button", { name: "Calendar" }).click();
  await expect(page.getByRole("region", { name: "Day", exact: true })).toBeVisible();
  await page.getByRole("radio", { name: "Week" }).click();
  await expect(page.getByRole("region", { name: "Week", exact: true })).toBeVisible();
  await page.getByRole("button", { name: "Briefing" }).click();

  // Settings: every kind of control, dropdowns (portals), accent (inline CSS variables), dialogs
  await page.getByRole("button", { name: "Settings" }).click();
  await page.getByRole("combobox", { name: "Refresh interval" }).click();
  await page.getByRole("option", { name: "30 min" }).click();
  await page.getByRole("radio", { name: "purple" }).click();
  await page.getByRole("radio", { name: "Dark" }).click();
  await page.getByRole("switch", { name: "AI summaries" }).click();
  await expect(page.getByRole("dialog", { name: "Turn on AI summaries?" })).toBeVisible();
  await page.getByRole("dialog").getByRole("button", { name: "Cancel" }).click();
  await page.getByRole("button", { name: "Erase…" }).click();
  await page.getByRole("dialog").getByRole("button", { name: "Cancel" }).click();

  // Accounts + add sheet
  await page.getByRole("button", { name: /Accounts & calendars/ }).click();
  await page.getByRole("button", { name: "Add a calendar" }).click();
  await expect(page.getByRole("dialog", { name: "Add a calendar" })).toBeVisible();
  await page.keyboard.press("Escape");

  // The compact panel
  await page.goto("/#/upnext");
  await expect(page.getByText(/Up next/)).toBeVisible();

  expect(await violations(page)).toEqual([]);
  expect(errors).toEqual([]);
});

test("a deliberately injected inline script is blocked (the policy really is enforced)", async ({ page }) => {
  await strictPage(page);
  await page.goto("/");
  const ran = await page.evaluate(() => {
    const s = document.createElement("script");
    s.textContent = "window.__injected = true";
    document.head.appendChild(s);
    return (window as unknown as { __injected?: boolean }).__injected === true;
  });
  expect(ran).toBe(false);
  expect((await violations(page)).some((v) => v.startsWith("script-src"))).toBe(true);
});
