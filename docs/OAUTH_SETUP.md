# Connecting Google and Microsoft sign-in

Firstlight talks to Google and Microsoft directly from your computer, so each needs a small "app
registration" that gives Firstlight an ID. You do this once. The IDs are not passwords: they identify the app,
and sign-in is protected by PKCE. They stay out of git (`.env` is ignored).

Console layouts change; the names below are what to look for.

## Google (about 10 minutes)

1. Open <https://console.cloud.google.com/> and create a project (for example "Firstlight").
2. **APIs & Services → Library**: search "Google Calendar API" and click **Enable**.
3. **APIs & Services → OAuth consent screen** (may be called "Google Auth Platform"):
   - User type **External**. App name "Firstlight". Add your own email as support and developer contact.
   - **Scopes**: add `.../auth/calendar.readonly`, `openid` and `email`.
   - **Test users**: add the Google accounts you want to connect (while the app is in *Testing*, only they can sign in).
4. **APIs & Services → Credentials → Create credentials → OAuth client ID**:
   - Application type **Desktop app**. Name it anything.
   - Copy the **Client ID** and **Client secret**. (Google's desktop "secret" is not confidential; it still goes in `.env`.)
5. Put them in `.env`:
   ```
   GOOGLE_OAUTH_CLIENT_ID=123456-abc.apps.googleusercontent.com
   GOOGLE_OAUTH_CLIENT_SECRET=GOCSPX-...
   ```

**Know before you start:** while the app is in *Testing* status, Google expires sign-ins after **7 days**, so you will
be asked to reconnect weekly. Publishing the app removes that, but needs Google's verification (Phase 6).

## Microsoft (about 5 minutes)

1. Open <https://entra.microsoft.com/> → **Identity → Applications → App registrations → New registration**.
2. Name "Firstlight". **Supported account types**: *Accounts in any organizational directory and personal Microsoft
   accounts* (this is what lets both Outlook.com and work accounts sign in).
3. **Redirect URI**: platform **Mobile and desktop applications**, value `http://localhost`. Click **Register**.
4. On the app's page copy the **Application (client) ID**.
5. **Authentication** → under *Advanced settings* set **Allow public client flows** to **Yes**.
6. **API permissions** should list Microsoft Graph delegated `User.Read`. Add `Calendars.Read`
   (and `offline_access` if it is not already there).
7. Put it in `.env`:
   ```
   MICROSOFT_OAUTH_CLIENT_ID=00000000-0000-0000-0000-000000000000
   ```

**Know before you start:** many company (Microsoft 365 work) tenants require an administrator to approve new apps.
If a work account says "Need admin approval", that is the tenant's policy, not a bug; personal Outlook.com accounts
work without it.

## Use it

1. `cp .env.example .env` and fill in the values above.
2. Restart the app: `npm run tauri dev` (the IDs are read when the app is built, so a restart rebuilds it).
3. **Add a calendar → Google / Microsoft**. The buttons say "Setup needed" until the matching ID is present.
4. Your browser opens; choose the account and allow calendar access; return to Firstlight.

If something fails, the app shows a short plain-language message. Nothing sensitive is logged.
