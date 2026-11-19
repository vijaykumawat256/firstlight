import type { EventTemplate } from "@/api";

/** Ready-made kinds of meeting. Their agenda goes into the meeting's private note when the event is added. */
export const BUILT_IN_TEMPLATES: EventTemplate[] = [
  { name: "1:1", minutes: 30, agenda: "- How are things going?\n- Updates since last time\n- Blockers\n- Follow-ups" },
  { name: "Interview", minutes: 60, agenda: "- Introductions\n- Their experience\n- Questions for them\n- Their questions\n- Next steps" },
  { name: "Standup", minutes: 15, agenda: "- Yesterday\n- Today\n- Anything blocking" },
  { name: "Kickoff", minutes: 60, agenda: "- Goals\n- Scope and what is out of scope\n- Who does what\n- Dates\n- Risks\n- Next steps" },
];

/** Yours first (so one of yours with the same name wins), then the built-in ones not shadowed. */
export function allTemplates(mine: EventTemplate[]): EventTemplate[] {
  const names = new Set(mine.map((t) => t.name.toLowerCase()));
  return [...mine, ...BUILT_IN_TEMPLATES.filter((t) => !names.has(t.name.toLowerCase()))];
}

/** The template an event title starts with: "1:1 with Sam" → 1:1, "Standup" → Standup. Longest name wins. */
export function templateFor(title: string, templates: EventTemplate[]): EventTemplate | null {
  const t = title.trim().toLowerCase();
  const match = templates
    .filter((x) => {
      const n = x.name.toLowerCase();
      return t === n || t.startsWith(`${n} `) || t.startsWith(`${n}:`) || t.startsWith(`${n},`);
    })
    .sort((a, b) => b.name.length - a.name.length);
  return match[0] ?? null;
}
