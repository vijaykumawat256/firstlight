import { fixtureEvents } from "@/api/fixture";
import type { CalEvent } from "@/api";
import { buildBriefing, isShown, startOfDay } from "./present";

const at = (h: number, m = 0, dayOffset = 0) => {
  const d = startOfDay(new Date(2026, 2, 10), dayOffset);
  d.setHours(h, m, 0, 0);
  return d;
};
const base = new Date(2026, 2, 10, 12, 0);

const mk = (id: string, from: Date, to: Date, extra: Partial<CalEvent> = {}): CalEvent => ({
  id,
  calendar_id: "c",
  remote_id: id,
  ical_uid: id,
  title: id,
  when: { kind: "timed", start: from.toISOString(), end: to.toISOString(), tzid: null },
  location: null,
  description: null,
  join_url: null,
  status: "confirmed",
  my_response: "accepted",
  organizer: null,
  recurrence_master_id: null,
  updated_at: "2026-01-01T00:00:00Z",
  ...extra,
});

const allDay = (id: string, start: string, end: string): CalEvent => ({
  ...mk(id, at(0), at(0)),
  when: { kind: "all_day", start, end },
});

describe("buildBriefing", () => {
  it("handles an empty day", () => {
    const m = buildBriefing([], base);
    expect(m.summary).toBe("Your day is clear.");
    expect(m.next).toBeNull();
    expect(m.tomorrow.timed).toHaveLength(0);
  });

  it("says you're done when every event has ended", () => {
    const m = buildBriefing([mk("a", at(9), at(10))], base);
    expect(m.summary).toBe("You're done for today.");
    expect(m.past).toHaveLength(1);
    expect(m.remaining).toHaveLength(0);
  });

  it("splits past from remaining and picks the next event", () => {
    const m = buildBriefing([mk("a", at(9), at(10)), mk("b", at(14), at(15)), mk("c", at(16), at(17))], base);
    expect(m.past.map((t) => t.event.id)).toEqual(["a"]);
    expect(m.next?.event.id).toBe("b");
    expect(m.summary).toContain("2 events left today.");
    expect(m.summary).toContain("Next: b at");
  });

  it("uses a countdown when the next event is under an hour away", () => {
    const m = buildBriefing([mk("soon", at(12, 25), at(13))], base);
    expect(m.summary).toContain("in 25 min");
  });

  it("treats an event in progress as next and says so", () => {
    const m = buildBriefing([mk("now", at(11, 30), at(12, 30))], base);
    expect(m.nextInProgress).toBe(true);
    expect(m.summary).toContain("is on now");
  });

  it("detects overlapping events as conflicts but not back-to-back ones", () => {
    const overlap = buildBriefing([mk("a", at(13), at(14)), mk("b", at(13, 30), at(14, 30))], base);
    expect(overlap.conflicts).toHaveLength(1);
    const b2b = buildBriefing([mk("a", at(13), at(14)), mk("b", at(14), at(15))], base);
    expect(b2b.conflicts).toHaveLength(0);
  });

  it("ignores conflicts that are already over", () => {
    const m = buildBriefing([mk("a", at(9), at(10)), mk("b", at(9, 30), at(10, 30))], base);
    expect(m.conflicts).toHaveLength(0);
  });

  it("hides cancelled and declined events", () => {
    const m = buildBriefing(
      [
        mk("x", at(13), at(14), { status: "cancelled" }),
        mk("y", at(15), at(16), { my_response: "declined" }),
        mk("z", at(17), at(18)),
      ],
      base,
    );
    expect(m.remaining.map((t) => t.event.id)).toEqual(["z"]);
    expect(isShown(mk("t", at(1), at(2), { status: "tentative" }))).toBe(true);
  });

  it("keeps an event that crosses midnight on both days", () => {
    const night = mk("night", at(22), at(2, 0, 1));
    expect(buildBriefing([night], base).remaining).toHaveLength(1);
    expect(buildBriefing([night], base).tomorrow.timed).toHaveLength(1);
  });

  it("reports all-day-only days", () => {
    const m = buildBriefing([allDay("bday", "2026-03-10", "2026-03-11")], base);
    expect(m.allDayToday).toHaveLength(1);
    expect(m.summary).toBe("Nothing scheduled, just all-day items.");
  });

  it("does not show a multi-day all-day event once it has ended", () => {
    const m = buildBriefing([allDay("trip", "2026-03-07", "2026-03-10")], base);
    expect(m.allDayToday).toHaveLength(0);
  });

  it("flags an early start tomorrow and handles an empty tomorrow", () => {
    expect(buildBriefing([mk("e", at(6, 45, 1), at(9, 30, 1))], base).tomorrow.earlyStart).toBe(true);
    expect(buildBriefing([mk("l", at(10, 0, 1), at(11, 0, 1))], base).tomorrow.earlyStart).toBe(false);
    expect(buildBriefing([mk("a", at(14), at(15))], base).tomorrow.timed).toHaveLength(0);
  });

  it("collects unanswered invitations", () => {
    const m = buildBriefing([mk("q", at(14), at(15), { my_response: "needs_action" })], base);
    expect(m.needsResponse).toHaveLength(1);
  });

  it("works on the full fixture dataset", () => {
    // 09:30: the 10:30 design review and 11:00 1:1 are still ahead and overlap.
    const morning = new Date();
    morning.setHours(9, 30, 0, 0);
    const m = buildBriefing(fixtureEvents(morning), morning);
    expect(m.conflicts.length).toBeGreaterThan(0);
    expect(m.allDayToday).toHaveLength(1);
    expect(m.remaining.every((t) => isShown(t.event))).toBe(true);
    expect(m.tomorrow.earlyStart).toBe(true);
  });
});
