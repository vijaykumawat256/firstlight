# Workday assistant plan (approved 2026-11-18)

The user approved this plan with "go with your recommendations, yes task sync also". Build in order; after each step run every check, commit (never push without the user's "push"), then go on.

Rules for every part: stored only on this computer in the encrypted database; erased by "Erase all data"; never sent to AI or a provider unless the part says so and the person agreed; every new notification has a switch in Settings and follows quiet hours, workdays and focus time; unit tests, browser tests and the Windows build check for each part.

Decisions taken (the recommendations):
1. Tasks live on this computer; sync with Google Tasks and Microsoft To Do is added (step 7) as an opt-in per account.
2. Wrap-up notification on by default; break nudge and stop-work reminder off.
3. Travel: a manual buffer with no addresses sent; maps (Apple Maps, Mac only) opt-in and off by default.
4. Task titles are not part of the AI summary. "Plan my day" sends task titles, lengths and free stretches only, behind its own consent.
5. Order as below.
6. Task sync: Google and Microsoft, one at a time.
7. Emails: sent with one tap after the exact message is shown, never fully automatically.
8. Maps: Mac only to start.
9. AI plan: opt-in with its own consent screen.
10. Phone version: needs the user's decisions on docs/PHASE7_DESIGN.md (hosting, on-call, pricing); not built until then.

| Step | Part | Contents | State |
|---|---|---|---|
| 1 | A To-do list | tasks table, due date/time, carry-over, briefing section, Tasks page, quick add "task ...", plan it into a free stretch, note line to task, reminders | done |
| 2 | E5 AI plan my day | suggestions for today's free stretches; sends task titles + lengths + free stretches only; own consent; accept blocks time | done |
| 3 | B Daily rhythm | end-of-day wrap-up (on), break nudge (off), lunch protection, stop-work reminder (off) | done |
| 4 | C Meeting flow | follow-ups after meetings with notes, running late (mail draft), templates | done |
| 5 | E3 Sending emails | one-tap send after preview for running late / follow-ups: Yahoo, iCloud (exists), Microsoft Mail.Send, Gmail gmail.send (restricted scope) | done |
| 6 | D Context (+E4) | extra time zones, travel buffer, weekly review; Apple Maps travel times on Mac, opt-in | done |
| 7 | E1 Task sync | Google Tasks (date only, no time) and Microsoft To Do, opt-in per account, last change wins | done |
| 8 | E2 Phone version | Phase 7; waiting for the user's decisions | waiting |
