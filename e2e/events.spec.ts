import { expect, test } from "@playwright/test";

test.use({ reducedMotion: "reduce" });

const at = (h: number, m: number) => new Date(new Date().setHours(h, m, 0, 0));

test("add an event of your own: it shows in the briefing, can be edited, and deleted", async ({ page }) => {
  await page.clock.install({ time: at(9, 30) });
  await page.goto("/");
  await page.getByRole("button", { name: "New event" }).click();
  const sheet = page.getByRole("dialog", { name: "New event" });
  await expect(sheet).toContainText("Saved only on this computer");
  await expect(sheet.getByRole("button", { name: "Add event" })).toBeDisabled(); // needs a title

  await sheet.getByLabel("Title").fill("Pick up the car");
  await sheet.getByLabel("Start time").fill("16:00");
  await sheet.getByLabel("End time").fill("16:30");
  await sheet.getByLabel("Location").fill("Garage on 5th");
  await sheet.getByLabel("Meeting link").fill("https://meet.google.com/xyz-abcd-efg");
  await sheet.getByRole("button", { name: "Add event" }).click();

  await expect(page.getByText("Event added").first()).toBeVisible();
  const row = page.getByRole("button", { name: /Pick up the car/ });
  await expect(row).toBeVisible();

  // Edit
  await row.click();
  const detail = page.getByRole("dialog", { name: "Pick up the car" });
  await expect(detail.getByRole("button", { name: "Join" })).toBeVisible();
  await detail.getByRole("button", { name: "Edit" }).click();
  const edit = page.getByRole("dialog", { name: "Edit event" });
  await edit.getByLabel("Title").fill("Pick up the car (moved)");
  await edit.getByRole("button", { name: "Save" }).click();
  await expect(page.getByRole("button", { name: /Pick up the car \(moved\)/ })).toBeVisible();
  await expect(page.getByRole("button", { name: /^Pick up the car,? / })).toHaveCount(0);

  // Delete asks first
  await page.getByRole("button", { name: /Pick up the car \(moved\)/ }).click();
  const d2 = page.getByRole("dialog", { name: "Pick up the car (moved)" });
  await d2.getByRole("button", { name: "Delete" }).click();
  await expect(d2.getByRole("button", { name: "Yes, delete it" })).toBeVisible();
  await d2.getByRole("button", { name: "Keep" }).click();
  await expect(d2.getByRole("button", { name: "Delete" })).toBeVisible();
  await d2.getByRole("button", { name: "Delete" }).click();
  await d2.getByRole("button", { name: "Yes, delete it" }).click();
  await expect(page.getByRole("button", { name: /Pick up the car/ })).toHaveCount(0);
});

test("bad input is explained, not saved", async ({ page }) => {
  await page.clock.install({ time: at(9, 30) });
  await page.goto("/");
  await page.getByRole("button", { name: "New event" }).click();
  const sheet = page.getByRole("dialog", { name: "New event" });
  await sheet.getByLabel("Title").fill("Broken link");
  await sheet.getByLabel("Meeting link").fill("http://zoom.us/j/1");
  await sheet.getByRole("button", { name: "Add event" }).click();
  await expect(sheet.getByRole("alert")).toContainText("must start with https://");

  await sheet.getByLabel("Meeting link").fill("");
  await sheet.getByLabel("End time").fill("00:00");
  await sheet.getByLabel("Start time").fill("10:00");
  await sheet.getByRole("button", { name: "Add event" }).click();
  await expect(sheet.getByRole("alert")).toContainText("end after it starts");
});

test("all-day events, and events from connected calendars cannot be edited or deleted", async ({ page }) => {
  await page.clock.install({ time: at(9, 30) });
  await page.goto("/");
  await page.getByRole("button", { name: /Design review/ }).first().click();
  const detail = page.getByRole("dialog", { name: "Design review" });
  await expect(detail.getByRole("button", { name: "Edit" })).toHaveCount(0);
  await expect(detail.getByRole("button", { name: "Delete" })).toHaveCount(0);
  await page.keyboard.press("Escape");

  await page.getByRole("button", { name: "New event" }).click();
  const sheet = page.getByRole("dialog", { name: "New event" });
  await sheet.getByLabel("Title").fill("Conference");
  await sheet.getByRole("switch", { name: "All day" }).click();
  await expect(sheet.getByLabel("Start time")).toHaveCount(0);
  await sheet.getByRole("button", { name: "Add event" }).click();
  await expect(page.getByText("Event added").first()).toBeVisible();
  await page.getByRole("button", { name: "Calendar" }).click();
  await expect(page.getByRole("button", { name: /Conference/ }).first()).toBeVisible();
});

test("the new-event form has no accessibility violations", async ({ page }) => {
  const { default: AxeBuilder } = await import("@axe-core/playwright");
  await page.clock.install({ time: at(9, 30) });
  await page.goto("/");
  await page.getByRole("button", { name: "New event" }).click();
  await expect(page.getByRole("dialog", { name: "New event" })).toBeVisible();
  const r = await new AxeBuilder({ page }).withTags(["wcag2a", "wcag2aa", "wcag21a", "wcag21aa", "wcag22aa"]).analyze();
  expect(r.violations.map((v) => `${v.id}: ${v.nodes[0]?.target.join(" ")}`)).toEqual([]);
});
