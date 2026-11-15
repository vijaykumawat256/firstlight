# Firstlight: working notes for agents

The app's name is **Firstlight** everywhere: window, program file (`Firstlight`), crates (`firstlight-core`, package `firstlight`), bundle identifier (`com.firstlight.desktop`), keychain service, data folder, database file (`firstlight.db`), env vars (`FIRSTLIGHT_PROFILE`, `FIRSTLIGHT_UPDATE_BASE`) and the settings export format (`firstlight-settings`). It used to be called Daybrief, and the old names are spelled out in exactly two places so nothing else can drift back: `crates/core/src/legacy.rs` (moves an old installation's database and secrets across on first start; see its tests) and `LEGACY_EXPORT_FORMAT` in `settings/mod.rs` (old settings files still import). Do not reintroduce the old name anywhere else.

The full brief is `PLAN.md`; read it first. Open decisions and the reasons behind what is not built: `docs/NEXT.md`; the Phase 7 proposal awaiting approval: `docs/PHASE7_DESIGN.md` (no Phase 7 code until the user approves it). This file records what is true *now* and what you'd otherwise rediscover.

## Status
Phases 1-5 built; Phase 6 built as far as it can be without the user's accounts (see `docs/RELEASE.md`: signing, notarisation, Windows run, live update test, external review remain). Phase 7 needs an approved design doc first. Do not build ahead.
Phase 5 delivered: email digest (`crates/core/src/mail`: Apple Mail via AppleScript, Gmail, Microsoft Graph), optional AI summaries (`crates/core/src/ai`: Anthropic with the user's key, or a local model), shell glue in `src-tauri/src/mail_ai.rs`, Inbox section in the briefing, Email and AI settings with a consent dialog that shows the exact text sent.
Phase 5 gaps: IMAP is built (`mail/imap`) for Yahoo, iCloud and any other provider by server name (custom host, port 993, TLS only), but never run with a real login; "people in today's meetings" is approximated by meeting organiser names only (attendees are not stored); Gmail and Graph mail never ran against the live services; the Apple Mail AppleScript compiles against Mail's dictionary but has not been run (it needs the user's Automation permission).
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
- The app shows in the Dock (macOS) and the taskbar (Windows) by default (`general.show_in_dock`, applied by `apply_dock`: macOS switches the activation policy between Regular and Accessory, other systems toggle the taskbar button); with it off it lives in the menu bar or tray only. Clicking the Dock icon reopens the window (`RunEvent::Reopen`). Closing the window only hides it; Cmd+Q or the tray's Quit really quits.

## Phase 2 notes
- Providers: `crates/core/src/providers/{fixture,ics,caldav,google,microsoft}`; add new ones to `src-tauri/src/providers.rs`.
- Sync: `crates/core/src/sync` (pure, tested) + `src-tauri/src/scheduler.rs` (thread, own DB connection, backoff).
- Adding an account always goes through `commands::connect` (keychain write, first sync, rollback on failure).
- OAuth client IDs come from `.env` at build time (`src-tauri/build.rs`); guide in `docs/OAUTH_SETUP.md`.
- Mock HTTP server for tests: `crates/core/src/http.rs` `testserver`. Live check: `cargo test -p firstlight-core live_ -- --ignored --nocapture`.
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
- AI providers: `settings::AiProvider` (anthropic, local, openai, xai, gemini, mistral, groq, openrouter, custom). The OpenAI-style ones share `ai::OpenAiStyleSummariser` (`{base}/chat/completions`, Bearer key; OpenAI itself needs `max_completion_tokens`). Each has its own keychain key (`AiProvider::key_name`; Anthropic keeps `ai-key-anthropic`). `custom` takes an https address (`valid_custom_endpoint`) and its consent is tied to the host (`consented_host`; sanitize drops consent if the host changes; `apply_and_save` preserves both fields; import clears them). xAI and Google answer a bad key with 400, handled in `classify`/`says_bad_key`. All six preset addresses were probed live with a fake key (they answer about the key, not 404). Default model names are only starting points and will age: the test call in `enable_ai` says if one is gone. "Sign in with OpenRouter" (`ai/openrouter.rs`, `sign_in_openrouter`) is the only provider sign-in: it returns a key, never turns AI on, and has not been run against a real OpenRouter account.
- AI consent (`settings.ai.consented`) is set ONLY by `enable_ai` (which requires `confirm`, and test-calls the service first). `set_settings` preserves the stored value; `AppSettings::import` clears it. Never add another path that sets it.
- The AI prompt is built by `build_prompt` only; the Settings preview shows `Prompt::disclosure()`, the same text. Never put join links, descriptions, attendees, organisers, snippets or email addresses in it (a test checks this).
- Mail scopes: `gmail.readonly` and `Mail.Read` are requested only by `enable_account_mail`, as an upgrade of the existing account's grant (same keychain entry). Gmail's scope is a Google *restricted* scope: public release needs Google's verification and a security assessment (Phase 6).
- Apple Mail: `mail/applemail.rs` holds the script and parser; `mail_ai.rs` runs `osascript` with a 40 s limit and a private 0700 scratch directory for calendar attachments. The script receives only numbers and a path, and a test forbids write verbs in it.
- Mail digest cache is 3 minutes; AI summary cache is 5 minutes, keyed by the facts (not the clock line) so an unchanged day is not re-sent.

## Phase 6 notes
- `FIRSTLIGHT_PROFILE=<name>` gives a separate keychain service and DB file: use it for any release-build experiment, then delete the leftovers (commands in `docs/RELEASE.md`).
- Updater is behind `--features updater`; config in `src-tauri/tauri.release.conf.json` (placeholder pubkey/endpoint). Build signing needs `TAURI_SIGNING_PRIVATE_KEY` as key *contents*. Never commit keys.
- Windows code (`unlock.rs` `#[cfg(windows)]`) is type-checked only, never run. The `windows` crate needs `Win32_Graphics_Gdi` for `RegisterClassW`.
- Screens are lazy-loaded; e2e must wait for the region/heading before asserting.
- Crash reports are local, opt-in, content-free (`crash.rs`, `crash_reports.rs`). Licences page data: `npm run licences`.
- Measured (release, hidden): idle RSS about 100 MB, CPU 0%.

## IMAP notes (Yahoo, iCloud)
- `crates/core/src/mail/imap`: own small client (no IMAP crate): `wire.rs` (bounded reader/tokenizer), `mime.rs` (header decoding), `mod.rs` (presets, session, `TlsConnect`). Tests use a scripted loopback server; `cargo test -p firstlight-core live_imap -- --ignored --nocapture` checks TLS + greeting against the real servers (no login; verified working).
- Yahoo and iCloud are `ImapPreset` variants with a built-in server. `ImapPreset::Custom` takes a host from the add-mailbox dialog only, validated by `valid_host` (plain lower-case public-style name; no IPs, `localhost`, `.local/.lan/.home/.internal` etc.). Port 993 only, TLS only. `TlsConnect` re-checks the name, `ImapMailbox::is_valid` drops bad records on load, imports clear mailboxes, and a Yahoo/iCloud record with a `host` is invalid. Custom mailboxes have `OpenTarget::None` (no trustworthy web address), so their rows are not clickable.
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
- The copy icon always sits right after the link it copies (`CopyLinkButton`): on its own line under the title in list rows, beside the link line on the up-next card, after the address in the event detail. List rows use a stretched title button (`after:absolute after:inset-0`) so the whole row opens the event while the copy icon and Join sit above it (`relative z-10`); do not turn the row back into one big button, nesting buttons is invalid.
- Copy is an icon (never a text button) and is offered for ANY link an event carries (`eventLink` in `links.ts`: the meeting link, else the first address in the place or notes); Join appears only for a recognised meeting link. Row labels: "Join <title>", "Copy link for <title>".
- The briefing can show the days after tomorrow: `UpcomingDays.tsx` ("Show the next 7 days", then 7 more at a time, up to 56 because one `get_events` call covers about two months; closed by default; days with nothing are left out).
- Axe only judges what is on screen: expanding a section scrolls new elements into view and can surface contrast problems that were hidden before (this is how the "Early start" badge was found at 4.26:1; `--warning` is now `#8f4200`). Scan after scrolling the new content into view.
- Meeting links: Join and Copy link on the up-next card, the event detail, and beside every upcoming row in Today that has a link (`TimelineRow` in `parts.tsx`; not on finished meetings; the buttons sit beside the row button because a button cannot nest in a button). A link used as the event's place shows as "Zoom link" etc. (`links.ts`). Detection is `crates/core/src/join.rs`: a provider's own conference field (any https link), otherwise a link in the place or notes on a list of known video services (zoom, meet, teams, webex, whereby, jitsi, bluejeans, chime, goto, skype, zoho, ringcentral, 8x8, lifesize, join.me, livestorm, around, gather); anything else is not offered as Join so a random web page is never presented as a meeting. The compact up-next window has Join only.
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

## Writing to the Calendar app (macOS, opt-in)
- `general.write_to_calendar_app` (OFF by default; Settings, Privacy & data, only where the Calendar app is supported). When on and access is granted (`calendar_writing_allowed`), the New event form offers "Save to" a writable calendar. Permission is re-checked on every write.
- Only events Firstlight created can be changed or removed: `own_events` keeps a `remote` pointer (`RemoteRef`), `local::resolve` maps a synced Calendar-app row back to it. The Calendar app is written first, then the record; a failed record write undoes a new Calendar-app event. Series are changed whole; "this day only" actions are hidden for these events. `export_all` and reminders skip them (the Calendar app owns those).
- Rust: `SystemSource::save_event/delete_event` (default: error), macOS impl in `eventkit.rs`. Never run against the real Calendar app yet (needs the user's permission click). Other platforms: nothing writes yet; CalDAV/Google/Microsoft write is design-only in `docs/NEXT.md`.
- Browser fixture: `?writeon` turns the setting on, `?calfail` makes the save fail, `?withcaldav` adds a CalDAV account (one changeable calendar, one read-only).
- CalDAV write (works on every OS): `providers/caldav/write.rs`, `CaldavWriter` implements `CalendarWriter` (the Calendar app does too, through a blanket impl for `SystemSource`). OFF per account: `general.writable_accounts` (ids), a switch on the account in Accounts; a settings file can never set it. A calendar is writable only if the server's `current-user-privilege-set` allows it (a server that says nothing counts as read-only). New event = `PUT firstlight-<id>.ics` with `If-None-Match: *`; edit = `PUT` to the stored address; remove = `DELETE` (404/410 = done). Addresses are confined to the user's server and to that calendar (plain `*.ics` name, no nesting), checked before anything is sent. The UID is the file name (`uid_for`), which is how `local::resolve` matches the synced copy back (account + UID). 403 on a write is "not allowed", never "sign in again". No METHOD/ORGANIZER/ATTENDEE, so servers send no mail. Timed repeats use `TZID` without a VTIMEZONE (read back fine by our reader; other clients not checked). The store lock is held during the request (up to the 30 s HTTP timeout). Never run against a real CalDAV server: only a mock.

## Look and one-day edits
- The default accent follows the macOS accent colour (`get_system_accent`, mapped by `accent_name`; blue, yellow, graphite and non-macOS give the normal default). The applier re-reads it on window focus. The up-next window's capability file allows that one read-only command.
- Optional time-of-day tint: `appearance.ambient` (off by default) sets `data-ambient` on `<html>` (morning/day/evening/night, re-evaluated every 5 minutes); the CSS is a faint wash from the top (`tokens.css`). Axe-tested in light and dark. PLAN.md says no decorative gradients; this exists because the user asked, and stays opt-in.
- One day of a repeating own event: `local::skip_occurrence` records the occurrence key in the record's `skipped` list (kept across series edits). "Edit this day" is create-a-one-off then skip (create first, so a failure leaves the series intact). Browser fixture mirrors it.

## Menu bar text, join shortcut, holidays
- Menu-bar text: `general.tray_next_meeting` (OFF by default: a title in the menu bar is visible in screen shares; `notify.hide_titles` turns it into "Meeting"). The notifier tick calls `set_tray_text`, which only touches the tray when the text changed. Text comes from `briefing::glance::tray_label`. macOS shows it next to the icon; Windows only has a tooltip. Not seen on a real menu bar yet.
- Join shortcut: `general.join_shortcut`, registered next to the show/hide one in `apply_shortcut(app, show, join)` (the two keys must differ; `validate`/`sanitize` enforce it). `glance::join_target` prefers a meeting starting within the lead time over the running one. With nothing to join it shows a "Nothing to join" notification.
- Holidays: a picker of 17 public Google holiday feeds (`accounts/holidays.ts`), added as ordinary ICS accounts. `cargo test -p firstlight-core live_every_offered -- --ignored` fetches and parses every one with the production client (all 17 passed); keep its id list in step with the TS list. Birthdays come from the Calendar-app provider (Contacts' Birthdays calendar), not from the picker.

## Search, snooze, export
- Search: `features/search` (`SearchHost` is mounted once in `App.tsx`; Cmd/Ctrl+K toggles it, the briefing header has a Search button). Matching is `searchEvents` (every word, accent-insensitive, title and place; cancelled/declined hidden; upcoming first) and `parseJump` (today/tomorrow/weekday/ISO/"dec 24") in `search.ts`, both unit-tested. It loads 30 days back to 90 ahead in two requests because `get_events` is capped at 62 days. A day result opens the calendar on that date through `nav.calendarDate`.
- Snooze: heads-ups (`Kind::Before`) and own reminders carry a "Snooze 10 min" action (action type `snoozable`, registered in `lib.rs`). The handler stores the shown text in `briefing::snooze::SnoozeQueue` (in memory: a restart forgets pending snoozes) and the notifier tick re-shows it. Daily summaries have no button. Only the queue is tested; the real button press needs a bundled app on a real Mac.
- Export: `local::export_all` writes every own event (repeats as RRULE, removed days as EXDATE, reminders as VALARM) to one `.ics` in Downloads; the round trip through our own reader is tested. Button in Settings, Privacy & data.

## Full screen and wake
- `src-tauri/src/fullscreen.rs`: macOS reads the on-screen window list (no permission needed) and calls a window full screen when a layer-0 window that is not ours exactly covers a display (a maximised window stops below the menu bar, so it never matches); the pure `covers_a_display` is unit-tested. `live_probe` (ignored) proves the window list is actually read on a real Mac. Windows uses `SHQueryUserNotificationState` (type-checked in a scratch crate only). Wired into the unlock decision when `unlock.skip_when_fullscreen` is on (Settings, When it appears).
- Wake fallback (`unlock.rs`): `NSWorkspaceDidWakeNotification`; if no real `screenIsUnlocked` follows within 20 s, the wake counts as an unlock. Pure logic tested; the real sleep/wake has never been observed (cannot be done from a test).

## Windows window and tray
- macOS gets a transparent window with vibrancy (`tauri.conf.json`). Windows uses `tauri.windows.conf.json`, an ordinary opaque window (Mica/transparency could come out invisible on Windows 10 or with some drivers), and `main.tsx` sets `data-platform="windows"` so the page paints its own solid background (`tokens.css`). The tray icon is the coloured 32x32 app icon off macOS: the black template glyph (`tray.png`) disappears on a dark taskbar. None of this has been seen on a real Windows machine.
