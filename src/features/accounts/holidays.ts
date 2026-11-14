/**
 * Public holiday calendars, offered as one tap. They are ordinary calendar links (read like any other
 * link, nothing is signed in and nothing is sent about you) published by Google for each country.
 * Every address below was fetched and checked to be a real calendar before it was listed.
 */
export interface HolidayCalendar {
  id: string;
  label: string;
  url: string;
}

const link = (id: string) => `https://calendar.google.com/calendar/ical/${id}%23holiday%40group.v.calendar.google.com/public/basic.ics`;

export const HOLIDAY_CALENDARS: HolidayCalendar[] = [
  { id: "australia", label: "Australia", url: link("en.australian") },
  { id: "brazil", label: "Brazil", url: link("pt_br.brazilian") },
  { id: "canada", label: "Canada", url: link("en.canadian") },
  { id: "france", label: "France", url: link("fr.french") },
  { id: "germany", label: "Germany", url: link("de.german") },
  { id: "india", label: "India", url: link("en.indian") },
  { id: "ireland", label: "Ireland", url: link("en.irish") },
  { id: "italy", label: "Italy", url: link("it.italian") },
  { id: "japan", label: "Japan", url: link("ja.japanese") },
  { id: "malaysia", label: "Malaysia", url: link("en.malaysia") },
  { id: "netherlands", label: "Netherlands", url: link("nl.dutch") },
  { id: "new-zealand", label: "New Zealand", url: link("en.new_zealand") },
  { id: "philippines", label: "Philippines", url: link("en.philippines") },
  { id: "singapore", label: "Singapore", url: link("en.singapore") },
  { id: "spain", label: "Spain", url: link("es.spain") },
  { id: "uk", label: "United Kingdom", url: link("en.uk") },
  { id: "usa", label: "United States", url: link("en.usa") },
];
