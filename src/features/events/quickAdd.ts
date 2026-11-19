import type { EventInput } from "@/api";

/** What a typed line such as "Lunch with Riya friday 1pm at Cafe Nero" means. */
export interface QuickEvent {
  title: string;
  allDay: boolean;
  /** Local start (midnight of the first day when all day). */
  start: Date;
  /** Local end (midnight after the last day when all day). */
  end: Date;
  location: string | null;
}

const WEEKDAYS = ["sunday", "monday", "tuesday", "wednesday", "thursday", "friday", "saturday"];
const MONTHS = ["january", "february", "march", "april", "may", "june", "july", "august", "september", "october", "november", "december"];
/** A question ("what's on friday?") is for search or Ask, never a new event. */
const QUESTION = /^(what|whats|what's|when|where|who|how|do|does|did|am|is|are|any|show|find|list)\b/i;

const WD = "(sun|mon|tue|tues|wed|thu|thur|thurs|fri|sat)(?:day|sday|nesday|rsday|urday)?";
const MON = "(jan|feb|mar|apr|may|jun|jul|aug|sep|sept|oct|nov|dec)[a-z]*";
const T = "(noon|midday|midnight|\\d{1,2}(?:[:.]\\d{2})?(?:\\s*[ap]\\.?m\\.?)?)";
const meridiem = (t: string): "am" | "pm" | null => {
  const x = /([ap])\.?m\.?\s*$/i.exec(t);
  return x ? (x[1]!.toLowerCase() === "a" ? "am" : "pm") : null;
};

const day0 = (d: Date, add = 0) => new Date(d.getFullYear(), d.getMonth(), d.getDate() + add);
const real = (y: number, m: number, d: number): Date | null => {
  const x = new Date(y, m, d);
  return x.getFullYear() === y && x.getMonth() === m && x.getDate() === d ? x : null;
};
const weekday = (w: string) => WEEKDAYS.findIndex((d) => d.startsWith(w.slice(0, 3)));
const monthOf = (w: string) => MONTHS.findIndex((m) => m.startsWith(w.slice(0, 3)));

interface Clock {
  h: number;
  m: number;
  /** am/pm (or a colon, or 24-hour) said, so the hour is not a guess. */
  sure: boolean;
}

function clock(raw: string): Clock | null {
  const t = raw.toLowerCase().replace(/\s+/g, "");
  if (t === "noon" || t === "midday") return { h: 12, m: 0, sure: true };
  if (t === "midnight") return { h: 0, m: 0, sure: true };
  const x = /^(\d{1,2})(?:[:.](\d{2}))?([ap])?(?:\.?m\.?)?$/.exec(t);
  if (!x) return null;
  let h = Number(x[1]);
  const m = Number(x[2] ?? 0);
  if (m > 59) return null;
  const ap = x[3];
  if (ap) {
    if (h < 1 || h > 12) return null;
    if (ap === "p" && h !== 12) h += 12;
    if (ap === "a" && h === 12) h = 0;
    return { h, m, sure: true };
  }
  if (h > 23) return null;
  return { h, m, sure: x[2] !== undefined || h >= 13 || h === 0 };
}

/** An hour given without am/pm: 1 to 7 is the afternoon or evening ("at 3" is 3 pm), 8 to 11 the morning. */
const guess = (c: Clock): Clock => (c.sure || c.h >= 12 || c.h >= 8 ? c : { ...c, h: c.h + 12, sure: true });

/** Removes the first match of `re` from the text when `accept` (given its groups) agrees. */
function take(text: string, re: RegExp, accept: (g: string[]) => boolean): string {
  const x = re.exec(text);
  if (!x || !accept(x.slice(1).map((g) => g ?? ""))) return text;
  return `${text.slice(0, x.index)} ${text.slice(x.index + x[0].length)}`;
}

/**
 * Understands one line typed to make an event: a title, a day ("today", "tomorrow", "friday", "next tue", "24 dec",
 * "dec 24", "2026-12-24", "in 3 days"), a time or range ("1pm", "at 9", "13:30", "noon", "2-3pm", "from 10 to
 * 11:30") or a length ("for 45 min", "for 2 hours"), and a place after "at" or "@". A day without a time is all day;
 * a time without a day is today, or tomorrow once that time has passed. One hour unless said otherwise.
 * Returns null when there is no title, no day or time, or the line reads like a question.
 */
export function parseQuickAdd(input: string, now: Date, opts: { place?: boolean } = {}): QuickEvent | null {
  let text = ` ${input.trim().replace(/\s+/g, " ")} `;
  if (!input.trim() || input.trim().endsWith("?") || QUESTION.test(input.trim())) return null;

  let date: Date | null = null;
  let start: Clock | null = null;
  let end: Clock | null = null;
  let minutes: number | null = null;
  let allDay = false;
  let evening = false;
  const today = day0(now);

  // A range first, so "2-3pm" is not read as two separate times. Plain numbers count only after "from" or "at", or
  // when one side has am/pm or minutes ("2 to 3 people" is not a time).
  text = take(text, new RegExp(`\\s(from\\s+|at\\s+)?${T}\\s*(?:-|–|to|until|till)\\s*${T}(?=[\\s,.]|$)`, "i"), ([lead, a, b]) => {
    let s = clock(a!);
    let e = clock(b!);
    if (!s || !e || (!lead && !s.sure && !e.sure)) return false;
    const endAp = meridiem(b!);
    if (!meridiem(a!) && endAp && s.h <= 12) {
      // "2-3pm": the end's am/pm applies to the start too, unless that puts the start after the end ("11-1pm").
      const same = clock(`${a}${endAp}`);
      s = same && same.h * 60 + same.m < e.h * 60 + e.m ? same : (clock(`${a}${endAp === "pm" ? "am" : "pm"}`) ?? s);
    }
    s = guess(s);
    e = guess(e);
    if (e.h * 60 + e.m <= s.h * 60 + s.m && e.h < 12) e = { ...e, h: e.h + 12 };
    if (e.h * 60 + e.m <= s.h * 60 + s.m) return false;
    start = s;
    end = e;
    return true;
  });

  text = take(text, /\sfor\s+(an?|half an|\d+(?:\.\d+)?)\s*(h|hr|hrs|hour|hours|m|min|mins|minute|minutes)(?=[\s,.]|$)/i, ([n, unit]) => {
    const amount = n === "half an" ? 0.5 : /^an?$/i.test(n!) ? 1 : Number(n);
    const inHours = unit!.toLowerCase().startsWith("h");
    minutes = Math.round(inHours ? amount * 60 : amount);
    if (n === "half an" && !inHours) minutes = null;
    return minutes !== null && minutes >= 5 && minutes <= 24 * 60;
  });

  if (!start) {
    // A single time: with am/pm, a colon, "noon", or after "at"/"@" (so "3 people" is not 3 o'clock).
    text = take(text, new RegExp(`\\s(?:(?:at|@)\\s*)?(noon|midday|midnight|\\d{1,2}(?:[:.]\\d{2})?\\s*[ap]\\.?m\\.?)(?=[\\s,.]|$)`, "i"), ([a]) => {
      start = clock(a!);
      return !!start;
    });
  }
  if (!start) {
    text = take(text, /\s(?:at|@)\s*(\d{1,2}(?:[:.]\d{2})?)(?=[\s,.]|$)/i, ([a]) => {
      const c = clock(a!);
      start = c && guess(c);
      return !!start;
    });
  }

  text = take(text, /\sall[\s-]day(?=[\s,.]|$)/i, () => (allDay = true));
  text = take(text, /\s(?:on\s+)?(today|tonight|this evening|tomorrow|tmrw|tmr)(?=[\s,.]|$)/i, ([w]) => {
    const word = w!.toLowerCase();
    date = word.startsWith("t") && word !== "today" && word !== "tonight" ? day0(today, 1) : today;
    evening = word === "tonight" || word === "this evening";
    return true;
  });
  if (!date) {
    text = take(text, new RegExp(`\\s(?:on\\s+)?(next\\s+|this\\s+)?${WD}(?=[\\s,.]|$)`, "i"), ([next, w]) => {
      const wd = weekday(w!.toLowerCase());
      if (wd < 0) return false;
      let ahead = (wd - today.getDay() + 7) % 7;
      if (ahead === 0 && next!.trim()) ahead = 7; // "next friday" on a Friday is a week away
      date = day0(today, ahead);
      return true;
    });
  }
  if (!date) {
    text = take(text, /\sin\s+(a|one|\d{1,3})\s+(day|days|week|weeks)(?=[\s,.]|$)/i, ([n, unit]) => {
      const count = /^(a|one)$/i.test(n!) ? 1 : Number(n);
      date = day0(today, unit!.toLowerCase().startsWith("w") ? count * 7 : count);
      return true;
    });
  }
  if (!date) {
    text = take(text, /\s(?:on\s+)?(\d{4})-(\d{1,2})-(\d{1,2})(?=[\s,.]|$)/, ([y, m, d]) => {
      date = real(Number(y), Number(m) - 1, Number(d));
      return !!date;
    });
  }
  if (!date) {
    const pick = (m: number, d: number, y: string) => {
      if (m < 0) return false;
      if (y) date = real(Number(y), m, d);
      else {
        const thisYear = real(today.getFullYear(), m, d);
        date = thisYear && thisYear >= today ? thisYear : real(today.getFullYear() + 1, m, d);
      }
      return !!date;
    };
    text = take(text, new RegExp(`\\s(?:on\\s+)?(\\d{1,2})(?:st|nd|rd|th)?\\s+(?:of\\s+)?${MON}(?:\\s+(\\d{4}))?(?=[\\s,.]|$)`, "i"), ([d, m, y]) =>
      pick(monthOf(m!.toLowerCase()), Number(d), y!),
    );
    if (!date)
      text = take(text, new RegExp(`\\s(?:on\\s+)?${MON}\\s+(\\d{1,2})(?:st|nd|rd|th)?(?:,?\\s+(\\d{4}))?(?=[\\s,.]|$)`, "i"), ([m, d, y]) =>
        pick(monthOf(m!.toLowerCase()), Number(d), y!),
      );
  }

  // What is left: the title, and a place after the last " at " or "@".
  let location: string | null = null;
  if (opts.place !== false) text = take(text, /\s(?:at|@)\s+([^@]+?)\s*$/i, ([place]) => {
    location = place!.trim().replace(/[,.;]+$/, "") || null;
    return !!location;
  });
  const title = text
    .replace(/\s+/g, " ")
    .replace(/^[\s,.;:–-]+|[\s,.;:–-]+$/g, "")
    .replace(/\s+(on|at|from|for)$/i, "")
    .trim();
  if (!title || (!date && !start)) return null;

  const pickedStart = start as Clock | null;
  const pickedEnd = end as Clock | null;
  if (!pickedStart && evening) {
    // "dinner tonight": seven in the evening.
    const at = new Date((date ?? today).getFullYear(), (date ?? today).getMonth(), (date ?? today).getDate(), 19);
    return { title: cap(title), allDay: false, start: at, end: new Date(at.getTime() + 3_600_000), location };
  }
  if (!pickedStart || allDay) {
    const first = date ?? today;
    return { title: cap(title), allDay: true, start: first, end: day0(first, 1), location };
  }
  let day: Date = date ?? today;
  let s = new Date(day.getFullYear(), day.getMonth(), day.getDate(), pickedStart.h, pickedStart.m);
  if (!date && s <= now) {
    day = day0(today, 1);
    s = new Date(day.getFullYear(), day.getMonth(), day.getDate(), pickedStart.h, pickedStart.m);
  }
  const e = pickedEnd
    ? new Date(day.getFullYear(), day.getMonth(), day.getDate(), pickedEnd.h, pickedEnd.m)
    : new Date(s.getTime() + ((minutes as number | null) ?? 60) * 60_000);
  return { title: cap(title), allDay: false, start: s, end: e, location };
}

const cap = (s: string) => s.charAt(0).toUpperCase() + s.slice(1);

const pad = (n: number) => String(n).padStart(2, "0");
const ymd = (d: Date) => `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;

/** The event to save. All-day events send their last day (inclusive), as the form does. */
export function quickToInput(q: QuickEvent): EventInput {
  return {
    title: q.title,
    all_day: q.allDay,
    start: q.allDay ? ymd(q.start) : q.start.toISOString(),
    end: q.allDay ? ymd(day0(q.end, -1)) : q.end.toISOString(),
    location: q.location,
    join_url: null,
    notes: null,
    repeat: null,
    reminder_minutes: null,
    invitees: [],
  };
}

/** A task typed in one line: "call the bank friday 3pm". The day and time are optional (no day = someday). */
export interface QuickTask {
  title: string;
  /** YYYY-MM-DD */
  dueDate: string | null;
  /** HH:MM */
  dueTime: string | null;
}

/** Words that say a line in search is a task, not an event: "task …", "todo …", "to do: …". */
const TASK_PREFIX = /^\s*(?:task|todo|to-do|to do)\s*:?\s+/i;

/** In search, only a line starting with "task" or "todo" is a task. Returns the rest of the line, or null. */
export function taskLine(input: string): string | null {
  const m = TASK_PREFIX.exec(input);
  const rest = m ? input.slice(m[0].length).trim() : "";
  return rest ? rest : null;
}

/** Reads a task line; "at" is kept in the title ("buy milk at the shop"). */
export function parseQuickTask(input: string, now: Date): QuickTask | null {
  const text = input.trim().replace(/\s+/g, " ");
  if (!text) return null;
  const q = parseQuickAdd(text, now, { place: false });
  if (!q) return { title: cap(text), dueDate: null, dueTime: null };
  return { title: q.title, dueDate: ymd(q.start), dueTime: q.allDay ? null : `${pad(q.start.getHours())}:${pad(q.start.getMinutes())}` };
}
