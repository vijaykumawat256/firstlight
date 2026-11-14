# Daybrief: working notes for agents

The full brief is `PLAN.md`; read it first. This file records what is true *now* and what you'd otherwise rediscover.

## Status
Phases 1-5 built; Phase 6 built as far as it can be without the user's accounts (see `docs/RELEASE.md`: signing, notarisation, Windows run, live update test, external review remain). Phase 7 needs an approved design doc first. Do not build ahead.
Phase 5 delivered: email digest (`crates/core/src/mail`: Apple Mail via AppleScript, Gmail, Microsoft Graph), optional AI summaries (`crates/core/src/ai`: Anthropic with the user's key, or a local model), shell glue in `src-tauri/src/mail_ai.rs`, Inbox section in the briefing, Email and AI settings with a consent dialog that shows the exact text sent.
Phase 5 gaps: IMAP (Yahoo/iCloud/other mail) is NOT built; "people in today's meetings" is approximated by meeting organiser names only (attendees are not stored); Gmail and Graph mail never ran against the live services; the Apple Mail AppleScript compiles against Mail's dictionary but has not been run (it needs the user's Automation permission).
Phase 4 delivered: one typed, versioned settings document (`crates/core/src/settings`), tolerant per-field loading + migration from the old scattered keys, Settings screen with all seven sections, live apply (theme, accent, clock, week start, default view, shortcut, sync interval, notifications, unlock rules), export/import, "Erase all data".
Phase 3 delivered: briefing engine, unlock rules, notifications, `get_briefing`, event detail, compact up-next window, day/week views.
Known gaps: full-screen-app detection (rule exists, detection stubbed, so no setting is shown for it); notification click does not open the app; the macOS system accent colour is not read (accent is Default or a palette colour).
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
- Windows: nothing built or run locally; CI covers it once pushed. Unlock detection is a stub there.
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
