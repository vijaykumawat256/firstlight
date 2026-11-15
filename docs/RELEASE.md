# Releasing Firstlight

Everything here needs accounts or machines only you have. Nothing below has been run end to end; the pieces that *were* verified are marked.

## 0. One-time prerequisites

| Need | Why | Notes |
|---|---|---|
| Apple Developer Program membership | Developer ID signing + notarisation | A "Developer ID Application" certificate in your login keychain |
| Windows code-signing certificate (OV or EV) | SmartScreen reputation | EV gives instant reputation; OV builds it over time |
| Updater signing key | Signed auto-updates | See section 3. Lose it and no installed copy can ever update. |
| Update hosting (static HTTPS) | Serves `latest.json` and bundles | Any static host; no server code |
| Privacy contact address | `docs/PRIVACY.md` "Contact" line is still a placeholder | Fill in before publishing |
| Google OAuth verification | Gmail scope is *restricted* | Needs a third-party security assessment (CASA). Without it, Gmail mail works only for up to 100 test users. Calendar-only scopes are lighter. |
| Microsoft publisher verification | Removes the "unverified" consent warning | Partner Center account |

## 0b. Installers anyone can install (no signing yet)

- **Mac, on this Mac:** `APPLE_SIGNING_IDENTITY=- npm run tauri build -- --bundles app,dmg` (the `-` gives the app an intact ad-hoc seal; without it a downloaded copy can be called "damaged"; checked with `codesign --verify --deep --strict`). The files appear in `target/release/bundle/` (`macos/Firstlight.app` and `dmg/Firstlight_0.1.0_aarch64.dmg`). This builds for the kind of Mac it runs on (Apple silicon here); an Intel Mac needs a build made on an Intel Mac or in CI.
- **Windows (and Mac) in GitHub:** `.github/workflows/installers.yml`. After pushing, open the repository's Actions tab, choose **Installers**, **Run workflow** (or push a tag like `v0.1.0`). The run ends with downloadable files: `firstlight-windows` (an `.exe` installer that installs for the current user, no administrator needed) and `firstlight-mac` (the `.dmg`). Add `GOOGLE_OAUTH_CLIENT_ID`, `GOOGLE_OAUTH_CLIENT_SECRET` and `MICROSOFT_OAUTH_CLIENT_ID` under Settings, Secrets and variables, Actions, so Google and Microsoft sign-in work in those builds.
- **Not verified:** the Windows installer has never been built or run by us. The Windows code (unlock detection, full-screen check) was only type-checked, so the first run may stop on a compile error that has to be fixed.
- **Unsigned:** Windows shows "unknown publisher" (SmartScreen: More info, Run anyway) and macOS says it cannot verify the developer (right-click the app, Open). Signing is section 1 and 2 below.
- **Android (APK):** not supported. This is a desktop app (menu bar, global shortcuts, unlock detection, start at login, the Calendar app link are all desktop-only). A phone version is the web build in Phase 7 (`docs/PHASE7_DESIGN.md`) or a separate mobile app project.

## 0c. After the rename (Daybrief to Firstlight)

- The first start of a build with the new identity moves an old installation's data across automatically (`legacy.rs`): the database, account sign-ins, AI keys and mailbox passwords go from the old keychain service and folder to the new ones, are checked, and only then removed from the old place. macOS may ask to allow access to the old keychain entries: choose Always Allow. If it cannot be done (access refused), nothing old is touched and it is tried again at the next start.
- macOS ties these to the app's identity, so after the rename they are asked for again: notifications, Calendar access, and Automation for Mail. Start at login must be switched off and on once in Settings so the login item points at the renamed app.
- A Windows install of the old name is a different app: uninstall it, then install the new one (its data is under `%APPDATA%\com.daybrief.app` and is moved on first start, if the same Windows user opens the new app).

## 1. macOS: build, sign, notarise

```sh
. "$HOME/.cargo/env"
export APPLE_SIGNING_IDENTITY="Developer ID Application: Your Name (TEAMID)"
export APPLE_ID="you@example.com"
export APPLE_PASSWORD="<app-specific password>"   # appleid.apple.com
export APPLE_TEAM_ID="TEAMID"
export GOOGLE_OAUTH_CLIENT_ID=... MICROSOFT_OAUTH_CLIENT_ID=...   # see docs/OAUTH_SETUP.md
npm run tauri build -- --features updater --config src-tauri/tauri.release.conf.json
```

Tauri signs with the hardened runtime and `src-tauri/entitlements.plist`, then notarises and staples when the `APPLE_*` variables are set.

Verified locally: the bundle builds with `--features updater`; `LSMinimumSystemVersion` is 13.0; an ad-hoc signature with `--options runtime` and our entitlements verifies with `codesign --verify --strict`. **Not verified:** a real Developer ID signature, notarisation, Gatekeeper on a clean Mac.

After notarising, check on a machine that has never run the app:
`spctl -a -vv Firstlight.app` should say `accepted, source=Notarized Developer ID`.

## 2. Windows

Build on Windows (or a Windows CI runner) with the same command. Sign with `signCommand` in the Tauri bundle config, or Azure Trusted Signing / your EV token tooling.

**Not verified:** nothing has been built or run on Windows. The unlock-detection code (`src-tauri/src/unlock.rs`, `#[cfg(windows)]`) type-checks against the `windows` crate in a scratch crate but has never executed. First things to test on a real machine:
1. Lock then unlock: does the window appear?
2. Does the tray icon appear and Quit work?
3. Does WebView2 exist on a clean install (the installer should bootstrap it)?
4. Windows system-calendar provider: not implemented; PLAN.md says drop it unless packaging is not needed.
5. Notification attribution and the global shortcut.

## 3. Updater key (do this once, keep it forever)

```sh
npx tauri signer generate -w ~/.tauri/firstlight-updater.key
```

- Use a strong password. Back up the key file **and** the password somewhere separate from the repo (password manager plus an offline copy).
- Put the printed **public** key into `src-tauri/tauri.release.conf.json` (`plugins.updater.pubkey`) and the real endpoint URL into `endpoints`.
- At build time set `TAURI_SIGNING_PRIVATE_KEY` to the key's *contents* (not a path) and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`.
- Never commit the private key. Rotating it means shipping a build with the new public key *signed by the old key*; plan that before you need it.

Verified: with a throwaway key, the build produced `Firstlight.app.tar.gz` and its `.sig`, and the app started with the updater feature enabled. **Not verified:** a real update from a hosted `latest.json`.

### Update server layout
```
{base}/stable/latest.json
{base}/beta/latest.json
{base}/<version>/Firstlight.app.tar.gz  (+ .sig contents inlined in latest.json)
```
The app picks the channel from Settings (General). Override the base at build time with `FIRSTLIGHT_UPDATE_BASE`. `latest.json` follows the Tauri v2 static format (`version`, `notes`, `pub_date`, `platforms.{darwin-aarch64,darwin-x86_64,windows-x86_64}.{url,signature}`).

Test plan for the first release: install version N from the notarised build, publish N+1 to `beta`, switch the channel, confirm the toast, install, relaunch, confirm data and sign-ins survived. Then test a *tampered* bundle (wrong signature) is refused.

## 4. Measured performance (release build, macOS, ad-hoc signed, `--hidden`)

| Metric | Result | Proposed budget |
|---|---|---|
| Idle RSS after 15 s, hidden | about 100 MB | under 200 MB |
| Idle CPU over 15 s | 0.0% | under 1% |
| Bundle size | 18.9 MiB (.app), 8.9 MiB (updater tarball) | under 30 MiB |

Cold start to a visible window was **not** measured precisely. Do it by hand with a stopwatch on your slowest machine; budget under 1 s from unlock to usable window. Re-measure with the window open and with several accounts synced.

Use `FIRSTLIGHT_PROFILE=<name>` when running a release build next to your dev data: it uses a separate keychain service and database file. Delete the leftovers afterwards (`security delete-generic-password -s com.firstlight.desktop.<name>` and the `firstlight-<name>.db*` files).

## 5. QA matrix before tagging

- [ ] Clean macOS 13, 14, 15 (and latest): install, first run, add each account type, unlock shows the window
- [ ] Calendar app permission prompt: Allow and Deny both behave
- [ ] Apple Mail permission (Automation) prompt and digest
- [ ] Google and Microsoft sign-in against the live services, including a revoked grant and an offline start
- [ ] Yahoo and iCloud mailbox with real app passwords: add, digest shows unread, nothing becomes read, wrong password message, remove, revoke the app password and see "reconnect"
- [ ] Gmail / Graph mail upgrade flow; AI consent dialog; key removal
- [ ] Notifications appear under "Firstlight" (not the terminal) and click behaviour is acceptable (known gap: click does not open the app)
- [ ] VoiceOver pass through Briefing, Day, Week, Settings, About (axe passes, a screen reader has not been used)
- [ ] Light, dark, each accent, reduced motion, increased contrast
- [ ] Start at login across a real logout/login
- [ ] Erase all data leaves no keychain entries (`security dump-keychain | grep firstlight`)
- [ ] Update flow (section 3) including a tampered bundle
- [ ] Windows pass (section 2)

## 6. Automated gates (run before every release)

```sh
cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings
cargo clippy -p firstlight --features updater -- -D warnings
cargo test --workspace && cargo deny check
npm run check && npm run e2e
npm run licences   # regenerates src/assets/licences.json (see scripts/gen-licences.mjs)
```

## 7. Known gaps shipped knowingly
IMAP covers Yahoo and iCloud only, with no previews or invitation details; full-screen-app detection stubbed; wake-from-sleep fallback absent; macOS system accent colour not read; Google/Microsoft sync re-fetches the whole window each time; people-in-meetings uses organiser names only.
