import { describe, expect, it } from "vitest";
import { parseQuickAdd, parseQuickTask, quickToInput, taskLine } from "./quickAdd";

// Wednesday 18 November 2026, 10:00 local time.
const NOW = new Date(2026, 10, 18, 10, 0);
const at = (d: number, h: number, m = 0, month = 10, y = 2026) => new Date(y, month, d, h, m);

function parse(text: string) {
  const q = parseQuickAdd(text, NOW);
  if (!q) throw new Error(`not understood: ${text}`);
  return q;
}

describe("parseQuickAdd", () => {
  it("reads a title, a weekday, a time and a place", () => {
    const q = parse("Lunch with Riya friday 1pm at Cafe Nero");
    expect(q).toEqual({ title: "Lunch with Riya", allDay: false, start: at(20, 13), end: at(20, 14), location: "Cafe Nero" });
  });

  it("understands time ranges, with the end's am/pm applying to the start", () => {
    expect(parse("Dentist tomorrow 2-3pm")).toMatchObject({ title: "Dentist", start: at(19, 14), end: at(19, 15) });
    expect(parse("Planning thu 11-1pm")).toMatchObject({ start: at(19, 11), end: at(19, 13) });
    expect(parse("Workshop from 9:30 to 11 on friday")).toMatchObject({ title: "Workshop", start: at(20, 9, 30), end: at(20, 11) });
    expect(parse("Review 13:00–14:30 tomorrow")).toMatchObject({ start: at(19, 13), end: at(19, 14, 30) });
  });

  it("takes a length", () => {
    expect(parse("Call mum today 6pm for 30 min")).toMatchObject({ title: "Call mum", start: at(18, 18), end: at(18, 18, 30) });
    expect(parse("Deep work tomorrow 9am for 2 hours")).toMatchObject({ start: at(19, 9), end: at(19, 11) });
    expect(parse("Coffee tomorrow 4pm for an hour")).toMatchObject({ end: at(19, 17) });
  });

  it("guesses the afternoon for a small hour after 'at'", () => {
    expect(parse("Meet Sam at 3 tomorrow")).toMatchObject({ title: "Meet Sam", start: at(19, 15) });
    expect(parse("Standup at 9 tomorrow")).toMatchObject({ start: at(19, 9) });
    expect(parse("Lunch tomorrow at noon")).toMatchObject({ start: at(19, 12) });
  });

  it("uses today for a time still ahead, and tomorrow once it has passed", () => {
    expect(parse("Gym 6pm")).toMatchObject({ start: at(18, 18) });
    expect(parse("Gym 8am")).toMatchObject({ start: at(19, 8) });
  });

  it("makes an all-day event for a day without a time", () => {
    expect(parse("Mum's birthday dec 24")).toEqual({ title: "Mum's birthday", allDay: true, start: at(24, 0, 0, 11), end: at(25, 0, 0, 11), location: null });
    expect(parse("Holiday 3rd of march")).toMatchObject({ allDay: true, start: at(3, 0, 0, 2, 2027) });
    expect(parse("Conference 2026-12-01")).toMatchObject({ allDay: true, start: at(1, 0, 0, 11) });
    expect(parse("Pay rent in 3 days")).toMatchObject({ allDay: true, start: at(21, 0) });
    expect(parse("Offsite friday all day")).toMatchObject({ allDay: true, start: at(20, 0) });
  });

  it("knows 'tonight' and 'next'", () => {
    expect(parse("Dinner with Ana tonight")).toMatchObject({ title: "Dinner with Ana", start: at(18, 19), end: at(18, 20) });
    expect(parse("Team sync wednesday 3pm")).toMatchObject({ start: at(18, 15) });
    expect(parse("Team sync next wednesday 3pm")).toMatchObject({ start: at(25, 15) });
  });

  it("does not mistake ordinary numbers or words for times", () => {
    expect(parse("Order 3 chairs friday")).toMatchObject({ title: "Order 3 chairs", allDay: true });
    expect(parse("Call 3 a friend tomorrow")).toMatchObject({ title: "Call 3 a friend", allDay: true });
    expect(parse("Seat 2 to 3 people friday")).toMatchObject({ title: "Seat 2 to 3 people", allDay: true });
  });

  it("returns null for questions, bare titles and empty text", () => {
    expect(parseQuickAdd("what's on friday?", NOW)).toBeNull();
    expect(parseQuickAdd("when is lunch tomorrow", NOW)).toBeNull();
    expect(parseQuickAdd("Dentist", NOW)).toBeNull();
    expect(parseQuickAdd("friday 3pm", NOW)).toBeNull();
    expect(parseQuickAdd("   ", NOW)).toBeNull();
  });
});

describe("quickToInput", () => {
  it("sends the last day of an all-day event, as the form does", () => {
    const i = quickToInput(parse("Offsite dec 24"));
    expect(i).toMatchObject({ all_day: true, start: "2026-12-24", end: "2026-12-24", reminder_minutes: null, invitees: [] });
  });

  it("sends instants for a timed event", () => {
    const i = quickToInput(parse("Lunch friday 1pm"));
    expect(i.start).toBe(at(20, 13).toISOString());
    expect(i.end).toBe(at(20, 14).toISOString());
  });
});

describe("tasks typed in one line", () => {
  it("reads an optional day and time, and keeps 'at' in the title", () => {
    expect(parseQuickTask("call the bank friday 3pm", NOW)).toEqual({ title: "Call the bank", dueDate: "2026-11-20", dueTime: "15:00" });
    expect(parseQuickTask("buy milk at the shop tomorrow", NOW)).toEqual({ title: "Buy milk at the shop", dueDate: "2026-11-19", dueTime: null });
    expect(parseQuickTask("read the report", NOW)).toEqual({ title: "Read the report", dueDate: null, dueTime: null });
    expect(parseQuickTask("  ", NOW)).toBeNull();
  });

  it("in search, only lines that start with task or todo are tasks", () => {
    expect(taskLine("task call the bank")).toBe("call the bank");
    expect(taskLine("To do: send report")).toBe("send report");
    expect(taskLine("todo")).toBeNull();
    expect(taskLine("taskforce meeting friday")).toBeNull();
    expect(taskLine("lunch friday")).toBeNull();
  });
});
