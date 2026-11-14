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

## Known gaps and decisions
- **Dev-only CSP relaxation**: `devCsp` allows inline styles and the Vite dev server. The release CSP does not.
- **`macOSPrivateApi` is enabled** (needed for the translucent window). It rules out Mac App Store distribution. Direct, notarised distribution is the plan.
- **Fixture data only** in Phase 1. No network access exists in the app yet.
- **Dependency audits, run locally on 2026-11-14**: `cargo audit` found no vulnerabilities; its 5 warnings (unmaintained `proc-macro-error`, `zbus_names`, `zvariant`, `zvariant_derive`; unsound `glib` 0.18) are Linux-only transitive dependencies of Tauri and are not compiled for macOS or Windows (verified with `cargo tree`). Revisit if Linux is ever supported. `npm audit --omit=dev` found 0; plain `npm audit` reports a moderate `minimatch` issue reachable only through the lint tooling (`typescript-eslint`), not shipped. CI fails only on high severity for npm.
- Phase 6 adds code signing, notarisation, signed updates, and an external security review.
