# Security review (internal)

An internal pass before launch, 2026-11-21. It is not a substitute for an outside review: before a public release, have
someone independent look at the areas marked **outside review**. Each finding says what was done.

## Threat model in one paragraph

Firstlight has no server. What it protects is the person's own data on their own computer (an encrypted database, keys
in the system keychain) and what crosses to services they connected (calendar and mail providers, an AI service if they
turned it on, a synced folder if they turned it on). Attackers worth designing against: a web page or a calendar invite
with hostile content (it reaches the screen and the AI prompt), a hostile server answer (calendar, mail, update), a file
put in the synced folder by someone without the sync code, a tampered update, and anyone who gets the settings export.
Someone already running code as the person on their computer is out of scope (they can read the keychain).

## Findings and fixes

| # | Area | Finding | State |
|---|---|---|---|
| 1 | Sharing between computers | Tasks arriving from another computer were stored without the length checks a typed task gets (title, notes, list size). | **Fixed**: `task_within_limits`; tested |
| 2 | Sharing between computers | Own events arriving from another computer were stored as given: no form checks, and a record could carry a pointer to a Calendar-app or CalDAV event, giving this computer something to change there. | **Fixed**: `local::valid_shared_record` (same checks as the form, no pointers); tested |
| 3 | Sharing between computers | Checking whether a folder needs a code read every file whole, so a huge file put in the folder could use up memory. | **Fixed**: same 32 MB limit as syncing |
| 4 | Sharing between computers | Another computer's name was shown at any length. | **Fixed**: 80 characters |
| 5 | Backups | A restore names a file; a crafted name could point outside the backups folder. | **Safe by design**: only a name in the folder listing that matches the exact backup pattern can be restored (`parse_name`); tested with `../` |
| 6 | Problem reports | The log could contain a home-folder path (the person's name) or an address in an error. | **Mitigated**: home folder becomes `~`, address-shaped words become `[address]`, only warnings and errors are included, the whole text is shown and editable before saving, and the app sends nothing |
| 7 | Release workflow | Signing secrets must never reach code from a pull request. | **Safe**: the Installers workflow runs only on tags and by hand (people with write access); CI on pull requests uses no secrets |
| 8 | Release workflow | The Windows `signCommand` is built from repository *variables*. | **Accepted**: only repository admins can set them |
| 9 | Updates | A tampered update or update file. | **Safe by design**: the update key's public half is built into the app and every download is checked against it; the address is fixed at build time (`updates.rs`) |

## Already in place (checked again)

- Database: SQLCipher with a random 256-bit key kept only in the keychain; backups are encrypted copies with the same key.
- Content Security Policy: scripts only from the app itself, no remote content, `object-src 'none'` (`tauri.conf.json`;
  `e2e/csp.spec.ts`). Windows have only the commands they need (capability files, with tests for the capture box and
  the up-next window).
- Links: only `https` opens in the browser (`assertHttps`); mail drafts re-check every address; join links are matched
  against known video services.
- AI: prompts are built in one place each and tests pin exactly what they contain (no notes, attendees, addresses or
  links); answers are parsed and checked, never run or shown as instructions.
- Mail: IMAP is read-only (a test forbids write commands); Apple Mail's script has no write verbs (tested); sending
  needs a separate switch and shows the full message first, with a limit of 10 an hour.
- Settings export carries no secrets (a test scans it); import clears every permission-like field.
- Folder sync files: XChaCha20-Poly1305 with a random nonce per write; a file that fails to decrypt is counted and
  ignored.

## For an outside review

1. The OAuth flows (loopback redirect, PKCE, state) against live Google and Microsoft.
2. The SMTP and IMAP clients (`mail/smtp.rs`, `mail/imap`): own implementations, tested only against scripted servers.
3. The ICS parser (`crates/logic/src/ics`), which reads calendar files from anywhere (fuzzed in `fuzz.rs`, but worth a
   second look).
4. The PowerShell scripts for Outlook on Windows (`providers/system/outlook.rs`): inputs are hex ids only, but the
   pattern deserves review.
5. macOS entitlements and the hardened runtime once signed with a real Developer ID.
