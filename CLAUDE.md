# Firstlight: working notes for agents

The app's name is **Firstlight** (renamed from the placeholder "Daybrief"). Only the display name changed: the bundle identifier (`com.daybrief.app`), keychain service, database file, crate names, `DAYBRIEF_*` env vars and the `daybrief-settings` export format keep the old word on purpose, so existing data keeps working. Changing the identifier is a separate decision (it orphans the keychain key and database).

The full brief is `PLAN.md`; read it first. This file records what is true *now* and what you'd otherwise rediscover.

## Status
Phases 1-5 built; Phase 6 built as far as it can be without the user's accounts (see `docs/RELEASE.md`: signing, notarisation, Windows run, live update test, external review remain). Phase 7 needs an approved design doc first. Do not build ahead.
Phase 5 delivered: email digest (`crates/core/src/mail`: Apple Mail via AppleScript, Gmail, Microsoft Graph), optional AI summaries (`crates/core/src/ai`: Anthropic with the user's key, or a local model), shell glue in `src-tauri/src/mail_ai.rs`, Inbox section in the briefing, Email and AI settings with a consent dialog that shows the exact text sent.
Phase 5 gaps: IMAP is built for Yahoo and iCloud only (`mail/imap`; other providers are deliberately not offered), but never run with a real login; "people in today's meetings" is approximated by meeting organiser names only (attendees are not stored); Gmail and Graph mail never ran against the live services; the Apple Mail AppleScript compiles against Mail's dictionary but has not been run (it needs the user's Automation permission).
Phase 4 delivered: one typed, versioned settings document (`crates/core/src/settings`), tolerant per-field loading + migration from the old scattered keys, Settings screen with all seven sections, live apply (theme, accent, clock, week start, default view, shortcut, sync interval, notifications, unlock rules), export/import, "Erase all data".
Phase 3 delivered: briefing engine, unlock rules, notifications, `get_briefing`, event detail, compact up-next window, day/week views.
Known gaps: Google/Microsoft incremental sync (deliberately not built until live accounts can verify it; see docs/NEXT.md). Notification click now brings the app forward (`on_action` in `lib.rs`) but has not been seen working: in `tauri dev` notifications come from the terminal app, so only a bundled build can show it.
Calendar app (macOS EventKit) provider: built; its permission prompt has not been clicked through yet.
Queued, not built: Apple Mail digest (unread, important senders, invitations) via AppleScript; planned with Phase 5.

## Commands
- Run app: `npm run tauri dev` (needs Rust: `. "$HOME/.cargo/env"`; each shell call starts without cargo on PATH)
- All checks: `npm run check` (types, lint, vitest) and `cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`
- Browser tests + screenshots: `npm run e2e` (writes `e2e/screenshots/`, gitignored). Uses the browser fixture backend, not Tauri.
- Design gallery (dev only): `http://localhost:1420/#/gallery`

## Layout and rules
- `crates/core`: no Tauri imports, ever. Sync `Provider` trait (call from worker threads).
- `src-tauri`: thin shell. New command = add to `build.rs` list, `capabilities/default.json` (`allow-<name>`), `generate_handler!`, `src/api`.
- UI talks to the outside only through `src/api` (`Backend`). `fixture.ts` is browser-only fake data mirroring the Rust fixture.
- The briefing logic lives in Rust (`crates/core/src/briefing`). `src/api/fixtureBriefing.ts` is a small browser-only stand-in for tests; do not grow it.
- Styling: tokens in `src/design/tokens.css`; no raw hex/px in components. Calendar colours are names (`calendarColour()`).

## Gotchas
- keyring is pinned to **3.x**; 4.x has a different API we haven't evaluated.
- `bundled-sqlcipher-vendored-openssl` is used so the build needs no system OpenSSL.
- macOS `sed -i` needs `-i ''`; prefer the Edit tool.
- `cargo add` fails silently-ish if the crate has no `lib.rs` yet.
- Tray icon is a template image (`icons/tray.png`); app icon source is `icons/source-1024.png` (regenerate with `npx tauri icon`).
- App runs as a macOS Accessory app (no Dock icon). Quit from the tray menu.

## Phase 2 notes
- Providers: `crates/core/src/providers/{fixture,ics,caldav,google,microsoft}`; add new ones to `src-tauri/src/providers.rs`.
- Sync: `crates/core/src/sync` (pure, tested) + `src-tauri/src/scheduler.rs` (thread, own DB connection, backoff).
- Adding an account always goes through `commands::connect` (keychain write, first sync, rollback on failure).
- OAuth client IDs come from `.env` at build time (`src-tauri/build.rs`); guide in `docs/OAUTH_SETUP.md`.
- Mock HTTP server for tests: `crates/core/src/http.rs` `testserver`. Live check: `cargo test -p daybrief-core live_ -- --ignored --nocapture`.
- Browser fixture backend flags: `?fresh` (no accounts), `?attention`, `?slowoauth`, `?oauthfail`, `?nomicrosoft`.
- Google/Microsoft sync fetch the whole window every time (no sync tokens/delta); revisit once live credentials allow verification.

## Not yet verified (be honest in summaries)
- Google and Microsoft sign-in against the LIVE services (mock-tested only; needs the user's client IDs).
- macOS Calendar-app provider: mapping logic is unit-tested with a mock source, and the real EventKit permission check runs (state was NotDetermined). The prompt itself, and reading real events, still need a human to click Allow in the running app. `src-tauri/Info.plist` is embedded in the dev binary (verified with otool).
- Real-window appearance (vibrancy/titlebar overlay): could not screenshot the desktop app.
- Start-at-login across an actual logout/login; toggling was not exercised.
- Windows: nothing built or run locally; CI covers it once pushed. Unlock detection is implemented (`src-tauri/src/unlock.rs`, WTSRegisterSessionNotification) but has never been compiled or run on Windows.
- Wake-from-sleep fallback for unlock detection is not implemented.

## Phase 3 notes
- Notification ids embed the date or start time and live in the `notifications_sent` table, so restarts never repeat one and a moved meeting gets a fresh heads-up.
- Heads-ups ignore quiet hours on purpose; the morning and evening summaries obey them.
- Notification times use 12/24h from the `notify` setting; the default is read from macOS (`AppleICUForce24HourTime`).
- Calendar views hide cancelled and declined events (same rule as the briefing). Hour rows use the wall clock, so daylight-saving days line up.
- Notifications in `tauri dev` appear to come from the terminal app; a signed bundle (Phase 6) fixes the attribution.

## Phase 4 notes
- Settings live under one key, `app_settings` (JSON). Every consumer calls `store.load_settings()`; never read the old keys (`briefing`, `unlock_rules`, `notify`, `sync_interval_minutes`) directly. They are only read once, by the migration.
- Adding a setting: add the field with a default in `settings/mod.rs`, clamp it in `sanitize()`, mirror it in `src/api/types.ts` and `DEFAULT_SETTINGS` in `src/api/fixture.ts`, add the control in `SettingsScreen.tsx`. `merge_valid` keeps a bad value from costing its siblings.
- Settings never contain secrets (a test scans the export), so export/import are always safe.
- Erase keeps the empty encrypted database and its key; it removes every account credential from the keychain first, and stops if the keychain refuses.
- The tree is remounted when the clock format or week start changes (`SettingsApplier`), because they are read while rendering.
- The browser fixture engine (`fixtureBriefing.ts`) honours the briefing settings; keep it in step when the engine gains options.

## Phase 5 notes
- Email and AI are OFF by default. `get_mail_digest` returns before doing anything when email is off; `summarise_if_enabled` builds nothing and calls nothing when AI is off or consent is missing (both tested; the browser fixture counts calls as `__mailCalls` / `__aiCalls`).
- AI consent (`settings.ai.consented`) is set ONLY by `enable_ai` (which requires `confirm`, and test-calls the service first). `set_settings` preserves the stored value; `AppSettings::import` clears it. Never add another path that sets it.
- The AI prompt is built by `build_prompt` only; the Settings preview shows `Prompt::disclosure()`, the same text. Never put join links, descriptions, attendees, organisers, snippets or email addresses in it (a test checks this).
- Mail scopes: `gmail.readonly` and `Mail.Read` are requested only by `enable_account_mail`, as an upgrade of the existing account's grant (same keychain entry). Gmail's scope is a Google *restricted* scope: public release needs Google's verification and a security assessment (Phase 6).
- Apple Mail: `mail/applemail.rs` holds the script and parser; `mail_ai.rs` runs `osascript` with a 40 s limit and a private 0700 scratch directory for calendar attachments. The script receives only numbers and a path, and a test forbids write verbs in it.
- Mail digest cache is 3 minutes; AI summary cache is 5 minutes, keyed by the facts (not the clock line) so an unchanged day is not re-sent.

## Phase 6 notes
- `DAYBRIEF_PROFILE=<name>` gives a separate keychain service and DB file: use it for any release-build experiment, then delete the leftovers (commands in `docs/RELEASE.md`).
- Updater is behind `--features updater`; config in `src-tauri/tauri.release.conf.json` (placeholder pubkey/endpoint). Build signing needs `TAURI_SIGNING_PRIVATE_KEY` as key *contents*. Never commit keys.
- Windows code (`unlock.rs` `#[cfg(windows)]`) is type-checked only, never run. The `windows` crate needs `Win32_Graphics_Gdi` for `RegisterClassW`.
- Screens are lazy-loaded; e2e must wait for the region/heading before asserting.
- Crash reports are local, opt-in, content-free (`crash.rs`, `crash_reports.rs`). Licences page data: `npm run licences`.
- Measured (release, hidden): idle RSS about 100 MB, CPU 0%.

## IMAP notes (Yahoo, iCloud)
- `crates/core/src/mail/imap`: own small client (no IMAP crate): `wire.rs` (bounded reader/tokenizer), `mime.rs` (header decoding), `mod.rs` (presets, session, `TlsConnect`). Tests use a scripted loopback server; `cargo test -p daybrief-core live_imap -- --ignored --nocapture` checks TLS + greeting against the real servers (no login; verified working).
- Servers are the `ImapPreset` enum, port 993 only. Never accept a host from settings or the page. New provider = new enum variant + review.
- Read-only rules: `EXAMINE`, `BODY.PEEK`, headers only. A test forbids write verbs and body fetches in the commands sent; keep it passing.
- Mailboxes live in `settings.email.imap` (no secret); the app password is keychain key `imap-mail:{id}`. They change only via `add_imap_mailbox`/`remove_imap_mailbox`; `apply_and_save` preserves the stored list, export omits it, import clears it, erase deletes the passwords.
- No snippet and no calendar attachment for IMAP: invitations are detected from the subject and top-level Content-Type only, so they show without details.
- Not verified: an actual LOGIN/FETCH against Yahoo or iCloud (needs the user's app password). Yahoo wants an `ID` command before login; we send it when advertised (it is). Apple's IMAP username may need the Apple ID rather than an @icloud.com alias.

## Time zone
- Never store the time zone: call `crate::local_tz()` (or `deps.tz()`) each time. It reads the system zone fresh, so travel or a changed zone applies without a restart. Not exercised by actually changing the system zone; the webview's own clock formatting may still need a relaunch.

## Settings layout
- Settings is a list of categories (`HOME` in `SettingsScreen.tsx`); each opens its own page. The open page lives in the nav store (`settingsSection`) because changing clock format or week start remounts the tree. `go()` always resets it to the list.
- Adding a setting: put its row in the right page's `Group`. e2e tests open a page with `getByRole("main").getByRole("button", { name: /^General/ })`; the back button is "Settings" on a page and "Briefing" on the list.
- Toast text exists twice in the DOM (visible + screen-reader announcement): use `.first()` when asserting on it.

## Views and meeting links
- Calendar has Day, Week and Month (`MonthGrid.tsx`; a day click opens the Day view). Month is also an "Opens to" choice (`DefaultView::Month`; an old settings file without it is unaffected because unknown values fall back per field).
- When today and tomorrow are empty the briefing shows `ClearWeek` (next 14 days) instead of a bare empty state. Browser fixture flag `?quietdays` removes today's and tomorrow's events.
- Meeting links: Join and Copy link on the up-next card and event detail (`copyLink.ts`). Detection is `crates/core/src/join.rs` (known hosts, or a provider's own conference field for any https link). The compact up-next window has Join only.
- Sync cadence: on launch, on unlock or whenever the window is shown (`show_main` triggers the unlock-style sync, so the 2-minute cooldown applies), every `general.sync_interval_minutes` (default 10), and the manual refresh button. Nothing is pushed from providers, so a new event appears at the next sync.

## Sizes
- The real app window is narrow by design (about 420 wide) and resizable; the e2e viewport is 420x680 for that reason. Below 640px the month shows coloured dots instead of titles; the screens' content is capped at `max-w-2xl` and centred so a big laptop screen does not stretch rows. Tested for no sideways scroll at 390 and 1440.
- This is still a desktop app. A phone version means the Phase 7 web build; nothing native for phones is planned.

## Your own events
- Decision (user, 2026-11-14): the app lets the user add events of their own, stored **only on this computer** (`crates/core/src/local.rs`, provider kind `local`, account "On this computer", calendar `local-events`). It writes nothing to Google, Microsoft or any provider: the "read-only by default" rule in PLAN.md still holds for connected calendars. Writing to a provider would need new scopes and a fresh decision.
- Events go straight into the `events` table (`provider_for(Local)` is `None`, so sync skips the account). Only ids prefixed `local-events:` can be edited or deleted; everything is validated in Rust (`build_event`). Removing the account deletes them; Erase all data removes them.
- UI: `features/events/EventForm.tsx` (mounted only while open), "New event" in the briefing and calendar headers, Edit/Delete/Send invitation in `EventDetail` for own events. The user said "no files": attaching files was declined.
- What the user typed (repeat rule, reminder, invitees) is the record in the `own_events` table (migration 3); the dated `events` rows are *generated* from it by `local::refresh` (after every change, once a day from the notifier thread, and a removed account clears both). Repeats reuse the ICS recurrence engine (`invite::ics` builds the master with a rule made from an enum, never a raw RRULE from the page); a series is edited/deleted as a whole from any occurrence (`local-events:<id>:<occurrence>`).
- Reminders: per-event, timed events only. The notifier shows them itself (`local::due_reminders`) and the general "before each event" heads-up skips events that have one, so nobody is told twice.
- Invitations (`crates/core/src/invite.rs`): Firstlight NEVER sends mail. `prepare_invite` saves an `.ics` to Downloads (never overwriting), reveals it, and opens a `mailto:` draft in the user's own mail app. Recipients are validated strictly; subject/body are percent-encoded; a connected calendar's description is never copied into an email. METHOD:PUBLISH, no ORGANIZER/ATTENDEE, so replies don't come back to the user; the file just lets people add it to a calendar.
- Found while building this: an all-day recurring event ending on a date (`UNTIL=20261119`, as Google and Apple write it) used to expand to only its first day; fixed in `ics::date_until_to_datetime` with a regression test.

## Look and one-day edits
- The default accent follows the macOS accent colour (`get_system_accent`, mapped by `accent_name`; blue, yellow, graphite and non-macOS give the normal default). The applier re-reads it on window focus. The up-next window's capability file allows that one read-only command.
- Optional time-of-day tint: `appearance.ambient` (off by default) sets `data-ambient` on `<html>` (morning/day/evening/night, re-evaluated every 5 minutes); the CSS is a faint wash from the top (`tokens.css`). Axe-tested in light and dark. PLAN.md says no decorative gradients; this exists because the user asked, and stays opt-in.
- One day of a repeating own event: `local::skip_occurrence` records the occurrence key in the record's `skipped` list (kept across series edits). "Edit this day" is create-a-one-off then skip (create first, so a failure leaves the series intact). Browser fixture mirrors it.

## Full screen and wake
- `src-tauri/src/fullscreen.rs`: macOS reads the on-screen window list (no permission needed) and calls a window full screen when a layer-0 window that is not ours exactly covers a display (a maximised window stops below the menu bar, so it never matches); the pure `covers_a_display` is unit-tested. `live_probe` (ignored) proves the window list is actually read on a real Mac. Windows uses `SHQueryUserNotificationState` (type-checked in a scratch crate only). Wired into the unlock decision when `unlock.skip_when_fullscreen` is on (Settings, When it appears).
- Wake fallback (`unlock.rs`): `NSWorkspaceDidWakeNotification`; if no real `screenIsUnlocked` follows within 20 s, the wake counts as an unlock. Pure logic tested; the real sleep/wake has never been observed (cannot be done from a test).
