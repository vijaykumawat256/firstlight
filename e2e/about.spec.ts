import { expect, test, type Page } from "@playwright/test";

test.use({ reducedMotion: "reduce" });

async function openAbout(page: Page, query = "") {
  await page.goto(`/${query}`);
  await expect(page.getByRole("heading", { name: /Good (morning|afternoon|evening)/ })).toBeVisible();
  await page.getByRole("button", { name: "Settings" }).click();
  await page.getByRole("button", { name: /About Daybrief/ }).click();
  await expect(page.getByRole("heading", { name: "About", level: 1 })).toBeVisible();
}

test("about shows the version and explains that updates aren't set up in a development build", async ({ page }) => {
  await openAbout(page);
  await expect(page.getByText(/Version 0\.1\.0/)).toBeVisible();
  await expect(page.getByRole("button", { name: "Check for updates" })).toBeDisabled();
  await expect(page.getByText("Updates aren't set up in this build.")).toBeVisible();
});

test("checking for updates: up to date, available (and install), and failure", async ({ page }) => {
  await openAbout(page, "?updates");
  await page.getByRole("button", { name: "Check for updates" }).click();
  await expect(page.getByText("You're up to date.")).toBeVisible();

  const available = await page.context().newPage();
  await openAbout(available, "?updates&newversion");
  await available.getByRole("button", { name: "Check for updates" }).click();
  await expect(available.getByText("Version 0.2.0 is available.")).toBeVisible();
  await available.getByRole("button", { name: "Install and restart" }).click();
  await expect(available.getByRole("button", { name: "Installing…" })).toBeVisible();

  const failing = await page.context().newPage();
  await openAbout(failing, "?updates&updatefail");
  await failing.getByRole("button", { name: "Check for updates" }).click();
  await expect(failing.getByText("Couldn't check. Try again later.")).toBeVisible();
});

test("help explains the product in plain language", async ({ page }) => {
  await openAbout(page);
  await expect(page.getByRole("heading", { name: "When does Daybrief appear?" })).toBeVisible();
  await expect(page.getByText(/first time you unlock your computer each day/)).toBeVisible();
  await expect(page.getByRole("heading", { name: "Email and AI (both optional)" })).toBeVisible();
});

test("the privacy policy is the published document, and says what it must", async ({ page }) => {
  await openAbout(page);
  await page.getByRole("radio", { name: "Privacy" }).click();
  const policy = page.getByRole("region", { name: "Privacy policy" });
  await expect(policy).toContainText("no server, no account and no analytics");
  await expect(policy).toContainText("never receive your calendar");
  await expect(policy).toContainText("the exact text that will be sent");
  await expect(policy.getByRole("heading", { name: "What Daybrief never does" })).toBeVisible();
  await expect(policy.locator("script, a, img")).toHaveCount(0);
});

test("licences: searchable, with the real text for each package", async ({ page }) => {
  await openAbout(page);
  await page.getByRole("radio", { name: "Licences" }).click();
  await expect(page.getByText(/open-source packages/)).toBeVisible();
  const status = page.getByRole("status");
  await expect(status).toContainText(/\d+ packages/);

  await page.getByPlaceholder("Search packages or licences").fill("react");
  await expect(page.getByRole("button", { name: /^react 19/ })).toBeVisible();
  await page.getByRole("button", { name: /^react 19/ }).click();
  await expect(page.getByText(/Permission is hereby granted/).first()).toBeVisible(); // the MIT text itself

  await page.getByPlaceholder("Search packages or licences").fill("zzzz-no-such-package");
  await expect(status).toContainText("0 packages");

  // A package that ships no licence file says so honestly and points to its repository.
  await page.getByPlaceholder("Search packages or licences").fill("objc2-foundation");
  await page.getByRole("button", { name: /objc2-foundation/ }).click();
  await expect(page.getByText(/does not include its licence file/)).toBeVisible();
});

test("crash reports are off by default, and a saved one can be read and deleted", async ({ page }) => {
  await page.goto("/?crashreport");
  await page.getByRole("button", { name: "Settings" }).click();
  await expect(page.getByRole("switch", { name: "Save crash reports" })).toHaveAttribute("aria-checked", "false");
  await page.getByRole("button", { name: "View…" }).click();
  const dialog = page.getByRole("dialog", { name: "Last crash report" });
  await expect(dialog.getByLabel("Crash report")).toContainText("This report contains no calendar, email or account information.");
  await expect(dialog.getByLabel("Crash report")).toContainText("Stopped at: crates/core/src/ics/mod.rs:123");
  await dialog.getByRole("button", { name: "Delete" }).click();
  await expect(page.getByRole("button", { name: "View…" })).toHaveCount(0);
});

test("update settings: automatic checks and channel", async ({ page }) => {
  await page.goto("/");
  await page.getByRole("button", { name: "Settings" }).click();
  await expect(page.getByRole("switch", { name: "Check for updates automatically" })).toHaveAttribute("aria-checked", "true");
  await page.getByRole("radio", { name: "Beta" }).click();
  await expect(page.getByRole("radio", { name: "Beta" })).toHaveAttribute("aria-checked", "true");
  await page.getByRole("switch", { name: "Check for updates automatically" }).click();
  await expect(page.getByRole("radio", { name: "Beta" })).toHaveCount(0); // channel only matters while checking
});
