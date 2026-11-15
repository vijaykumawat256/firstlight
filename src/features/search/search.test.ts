import { describe, expect, it } from "vitest";
import type { CalEvent } from "@/api";
import { parseJump, searchEvents } from "./search";

const NOW = new Date(2026, 10, 14, 9, 30); // Saturday 14 November 2026

function ev(id: string, title: string, start: string, end: string, extra: Partial<CalEvent> = {}): CalEvent {
  return {
    id,
    calendar_id: "c",
    remote_id: id,
    ical_uid: id,
    title,
    when: { kind: "timed", start, end, tzid: null },
    location: null,
    description: null,
    join_url: null,
    status: "confirmed",
    my_response: "accepted",
    organizer: null,
    recurrence_master_id: null,
    updated_at: "2026-11-01T00:00:00Z",
    also_in: [],
    attendees: [],
    ...extra,
  };
}

const at = (d: number, h: number) => new Date(2026, 10, d, h).toISOString();
const EVENTS = [
  ev("past", "Sprint retro", at(12, 15), at(12, 16)),
  ev("soon", "Design review", at(14, 10), at(14, 11), { location: "Café Nero" }),
  ev("later", "Design workshop", at(20, 14), at(20, 15)),
  ev("running", "Planning", at(14, 9), at(14, 10)),
  ev("cancelled", "Design cancelled", at(15, 10), at(15, 11), { status: "cancelled" }),
  ev("declined", "Design declined", at(15, 12), at(15, 13), { my_response: "declined" }),
];

describe("searchEvents", () => {
  it("needs every word, ignores case and accents, and looks at the place too", () => {
    expect(searchEvents(EVENTS, "design", NOW).map((e) => e.id)).toEqual(["soon", "later"]);
    expect(searchEvents(EVENTS, "DESIGN review", NOW).map((e) => e.id)).toEqual(["soon"]);
    expect(searchEvents(EVENTS, "cafe", NOW).map((e) => e.id)).toEqual(["soon"]);
    expect(searchEvents(EVENTS, "design nero", NOW).map((e) => e.id)).toEqual(["soon"]);
    expect(searchEvents(EVENTS, "nothing like this", NOW)).toEqual([]);
  });

  it("puts upcoming and running first, then the past, and never shows cancelled or declined events", () => {
    expect(searchEvents(EVENTS, "n", NOW).map((e) => e.id)).toEqual(["running", "soon", "later", "past"]);
    const all = searchEvents(EVENTS, "design", NOW).map((e) => e.id);
    expect(all).not.toContain("cancelled");
    expect(all).not.toContain("declined");
  });

  it("with nothing typed lists what is coming up, and respects the limit", () => {
    expect(searchEvents(EVENTS, "  ", NOW).map((e) => e.id)).toEqual(["running", "soon", "later"]);
    expect(searchEvents(EVENTS, "n", NOW, 2)).toHaveLength(2);
  });

  it("shows an event once even if it was fetched twice", () => {
    expect(searchEvents([...EVENTS, ...EVENTS], "design", NOW)).toHaveLength(2);
  });
});

describe("parseJump", () => {
  const ymd = (d: Date | null) => (d ? `${d.getFullYear()}-${d.getMonth() + 1}-${d.getDate()}` : null);

  it("understands today, tomorrow and yesterday", () => {
    expect(ymd(parseJump("today", NOW))).toBe("2026-11-14");
    expect(ymd(parseJump(" Tomorrow ", NOW))).toBe("2026-11-15");
    expect(ymd(parseJump("yesterday", NOW))).toBe("2026-11-13");
  });

  it("understands weekdays as the next one, never today", () => {
    expect(ymd(parseJump("friday", NOW))).toBe("2026-11-20");
    expect(ymd(parseJump("sun", NOW))).toBe("2026-11-15");
    expect(ymd(parseJump("saturday", NOW))).toBe("2026-11-21");
    expect(ymd(parseJump("mon", NOW))).toBe("2026-11-16");
  });

  it("understands ISO dates and month-day dates, with or without a year", () => {
    expect(ymd(parseJump("2026-12-24", NOW))).toBe("2026-12-24");
    expect(ymd(parseJump("dec 24", NOW))).toBe("2026-12-24");
    expect(ymd(parseJump("24 December", NOW))).toBe("2026-12-24");
    expect(ymd(parseJump("Dec. 24, 2027", NOW))).toBe("2027-12-24");
    expect(ymd(parseJump("jan 5", NOW))).toBe("2027-1-5"); // already past this year: the next one
    expect(ymd(parseJump("nov 14", NOW))).toBe("2026-11-14");
  });

  it("refuses things that are not dates", () => {
    for (const q of ["", "design", "2026-02-31", "feb 30", "13/45", "2026-13-01", "sept", "mar", "marx 4", "monkey", "99 dec 2026x"]) {
      expect(parseJump(q, NOW), q).toBeNull();
    }
  });
});
