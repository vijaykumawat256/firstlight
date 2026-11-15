import { useState } from "react";
import type { Attendee } from "@/api";

const SHOWN = 6;

const ANSWER: Record<Attendee["response"], string> = {
  accepted: "Accepted",
  declined: "Declined",
  tentative: "Maybe",
  needs_action: "No reply yet",
  none: "",
};

/** "5 invited: 2 accepted, 1 maybe, 1 declined, 1 waiting". */
function summary(people: Attendee[]): string {
  const guests = people.filter((p) => !p.organizer || people.length === 1);
  const count = (r: Attendee["response"]) => guests.filter((p) => p.response === r).length;
  const parts = [
    [count("accepted"), "accepted"],
    [count("tentative"), "maybe"],
    [count("declined"), "declined"],
    [count("needs_action"), "waiting"],
  ]
    .filter(([n]) => (n as number) > 0)
    .map(([n, w]) => `${n} ${w}`);
  return `${people.length} ${people.length === 1 ? "person" : "people"}${parts.length ? `: ${parts.join(", ")}` : ""}`;
}

/** Who is on the event and how they answered, as the calendar reports it. Text only; nothing here is sent anywhere. */
export function Guests({ people }: { people: Attendee[] }) {
  const [all, setAll] = useState(false);
  if (people.length === 0) return null;
  const shown = all ? people : people.slice(0, SHOWN);
  return (
    <section aria-label="People" className="flex flex-col gap-2">
      <h3 className="text-footnote font-semibold uppercase tracking-wide text-fg-2">{summary(people)}</h3>
      <ul className="flex flex-col">
        {shown.map((p) => (
          <li key={p.email} className="flex items-baseline justify-between gap-3 py-1">
            <span className="min-w-0">
              <span className="block truncate select-text text-body text-fg">{p.name ?? p.email}</span>
              {p.name && <span className="block truncate select-text text-footnote text-fg-2">{p.email}</span>}
            </span>
            <span className={`shrink-0 text-footnote ${p.response === "declined" ? "text-danger" : "text-fg-2"}`}>
              {p.organizer ? "Organiser" : ANSWER[p.response]}
            </span>
          </li>
        ))}
      </ul>
      {people.length > SHOWN && (
        <button type="button" className="self-start text-body text-accent hover:underline" onClick={() => setAll((v) => !v)}>
          {all ? "Show fewer" : `Show all ${people.length}`}
        </button>
      )}
    </section>
  );
}
