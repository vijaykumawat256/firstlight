import AxeBuilder from "@axe-core/playwright";
import { expect, test, type Page } from "@playwright/test";

test.use({ reducedMotion: "reduce" });

const open = async (page: Page, query = "") => {
  await page.goto(`/${query}`);
  await page.getByRole("button", { name: "Settings" }).click();
  await page.getByRole("main").getByRole("button", { name: /^Your computers/ }).click();
  await expect(page.getByRole("heading", { name: "Your computers", level: 1 })).toBeVisible();
};

test("the first computer starts sharing through a synced folder and gets a code to copy", async ({ page, context }) => {
  await context.grantPermissions(["clipboard-read", "clipboard-write"]);
  await open(page);
  expect((await new AxeBuilder({ page }).analyze()).violations).toEqual([]);
  await page.getByRole("radio", { name: /iCloud Drive/ }).check();
  await page.getByRole("button", { name: "Start sharing" }).click();
  await expect(page.getByText("Sharing between your computers is on").first()).toBeVisible();
  await expect(page.getByText(/^FL1-/)).toBeVisible();
  await page.getByRole("button", { name: "Copy" }).click();
  expect(await page.evaluate(() => navigator.clipboard.readText())).toMatch(/^FL1-/);
  await expect(page.getByText("None yet. Turn sharing on with this code")).toBeVisible();
  expect((await new AxeBuilder({ page }).analyze()).violations).toEqual([]);
  await page.getByRole("button", { name: "Stop sharing" }).click();
  await expect(page.getByRole("button", { name: "Start sharing" })).toBeVisible();
});

test("a second computer needs the code, then gets the other computer's tasks", async ({ page }) => {
  await open(page, "?peerfolder&tasks");
  await page.getByRole("radio", { name: /Dropbox/ }).check();
  await page.getByRole("button", { name: "Start sharing" }).click();
  await expect(page.getByRole("alert")).toContainText("Type the sync code shown on that computer");
  await page.getByLabel(/Sync code/).fill("FL1-ABCD-EFGH-IJKL-MNOP-QRST-UVWX-YZ23-4567-ABCD-EFGH-IJKL-MNOP-QR");
  await page.getByRole("button", { name: "Start sharing" }).click();
  await expect(page.getByText("Work PC")).toBeVisible();
  await page.getByRole("button", { name: "Sync now" }).click();
  await expect(page.getByText("Everything is up to date").first()).toBeVisible();
  await page.getByRole("button", { name: "Settings", exact: true }).click();
  await page.getByRole("button", { name: "Briefing", exact: true }).click();
  await expect(page.getByRole("region", { name: "Tasks" })).toContainText("Prepare the slides");
});

test("a typed folder works on Windows, and a bad path is explained", async ({ page }) => {
  await open(page, "?windows");
  await expect(page.getByRole("radio", { name: /OneDrive/ })).toBeVisible();
  await page.getByLabel("Or another folder").fill("relative\\folder");
  await page.getByRole("button", { name: "Start sharing" }).click();
  await expect(page.getByRole("alert")).toContainText("Choose a folder");
  await page.getByLabel("Or another folder").fill("D:\\Sync");
  await page.getByRole("button", { name: "Start sharing" }).click();
  await expect(page.getByText("Sharing between your computers is on").first()).toBeVisible();
});
