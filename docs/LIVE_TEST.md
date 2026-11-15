# Live test guide (things only real accounts and a real machine can show)

Use a separate profile so your normal data is untouched:
`FIRSTLIGHT_PROFILE=livetest /Applications/Firstlight.app/Contents/MacOS/Firstlight`
(or install the new DMG and use it normally). Clean up afterwards with the commands in `docs/RELEASE.md` section 4.

For every step: note **what you did, what you expected, what happened**, and a screenshot if it looks wrong. Send me that list and I will fix each item.

## A. Needs your accounts (about 45 minutes)
1. **Google**: put the client ID and secret in `.env` (`docs/OAUTH_SETUP.md`), rebuild, Accounts → Add → Google. Expect: your events appear; remove the account; add it again.
2. **Microsoft**: same with the Microsoft ID. Also try cancelling the sign-in half way.
3. **Yahoo / iCloud mail** (needs an app password from the provider): Settings → Email → add the mailbox. Expect unread count and subjects; nothing becomes "read". Try a wrong password: expect a clear message.
4. **Sending**: Settings → Email → turn on "Send invitations from Firstlight". Make an event, Send invitation, Review and send, send it to yourself at another address. Expect the mail to match the review window and to carry an .ics that opens in your calendar.
5. **CalDAV** (iCloud, Fastmail or Nextcloud): add the account, Accounts → turn on the "Let Firstlight add events" switch, create an event with Save to = that calendar, make it repeat weekly at a fixed time. Then open the same calendar in Apple Calendar, Google Calendar or Outlook on the web: the time must be right and must stay right after a daylight-saving change (check a date past it). Edit it in Firstlight, then delete it; confirm each change shows up on the server.
6. **Mac Calendar app**: Settings → Privacy & data → "Let Firstlight add events to the Calendar app". Click Allow on the system prompt (and try Deny once in a second run). Create, edit and delete an event; check it in Calendar.app.
7. **Apple Mail digest**: Settings → Email → Apple Mail; click Allow on the Automation prompt. Expect unread and important senders.

8. **Ask your calendar** (needs AI on with your key): press Cmd/Ctrl+K, type "when is my next dentist appointment", choose the Ask row. Try also: "when am I free for 2 hours on Thursday", "what do I have next Tuesday", "show everything with Sam this month", "what is the weather" (should say it can't). Check the dates under the answer match what you meant (models can misread "next week"), and that the answer is right against your real calendar. Also try it with a local model.

9. **Meeting prep**: open a meeting that repeats (a weekly one). Expect a Prepare section with last time's date and what was written in it, the next time, what is right before and after, and a warning if it overlaps another meeting. With email on, an unread mail from the organiser or naming the meeting should appear (open the Inbox once first so it is fetched). Check it stays quiet (no section) for a one-off with nothing around it, and that no link or password from the notes shows.

10. **Conflict help**: find two overlapping meetings in your real calendar and press Sort out. Check the suggested one is sensible and the reasons are true; the free times really are free in your calendar; for a meeting someone else organised, "Write the message" opens your mail app with the times and no mention of the other meeting; for one you created in Firstlight, "Move here" changes it (in the Calendar app or CalDAV too, if you saved it there).

## B. Needs you at the machine (about 30 minutes)
1. Notifications show as "Firstlight". Click one: the app comes forward. Press Snooze 10 min: it returns.
2. Menu-bar text (Settings → General): shows the next meeting; "Meeting" when titles are hidden.
3. Join shortcut: press it with a meeting in 5 minutes: the link opens.
4. Start at login: turn on, log out, log in: it starts hidden.
5. Lock then unlock the screen: the briefing appears. Close the lid for a minute and open it: it appears (wake fallback).
6. Full screen: put a video full screen, unlock: with "skip when full screen" on, no window.
7. Screen reader: turn on VoiceOver (Cmd+F5) and move through Briefing, Calendar and Settings. Anything unnamed or in the wrong order, note it.

## C. Windows (about 30 minutes, on a Windows PC)
Install the Windows installer from the GitHub release / workflow artifact. Check: it starts; the window is readable (light and dark); the tray icon is visible on a dark taskbar; closing the window keeps it in the tray; lock (Win+L) then unlock shows the briefing; notifications appear; add an ICS or CalDAV account; Find a time and New event work. Note any crash text.

## C2. Classic Outlook on Windows (needs a PC with classic Outlook and an account in it)
Add a calendar -> "Outlook on this computer" -> Connect Outlook. Expect: Outlook may open in the background; your calendars (one per account) appear; today's events match Outlook, including a repeating meeting, an all-day event, a cancelled meeting and one you have not answered. Check the times against Outlook, in two time zones if you can. Then: with Outlook closed, press Refresh; add a meeting in Outlook and see it after the next refresh. If you use the new Outlook instead, the tile should be absent and "Sign in with Microsoft" is the way. Send me any error text.

## D. Release (your side)
Apple Developer account, Windows signing certificate, update key and hosting (`docs/RELEASE.md` sections 1 to 3), Google verification for Gmail access, an outside security review, and a legal read of `docs/PRIVACY.md`.
