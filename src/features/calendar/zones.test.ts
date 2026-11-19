import { describe, expect, it } from "vitest";
import { parseZoneQuestion, timeIn, zoneLabel } from "./zones";

describe("time zones", () => {
  it("label a zone by its city", () => {
    expect(zoneLabel("America/New_York")).toBe("New York");
    expect(zoneLabel("UTC")).toBe("UTC");
  });

  it("show a time in another zone, with minutes only when needed", () => {
    const t = new Date(Date.UTC(2026, 10, 18, 15, 0));
    expect(timeIn(t, "Europe/London")).toMatch(/^3\s?PM$/);
    expect(timeIn(t, "Asia/Kolkata")).toMatch(/^8:30\s?PM$/);
  });

  it("understand '3pm in London' as that moment", () => {
    const now = new Date(Date.UTC(2026, 10, 18, 9, 0));
    expect(parseZoneQuestion("3pm in London", now)?.at.toISOString()).toBe("2026-11-18T15:00:00.000Z");
    expect(parseZoneQuestion("9:30 am tokyo time", now)?.at.toISOString()).toBe("2026-11-19T00:30:00.000Z");
    expect(parseZoneQuestion("15:00 in Kolkata", now)?.zone).toBe("Asia/Kolkata");
    // London in summer is an hour ahead of UTC.
    expect(parseZoneQuestion("3pm in London", new Date(Date.UTC(2026, 6, 1, 9, 0)))?.at.toISOString()).toBe("2026-07-01T14:00:00.000Z");
    expect(parseZoneQuestion("3pm in Atlantis", now)).toBeNull();
    expect(parseZoneQuestion("lunch with Sam", now)).toBeNull();
    expect(parseZoneQuestion("3 people", now)).toBeNull();
  });
});
