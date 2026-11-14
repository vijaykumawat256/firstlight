# Security model

Status: Phase 1. Update this file whenever a phase changes what we store, send or trust.

## What we protect
- Calendar content (titles, attendees, locations, descriptions).
- Credentials: OAuth refresh tokens, app passwords, the database key (later phases add more).

## From whom
- Other local users, malware that can read user files, and anyone with a copy of a backup or the disk.
- Malicious content inside calendar events or emails (phishing links, script injection).
- A compromised dependency.
- Not in scope: malware running as the logged-in user with full keychain access, or a compromised OS.

## How (implemented in Phase 1)
| Control | Where |
|---|---|
| Database encrypted at rest (SQLCipher, random 256-bit raw key) | `crates/core/src/store` |
| Database key only in the OS keychain (Keychain / Credential Manager) | `src-tauri/src/keychain.rs`, `crates/core/src/secrets` |
| A wrong or missing key is refused, never "repaired" by wiping data | `Store::open`, `load_or_create_db_key` |
| UI can call only five named commands; each input is validated in Rust | `src-tauri/build.rs`, `capabilities/default.json`, `lib.rs` |
| No shell/filesystem access from the UI; opener allowed for `https://` URLs only | `capabilities/default.json`, `src/api/backend.ts` |
| Strict CSP: no remote resources, no inline scripts, no `eval` | `tauri.conf.json` |
| Event content never appears in `Debug` output or logs | `model::Event`, `Redacted` (tested) |
| Errors returned to the UI are generic; detail goes to the log without content | `lib.rs` `internal()` |
| Single instance; hides to tray instead of exiting | `lib.rs` |
| Private calendar links, CalDAV passwords and OAuth tokens only in the keychain; account removal deletes them first | `commands.rs`, `providers/*` |
| HTTP: https only, redirects never downgrade, 20 MB cap, errors never contain URLs | `crates/core/src/http.rs` (tested) |
| CalDAV and Graph links from a server are confined to that server's origin, so credentials cannot be redirected to another host | `providers/caldav`, `providers/microsoft` (tested) |
| OAuth: system browser only, PKCE S256 (RFC 7636 test vector), random `state` checked in constant time, loopback listener bound to 127.0.0.1, forged or foreign callbacks ignored | `crates/core/src/oauth` |
| Rotated refresh tokens are saved before use; `invalid_grant` marks the account for reconnection and is never retried in a loop | `oauth::TokenManager`, `sync` |
| Read-only scopes only (`calendar.readonly`, `Calendars.Read`); tests assert no write scope appears | `providers/google`, `providers/microsoft` |
| Browser is only ever opened to an allow-listed sign-in host over https | `commands.rs` `SIGN_IN_HOSTS` |
| XML from servers is parsed with no entity expansion and a depth cap | `crates/core/src/xml.rs` (tested) |
| A failed first sync rolls back: no half-connected account or stray credential is left | `commands.rs` `connect` |

## Known gaps and decisions
- **Dev-only CSP relaxation**: `devCsp` allows inline styles and the Vite dev server. The release CSP does not.
- **`macOSPrivateApi` is enabled** (needed for the translucent window). It rules out Mac App Store distribution. Direct, notarised distribution is the plan.
- **Network access exists since Phase 2** (calendar providers only). Google and Microsoft sign-in has been tested against mock servers but not yet against the live services.
- **Google sync is not incremental** (full window every sync) and Microsoft does not use delta queries yet; both avoid features that could not be verified without live credentials.
- **Google 'Testing' status** expires sign-ins after 7 days until the app is verified (Phase 6).
- **Dependency audits, run locally on 2026-11-14**: `cargo audit` found no vulnerabilities; its 5 warnings (unmaintained `proc-macro-error`, `zbus_names`, `zvariant`, `zvariant_derive`; unsound `glib` 0.18) are Linux-only transitive dependencies of Tauri and are not compiled for macOS or Windows (verified with `cargo tree`). Revisit if Linux is ever supported. `npm audit --omit=dev` found 0; plain `npm audit` reports a moderate `minimatch` issue reachable only through the lint tooling (`typescript-eslint`), not shipped. CI fails only on high severity for npm.
- Phase 6 adds code signing, notarisation, signed updates, and an external security review.
