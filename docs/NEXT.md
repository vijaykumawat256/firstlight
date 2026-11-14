# What is left, and the decisions behind it

Last updated 2026-11-14. This replaces guesses with the reasons, so a later session does not rediscover them.

## 1. Writing events to Google and Outlook (design, needs your approval of the choices below)

Today Firstlight adds events **only on this computer** (see CLAUDE.md, "Your own events"). Saving into your real Google or Outlook calendar is a different feature: it needs permission to change your calendar, and it can cause the provider to email other people. Nothing below is built.

### What it would do
- A per-account switch, off by default: "Let Firstlight add events to this calendar". Turning it on asks the provider for write permission in your browser, the same way email access is requested today (an upgrade of the existing sign-in, same keychain entry). Scopes: Google `calendar.events` (a *sensitive* scope), Microsoft `Calendars.ReadWrite`.
- The New event form gets "Save to": *On this computer* (the default) or one of your writable calendars.
- Firstlight can only **change or delete events it created**, remembered by their provider id. Events that already exist in your calendars stay read-only forever; it never edits or deletes anything you or someone else made. This keeps the damage from any bug or abuse small.
- Repeats, reminders and the meeting link map onto the provider's own fields (Google takes our rule text; Microsoft needs its structured recurrence, built from the same fixed choices). Nothing else is written.
- The event then comes back through the normal sync, de-duplicated by its calendar id; the local record keeps only a pointer and the last form values.

### The sharp edge: invitations
If you add invitees and save to Google or Outlook, **the provider emails them**. That is Google or Microsoft sending, not Firstlight, but it is still mail going out under your name. Proposal: the invitees field stays, and a separate, explicit, default-off switch says "Let Google email these people". Without it the event is created with no attendees, and the existing "Send invitation" (your own mail app) still works.

### Costs and risks
- **Google verification:** `calendar.events` is a sensitive scope, so a public release needs Google's brand and scope verification (not the paid security assessment that Gmail needs).
- **Failure handling:** offline or revoked permission means the save fails with a clear message and nothing is half-saved; no hidden queue in v1.
- **Erase all data** removes local data only; it does not delete events already created in your provider calendar, and the privacy policy will say so.
- **Testing:** mock servers for every call (as for the existing providers), plus one manual run with throwaway test accounts before release. I cannot do the live run without your accounts.

### Decisions needed
1. Approve "only events Firstlight created can be changed" (recommended), or do you want to edit existing events too?
2. Approve invitations being a separate, default-off switch (recommended).
3. Which comes first, Google or Microsoft? (Both are similar; one at a time halves the live testing.)

## 2. Incremental sync for Google and Microsoft: deliberately not built yet

Today each sync re-reads the whole window (30 days back, 90 forward) every 10 minutes. That is a little wasteful but always correct. Incremental sync only saves bandwidth, and getting it wrong **silently hides events**, which is the worst failure a calendar can have. I am not willing to ship it unverified:

- **Google:** a sync token cannot be combined with a time window, so the first request would have to be unbounded and recurring events handled differently from today. The exact behaviour (expired tokens, cancelled instances, series changes) can only be confirmed against a real account.
- **Microsoft:** the delta feed for the calendar view is tied to the window it was created with, so it must be rebuilt as the window slides; removed events arrive as markers that must be matched exactly.

Plan once Google and Microsoft sign-in work for you: build it behind a switch, run **both** methods side by side for a few days with a built-in comparison that reports any difference, and make a full re-read the automatic fallback on any doubt (and daily). Until then the current approach stands.

## 3. Decided: attaching files is not being built
You said no files, so it stays out.

## 4. Phase 7 (account, encrypted sync, web)
See [PHASE7_DESIGN.md](PHASE7_DESIGN.md). Waiting for your decisions at the end of that file; no code until then.

## 5. Things only you can do
- Fill in the Google ID and secret in `.env` (docs/OAUTH_SETUP.md), then try Microsoft and Google sign-in.
- `git push` (several local commits are not on GitHub yet), which also starts the first automatic build, including Windows.
- Decide whether to rename the internal identifier `com.daybrief.app` (costs a re-sign-in once real accounts are connected).
- Apple Developer account, Windows signing certificate, an update host, a privacy contact (docs/RELEASE.md).

## 6. Built but never seen working (try these)
- The menu-bar text next to the icon and the Join-next-meeting shortcut (turn both on in Settings, General).
- Reminder notifications actually appearing; Send invitation opening a draft in Mail.
- Notification click bringing the app forward (needs a bundled build, not `tauri dev`).
- Wake-from-sleep fallback (close the lid, open it, wait 20 seconds) and full-screen detection (put a video or presentation full screen, then unlock): both are tested as logic and the window reading is proven on this Mac, but the real sleep and a real full-screen app have not been seen.
- Yahoo, iCloud and custom-server IMAP logins (only the TLS handshake to the real servers has been checked).
- Google and Microsoft sign-in and mail, the Calendar permission prompt, the Apple Mail script, VoiceOver, a signed and notarised build, an update from a real server, and anything on Windows.
