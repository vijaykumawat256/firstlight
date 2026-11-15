# Firstlight privacy policy

*Last updated: November 2026. This is a plain-language description of exactly what Firstlight does with your information.*

## The short version

Firstlight has **no server, no account and no analytics**. It runs entirely on your computer. It talks only to the calendar and email services *you* connect, and to nobody else. We (the makers) never receive your calendar, your email, your passwords or anything about how you use the app.

## What is stored on your computer

- **Your events**, so the briefing appears instantly. They are kept in a database that is encrypted on disk. The key to that database lives only in your computer's keychain (macOS Keychain, Windows Credential Manager).
- **Your settings.** They contain no passwords, links or tokens, so a backup of your settings is always safe to keep.
- **A short activity log** (counts and error types, for example "synced 2 calendars"). It never contains event titles, names, addresses, links or message text.
- **Optionally, a crash report.** Only if you switch this on. It says which version stopped and where in the code, and nothing else. It stays on your computer; you can read it, copy it, or delete it.

## What is kept in your keychain

Your connection to each calendar: calendar links, app passwords, and sign-in permissions (tokens). If you use AI summaries with your own key, that key is kept here too. Removing an account, or choosing **Erase all data**, deletes these.

## Where information goes

Firstlight connects only to services you set up:

- **Your calendar providers** (Google, Microsoft, iCloud, Yahoo, or any calendar server or link you add) to read your events. By default Firstlight only reads. Two switches, both off until you turn them on, let it add the events you create to your Mac's Calendar app or to a CalDAV account you choose (iCloud, Yahoo, Fastmail, Nextcloud); it then changes or removes only the events it added itself, never anything you or someone else created. On Windows, Firstlight can read the calendars in the classic Outlook app already on your computer (read-only, nothing is sent anywhere; Outlook may open in the background).
- **Your mail provider**, only if you turn email on. Firstlight reads the sender, subject and a short preview of recent unread messages. It never stores whole messages, never loads images or links in them, and never changes or moves mail. It sends a message only if you turn on "Send invitations from Firstlight", and then only the invitation you reviewed, from your own Yahoo or iCloud mailbox, when you press Send. You can add events of your own; they are stored only on this computer, and are never sent to Google, Microsoft or any other service. If you share one with people, Firstlight opens a draft in your own mail app and saves a calendar file to your Downloads folder; the message is sent by your mail app, not by Firstlight. The Mail app on a Mac is read through macOS itself. For Yahoo, iCloud and other providers you name, Firstlight connects directly to that provider's mail server over an encrypted connection using an app-specific password that you create; the password is kept in your system keychain. For those it reads only the sender, subject and date (no preview) and never marks anything as read.
- **An AI service you choose** (Anthropic, OpenAI, xAI, Google, Mistral, Groq, OpenRouter, or an address you enter), only if you turn on AI summaries with your own key or, for OpenRouter, by signing in. See below.
- **The update server**, to learn whether a newer Firstlight exists. This sends only the app's version and your operating system, about once a day, and you can turn it off in Settings.

Each of these services has its own privacy policy, which applies to what you share with them.

## AI summaries (optional, off by default)

If you turn them on, a short summary of your day is written by an AI service you choose: Anthropic, OpenAI (ChatGPT models), xAI (Grok), Google Gemini, Mistral, Groq, OpenRouter, or another service you name, each with your own API key (or, for OpenRouter, by signing in to your account there), or a model running on your own computer. Each service is its own choice: agreeing to one does not agree to another, and for a service you name yourself the agreement is tied to its address, so changing the address turns AI off until you agree again. Before you agree, Firstlight shows you **the exact text that will be sent**. It contains event titles, times and places, counts, and (only if you choose) the names and subjects of mail from your important senders. It never contains meeting links, descriptions, attendees, message text or email addresses. It is sent about every five minutes while the briefing is open. With a model on your own computer, nothing leaves your computer. You can turn AI off at any time, and removing your key withdraws your agreement.

## Notifications

Notifications show event names unless you turn that off in Settings. Notifications can appear on your lock screen and in screen shares; the option exists for that reason.

## Your control

- **Erase all data** (Settings, Privacy) removes every account, saved password and link, event and setting from your computer.
- **Removing an account** deletes its events and its keychain entries.
- To stop Firstlight reading a calendar or mail account, remove the account here, and revoke its access in your Google or Microsoft account settings.

## What Firstlight never does

It does not collect analytics or usage data, show advertising, sell or share information, build a profile of you, or send anything to us.

## Children

Firstlight is a general-purpose tool and is not directed at children.

## Changes

If this policy changes, the new version will be in the app and the date above will change.

## Contact

Questions about privacy: add your contact address here before publishing.
