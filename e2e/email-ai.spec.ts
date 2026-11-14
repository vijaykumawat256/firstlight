import { expect, test, type Page } from "@playwright/test";

const SHOTS = "e2e/screenshots";

function todayAt(h: number, m: number): Date {
  const d = new Date();
  d.setHours(h, m, 0, 0);
  return d;
}

test.use({ reducedMotion: "reduce" });

async function loadBriefing(page: Page, query = "") {
  await page.clock.install({ time: todayAt(9, 30) });
  await page.goto(`/${query}`);
  await expect(page.getByRole("heading", { name: /Good (morning|afternoon|evening)/ })).toBeVisible();
}

async function openSettings(page: Page, query = "") {
  await loadBriefing(page, query);
  await page.getByRole("button", { name: "Settings" }).click();
  await expect(page.getByRole("heading", { name: "Settings", level: 1 })).toBeVisible();
}

const calls = (page: Page, name: "__mailCalls" | "__aiCalls") => page.evaluate((n) => (globalThis as unknown as Record<string, number>)[n] ?? 0, name);

// ---------------------------------------------------------------------------------- email

test("email is off by default: no inbox, no mail requests, and only the master switch shows", async ({ page }) => {
  await loadBriefing(page);
  await expect(page.getByRole("region", { name: "Inbox" })).toHaveCount(0);
  expect(await calls(page, "__mailCalls")).toBe(0);

  await page.getByRole("button", { name: "Settings" }).click();
  const email = page.getByRole("region", { name: "Email", exact: true });
  await expect(email.getByRole("switch", { name: "Show email in the briefing" })).toHaveAttribute("aria-checked", "false");
  await expect(email.getByText("Important senders")).toHaveCount(0);
  await expect(email.getByRole("button", { name: "Connect", exact: true })).toHaveCount(0);
  expect(await calls(page, "__mailCalls")).toBe(0);
});

test("connecting the Mail app shows the inbox in the briefing", async ({ page }) => {
  await openSettings(page);
  await page.getByRole("switch", { name: "Show email in the briefing" }).click();
  await expect(page.getByText("Choose where to read email from")).toBeVisible();
  await page.getByRole("button", { name: "Connect", exact: true }).click();
  await expect(page.getByRole("switch", { name: "Read the Mail app" })).toBeVisible();

  await page.getByRole("button", { name: "Briefing" }).click();
  const inbox = page.getByRole("region", { name: "Inbox" });
  await expect(inbox).toBeVisible();
  await expect(inbox).toContainText("37 unread");
  await expect(inbox.getByRole("list", { name: "Invitations" })).toContainText("Roadmap review");
  await expect(inbox.getByRole("list", { name: "Invitations" })).toContainText("Invitation from Jordan Lee");
  await expect(inbox.getByRole("list", { name: "Important email" })).toContainText("Priya Shah");
  await expect(inbox).toContainText("and 34 more unread");
  await inbox.getByRole("button", { name: /Agenda for Tuesday/ }).click(); // opens in the user's own app; must not throw
  await page.screenshot({ path: `${SHOTS}/inbox-light.png`, fullPage: true });
});

test("a refused Mail permission is explained", async ({ page }) => {
  await openSettings(page, "?macdenied");
  await page.getByRole("switch", { name: "Show email in the briefing" }).click();
  await page.getByRole("button", { name: "Connect", exact: true }).click();
  await expect(page.getByText("Couldn't connect Mail")).toBeVisible();
  await expect(page.getByText(/switch Mail on under Daybrief/)).toBeVisible();
  await expect(page.getByRole("button", { name: "Connect", exact: true })).toBeVisible(); // still not connected
});

test("inbox problems say what to do", async ({ page }) => {
  await loadBriefing(page, "?mailon&mailproblem=permission");
  const inbox = page.getByRole("region", { name: "Inbox" });
  await expect(inbox).toContainText("Allow Daybrief to control Mail");
  await expect(inbox.getByRole("button", { name: "Open Settings" })).toBeVisible();
  await page.screenshot({ path: `${SHOTS}/inbox-problem.png` });
});

test("Mail not running is a calm, one-line state", async ({ page }) => {
  await loadBriefing(page, "?mailon&mailproblem=not_running");
  await expect(page.getByRole("region", { name: "Inbox" })).toContainText("Open the Mail app to see your email here.");
});

test("an inbox with nothing important says so", async ({ page }) => {
  await loadBriefing(page, "?mailon&emptymail");
  const inbox = page.getByRole("region", { name: "Inbox" });
  await expect(inbox).toContainText("Nothing important in your inbox.");
  await expect(inbox).toContainText("4 unread");
});

test("important senders can be added, de-duplicated by the backend rules, and removed", async ({ page }) => {
  await openSettings(page, "?mailon");
  await page.getByLabel("Add a sender").fill("boss@acme.com");
  await page.getByRole("button", { name: "Add", exact: true }).click();
  await page.getByLabel("Add a sender").fill("@clients.org");
  await page.keyboard.press("Enter");
  const list = page.getByRole("list", { name: "Important senders" });
  await expect(list).toContainText("boss@acme.com");
  await expect(list).toContainText("@clients.org");
  await page.getByRole("button", { name: "Remove boss@acme.com" }).click();
  await expect(list).not.toContainText("boss@acme.com");
  await expect(page.getByRole("button", { name: "Add", exact: true })).toBeDisabled(); // empty field
});

test("a connected Google account can be allowed to share its mail", async ({ page }) => {
  await openSettings(page, "?mailon&withgoogle");
  const row = page.getByText("me@gmail.com", { exact: true });
  await expect(row).toBeVisible();
  await page.getByRole("button", { name: "Allow mail access" }).click();
  await expect(page.getByRole("switch", { name: "Read mail for me@gmail.com" })).toBeVisible();
});

// ------------------------------------------------------------------------------------- AI

test("AI is off by default: no summary, no AI requests", async ({ page }) => {
  await loadBriefing(page);
  await expect(page.getByText("Summary by AI")).toHaveCount(0);
  expect(await calls(page, "__aiCalls")).toBe(0);
  await page.getByRole("button", { name: "Settings" }).click();
  await expect(page.getByRole("switch", { name: "AI summaries" })).toHaveAttribute("aria-checked", "false");
  expect(await calls(page, "__aiCalls")).toBe(0);
});

test("turning on AI shows exactly what is sent and needs a key and agreement", async ({ page }) => {
  await openSettings(page);
  await page.getByRole("switch", { name: "AI summaries" }).click();
  const dialog = page.getByRole("dialog", { name: "Turn on AI summaries?" });
  await expect(dialog).toBeVisible();

  const sent = dialog.getByLabel("Exactly what will be sent");
  await expect(sent).toContainText("Design review (Room 4B)");
  await expect(sent).toContainText("INSTRUCTIONS FOR THE MODEL");
  await expect(sent).not.toContainText("zoom.us");
  await expect(dialog).toContainText("Anthropic's servers (api.anthropic.com)");
  // The destination must be visible without scrolling the dialog.
  await expect(dialog.getByText("Anthropic's servers (api.anthropic.com)")).toBeInViewport();
  await expect(dialog.getByLabel("Anthropic API key")).toBeInViewport();
  await page.screenshot({ path: `${SHOTS}/ai-consent.png` });

  // No key, no way to continue.
  await expect(dialog.getByRole("button", { name: "Turn on" })).toBeDisabled();
  // A rejected key leaves AI off and says why.
  await dialog.getByLabel("Anthropic API key").fill("sk-ant-rejected-key");
  await dialog.getByRole("button", { name: "Turn on" }).click();
  await expect(dialog.getByRole("alert")).toContainText("rejected the key");
  await dialog.getByRole("button", { name: "Cancel" }).click();
  await expect(page.getByRole("switch", { name: "AI summaries" })).toHaveAttribute("aria-checked", "false");

  // A good key turns it on.
  await page.getByRole("switch", { name: "AI summaries" }).click();
  await page.getByRole("dialog").getByLabel("Anthropic API key").fill("sk-ant-a-good-looking-key");
  await page.getByRole("dialog").getByRole("button", { name: "Turn on" }).click();
  await expect(page.getByRole("switch", { name: "AI summaries" })).toHaveAttribute("aria-checked", "true");
  await expect(page.getByText("Anthropic (Claude) · claude-haiku-5-5")).toBeVisible();
});

test("cancelling the agreement changes nothing", async ({ page }) => {
  await openSettings(page);
  await page.getByRole("switch", { name: "AI summaries" }).click();
  await page.getByRole("dialog").getByRole("button", { name: "Cancel" }).click();
  await expect(page.getByRole("switch", { name: "AI summaries" })).toHaveAttribute("aria-checked", "false");
  expect(await calls(page, "__aiCalls")).toBe(0);
});

test("a local model needs no key and says it stays on this computer", async ({ page }) => {
  await openSettings(page);
  await page.getByRole("switch", { name: "AI summaries" }).click();
  const dialog = page.getByRole("dialog", { name: "Turn on AI summaries?" });
  await dialog.getByRole("radio", { name: "On this computer" }).click();
  await expect(dialog).toContainText("runs on this computer");
  await expect(dialog.getByLabel("Model address")).toHaveValue("http://127.0.0.1:11434");
  await expect(dialog.getByLabel("Anthropic API key")).toHaveCount(0);
  await dialog.getByRole("button", { name: "Turn on" }).click();
  await expect(page.getByText("A model on this computer · claude-haiku-5-5")).toBeVisible();
});

test("the AI summary replaces the normal one and is labelled", async ({ page }) => {
  await loadBriefing(page, "?aion");
  await expect(page.getByText("A busy morning with a design review at 10:30")).toBeVisible();
  await expect(page.getByText("Summary by AI")).toBeVisible();
  await page.screenshot({ path: `${SHOTS}/ai-summary.png` });
});

test("the normal summary shows immediately and AI swaps in only when it arrives", async ({ page }) => {
  await loadBriefing(page, "?aion&aislow");
  await expect(page.getByText(/events? left today\./)).toBeVisible(); // built-in summary, no waiting
  await expect(page.getByText("Summary by AI")).toHaveCount(0);
  await expect(page.getByText("A busy morning with a design review at 10:30")).toBeVisible({ timeout: 8000 });
});

test("if the AI fails the briefing carries on with its own summary", async ({ page }) => {
  await loadBriefing(page, "?aion&aifail");
  await expect(page.getByText(/events? left today\./)).toBeVisible();
  await expect(page.getByText("Summary by AI")).toHaveCount(0);
  await expect(page.getByLabel("Up next")).toBeVisible();
});

test("important email can be included, and the preview shows it", async ({ page }) => {
  await openSettings(page, "?aion&mailon");
  await page.getByRole("switch", { name: "Include important email in AI summaries" }).click();
  await page.getByRole("button", { name: "See what's sent" }).click();
  const dialog = page.getByRole("dialog", { name: "What is sent" });
  await expect(dialog.getByLabel("Exactly what will be sent")).toContainText('Important email from Priya Shah: "Agenda for Tuesday"');
});

test("turning AI off and removing the key", async ({ page }) => {
  await openSettings(page, "?aion");
  await page.getByRole("switch", { name: "AI summaries" }).click(); // off
  await expect(page.getByRole("switch", { name: "AI summaries" })).toHaveAttribute("aria-checked", "false");
  await page.getByRole("switch", { name: "AI summaries" }).click(); // asks again: consent was kept, but turning on re-confirms
  await expect(page.getByRole("dialog", { name: "Turn on AI summaries?" })).toBeVisible();
  await page.getByRole("dialog").getByRole("button", { name: "Cancel" }).click();
});

test("removing the key withdraws consent and turns AI off", async ({ page }) => {
  await openSettings(page, "?aion");
  await page.getByRole("button", { name: "Remove key and turn off" }).click();
  await expect(page.getByRole("switch", { name: "AI summaries" })).toHaveAttribute("aria-checked", "false");
  await expect(page.getByText("Key removed")).toBeVisible();
});
