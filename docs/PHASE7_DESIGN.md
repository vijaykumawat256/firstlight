# Phase 7 design: optional account, end-to-end encrypted sync, and a web version

Status: **proposal, waiting for your approval. No code has been written.** PLAN.md section 9 requires this document first because Phase 7 adds a server and changes the threat model.

Read the "Decisions I need from you" list at the end first; everything else explains the recommendation behind it.

## 1. What this is, in one paragraph

You can create a Firstlight account with a passkey. Your connected accounts, their sign-in tokens, your settings and your own events are encrypted on your device with a key the server never has, and the server stores only ciphertext. A second computer, or a browser, signs in with the same passkey, receives the key from a device you already trust, and shows the same setup with no provider re-authentication. The desktop app keeps working exactly as it does today with no account at all.

## 2. Goals and non-goals

Goals (from PLAN.md): optional account; passkey sign-in; the server can never read tokens, calendar data or settings; a recovery path for a lost device; revoking a device cuts off its access; the web version reuses the React UI; the desktop app is unchanged without an account.

Non-goals for the first release: team or shared calendars; writing to other people's calendars; a phone app; syncing cached *events* (they are re-fetched from each provider, so only the small things that cannot be re-fetched are synced); email or push notifications from our server (we send nothing).

## 3. What is synced, and what is not

| Synced (encrypted, small) | Never leaves the device |
|---|---|
| Account records: provider, display name, which calendars are shown, colours | Cached events and email digests |
| Provider sign-in tokens (refresh tokens) and app passwords | The database encryption key of this device |
| Settings (the existing portable settings document, plus AI/mail choices but **not** AI consent) | AI consent (it must be given on each computer; see CLAUDE.md) |
| Your own events (the `own_events` records) | Notification history |

Total size is a few hundred kilobytes for a heavy user, which keeps cost and abuse limits trivial.

Tokens are the sensitive part. Syncing a refresh token means every enrolled device can act as you at that provider, so enrolling a device is a real security decision and is designed as one (section 6).

## 4. Keys, in plain language

All cryptography uses well-reviewed primitives from audited libraries (RustCrypto in the desktop and WASM builds, libsodium-compatible in the browser): XChaCha20-Poly1305 for encryption, HKDF-SHA-256 for deriving keys, X25519 for sharing the key with a new device, Argon2id only if a passphrase is ever used (it is not, by default).

1. **Vault key (VK).** 256 random bits, created on the first device. It never leaves a device unwrapped. Every record is encrypted under a subkey derived from it (`HKDF(VK, "record", record_id)`), with a random nonce and the record id, version and account id as authenticated data, so the server cannot swap one record for another or roll one back undetected.
2. **Device key.** Each device has its own X25519 key pair. The private half lives in that device's keychain (the browser uses a non-extractable WebCrypto key). The server holds, per device, a copy of the VK sealed to that device's public key. The server cannot open it.
3. **Recovery key (RK).** 128 random bits, shown once as 24 words when you create the account and offered for printing. A second copy of the VK is sealed under a key derived from the RK. The RK is the only way back in if every device is lost. We recommend it be written down; we cannot recover it.
4. **Passkey.** Proves *who you are to the server* (WebAuthn). It is not what encrypts your data. Where the platform supports the WebAuthn PRF extension the passkey can additionally unlock the VK on a new browser without another device; where it does not, a new browser needs a trusted device or the recovery key.

Consequence, stated honestly: **if you lose all devices and the recovery key, your synced setup is gone**. You would sign in again to each provider. That is the price of the server being unable to read your data, and it is the same trade other end-to-end encrypted services make.

## 5. Sign-up and sign-in

- **Sign-up:** choose "Create a Firstlight account". The app creates a passkey (WebAuthn registration against our server), generates the VK, device key and RK, uploads the sealed copies and the first encrypted records, and shows the recovery key.
- **Sign-in on a known device:** passkey assertion gives a short-lived session token; the VK is unsealed from the device key. No password exists anywhere.
- **No email address is required.** Without one we cannot send reset mail, which is a feature: there is no "reset by email" path an attacker could abuse. If you want account notices (a new device was enrolled), an optional notification email can be added later; its absence must never weaken anything.

## 6. Enrolling a device, and revoking one

**Enrolling** (new computer or browser):
1. The new device creates its key pair, signs in with the passkey, and shows a short code and a QR with its public key.
2. An already-enrolled device (or the recovery key) approves. The approving device displays the same **six-word check phrase** derived from both public keys; you confirm the phrases match, which defeats a server that tries to substitute its own key.
3. The approving device seals the VK to the new device's public key and uploads it. The new device downloads the encrypted records and decrypts them.
4. Provider tokens arrive with the records. For providers that allow it, the new device immediately refreshes and stores its own token chain; Microsoft rotates refresh tokens, so two devices sharing one chain would invalidate each other. **Open point:** the first release may instead ask you to sign in to each provider once on the new device and sync only the account records. This is safer and I recommend it for v1 (see decisions).

**Revoking:**
1. From any enrolled device you remove a device. The server deletes that device's sealed key and its sessions at once.
2. Because the revoked device may already hold the VK in memory or on disk, the approving device then **rotates the VK**: it generates a new VK, re-encrypts all records, and re-seals it for the remaining devices. Rotation is what makes revocation real for *future* data.
3. Anything the revoked device already saw, it still saw. For tokens, the app offers "revoke and sign in again" per provider, which invalidates the old refresh tokens at the source. Revoking a lost laptop should be accompanied by this, and the interface says so.

## 7. What the server can and cannot see

Cannot see: event titles, attendees, locations, notes, account names, tokens, passwords, settings, your own events, device names (encrypted under the VK), the recovery key, the VK.

Can see (unavoidable): that an account exists and when it was created; the number of enrolled devices; the passkey public keys; the number of records and their approximate sizes; when records change and from which IP address; request volume. Mitigations: ciphertext is padded to fixed size buckets; IP addresses in logs are kept for 7 days then dropped; records have opaque random ids; no analytics, no third-party scripts.

A full dump of server storage therefore contains public keys, opaque ciphertext and timestamps. This is the test in the Phase 7 "Done when" list, and it is checked by a script that decrypts nothing and greps the dump for any known token, title or setting.

## 8. The web version

- The React UI is reused as-is. `src/api` gets a second implementation (`web.ts`) next to `tauri.ts`; the UI never knows which one it is using (PLAN.md rule 2).
- **The Rust core runs in the browser as WebAssembly.** `crates/core` today links SQLCipher and a blocking HTTP client, neither of which exist in a browser. The work is to split it: a pure `core-logic` crate (model, briefing engine, ICS and recurrence, dedupe, invitations, own-event expansion, unlock rules) that compiles to WASM and to native, and a storage-and-network layer that stays desktop-only. This is a mechanical split because PLAN.md already forbids Tauri imports in core and the briefing engine is pure. The browser keeps its cache in IndexedDB, encrypted with a key derived from the VK.
- **Google and Microsoft are called directly from the browser** (both allow it with OAuth PKCE and their own CORS rules). **CalDAV and IMAP cannot be**, so they need the relay (section 9).
- **Honest limit of any web app:** the browser downloads the app's code from our server each time, so a compromised server could serve malicious code. The desktop app does not have this weakness (it is signed and updated through a pinned key). The web version is therefore the more convenient client and the desktop app remains the higher-assurance one. Mitigations: strict CSP with no inline script, no third-party scripts, Subresource Integrity, Trusted Types, a separate origin from any marketing site, reproducible builds with published hashes, and the VK held only in memory (never `localStorage`).

## 9. The relay for CalDAV and IMAP

A small stateless proxy that lets the browser reach servers it cannot (raw IMAP over TLS, CalDAV without CORS).

- **Trust model, stated plainly:** the relay must see the provider credentials (an app password) in memory for the duration of a request, because it has to log in as you. It stores nothing: no credentials at rest, no request or response bodies, no logs of content. It cannot read your synced vault. Someone who compromised the relay *while you were using it* could capture credentials in flight. That is a real exposure and the reason the relay is optional and off by default; users who do not accept it keep CalDAV and IMAP on the desktop app, where no relay is involved. The relay is open source and can be self-hosted.
- **Abuse and server-side request forgery:** an open proxy is dangerous, so it enforces: a valid session; only the same host rules the desktop app uses (plain public host names, no IP addresses, no `localhost` or private-network names, port 993 for IMAP and 443 for CalDAV); a check of the *resolved* address after DNS (refuse loopback, private, link-local and cloud-metadata ranges, re-checked on every redirect); TLS only with full certificate validation; response size and time caps; per-user and global concurrency limits.
- **Read-only:** the IMAP path allows only the same commands the desktop client sends (`EXAMINE`, `SEARCH`, `FETCH` of headers). It is not a general mail tunnel.

## 10. Rate limits, abuse and quotas

- Sign-up: per-IP limits and a proof-of-work challenge, so accounts cannot be mass-created for free storage.
- Per account: 2 MB of ciphertext, 500 records, 120 writes per hour, 600 reads per hour.
- Relay: 60 requests per minute per account, 4 concurrent, 5 MB responses, 15-second timeouts.
- Everything returns plain, content-free errors; the server never logs request bodies.

## 11. Deleting data

- "Delete my account" in the app removes every record, key wrap, device and passkey at once and returns when the server confirms.
- Backups that still contain the account are purged within 30 days, and the privacy policy says so.
- A signed-out or abandoned account is deleted after 12 months of inactivity, with a warning in the app first (no email is needed to receive it: it shows on next launch).
- "Erase all data" on a device keeps its current meaning (local data) and additionally offers to delete the account.

## 12. Hosting and cost (estimates, not quotes)

Recommended: a small Rust service (axum, `webauthn-rs`, SQLite or Postgres) on one managed VM or a container platform, with the relay as a separate small service so it can be scaled, rate-limited and, if wanted, self-hosted on its own.

| Item | Rough monthly cost |
|---|---|
| App server and database (small VM or container, backups) | 5 to 20 USD |
| Relay (separate small instance) | 5 to 10 USD |
| Domain | about 1 USD (12 per year) |
| Monitoring and log shipping | 0 to 10 USD |
| Total for the first few thousand users | about 10 to 40 USD |

Dominant cost is **your time**: on-call for a service that holds people's sign-in material, abuse handling, security updates, and legal basics (privacy policy, data-processing terms, a security contact). Cloudflare Workers with Durable Objects is a viable lower-ops alternative; it fits the sync API well and the relay poorly (raw TCP is limited), so I would still run the relay separately.

## 13. How it changes the threat model

Adds: a server that can be attacked, a web client that can be served bad code, device enrolment as a social-engineering target, a relay that sees credentials in flight.
Reduces nothing for users who never create an account: the desktop app's behaviour and network surface stay as they are today.
SECURITY.md gets a new section for each of the above when the code lands.

## 14. Delivery plan (if approved)

1. Split `crates/core` into `core-logic` (WASM-capable) and the desktop-only layer; no behaviour change; all current tests keep passing. **Value on its own and risk-free, so it can start first.**
2. Encryption library and test vectors (vault, wraps, record encryption); a model checker for the enrolment and rotation flows.
3. Sync server for accounts, devices and ciphertext records; the "server dump reveals nothing" test.
4. Desktop: account creation, enrolment, revocation, rotation, recovery; behind a setting that is off by default.
5. Web client: `web.ts`, WASM core, Google and Microsoft direct, IndexedDB cache.
6. Relay, with the abuse controls in section 9.
7. External security review of steps 2 to 6 before any public launch.

## Decisions I need from you

1. **Approve the overall shape?** Passkey-only sign-in, end-to-end encrypted vault, recovery key, no email required.
2. **Token sync in v1: yes or no?** My recommendation is **no**: sync account records and settings only, and sign in to each provider once per device. It removes the riskiest part (shared refresh tokens, Microsoft rotation clashes) and makes revoking a device simple. Tokens can follow in a later version.
3. **Relay in v1?** Recommendation: ship Google and Microsoft in the browser first, and add the relay afterwards, because it is the one component that sees credentials.
4. **Hosting:** a small VM or container platform you run, or Cloudflare. Which provider and region do you want, and who is on call?
5. **Is the web version acceptable as the less-assured client?** (Section 8.)
6. **Pricing:** free, or paid to cover running costs?
7. **Do I start with step 1** (the core split), which changes nothing for users and which every later step needs?
