/** Fixed calendar palette names. Colours themselves live in tokens.css as --cal-*. */
export const CALENDAR_COLOURS = [
  "red",
  "orange",
  "yellow",
  "green",
  "teal",
  "blue",
  "indigo",
  "purple",
  "pink",
  "brown",
] as const;

export type CalendarColour = (typeof CALENDAR_COLOURS)[number];

/** CSS value for a calendar colour name; unknown names fall back to blue. */
export function calendarColour(name: string): string {
  return `var(--cal-${(CALENDAR_COLOURS as readonly string[]).includes(name) ? name : "blue"})`;
}
