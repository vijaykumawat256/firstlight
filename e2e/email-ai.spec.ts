import AxeBuilder from "@axe-core/playwright";
import { expect, test, type Page } from "@playwright/test";
import { settle } from "./settle";

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

/** Opens one page of settings ("Email" or "AI summaries") from the settings list. */
async function openSection(page: Page, name: string) {
  await page.getByRole("button", { name: "Settings" }).click();
  await page.getByRole("main").getByRole("button", { name: new RegExp(`^${name}`) }).click();
  await expect(page.getByRole("heading", { name, level: 1 })).toBeVisible();
}

async function openSettings(page: Page, query = "", section = "Email") {
  await loadBriefing(page, query);
  await openSection(page, section);
}

/** Back to the briefing from a settings page. */
async function toBriefing(page: Page) {
  await page.getByRole("button", { name: "Settings", exact: true }).click();
  await page.getByRole("button", { name: "Briefing", exact: true }).click();
}

const calls = (page: Page, name: "__mailCalls" | "__aiCalls") => page.evaluate((n) => (globalThis as unknown as Record<string, number>)[n] ?? 0, name);

// ---------------------------------------------------------------------------------- email

test("email is off by default: no inbox, no mail requests, and only the master switch shows", async ({ page }) => {
  await loadBriefing(page);
  await expect(page.getByRole("region", { name: "Inbox" })).toHaveCount(0);
  expect(await calls(page, "__mailCalls")).toBe(0);

  await openSection(page, "Email");
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

  await toBriefing(page);
  const inbox = page.getByRole("region", { name: "Inbox" });
  await expect(inbox).toBeVisible();
  await expect(inbox).toContainText("37 unread");
  await expect(inbox.getByRole("list", { name: "Invitations" })).toContainText("Roadmap review");
  await expect(inbox.getByRole("list", { name: "Invitations" })).toContainText("Invitation from Jordan Lee");
  await expect(inbox.getByRole("list", { name: "Important email" })).toContainText("Priya Shah");
  await expect(inbox).toContainText("and 34 more unread");
  await inbox.getByRole("button", { name: /^(?!Make a task).*Agenda for Tuesday/ }).click(); // opens in the user's own app; must not throw
  await page.screenshot({ path: `${SHOTS}/inbox-light.png`, fullPage: true });
});

test("a refused Mail permission is explained", async ({ page }) => {
  await openSettings(page, "?macdenied");
  await page.getByRole("switch", { name: "Show email in the briefing" }).click();
  await page.getByRole("button", { name: "Connect", exact: true }).click();
  await expect(page.getByText("Couldn't connect Mail").first()).toBeVisible();
  await expect(page.getByText(/switch Mail on under Firstlight/).first()).toBeVisible();
  await expect(page.getByRole("button", { name: "Connect", exact: true })).toBeVisible(); // still not connected
});

test("inbox problems say what to do", async ({ page }) => {
  await loadBriefing(page, "?mailon&mailproblem=permission");
  const inbox = page.getByRole("region", { name: "Inbox" });
  await expect(inbox).toContainText("Allow Firstlight to control Mail");
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
  await openSection(page, "AI summaries");
  await expect(page.getByRole("switch", { name: "AI summaries" })).toHaveAttribute("aria-checked", "false");
  expect(await calls(page, "__aiCalls")).toBe(0);
});

test("turning on AI shows exactly what is sent and needs a key and agreement", async ({ page }) => {
  await openSettings(page, "", "AI summaries");
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
  await expect(page.getByText("Claude (Anthropic) · claude-haiku-5-5")).toBeVisible();
});

test("cancelling the agreement changes nothing", async ({ page }) => {
  await openSettings(page, "", "AI summaries");
  await page.getByRole("switch", { name: "AI summaries" }).click();
  await page.getByRole("dialog").getByRole("button", { name: "Cancel" }).click();
  await expect(page.getByRole("switch", { name: "AI summaries" })).toHaveAttribute("aria-checked", "false");
  expect(await calls(page, "__aiCalls")).toBe(0);
});

test("a local model needs no key and says it stays on this device", async ({ page }) => {
  await openSettings(page, "", "AI summaries");
  await page.getByRole("switch", { name: "AI summaries" }).click();
  const dialog = page.getByRole("dialog", { name: "Turn on AI summaries?" });
  await dialog.getByRole("combobox", { name: "Provider" }).click();
  await page.getByRole("option", { name: "A model on this device" }).click();
  await expect(dialog).toContainText("runs on this device");
  await expect(dialog.getByLabel("Model address")).toHaveValue("http://127.0.0.1:11434");
  await expect(dialog.getByLabel("Anthropic API key")).toHaveCount(0);
  await dialog.getByRole("button", { name: "Turn on" }).click();
  await expect(page.getByText("A model on this device · llama3.2")).toBeVisible();
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
  await openSettings(page, "?aion&mailon", "AI summaries");
  await page.getByRole("switch", { name: "Include important email in AI summaries" }).click();
  await page.getByRole("button", { name: "See what's sent" }).click();
  const dialog = page.getByRole("dialog", { name: "What is sent" });
  await expect(dialog.getByLabel("Exactly what will be sent")).toContainText('Important email from Priya Shah: "Agenda for Tuesday"');
});

test("turning AI off and removing the key", async ({ page }) => {
  await openSettings(page, "?aion", "AI summaries");
  await page.getByRole("switch", { name: "AI summaries" }).click(); // off
  await expect(page.getByRole("switch", { name: "AI summaries" })).toHaveAttribute("aria-checked", "false");
  await page.getByRole("switch", { name: "AI summaries" }).click(); // asks again: consent was kept, but turning on re-confirms
  await expect(page.getByRole("dialog", { name: "Turn on AI summaries?" })).toBeVisible();
  await page.getByRole("dialog").getByRole("button", { name: "Cancel" }).click();
});

test("removing the key withdraws consent and turns AI off", async ({ page }) => {
  await openSettings(page, "?aion", "AI summaries");
  await page.getByRole("button", { name: "Remove key and turn off" }).click();
  await expect(page.getByRole("switch", { name: "AI summaries" })).toHaveAttribute("aria-checked", "false");
  await expect(page.getByText("Key removed").first()).toBeVisible();
});

// ---------------------------------------------------------------------------------- Yahoo / iCloud (IMAP)

test("add and remove a Yahoo mailbox; the password never reaches the settings", async ({ page }) => {
  await openSettings(page, "?mailon");
  const email = page.getByRole("region", { name: "Email", exact: true });
  await email.getByRole("button", { name: "Add mailbox…" }).click();
  const dialog = page.getByRole("dialog", { name: "Add a mailbox" });
  await expect(dialog.getByRole("button", { name: "Connect" })).toBeDisabled();

  await dialog.getByLabel("Email address").fill("sam@yahoo.com");
  await dialog.getByLabel("App-specific password").fill("abcd efgh ijkl mnop");
  await expect(dialog.getByLabel("App-specific password")).toHaveAttribute("type", "password");
  await dialog.getByRole("button", { name: "Connect" }).click();

  await expect(dialog).toHaveCount(0);
  await expect(email.getByText("sam@yahoo.com · reading unread mail.")).toBeVisible();
  const stored = await page.evaluate(() => JSON.stringify(window.localStorage) + JSON.stringify(window.sessionStorage));
  expect(stored).not.toContain("abcd");

  await email.getByRole("button", { name: "Remove sam@yahoo.com" }).click();
  await expect(email.getByText("sam@yahoo.com · reading unread mail.")).toHaveCount(0);
});

test("iCloud shows its own password instructions, and a refused login explains itself", async ({ page }) => {
  await openSettings(page, "?mailon&imapbad");
  const email = page.getByRole("region", { name: "Email", exact: true });
  await email.getByRole("button", { name: "Add mailbox…" }).click();
  const dialog = page.getByRole("dialog", { name: "Add a mailbox" });
  await dialog.getByRole("radio", { name: "iCloud Mail" }).click();
  await expect(dialog.getByText(/account\.apple\.com/)).toBeVisible();
  await dialog.getByLabel("Email address").fill("sam@icloud.com");
  await dialog.getByLabel("App-specific password").fill("wxyz-wxyz-wxyz-wxyz");
  await dialog.getByRole("button", { name: "Connect" }).click();
  await expect(dialog.getByRole("alert")).toContainText("iCloud Mail didn't accept that");
  await expect(email.getByText("sam@icloud.com")).toHaveCount(0);
});

test("any other provider can be added with its server name, and a bad server name is refused", async ({ page }) => {
  await openSettings(page, "?mailon");
  const email = page.getByRole("region", { name: "Email", exact: true });
  await email.getByRole("button", { name: "Add mailbox…" }).click();
  const dialog = page.getByRole("dialog", { name: "Add a mailbox" });
  await expect(dialog.getByLabel("Mail server")).toHaveCount(0); // only for "Other"
  await dialog.getByRole("radio", { name: "Other" }).click();
  await expect(dialog.getByLabel("Mail server")).toBeVisible();
  await expect(dialog.getByRole("button", { name: "Connect" })).toBeDisabled();

  await dialog.getByLabel("Email address").fill("sam@fastmail.com");
  await dialog.getByLabel("App-specific password").fill("abcd-efgh");
  await dialog.getByLabel("Mail server").fill("127.0.0.1");
  await dialog.getByRole("button", { name: "Connect" }).click();
  await expect(dialog.getByRole("alert")).toContainText("mail server's name");

  await dialog.getByLabel("Mail server").fill("imap.fastmail.com");
  await dialog.getByRole("button", { name: "Connect" }).click();
  await expect(dialog).toHaveCount(0);
  await expect(email.getByText("sam@fastmail.com (imap.fastmail.com) · reading unread mail.")).toBeVisible();
  await email.getByRole("button", { name: "Remove sam@fastmail.com" }).click();
  await expect(email.getByText(/imap\.fastmail\.com/)).toHaveCount(0);
});

// ---------------------------------------------------------------------------------- other AI services

async function openConsent(page: Page, query = "") {
  await openSettings(page, query, "AI summaries");
  await page.getByRole("switch", { name: "AI summaries" }).click();
  const dialog = page.getByRole("dialog", { name: "Turn on AI summaries?" });
  await expect(dialog).toBeVisible();
  return dialog;
}

async function chooseProvider(page: Page, dialog: ReturnType<Page["getByRole"]>, name: string) {
  await dialog.getByRole("combobox", { name: "Provider" }).click();
  await page.getByRole("option", { name }).click();
}

test("OpenAI and Grok work with just an API key, each saying where the summary goes", async ({ page }) => {
  const dialog = await openConsent(page);
  await chooseProvider(page, dialog, "ChatGPT models (OpenAI)");
  await expect(dialog).toContainText("OpenAI's servers (api.openai.com)");
  await expect(dialog.getByLabel("OpenAI API key")).toBeVisible();
  await expect(dialog.getByLabel("Anthropic API key")).toHaveCount(0);
  await expect(dialog.getByLabel("Model", { exact: true })).toHaveValue("gpt-4o-mini");
  await expect(dialog.getByRole("button", { name: "Turn on" })).toBeDisabled(); // needs a key

  // Switching provider changes the destination, the key field and the starting model.
  await chooseProvider(page, dialog, "Grok (xAI)");
  await expect(dialog).toContainText("xAI's servers (api.x.ai)");
  await expect(dialog.getByLabel("Model", { exact: true })).toHaveValue("grok-3-mini");
  await dialog.getByLabel("xAI API key").fill("xai-a-good-looking-key");
  await dialog.getByRole("button", { name: "Turn on" }).click();
  await expect(page.getByText("Grok (xAI) · grok-3-mini")).toBeVisible();
  const on = await page.evaluate(() => (window as unknown as { __aiEnabledFor: { provider: string; model: string } }).__aiEnabledFor);
  expect(on).toEqual({ provider: "xai", model: "grok-3-mini" });
  // The key is saved for that provider only, and can be removed.
  await expect(page.getByText("Saved in the system keychain, for Grok (xAI).")).toBeVisible();
});

test("a rejected key is explained and nothing is turned on", async ({ page }) => {
  const dialog = await openConsent(page);
  await chooseProvider(page, dialog, "ChatGPT models (OpenAI)");
  await dialog.getByLabel("OpenAI API key").fill("rejected-key-0000");
  await dialog.getByRole("button", { name: "Turn on" }).click();
  await expect(dialog.getByRole("alert")).toContainText("rejected the key");
  await dialog.getByRole("button", { name: "Cancel" }).click();
  await expect(page.getByRole("switch", { name: "AI summaries" })).toHaveAttribute("aria-checked", "false");
});

test("another OpenAI-compatible service needs an https address, and the agreement names that host", async ({ page }) => {
  const dialog = await openConsent(page);
  await chooseProvider(page, dialog, "Another service (OpenAI-compatible)");
  await dialog.getByLabel("API key", { exact: true }).fill("my-private-key-123");
  const turnOn = dialog.getByRole("button", { name: "Turn on" });
  await expect(turnOn).toBeDisabled(); // no address yet
  await expect(dialog).toContainText("the service whose address you enter below");

  await dialog.getByLabel("Service address").fill("http://insecure.example.com/v1");
  await expect(turnOn).toBeDisabled(); // https only
  await dialog.getByLabel("Service address").fill("https://llm.example.com/v1");
  await expect(dialog).toContainText("The service's servers (llm.example.com)");
  await expect(turnOn).toBeEnabled();
  await turnOn.click();
  await expect(page.getByText("Another service (OpenAI-compatible) · gpt-4o-mini")).toBeVisible();
});

test("OpenRouter can be used by signing in, with no key to paste", async ({ page }) => {
  const dialog = await openConsent(page);
  await chooseProvider(page, dialog, "OpenRouter (many models)");
  await expect(dialog).toContainText("OpenRouter's servers (openrouter.ai)");
  const turnOn = dialog.getByRole("button", { name: "Turn on" });
  await expect(turnOn).toBeDisabled();
  await dialog.getByRole("button", { name: "Sign in with OpenRouter" }).click();
  await expect(page.getByText("Signed in to OpenRouter").first()).toBeVisible();
  await expect(dialog.getByLabel("OpenRouter API key")).toHaveAttribute("placeholder", /already saved/);
  await expect(turnOn).toBeEnabled();
  // Signing in only fetched a key: AI is still off until the person agrees to what is shown.
  expect(await page.evaluate(() => (window as unknown as Record<string, unknown>).__aiEnabledFor)).toBeUndefined();
  await turnOn.click();
  await expect(page.getByText("OpenRouter (many models) · openai/gpt-4o-mini")).toBeVisible();
});

test("a failed OpenRouter sign-in explains itself", async ({ page }) => {
  const dialog = await openConsent(page, "?orfail");
  await chooseProvider(page, dialog, "OpenRouter (many models)");
  await dialog.getByRole("button", { name: "Sign in with OpenRouter" }).click();
  await expect(dialog.getByRole("alert")).toContainText("didn't accept that sign-in");
  await expect(dialog.getByRole("button", { name: "Turn on" })).toBeDisabled();
});

test("the other AI services have no accessibility problems", async ({ page }) => {
  const dialog = await openConsent(page);
  await chooseProvider(page, dialog, "OpenRouter (many models)");
  await settle(page);
  const r = await new AxeBuilder({ page }).withTags(["wcag2a", "wcag2aa", "wcag21a", "wcag21aa", "wcag22aa"]).analyze();
  expect(r.violations.map((v) => `${v.id}: ${v.nodes[0]?.target.join(" ")}`)).toEqual([]);
});

test("on Windows, Outlook's inbox can be read without signing in, and a failure says why", async ({ page }) => {
  await openSettings(page, "?outlook&outlookfail");
  await page.getByRole("switch", { name: "Show email in the briefing" }).click();
  await expect(page.getByText("Senders and previews aren't read")).toBeVisible();
  await expect(page.getByText("Mail app", { exact: true })).toHaveCount(0);
  await page.getByRole("button", { name: "Connect" }).first().click();
  await expect(page.getByText("Couldn't read Outlook").first()).toBeVisible();

  await openSettings(page, "?outlook");
  await page.getByRole("switch", { name: "Show email in the briefing" }).click();
  await page.getByRole("button", { name: "Connect" }).first().click();
  await expect(page.getByRole("switch", { name: "Read Outlook" })).toBeChecked();
  await expect(page.getByText("Reading unread mail and meeting invitations from Outlook")).toBeVisible();
});
