# Firstlight privacy policy

*Last updated: 22 November 2026. A plain-language description of exactly what Firstlight does with your information.*

## The short version

Firstlight has **no server, no account and no analytics**. It runs entirely on your computer. It talks only to the services *you* connect, and to nobody else. We, the people who make Firstlight, never receive your calendar, your email, your tasks, your passwords or anything about how you use the app.

Every feature that sends something anywhere is **off until you turn it on**, and the ones that send text to an AI service show you **the exact text that will be sent** before you agree.

## What is stored on your computer

All of this stays on your computer, in a database that is encrypted on disk. The key to that database lives only in your system's keychain (macOS Keychain or Windows Credential Manager).

- **Your events**, from the calendars you connect, so the briefing appears instantly. This includes the guest list a calendar reports (names, addresses and answers), shown only to you.
- **Things you create in Firstlight:** your own events, tasks, private meeting notes, event templates and days off.
- **Your settings.** They contain no passwords, links or tokens.
- **Automatic backups:** encrypted copies of the database, made daily and before each update (the last 10 are kept), in the app's own folder. They use the same key as the database.
- **A short activity log** (counts and error types, for example "synced 2 calendars"). It never contains event titles, task titles, names, addresses, links or message text.
- **Optionally, a crash report**, only if you turn crash reports on. It says which version stopped and where in the code, and nothing else.

Mail you see in the briefing is held in memory for a few minutes and is not written to the database.

## What is kept in your keychain

Your connection to each service: calendar links, app passwords, sign-in permissions (tokens), AI keys you entered, the sync code for sharing between your computers, and the secret of your own Google sign-in app if you entered one. Removing an account, or choosing **Erase all data**, deletes them.

## Where information goes

Firstlight connects only to services you set up. Each has its own privacy policy, which applies to what you share with it.

- **Calendars** (Google, Microsoft, iCloud, Yahoo, any CalDAV server, calendar links, public holiday calendars) to read your events. On a Mac, Firstlight can read the Calendar app; on Windows, the classic Outlook app already on your computer. Both happen on your computer through the system, and nothing is sent anywhere.
- **Adding events to a calendar** (off by default): only if you turn it on for your Mac's Calendar app or a CalDAV account. Firstlight then changes or removes only events it created itself.
- **Task lists** (off by default): if you turn on task sync for a Google or Microsoft account, or Apple Reminders on a Mac, your tasks (titles, notes, due dates, done or not) are kept in step with that list.
- **Your mail** (off by default): if you turn email on, Firstlight reads the sender, subject and a short preview of recent unread messages, through the Mail app on a Mac, Gmail, Microsoft, Yahoo, iCloud or another mail server you name. From classic Outlook on Windows it reads only the unread count, subjects and meeting invitations. It never stores whole messages, never loads images or links, and never marks anything as read, moves or deletes mail.
- **Sending mail** (off by default): only if you turn sending on, and only a message you reviewed in full and chose to send: a meeting invitation or cancellation, a "running late" note, a meeting recap, a nudge or a status update. It goes from your own mailbox or account. Otherwise Firstlight only opens a draft in your own mail app.
- **Out-of-office reply** (off by default): if you allow it for a Google or Microsoft account, Firstlight sets that account's automatic reply, with the text you see and the dates of your time off.
- **AI services** (off by default): see "AI features" below.
- **Apple Maps** (Mac only, off by default): to estimate travel time to an in-person meeting, Firstlight sends Apple the two places involved (where you are coming from and the meeting's place).
- **Sharing between your computers** (off by default): your tasks, meeting notes, own events, templates and days off, **encrypted with a sync code only your devices have**, are kept where you choose: a folder you already sync (iCloud Drive, OneDrive, Dropbox or Google Drive); a hidden app folder in your own Google Drive or OneDrive, reached with a permission that covers only that folder and none of your other files; or nowhere at all, passed directly between your devices on the same network. Without the code the files can't be read. On the same network, each device announces itself every 30 seconds with an id made from the sync code, its device id and a port number (no personal information), and the encrypted data goes straight from one device to the other, never through the internet.
- **Sign-in** happens in your own browser, on Google's or Microsoft's own page. Firstlight never sees your Google or Microsoft password.
- **Updates:** about once a day Firstlight asks the update address (GitHub, which hosts the releases) whether a newer version exists. That request carries no personal information, though GitHub sees your internet address as with any download. You can turn this off in Settings.

## AI features (optional, off by default)

You choose the service (Anthropic, OpenAI, xAI, Google Gemini, Mistral, Groq, OpenRouter, another service you name, or a model running on your own computer) and use your own key. Agreeing to one service does not agree to another. With a model on your own computer, nothing leaves your computer.

- **Summary of your day:** sends event titles, times and places and some counts, and (only if you choose) the names and subjects of mail from your important senders. It never contains meeting links, descriptions, guest lists, message text or email addresses. You see the exact text before you agree.
- **Asking questions** in Search: only the question you typed, today's date and your time zone are sent, never your events. The answer is worked out on your computer from your own calendar.
- **Planning your day:** needs its own, separate agreement. Sends the time now, your free stretches today (times only) and your task titles with how long each takes and how many days it has waited. No events, notes or other details.

You can turn AI off at any time; removing your key withdraws your agreement.

## Things that stay on your computer

- **Do Not Disturb during meetings** (Mac) runs two shortcuts you made yourself in the Shortcuts app. Nothing is sent anywhere.
- **Notifications** show event and task names unless you turn that off in Settings. They can appear on your lock screen and in screen shares, which is why the option exists.
- **Problem reports:** "Report a problem" shows you the whole report first. It contains no calendar, email or account content. Firstlight saves it as a file; it is sent only if you attach it to an email yourself.

## Your control

- **Erase all data** (Settings, Privacy & data) removes every account, saved password and link, event, task, note and setting from your computer.
- **Removing an account** deletes its events and its keychain entries.
- **Settings export** never contains passwords, links, tokens, permissions or sign-in app details.
- To stop Firstlight reading a Google or Microsoft account, remove it here, and also revoke its access in your Google or Microsoft account settings.

## What Firstlight never does

It does not collect analytics or usage data, show advertising, sell or share information, build a profile of you, or send anything to us.

## Children

Firstlight is a general-purpose tool and is not directed at children.

## Changes

If this policy changes, the new version will be in the app and on the project's GitHub page, and the date above will change.

## Contact

Questions about privacy: open an issue on the project's GitHub page (github.com/vijaykumawat256/firstlight/issues), or use the contact address listed there.
