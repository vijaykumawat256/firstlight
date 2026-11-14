# Daybrief: working notes for agents

The full brief is `PLAN.md`; read it first. This file records what is true *now* and what you'd otherwise rediscover.

## Status
Phases 1-4 built, except what is listed under "Not yet verified". Next: Phase 5 (email digest + optional AI summaries; includes the queued Apple Mail digest). Do not build ahead.
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
