import AxeBuilder from "@axe-core/playwright";
import { expect, test } from "@playwright/test";
import { settle } from "./settle";

test.use({ reducedMotion: "reduce" });

test("a new installation gets a set-up card with three steps, which goes once they are done", async ({ page }) => {
  await page.goto("/?fresh");
  await page.getByRole("button", { name: "Add a calendar" }).click();
  await page.getByRole("button", { name: /Try with sample data/ }).click();
  const card = page.getByRole("region", { name: "Get set up" });
  await expect(card).toContainText("0 of 3 done");
  await settle(page);
  expect((await new AxeBuilder({ page }).include('section[aria-label="Get set up"]').analyze()).violations).toEqual([]);

  await card.getByRole("switch", { name: "Open Firstlight when you sign in" }).click();
  await expect(card).toContainText("1 of 3 done");
  await card.getByRole("radio", { name: "Every unlock" }).click();
  await expect(card).toContainText("2 of 3 done");
  await card.getByRole("button", { name: "Looks right" }).click();
  await expect(card).toHaveCount(0);

  // The choice was saved as the unlock rule.
  await page.getByRole("button", { name: "Settings" }).click();
  await page.getByRole("main").getByRole("button", { name: /^When it appears/ }).click();
  await expect(page.getByRole("switch", { name: /every unlock/i })).toBeChecked();
});

test("the set-up card can be hidden, and someone already using the app never sees it", async ({ page }) => {
  await page.goto("/");
  await expect(page.getByRole("heading", { name: /Good (morning|afternoon|evening)/ })).toBeVisible();
  await expect(page.getByRole("region", { name: "Get set up" })).toHaveCount(0);

  await page.goto("/?fresh");
  await page.getByRole("button", { name: "Add a calendar" }).click();
  await page.getByRole("button", { name: /Try with sample data/ }).click();
  await page.getByRole("region", { name: "Get set up" }).getByRole("button", { name: "Hide" }).click();
  await expect(page.getByRole("region", { name: "Get set up" })).toHaveCount(0);
});
