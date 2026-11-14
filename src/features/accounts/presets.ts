/** CalDAV servers offered as one-tap presets. Keep in step with `PRESETS` in daybrief-core. */
export interface CaldavPreset {
  id: string;
  label: string;
  server: string;
  usernameHint: string;
  /** Where the user creates an app password. https only. */
  helpUrl: string;
  helpLabel: string;
}

export const CALDAV_PRESETS: CaldavPreset[] = [
  {
    id: "icloud",
    label: "iCloud",
    server: "https://caldav.icloud.com",
    usernameHint: "Your Apple ID email",
    helpUrl: "https://support.apple.com/en-us/102654",
    helpLabel: "How to make an app-specific password for iCloud",
  },
  {
    id: "yahoo",
    label: "Yahoo",
    server: "https://caldav.calendar.yahoo.com",
    usernameHint: "Your Yahoo email address",
    helpUrl: "https://help.yahoo.com/kb/SLN15241.html",
    helpLabel: "How to generate a Yahoo app password",
  },
  {
    id: "fastmail",
    label: "Fastmail",
    server: "https://caldav.fastmail.com",
    usernameHint: "Your Fastmail email address",
    helpUrl: "https://www.fastmail.help/hc/en-us/articles/360058752854-App-passwords",
    helpLabel: "How to create a Fastmail app password",
  },
];

export const OTHER_SERVER = "other";
