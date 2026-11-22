# Launch plan

What stands between Firstlight and people using it. Items 1–5 need your accounts, money or real users: each says exactly
what to set up and where it goes. Items 6–12 are built into the app and the release workflow; they switch on once
1–5 give them what they need. Progress lives in the **State** column.

| # | Item | State |
|---|---|---|
| 1 | Google and Microsoft sign-in on real accounts | **Waiting on you**: sign-in IDs (section 1) |
| 2 | Google verification | **Waiting on you**: decision + submission (section 2) |
| 3 | Code signing (Mac and Windows) | **Waiting on you**: certificates; workflow ready (section 3) |
| 4 | Beta with 10–20 people | **Waiting on you**: people; plan below (section 4) |
| 5 | Website, privacy contact, support address, price | **Waiting on you**: section 5 |
| 6 | Auto-update through GitHub Releases | Built; needs the update key + a public releases repository (section 3c) |
| 7 | One Mac installer for Apple chips and Intel | Built (universal build in `installers.yml`) |
| 8 | First-run set-up card | Built (`GettingStarted.tsx`) |
| 9 | Automatic backups and restore | Built (`crates/core/src/backup.rs`, Settings, Privacy & data) |
| 10 | Problem reports you receive | Built (About, Report a problem); needs the support address (section 5) |
| 11 | Security review | Internal pass done (`docs/SECURITY_REVIEW.md`); an outside review is still advised before a public launch |
| 12 | Outlook mail digest on Windows | See `CLAUDE.md`, "Outlook mail on Windows" |

## 1. Real Google and Microsoft accounts (the biggest unknown)

Sign-in, calendar sync, mail, sending, task sync and the out-of-office reply have only run against stand-ins.

1. Make the two app registrations: `docs/OAUTH_SETUP.md` (about 15 minutes).
2. Put the IDs in `.env` for builds on your Mac, **and** as repository secrets for the installers: Settings, Secrets
   and variables, Actions: `GOOGLE_OAUTH_CLIENT_ID`, `GOOGLE_OAUTH_CLIENT_SECRET`, `MICROSOFT_OAUTH_CLIENT_ID`.
3. Push a tag (for example `v0.2.0-beta.1`) to build installers with them.
4. Work through `docs/LIVE_TEST.md` section A with one Google and one Microsoft account, then send me the notes.

While the Google app is in *Testing*, only the test users you list can sign in, and Google signs them out every 7 days.

## 2. Google verification

What each feature asks Google for decides how heavy the review is:

| Feature | Scope | Google's class | What it takes |
|---|---|---|---|
| Calendars | `calendar.readonly` | Sensitive | Verification: privacy policy, home page, a short video of the sign-in, a few weeks |
| Task sync | `tasks` | Sensitive | Same review |
| Out-of-office reply | `gmail.settings.basic` | Sensitive | Same review |
| Mail digest | `gmail.readonly` | **Restricted** | The above **plus a yearly security assessment (CASA) by an approved lab**, paid |
| Sending notes and recaps | `gmail.send` | **Restricted** | Same as above |

**Recommendation:** launch with the sensitive scopes only (calendars, tasks, out-of-office). Gmail users still get a
mail digest through Apple Mail on a Mac, or IMAP with an app password. Add the restricted Gmail features once there are
enough users to justify the assessment. Microsoft has no equivalent: do **publisher verification** in Partner Center
(free) so the consent screen doesn't say "unverified".

## 3. Code signing and updates

The Installers workflow (`.github/workflows/installers.yml`) turns each of these on by itself once its secrets
exist, and leaves a note in the run when they don't.

### 3a. Mac (Apple Developer Program, US$99 a year)
1. Join at developer.apple.com, then create a **Developer ID Application** certificate (Xcode, Settings, Accounts,
   Manage Certificates, +).
2. Export it from Keychain Access as a `.p12` with a password, then `base64 -i cert.p12 | pbcopy`.
3. Repository secrets: `APPLE_CERTIFICATE` (the base64 text), `APPLE_CERTIFICATE_PASSWORD`,
   `APPLE_SIGNING_IDENTITY` (for example `Developer ID Application: Your Name (TEAMID)`), `APPLE_ID`,
   `APPLE_PASSWORD` (an app-specific password from appleid.apple.com), `APPLE_TEAM_ID`.
4. The next tag builds a signed, notarised app. Check on another Mac: `spctl -a -vv /Applications/Firstlight.app`
   should say `source=Notarized Developer ID`.

### 3b. Windows
- **Azure Trusted Signing** (about US$10 a month; check that your country and business type are eligible): make a
  Trusted Signing account and certificate profile, and an app registration allowed to sign with it. Secrets
  `AZURE_CLIENT_ID`, `AZURE_CLIENT_SECRET`, `AZURE_TENANT_ID`; variables `TRUSTED_SIGNING_ENDPOINT` (for example
  `https://eus.codesigning.azure.net`), `TRUSTED_SIGNING_ACCOUNT`, `TRUSTED_SIGNING_PROFILE`.
- Or an OV certificate from a certificate authority (about US$200–400 a year), which needs a different `signCommand`;
  ask me to switch the workflow if you go this way.
- SmartScreen warnings fade as a signed app is downloaded more; they don't vanish on day one.

### 3c. Updates (free)
Installed copies check `https://github.com/<releases repo>/releases/download/stable/latest.json` (or `beta/`). The app
can't download from a **private** repository, so:
1. Create a **public** repository just for releases, for example `firstlight-releases` (it holds installers only,
   no code). Repository variable `RELEASES_REPO` = `yourname/firstlight-releases`.
2. A fine-grained token with *Contents: read and write* on that repository only: secret `RELEASES_TOKEN`.
3. Make the update key once and **keep it forever** (lose it and no installed copy can update again):
   `npx tauri signer generate -w ~/.tauri/firstlight-updater.key`. Back up the file and its password in a password
   manager. Secrets `TAURI_SIGNING_PRIVATE_KEY` (the file's *contents*) and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`;
   variable `UPDATER_PUBKEY` (the contents of the `.pub` file).
4. Releases: a tag `v0.2.0` goes to everyone (the stable and beta channels); `v0.2.0-beta.1` only to people who chose
   Beta in Settings, General. The version comes from the tag.
5. First test: install `v0.2.0-beta.1`, choose Beta, push `v0.2.0-beta.2`, then About, Check for updates, Install and
   restart. Your data and sign-ins should be unchanged, and a backup "Before an update" should appear in Settings,
   Privacy & data.

## 4. Beta (2–3 weeks, 10–20 people)

- **Who:** at least 5 on Windows (3 of them with classic Outlook on a work PC), 5 on Mac, a few with Google and a few
  with Microsoft work accounts, and two or three who aren't technical.
- **How:** send the `beta` installers and a one-page "what to try" (`docs/LIVE_TEST.md` B and C, shortened). Ask them to
  use About, Report a problem for anything odd.
- **What tells us it works:** setup takes under 2 minutes; the briefing is opened on at least 4 of 5 workdays without
  them launching it; no crashes; nobody loses data. Ask after week one: "What would you miss if it went away?"
- **Fix, release `-beta.N`, repeat.** The auto-update makes this cheap once 3c is done.

## 5. Website, privacy, support, price

- **Privacy policy:** `docs/PRIVACY.md` is written; fill in the contact line (it says "add your contact address here").
  Google's review needs it at a public web address.
- **Support address:** repository variable `SUPPORT_EMAIL`. "Report a problem" then offers "Save and email".
- **Website:** one page is enough for Google's review and for people to find it: the promise ("Your workday, ready
  when you unlock. Private: nothing leaves your computer."), three screenshots, download buttons pointing at the latest
  release in the public releases repository, the privacy policy, and the support address. GitHub Pages on the releases
  repository is free.
- **Price (a suggestion):** free for calendars and tasks; a one-time or yearly "Pro" for mail, AI, sharing between
  computers and the time report. A server-free app has almost no running costs, so a one-time price is realistic.
  Licence checks would need building; leave it free during the beta.
