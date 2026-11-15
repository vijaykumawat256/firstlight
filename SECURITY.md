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
| CalDAV accounts are read-only unless the user switches on writing for that account (off by default, never set by an imported file); then Firstlight only creates, replaces and deletes the `firstlight-*.ics` files it made, at addresses checked against the signed-in server and the chosen calendar before any request, never overwrites an existing file, sends no invitations (no METHOD/ATTENDEE), and treats a refusal as a refusal rather than a revoked login | `providers/caldav/write.rs` |
| Calendar app access is read-only EventKit unless the user turns on "Let Firstlight add events to the Calendar app" (off by default); with it on, only events Firstlight itself created can be changed or deleted (matched by a stored pointer), permission is re-checked on every write, and a failed write leaves nothing half-saved. Permission is the user's per-app macOS grant, and losing it marks the account for attention instead of returning an empty calendar that would wipe the cache | `providers/system`, `eventkit.rs` |
| Classic Outlook on Windows is read-only. Firstlight starts `powershell.exe` hidden, sends the script on standard input (never on a command line), and reads JSON back with a 60 s limit and a 16 MB output cap. The script only reads (a test forbids Save, Delete, CreateItem, Send, Move); the only values put into it are the calendar id, which must be hexadecimal, and two numbers. It cannot be used for anything else because the page never supplies script text. No sign-in or password is involved: Outlook answers for the accounts already in it. The new Outlook has no such interface and is not read | `providers/system/outlook.rs`, `outlook_win.rs` |
| "Ask your calendar" sends the AI provider only the typed question, today's date and the time zone (never an event, title, place or name). The reply is untrusted: it must parse to a plan whose every field passes strict checks (`logic/src/ask.rs`), is executed locally against the stored events, and is never displayed itself. Same consent gate as the summary; 30 questions per hour; nothing stored | `ai/mod.rs`, `logic/src/ask.rs`, `mail_ai.rs` |
| Settings are one validated document with no secrets in it (tested); loading never fails and never trusts a value outside its range; imports are size-limited and format-checked | `crates/core/src/settings` |
| "Erase all data" deletes each account's keychain entry first, then every row, then compacts the file; if the keychain refuses, nothing is deleted so credentials are never orphaned | `crates/core/src/erase.rs` (tested) |
| Sending an invitation is off by default and never sends on save or on review: it needs the setting plus a press of Send on the exact message shown; only Yahoo and iCloud servers (built in) are used; the app password is sent only after TLS (implicit, or STARTTLS which must be advertised and succeed), recipients are strictly validated (no header injection), at most 20 per message and 10 messages an hour | `mail/smtp.rs`, `commands.rs` |
| Email is off by default and runs no mail code until enabled; only sender, subject and a 160-character plain-text snippet are kept, markup is stripped, nothing is rendered, and messages open in the user's own app via validated https mail hosts or `message://` links | `crates/core/src/mail`, `mail_ai.rs` (tested) |
| Mail permission is separate from calendar permission and requested only when the user turns email on for that account | `enable_account_mail` |
| The Apple Mail AppleScript is read-only (a test forbids write verbs), takes only numbers and a path as arguments, runs under a hard time limit, and attachments are read only from a private 0700 folder, size-capped, then deleted | `mail/applemail.rs`, `mail_ai.rs` |
| AI is off by default; with it off nothing is built or sent (tested). Consent is per provider, recorded only by the explicit enable action, and cannot come from an imported file or a settings save | `crates/core/src/ai`, `settings` |
| The AI prompt contains only titles, times, places, counts and (optionally) sender names and subjects; never join links, descriptions, attendees, organisers, snippets or addresses. The user sees this exact text before agreeing | `ai::build_prompt` (tested) |
| AI keys (one per provider) live in the keychain, are never returned to the page, and never appear in errors or request bodies (they travel only in the request header to the provider's own address); local models are reachable only on loopback (including redirects). A custom AI address must be https with a plain public host name (no IP address, `localhost`, private-network name, credentials or query), and the user's agreement is tied to that host: another host turns AI off until it is agreed again. "Sign in with OpenRouter" is PKCE with a loopback redirect and only yields an API key for the user's own account | `mail_ai.rs`, `ai/`, `settings::valid_custom_endpoint`, `http.rs` (tested) |
| IMAP mail (Yahoo, iCloud, or another provider the user names) is read-only by construction: Yahoo and iCloud have their servers built in, a custom server must be a plain host name (no IP addresses, `localhost` or private-network suffixes) typed by the user in the add-mailbox dialog, a settings file can never add or change a mailbox (imports clear them), port 993 only with TLS from the first byte and a certificate checked for that name by the OS verifier (no plain-text or STARTTLS fallback), `EXAMINE` and `BODY.PEEK` only, headers only (no bodies), credentials limited to printable ASCII so no command can be injected, every server length capped, app password only in the keychain and removed with the mailbox or on erase | `mail/imap` (tested against a scripted server, hostile replies, fuzz) |
| Events the user adds themselves stay in the local database only (no provider write access is ever requested); input is validated in Rust (title, lengths, https-only link, date order and span) and only the app's own calendar can be changed or deleted, whatever id the page sends | `local.rs` (tested) |
| Invitations never leave through Firstlight: it prepares a `mailto:` draft and an `.ics` file and your own mail app sends it. Addresses are validated strictly (no quotes, brackets, spaces, control characters; at most 20), subject and body are percent-encoded so text cannot add headers or recipients, calendar-file text is escaped and folded so event text cannot inject lines, and repeat rules are built from a fixed enum rather than accepted as text | `invite.rs`, `local.rs` (tested, incl. hostile text) |
| A failed first sync rolls back: no half-connected account or stray credential is left | `commands.rs` `connect` |

## Known gaps and decisions
- **Dev-only CSP relaxation**: `devCsp` allows inline styles and the Vite dev server. The release CSP does not.
- **`macOSPrivateApi` is enabled** (needed for the translucent window). It rules out Mac App Store distribution. Direct, notarised distribution is the plan.
- **Network access exists since Phase 2** (calendar providers only). Google and Microsoft sign-in has been tested against mock servers but not yet against the live services.
- **Google sync is not incremental** (full window every sync) and Microsoft does not use delta queries yet; both avoid features that could not be verified without live credentials.
- **Google 'Testing' status** expires sign-ins after 7 days until the app is verified (Phase 6).
- **Dependency audits, run locally on 2026-11-14**: `cargo audit` found no vulnerabilities; its 5 warnings (unmaintained `proc-macro-error`, `zbus_names`, `zvariant`, `zvariant_derive`; unsound `glib` 0.18) are Linux-only transitive dependencies of Tauri and are not compiled for macOS or Windows (verified with `cargo tree`). Revisit if Linux is ever supported. `npm audit --omit=dev` found 0; plain `npm audit` reports a moderate `minimatch` issue reachable only through the lint tooling (`typescript-eslint`), not shipped. CI fails only on high severity for npm.
- **Prompt injection:** event titles and email subjects are untrusted text that goes into the AI prompt. The model is told to treat them as data, has no tools, and its reply is shown as plain text only, so the worst outcome is a misleading summary. Turn AI off if that is unacceptable.
- **Gmail restricted scope:** reading Gmail needs Google's verification and a third-party security assessment before public release.
- **Phase 6 controls (built, see `docs/RELEASE.md` for what is unverified):** hostile ICS/RRULE input is bounded (expansion step cap, checked arithmetic, 10-year event cap) and fuzzed; `panic=unwind` with `contained()` around background loops; separate least-privilege capabilities for the main and up-next windows; strict CSP tested against the production build; updates are signature-checked against a pinned public key (feature `updater`); crash reports are local, opt-in and content-free; hardened runtime + minimal entitlements.
- **Still open:** real Developer ID signing and notarisation, Windows signing, an external security review, and a live update test have not been done.
