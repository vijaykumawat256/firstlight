/** A friendly name for a video-call link, for places where the raw address would be noise. */
const NAMES: [string, string][] = [
  ["zoom.us", "Zoom"],
  ["zoomgov.com", "Zoom"],
  ["zoom.com", "Zoom"],
  ["meet.google.com", "Google Meet"],
  ["teams.microsoft.com", "Microsoft Teams"],
  ["teams.microsoft.us", "Microsoft Teams"],
  ["teams.live.com", "Microsoft Teams"],
  ["webex.com", "Webex"],
  ["whereby.com", "Whereby"],
  ["meet.jit.si", "Jitsi Meet"],
  ["bluejeans.com", "BlueJeans"],
  ["chime.aws", "Amazon Chime"],
  ["gotomeeting.com", "GoToMeeting"],
  ["meet.goto.com", "GoToMeeting"],
  ["join.skype.com", "Skype"],
];

/** "Zoom", "Google Meet"…, or the site's name for any other https link; null if it is not a web link. */
export function videoServiceName(url: string): string | null {
  let host: string;
  try {
    const u = new URL(url);
    if (u.protocol !== "https:") return null;
    host = u.hostname.toLowerCase();
  } catch {
    return null;
  }
  const known = NAMES.find(([h]) => host === h || host.endsWith(`.${h}`));
  return known ? known[1] : host.replace(/^www\./, "");
}

/** What a row should say for an event's place: a link becomes "Zoom call" instead of the address. */
export function placeLabel(location: string | null): string | null {
  if (!location) return null;
  const t = location.trim();
  if (!/^https:\/\/\S+$/i.test(t)) return location;
  const name = videoServiceName(t);
  return name ? `${name} link` : location;
}

const LINK_RE = /https?:\/\/[^\s<>"')\]]+/i;

/** The first web address in some text, without the full stop or comma a sentence puts after it. */
export function firstLink(text: string | null | undefined): string | null {
  const m = text ? LINK_RE.exec(text) : null;
  return m ? m[0].replace(/[.,;:!?]+$/, "") : null;
}

/**
 * Any link an event carries: the meeting link, otherwise the first address in its place or notes.
 * Copying it is always safe (nothing is opened); only a recognised meeting link gets a Join button.
 */
export function eventLink(e: { join_url: string | null; location: string | null; description: string | null }): string | null {
  return e.join_url ?? firstLink(e.location) ?? firstLink(e.description);
}
