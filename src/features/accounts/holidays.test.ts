import { describe, expect, it } from "vitest";
import { HOLIDAY_CALENDARS } from "./holidays";

describe("holiday calendars", () => {
  it("are unique, sorted by name, and https links to a calendar file", () => {
    const ids = HOLIDAY_CALENDARS.map((h) => h.id);
    expect(new Set(ids).size).toBe(ids.length);
    const labels = HOLIDAY_CALENDARS.map((h) => h.label);
    expect([...labels].sort((a, b) => a.localeCompare(b))).toEqual(labels);
    for (const h of HOLIDAY_CALENDARS) {
      expect(h.url).toMatch(/^https:\/\/calendar\.google\.com\/calendar\/ical\/[A-Za-z0-9._%-]+\/public\/basic\.ics$/);
      expect(h.url).toContain("%23holiday%40group.v.calendar.google.com");
    }
  });
});
