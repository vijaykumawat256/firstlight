import type { Briefing, BriefingItem, CalEvent } from "@/api";
import { summarise } from "./summary";

const ev = (id: string, title = id): CalEvent => ({
  id, calendar_id: "c", remote_id: id, ical_uid: id, title,
  when: { kind: "timed", start: "2026-03-10T14:00:00Z", end: "2026-03-10T15:00:00Z", tzid: null },
  location: null, description: null, join_url: null, status: "confirmed", my_response: "accepted",
  organizer: null, recurrence_master_id: null, updated_at: "2026-01-01T00:00:00Z", also_in: [],
    attendees: [],
});
const item = (id: string, start: string, end: string): BriefingItem => ({ event: ev(id), start, end });

const base: Briefing = {
  generated_at: "2026-03-10T12:00:00Z", date: "2026-03-10", state: "upcoming", all_day_today: [], past: [], remaining: [], next: null,
  conflicts: [], back_to_back_runs: [], free_gaps: [], needs_response: [], tomorrow: { timed: [], all_day: [], early_start: false },
};
const now = new Date("2026-03-10T12:00:00Z");

describe("summarise", () => {
  it("handles the quiet states", () => {
    expect(summarise({ ...base, state: "clear" }, now)).toBe("Your day is clear.");
    expect(summarise({ ...base, state: "all_day_only" }, now)).toBe("Nothing scheduled, just all-day items.");
    expect(summarise({ ...base, state: "done" }, now)).toBe("You're done for today.");
  });
  it("uses a countdown inside the hour and a clock time beyond it", () => {
    const soon = item("soon", "2026-03-10T12:25:00Z", "2026-03-10T13:00:00Z");
    expect(summarise({ ...base, remaining: [soon], next: { ...soon, in_progress: false } }, now)).toBe("1 event left today. Next: soon in 25 min.");
    const later = item("later", "2026-03-10T14:00:00Z", "2026-03-10T15:00:00Z");
    expect(summarise({ ...base, remaining: [later, later], next: { ...later, in_progress: false } }, now)).toMatch(/^2 events left today\. Next: later at /);
  });
  it("says so when something is on now, and mentions overlaps", () => {
    const cur = item("cur", "2026-03-10T11:30:00Z", "2026-03-10T12:30:00Z");
    const s = summarise({ ...base, state: "in_progress", remaining: [cur], next: { ...cur, in_progress: true }, conflicts: [{ a: "a", b: "b", overlap_minutes: 30 }] }, now);
    expect(s).toContain("is on now");
    expect(s).toContain("1 overlap to sort out.");
  });
});
