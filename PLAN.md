# Firstlight — Build Plan

> The app is called Firstlight. Keep the name in one constant so it is easy to change.

This document is the full brief for building the app. It is written for an AI coding agent working in this folder. Read all of sections 1–8 before writing code, then work through the phases in section 9 in order.

---

## 1. What we are building

A desktop app (macOS first, Windows second) that:

1. Starts with the computer and reacts when the user unlocks it.
2. Shows a **daily briefing**: what is left today, what is coming tomorrow, conflicts, and reminders.
3. Pulls calendars from many sources into one view: Google, Outlook/Microsoft 365, Yahoo, iCloud, any CalDAV server, ICS links, and the calendars already set up in the operating system.
4. Later adds an email digest, optional AI summaries, and an optional account so the same setup appears on other devices and in a browser.

Everything the user can reasonably want to change is a setting.

### Product principles

- **The briefing is the product.** The full calendar view is secondary. Every decision should make "what do I need to know right now" faster to read.
- **Local-first.** The app talks directly to the providers from the user's device. We run no server until Phase 7, and the desktop app must always work fully without one.
- **Read-only by default.** Write access is only requested if the user turns on editing (not in scope before Phase 7; do not build it earlier).
- **Sign in once.** After connecting an account the user should not be asked again unless the provider revokes access.
- **Never annoying.** The unlock behaviour is governed by rules (section 6). When in doubt, show less.

### Explicitly out of scope

- Reading other apps' data files (Outlook OST/PST, Apple Mail folders, browser profiles). Not reliable, not safe. Use the connection methods in section 5 only.
- A full email client (compose, folders, search). Email is a digest only.
- A native phone app. Phase 7 delivers a responsive web version instead.

---

## 2. Tech stack (decided — do not substitute without asking the user)

| Layer | Choice |
|---|---|
| App shell | Tauri 2 |
| Backend language | Rust (stable) |
| UI | React 18+ with TypeScript (strict), Vite |
| Styling | Tailwind CSS v4, themed entirely from CSS-variable design tokens |
| Accessible primitives | Radix UI |
| Icons | Lucide (do not use SF Symbols; their licence does not cover Windows or web) |
| UI state | Zustand for app state, TanStack Query for data from the backend |
| Local database | SQLite with SQLCipher (`rusqlite` with the `bundled-sqlcipher` feature) |
| Secrets | OS keychain through the `keyring` crate (Keychain on macOS, Credential Manager on Windows) |
| HTTP | `reqwest` with rustls |
| OAuth | `oauth2` crate, authorization-code flow with PKCE |
| Calendar parsing | `icalendar` for ICS, `rrule` for recurrence, `chrono` + `chrono-tz` for time |
| Tauri plugins | autostart, single-instance, notification, updater, opener, window-state, deep-link |
| Tests | `cargo test`, Vitest + Testing Library, Playwright for a few end-to-end flows |

---

## 3. Architecture

```
calendar/
├── PLAN.md
├── CLAUDE.md                  # short working notes for agents (create in Phase 1)
├── .env.example               # OAuth client IDs; real .env is gitignored
├── crates/
│   └── core/                  # pure Rust, NO Tauri dependency
│       ├── model/             # Account, Calendar, Event, Briefing types
│       ├── providers/         # one module per provider, all behind one trait
│       ├── sync/              # scheduler, incremental sync, dedupe
│       ├── briefing/          # rule-based briefing engine (pure functions)
│       ├── store/             # database access, migrations
│       └── secrets/           # trait; keychain implementation lives in the shell
├── src-tauri/                 # thin shell: windows, tray, autostart, unlock, commands
└── src/                       # React UI
    ├── design/                # tokens.css, primitives (Button, Card, ...)
    ├── features/              # briefing, calendar, onboarding, settings, email
    └── api/                   # typed interface to the backend
```

Rules that keep Phase 7 (web version) possible:

1. **`crates/core` never imports Tauri.** Storage, secrets and HTTP are passed in through traits.
2. **The UI never calls Tauri directly.** All backend calls go through `src/api`, a typed interface with one Tauri implementation now and a web implementation later.
3. **The briefing engine is pure.** Input: events, the current time, settings. Output: a briefing struct. No I/O, so it is trivially testable.
4. **A connected account is a portable record** (provider, method, display name, settings, a reference to where its secret lives). It must not assume the secret is in this machine's keychain.

### Provider trait

Every source implements the same interface:

- `list_calendars(account) -> Vec<Calendar>`
- `sync_events(calendar, sync_state, window) -> SyncResult` (changed events, deleted IDs, new sync state)
- `capabilities()` (supports incremental sync, supports email, needs polling, ...)

Include a **fixture provider** from Phase 1 that serves realistic fake data (recurring meetings, all-day events, conflicts, different timezones, a cancelled event). All UI work and tests use it, so development never blocks on real credentials.

### Data model (SQLite)

- `accounts` — id, provider, auth_method, display_name, email, status, secret_ref, created_at
- `calendars` — id, account_id, remote_id, name, colour, visible, read_only
- `events` — id, calendar_id, remote_id, ical_uid, title, start_utc, end_utc, start_tzid, all_day, location, description, join_url, status, my_response, organizer, recurrence_master_id, updated_at
- `sync_state` — calendar_id, token, last_synced_at, last_error
- `settings` — key, JSON value
- `briefing_log` — date, kind, shown_at (drives the "first unlock of the day" rule)

Decisions:

- Store instants in UTC plus the original timezone ID. All-day events are stored as dates, never as midnight timestamps.
- Store expanded instances for a rolling window (30 days back, 90 days forward). Expand ICS/CalDAV recurrence with the `rrule` crate; Google and Microsoft can return instances already expanded.
- **Dedupe**: the same meeting often appears in two accounts. Treat events with the same `ical_uid` and start time as one, and show which calendars it came from.
- **Join links**: extract from the provider's conference field first, then by pattern from location and description (Zoom, Google Meet, Teams, Webex). Only `https` links are ever opened.

---

## 4. Security requirements (apply from Phase 1, not at the end)

Every item here is a requirement, and each phase's work is not done if it breaks one.

1. **No passwords for OAuth providers.** Sign-in opens the system browser, uses PKCE, and returns through a loopback redirect on `127.0.0.1` with a random port and a checked `state` value. Never use an embedded webview for sign-in.
2. **Secrets only in the OS keychain**: refresh tokens, app passwords, the database key, any AI API key. Never in SQLite, config files, logs, or the frontend's memory longer than needed.
3. **Encrypted database.** Generate a random 32-byte key on first run, store it in the keychain, open SQLite through SQLCipher with it.
4. **Least privilege scopes.** Request read-only calendar scopes at connect time. Request mail scopes only when the user enables the email digest.
5. **Tauri capabilities are minimal.** Each window gets only the commands it needs. No general shell or filesystem access from the UI.
6. **Strict Content Security Policy.** No remote scripts, no inline scripts, no `eval`. The UI loads no remote resources except through the Rust backend.
7. **Untrusted content stays inert.** Event descriptions and email content are rendered as plain text. If HTML must ever be shown, sanitise it and block remote images.
8. **Validate everything crossing the UI/backend boundary.** Every Tauri command validates its input in Rust.
9. **No content in logs.** Logs may contain IDs, counts and error codes. Never titles, attendees, email subjects or tokens. Add a redaction test.
10. **Tokens refresh safely.** Persist rotated refresh tokens immediately (Microsoft rotates them). On a revoked token, mark the account "needs attention" and show one clear prompt; never loop.
11. **Supply chain.** `cargo audit`, `cargo deny` and `npm audit` run in CI. Lockfiles are committed. Add new dependencies sparingly and say why.
12. **Signed distribution** (Phase 6): signed and notarised macOS builds with hardened runtime, signed Windows installer, signed updates through the Tauri updater.
13. **AI is opt-in.** Nothing is sent to any AI service unless the user turns it on and sees exactly what is shared.
14. **Never commit secrets.** `.env` is gitignored from the first commit.

Write a short `SECURITY.md` threat model in Phase 1 (what we protect, from whom, how) and keep it current.

---

## 5. Ways to connect a calendar

| Method | Covers | How |
|---|---|---|
| Google sign-in | Gmail / Google Workspace | OAuth (desktop client), Calendar API, incremental sync with sync tokens |
| Microsoft sign-in | Outlook.com, Microsoft 365 | OAuth (public client), Microsoft Graph `calendarView` delta queries |
| CalDAV + app password | Yahoo, iCloud, Fastmail, Nextcloud, others | Server discovery, `REPORT` queries, ctag/etag change detection |
| ICS link | Any published calendar | Fetch on a timer with `ETag` / `If-Modified-Since` |
| System calendar | Whatever is set up in the OS | macOS EventKit (via `objc2-event-kit`), with the calendar permission prompt |

Notes for the agent:

- **Google desktop clients are issued a "client secret" that Google does not treat as confidential.** It is sent in the token exchange and may ship in the app. PKCE is still required.
- Google only returns a refresh token on first consent; request offline access.
- While the Google app is in "testing" status, refresh tokens expire after 7 days. This is expected during development.
- Provide presets for Yahoo and iCloud CalDAV (server address filled in, with a link explaining how to create an app password).
- **Windows system calendar is unverified.** The Windows Appointments API may need a packaged app identity. Investigate in Phase 6; if it needs packaging we do not otherwise want, drop it and say so.
- If a sync token is rejected as expired, do a clean full resync of that calendar.

---

## 6. Launch and unlock behaviour

- **Start at login** through the autostart plugin, launched hidden with a tray/menu-bar icon. Single instance only.
- **Unlock detection** lives in the Tauri shell:
  - macOS: observe the distributed notification `com.apple.screenIsUnlocked` (plus wake notifications as a fallback).
  - Windows: register for session notifications and handle the session-unlock message.
- **Rules (all configurable, these are the defaults):**
  - First unlock of the day after a set time (default 06:00): open the full briefing window.
  - Later unlocks: do nothing, unless an event starts within 15 minutes, then show a compact "up next" panel.
  - Minimum 30 minutes between automatic appearances.
  - Never appear during quiet hours, on days the user disabled, or when another app is in full screen.
  - The user can always open the briefing from the tray icon or a global shortcut.
- The window must appear in under 500 ms from cached data, then refresh in the background.

---

## 7. Design system

The goal is a calm, native-feeling app in the style of Apple's own apps. Build the system first (Phase 1) and build every screen only from it.

- **Tokens** in `src/design/tokens.css` as CSS variables: colour (semantic names such as `--bg`, `--surface`, `--text`, `--text-secondary`, `--accent`, `--danger`), spacing on a 4 px scale, radii, shadows, type scale, motion durations and easings. Tailwind's theme maps to these. No raw hex values or pixel sizes in components.
- **Type**: system font stack (San Francisco on macOS, Segoe UI Variable on Windows). A small scale: large title, title, headline, body, caption. Use tabular numbers for times.
- **Colour**: light and dark themes from the same tokens, following the system setting with a manual override. Follow the system accent colour where available. Calendar colours come from a fixed, tested palette that works in both themes.
- **Materials**: translucent window background on macOS (vibrancy) and Mica on Windows where supported, with a solid fallback.
- **Motion**: short and purposeful (150–250 ms). Respect the reduced-motion setting.
- **Layout**: generous spacing, one primary action per screen, no dense toolbars.
- **Primitives** to build once and reuse: Button, IconButton, Card, List row, Switch, Select, Segmented control, Text field, Dialog, Sheet, Tooltip, Toast, Badge, Empty state, Skeleton.
- **Every screen has** loading, empty, offline and error states designed, not just the happy path.
- **Accessibility**: full keyboard navigation, visible focus rings, screen-reader labels, contrast at WCAG AA or better.
- Copy is short and plain. Times follow the user's locale and 12/24-hour setting.

Key screens: Onboarding, Briefing (main), Up-next compact panel, Calendar (day and week), Event detail, Accounts, Settings.

### Visual direction (concrete starting values)

The reference points are Apple Calendar, Reminders and Weather on current macOS, plus Things 3, Linear and Raycast for restraint and polish. Modern here means quiet: content first, almost no chrome, depth from materials and soft shadow instead of borders.

Starting tokens (tune by eye, keep the relationships):

| Token | Light | Dark |
|---|---|---|
| `--bg` (window) | `#F5F5F7` | `#1C1C1E` |
| `--surface` (cards) | `#FFFFFF` | `#2C2C2E` |
| `--surface-raised` | `#FFFFFF` | `#3A3A3C` |
| `--text` | `#1D1D1F` | `#F5F5F7` |
| `--text-secondary` | `#6E6E73` | `#98989D` |
| `--separator` | `rgba(0,0,0,0.08)` | `rgba(255,255,255,0.10)` |
| `--accent` (fallback) | `#007AFF` | `#0A84FF` |
| `--danger` | `#FF3B30` | `#FF453A` |

- **Type scale** (px / weight): caption 11 / 400, footnote 12 / 400, body 13 / 400, headline 15 / 600, title 20 / 600, large title 28 / 700. The briefing greeting may go to 34 / 700. Tight letter-spacing on the large sizes, default on body.
- **Radii**: 6 for controls, 10 for cards and rows, 14 for sheets and dialogs. Nested radii shrink by the padding between them.
- **Spacing**: 4, 8, 12, 16, 24, 32, 48. Card padding 16; gaps between sections 24–32.
- **Shadows**: one soft, low shadow for raised surfaces (around `0 1px 2px` plus `0 8px 24px` at very low opacity). In dark mode rely on the lighter surface colour, not shadow.
- **Motion**: 200 ms with `cubic-bezier(0.2, 0, 0, 1)` for most transitions; a gentle spring for sheets and the briefing window appearing. Lists animate insertions and removals. Nothing bounces for attention.
- **Calendar colours**: red, orange, yellow, green, teal, blue, indigo, purple, pink, brown, in the system hues, each with a dark-mode variant. Events show the colour as a thin leading bar or a soft tint, never a saturated full block.

### Briefing screen layout

This screen sets the tone for the whole app, so build it with the most care.

1. **Greeting and date**: large title, with the one or two sentence summary under it in secondary text.
2. **Up next**: a single prominent card with title, time, countdown, location, and the Join button as the only filled button on the screen.
3. **Rest of today**: a clean timeline list. Time in tabular figures on the left, thin colour bar, title, secondary line. Past events dimmed. A "now" marker.
4. **Tomorrow**: a quieter, smaller section.
5. **Needs attention**: conflicts and unanswered invitations, shown only when there are any.

The whole briefing should fit without scrolling on a normal day. Default window is narrow and tall (around 420 × 640), centred or near the menu bar, with a translucent background.

### What to avoid

- Gradients as decoration, glows, neon accents, and purple-to-blue "AI" styling.
- Borders around everything. Use spacing, surface colour and separators.
- More than one accent colour, or accent used for anything other than interactive and "now" elements.
- Emoji as icons, mixed icon sets, or icons at inconsistent sizes and stroke weights.
- Dense tables, heavy toolbars, modal dialogs for things a sheet or inline control can do.
- Custom-styled scrollbars, custom title bars that break native window behaviour, non-native context menus.
- Web-looking patterns: hover-lift cards, big rounded pill buttons everywhere, hero sections, centred marketing copy.
- Layout shift while data loads. Use skeletons sized like the final content.

### Design review

At the end of every phase that changes the UI, take screenshots of each affected screen in light and dark themes and at the smallest supported window size, look at them critically against this section, and fix what falls short before calling the phase done. Show the screenshots to the user in the phase summary.

---

## 8. Working rules for the agent

1. Work one phase at a time, in order. A phase is finished only when every "Done when" item passes.
2. At the end of each phase: run all checks, update `CLAUDE.md` with anything a future session needs, and give the user a short summary including anything not finished.
3. Before each commit these must pass: `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test`, `tsc --noEmit`, `eslint`, `vitest run`.
4. Write tests with the code, not after. Recurrence, timezones, dedupe and the briefing engine need thorough unit tests, including daylight-saving changes and events crossing midnight.
5. Use the fixture provider for UI and tests. Never put real account data in tests or commits.
6. **Stop and ask the user** when you need: OAuth client IDs, a developer account, a signing certificate, a choice this plan does not make, or a change to the tech stack.
7. Do not build ahead. No email code before Phase 5, no server code before Phase 7.
8. Report honestly. If something is stubbed, flaky or untested on Windows, say so.

### Things only the user can do (ask when the phase needs them)

- Phase 2: create a Google Cloud project with a "Desktop app" OAuth client; register an app in Microsoft Entra (public client, personal and work accounts, redirect `http://localhost`).
- Phase 6: Apple Developer Program membership, a Windows code-signing certificate, a privacy policy page, Google OAuth verification (plus a third-party security assessment if Gmail scopes are requested).
- Phase 7: a domain and hosting for the sync service.

---

## 9. Phases

### Phase 1 — Foundation

Goal: a running, secure, good-looking shell with fake data.

- `git init`, `.gitignore`, workspace layout from section 3, `CLAUDE.md`, `SECURITY.md`, `.env.example`.
- Tauri 2 + React + TypeScript + Tailwind scaffold; strict CSP; minimal capabilities.
- CI (GitHub Actions) on macOS and Windows: format, lint, tests, audits.
- Design tokens, light/dark themes, and all primitives from section 7, with a simple gallery page for reviewing them.
- Encrypted database with migrations; keychain wrapper; database key generated and stored on first run.
- `crates/core` with the model, the provider trait, and the fixture provider.
- Typed `src/api` layer.
- Tray/menu-bar icon, single instance, start-at-login toggle, hide-to-tray on close.
- Unlock detection on macOS emitting an event to the app (Windows implementation can be stubbed behind the same interface until Phase 6).
- A first Briefing screen showing today's fixture events.

Done when:
- The app builds and runs on macOS; CI is green on macOS and Windows.
- Relaunching at login works and unlocking the Mac produces a logged unlock event.
- The database file is unreadable without the key, and no secret exists outside the keychain.
- Briefing screen renders fixture data in light and dark themes, navigable by keyboard.
- The briefing screen follows the layout in section 7 and passes the design review there; the user has seen the screenshots and approved the look before Phase 2 starts.

### Phase 2 — Calendars

Goal: real calendars, synced reliably, signed in once.

- OAuth flow (system browser, PKCE, loopback) shared by Google and Microsoft.
- Google provider and Microsoft provider with incremental sync.
- ICS link provider.
- macOS system calendar provider (EventKit) with the permission prompt.
- CalDAV provider with Yahoo and iCloud presets.
- Sync scheduler: on launch, on unlock, on a timer (default 10 minutes), manual refresh; backoff on errors; offline detection.
- Token refresh and rotation; "needs attention" state for revoked accounts.
- Recurrence expansion, timezone handling, all-day events, dedupe, join-link extraction.
- Onboarding flow and Accounts screen: add, rename, choose visible calendars, pick colours, remove (removal deletes cached data and the keychain entry).

Done when:
- A Google account and a Microsoft account can each be connected in under a minute and stay connected across restarts with no further prompts.
- Changes made in the provider appear after the next sync; deletions disappear.
- A meeting present in two accounts shows once.
- Tests cover recurrence exceptions, daylight-saving transitions, all-day and multi-day events, expired sync tokens, and revoked tokens.
- The app works offline from cache and recovers when the network returns.

### Phase 3 — Briefing

Goal: the screen people open the app for.

- Briefing engine (pure, rule-based) producing:
  - a one or two sentence summary of the rest of today;
  - remaining events with the next one highlighted and a countdown;
  - conflicts and back-to-back runs;
  - free gaps of a useful length;
  - tomorrow: first event, count, anything unusually early, all-day items;
  - items needing a response.
- Declined events hidden and tentative ones marked, by default.
- Briefing window and compact "up next" panel, driven by the unlock rules in section 6.
- "Join" button for meetings with a link.
- Notifications: before events (default 10 minutes), a morning briefing, an evening look-ahead for tomorrow.
- Day and week calendar views and an event detail view.

Done when:
- First unlock of the day opens the briefing; later unlocks follow the rules and never appear more often than the minimum interval.
- The briefing appears in under 500 ms from cache.
- Engine tests cover: empty day, day already over, overlapping events, events crossing midnight, all-day only, tomorrow empty.
- Notifications fire on time and are not duplicated after a restart.

### Phase 4 — Settings

Goal: everything configurable, with sensible defaults so nobody has to configure anything.

- Settings screen in sections: General, Briefing, Unlock behaviour, Notifications, Calendars, Appearance, Privacy.
- Covers: start at login, unlock rules, quiet hours, working hours and days, which sections the briefing shows, notification timing, sync frequency, theme, accent, 12/24-hour time, week start, default view, global shortcut, hidden calendars.
- Settings are typed, validated, versioned, and migrate safely.
- Export and import settings (never secrets). "Erase all data" option that wipes the database and keychain entries.

Done when:
- Every default in sections 6 and 9 can be changed in the UI and takes effect without a restart.
- A corrupt or old settings value falls back to the default instead of crashing.
- Erase leaves nothing behind in the database or keychain.

### Phase 5 — Email digest and optional AI summaries

Goal: a little email awareness in the briefing, and a smarter summary for those who want it.

- Email is off by default. Turning it on requests the mail scope at that moment.
- Sources: Gmail API, Microsoft Graph mail, IMAP for Yahoo/iCloud/others.
- Digest only: unread count, messages from important senders (user-defined list plus people in today's meetings), messages that look like invitations. Sender, subject, short plain-text snippet. Clicking opens the message in the user's own mail app or webmail.
- No HTML rendering, no remote images, no message bodies stored beyond the snippet.
- AI summaries, off by default, behind a provider interface: the user supplies their own API key (kept in the keychain) or points at a local model. Show exactly what will be sent before enabling. Fall back to the rule-based summary on any failure.

Done when:
- With email off, no mail scope is requested and no mail code runs.
- The digest appears in the briefing and respects the important-senders list.
- With AI off, there are no network calls to any AI service (assert this in a test).
- AI failure or timeout never delays or breaks the briefing.

### Phase 6 — Release

Goal: something that can be handed to other people.

- Windows parity: unlock detection, autostart, Mica, Credential Manager, installer; decide on the Windows system calendar (section 5).
- Code signing and notarisation on macOS; signed installer on Windows; signed auto-updates with a release channel.
- Opt-in crash reporting with all content stripped.
- Performance pass: cold start, memory when idle in the tray, battery impact of sync.
- Accessibility pass with a screen reader on both platforms.
- Security review against section 4 and `SECURITY.md`; fix everything found.
- First-run experience, in-app help, privacy policy link, licences page.
- Release checklist document.

Done when:
- A fresh macOS and a fresh Windows machine can install, connect an account, and receive an update without security warnings.
- Idle memory and CPU are within targets agreed with the user.
- No open high-severity findings from the security review.

### Phase 7 — Optional account, encrypted sync, and web version

Goal: sign in once to Firstlight and see the same setup on another device or in a browser.

**Before writing code, produce a design document and get the user's approval.** This phase introduces a server and changes the threat model.

Direction already agreed:

- The account is optional. The desktop app keeps working without it.
- Sign-in to Firstlight uses passkeys.
- **End-to-end encrypted vault.** Connected accounts, their tokens and settings are encrypted on the device with a key the server never sees. The server stores only ciphertext. Include a recovery key for a lost device.
- The web version reuses the React UI through a web implementation of `src/api`.
- Google and Microsoft can be called directly from the browser. CalDAV and IMAP cannot, so they need a small relay; design it so it cannot read or store credentials at rest.

The design document must cover: key derivation and recovery, device enrolment and revocation, what the server can and cannot see, the relay's trust model, how the core logic runs in the browser, rate limiting and abuse, data deletion, hosting, and cost.

Done when:
- A second device or browser shows the same accounts and settings after signing in, with no provider re-authentication.
- A dump of the server's storage reveals no tokens, calendar data or settings.
- Revoking a device cuts off its access.
- The desktop app behaves identically with no account.
