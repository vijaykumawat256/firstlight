//! Where my time went: meeting and focus time between two days, by calendar, by project and by meeting, and the
//! rows for a spreadsheet (CSV) for timesheets and billing.
//!
//! A project is a tag in the title: "[Acme] Weekly sync" or "Weekly sync #acme". Overlapping meetings each count
//! their own length in the rows (a timesheet lists what was booked); the totals count overlapping time once.

use crate::briefing::{BriefingSettings, day_bounds, is_shown};
use crate::model::{Event, When};
use chrono::{DateTime, Duration, NaiveDate, Utc};
use chrono_tz::Tz;
use serde::Serialize;
use std::collections::{HashMap, HashSet};

/// The longest stretch a report covers.
pub const MAX_DAYS: i64 = 92;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TimeRow {
    pub date: NaiveDate,
    pub start: String,
    pub end: String,
    pub minutes: i64,
    pub title: String,
    pub calendar: String,
    pub project: Option<String>,
    /// "meeting" or "focus".
    pub kind: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Total {
    pub name: String,
    pub minutes: i64,
    pub count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TimeReport {
    pub from: NaiveDate,
    pub to: NaiveDate,
    /// Meeting time with overlaps counted once.
    pub meeting_minutes: i64,
    pub focus_minutes: i64,
    pub meetings: usize,
    pub by_calendar: Vec<Total>,
    pub by_project: Vec<Total>,
    /// The meetings that took the most time (top 10).
    pub by_title: Vec<Total>,
    /// Meeting minutes per day, every day of the range.
    pub by_day: Vec<(NaiveDate, i64)>,
    pub rows: Vec<TimeRow>,
}

/// The project tag of a title: a leading "[Name]" or the first "#name".
pub fn project_of(title: &str) -> Option<String> {
    let t = title.trim();
    if let Some(rest) = t.strip_prefix('[')
        && let Some((name, _)) = rest.split_once(']')
    {
        let name = name.trim();
        if !name.is_empty() && name.chars().count() <= 40 {
            return Some(name.to_string());
        }
    }
    t.split_whitespace()
        .find_map(|w| w.strip_prefix('#'))
        .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric() && c != '-' && c != '_'))
        .filter(|w| !w.is_empty() && w.chars().count() <= 40)
        .map(|w| w.to_lowercase())
}

/// A stretch of time, start to end.
type Span = (DateTime<Utc>, DateTime<Utc>);

fn covered(mut spans: Vec<Span>) -> i64 {
    spans.sort();
    let mut total = 0;
    let mut cur: Option<(DateTime<Utc>, DateTime<Utc>)> = None;
    for (s, e) in spans {
        match cur {
            Some((cs, ce)) if s <= ce => cur = Some((cs, ce.max(e))),
            Some((cs, ce)) => {
                total += (ce - cs).num_minutes();
                cur = Some((s, e));
            }
            None => cur = Some((s, e)),
        }
    }
    total + cur.map_or(0, |(s, e)| (e - s).num_minutes())
}

fn totals(map: HashMap<String, (i64, usize)>, limit: usize) -> Vec<Total> {
    let mut v: Vec<Total> = map
        .into_iter()
        .map(|(name, (minutes, count))| Total {
            name,
            minutes,
            count,
        })
        .collect();
    v.sort_by(|a, b| b.minutes.cmp(&a.minutes).then_with(|| a.name.cmp(&b.name)));
    v.truncate(limit);
    v
}

/// The report for `from..=to` (local days). `calendars` maps calendar ids to names; `focus` holds focus-time event
/// ids. Only time up to `now` counts (a report on this week does not include the meetings still to come).
#[allow(clippy::too_many_arguments)]
pub fn build(
    events: &[Event],
    calendars: &HashMap<String, String>,
    focus: &HashSet<String>,
    from: NaiveDate,
    to: NaiveDate,
    now: DateTime<Utc>,
    tz: Tz,
    bs: &BriefingSettings,
) -> TimeReport {
    let (start, _) = day_bounds(from, tz);
    let (_, end) = day_bounds(to, tz);
    let end = end.min(now);
    let mut rows = Vec::new();
    let mut meeting_spans = Vec::new();
    let mut focus_spans = Vec::new();
    let mut by_calendar: HashMap<String, (i64, usize)> = HashMap::new();
    let mut by_project: HashMap<String, (i64, usize)> = HashMap::new();
    let mut by_title: HashMap<String, (i64, usize)> = HashMap::new();
    let mut per_day: HashMap<NaiveDate, Vec<Span>> = HashMap::new();
    let mut seen = HashSet::new();
    for e in events.iter().filter(|e| is_shown(e, bs)) {
        let When::Timed {
            start: s, end: f, ..
        } = e.when
        else {
            continue;
        };
        if s >= end || f <= start || f <= s || !seen.insert(e.id.clone()) {
            continue;
        }
        let (s, f) = (s.max(start), f.min(end));
        let minutes = (f - s).num_minutes();
        if minutes <= 0 {
            continue;
        }
        let is_focus = focus.contains(&e.id);
        let local = s.with_timezone(&tz);
        let calendar = calendars
            .get(&e.calendar_id)
            .cloned()
            .unwrap_or_else(|| "Calendar".into());
        let title = e.title.trim().to_string();
        let project = project_of(&title);
        rows.push(TimeRow {
            date: local.date_naive(),
            start: local.format("%H:%M").to_string(),
            end: f.with_timezone(&tz).format("%H:%M").to_string(),
            minutes,
            title: title.clone(),
            calendar: calendar.clone(),
            project: project.clone(),
            kind: if is_focus { "focus" } else { "meeting" },
        });
        if is_focus {
            focus_spans.push((s, f));
            continue;
        }
        meeting_spans.push((s, f));
        per_day.entry(local.date_naive()).or_default().push((s, f));
        let add = |m: &mut HashMap<String, (i64, usize)>, k: String| {
            let v = m.entry(k).or_default();
            v.0 += minutes;
            v.1 += 1;
        };
        add(&mut by_calendar, calendar);
        if let Some(p) = project {
            add(&mut by_project, p);
        }
        if !title.is_empty() {
            add(&mut by_title, title);
        }
    }
    rows.sort_by(|a, b| (a.date, &a.start, &a.title).cmp(&(b.date, &b.start, &b.title)));
    let days = (to - from).num_days().clamp(0, MAX_DAYS);
    let by_day = (0..=days)
        .map(|d| from + Duration::days(d))
        .map(|d| (d, per_day.remove(&d).map(covered).unwrap_or(0)))
        .collect();
    TimeReport {
        from,
        to,
        meeting_minutes: covered(meeting_spans.clone()),
        focus_minutes: covered(focus_spans),
        meetings: meeting_spans.len(),
        by_calendar: totals(by_calendar, 20),
        by_project: totals(by_project, 20),
        by_title: totals(by_title, 10),
        by_day,
        rows,
    }
}

/// One spreadsheet cell: quoted, quotes doubled, and a leading = + - @ (or tab/return) defused with an apostrophe so
/// a spreadsheet never runs a title as a formula.
fn cell(s: &str) -> String {
    let s: String = s
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    let s = if s.starts_with(['=', '+', '-', '@']) {
        format!("'{s}")
    } else {
        s
    };
    format!("\"{}\"", s.replace('"', "\"\""))
}

/// The rows as CSV (Date, Start, End, Minutes, Hours, Title, Calendar, Project, Kind), with a header line.
pub fn csv(r: &TimeReport) -> String {
    let mut out = String::from("Date,Start,End,Minutes,Hours,Title,Calendar,Project,Kind\r\n");
    for row in &r.rows {
        out.push_str(&format!(
            "{},{},{},{},{:.2},{},{},{},{}\r\n",
            row.date,
            row.start,
            row.end,
            row.minutes,
            row.minutes as f64 / 60.0,
            cell(&row.title),
            cell(&row.calendar),
            cell(row.project.as_deref().unwrap_or("")),
            row.kind
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{EventStatus, Response};

    const NY: Tz = chrono_tz::America::New_York;

    fn t(s: &str) -> DateTime<Utc> {
        s.parse().unwrap()
    }

    fn ev(id: &str, cal: &str, title: &str, start: &str, end: &str) -> Event {
        Event {
            id: id.into(),
            calendar_id: cal.into(),
            remote_id: id.into(),
            ical_uid: id.into(),
            title: title.into(),
            when: When::Timed {
                start: t(start),
                end: t(end),
                tzid: None,
            },
            location: None,
            description: None,
            join_url: None,
            status: EventStatus::Confirmed,
            my_response: Response::Accepted,
            organizer: None,
            recurrence_master_id: None,
            updated_at: t(start),
            also_in: vec![],
            attendees: vec![],
        }
    }

    #[test]
    fn projects_come_from_a_bracket_or_a_hashtag() {
        assert_eq!(project_of("[Acme] Weekly sync").as_deref(), Some("Acme"));
        assert_eq!(
            project_of("Weekly sync #Acme-2.").as_deref(),
            Some("acme-2")
        );
        assert_eq!(project_of("Lunch"), None);
        assert_eq!(project_of("[] nothing"), None);
    }

    #[test]
    fn time_is_totalled_by_calendar_project_and_meeting_with_overlaps_once() {
        let cals: HashMap<String, String> = [
            ("w".to_string(), "Work".to_string()),
            ("p".to_string(), "Personal".to_string()),
        ]
        .into();
        let events = vec![
            ev(
                "a",
                "w",
                "[Acme] Kickoff",
                "2026-11-16T15:00:00Z",
                "2026-11-16T16:00:00Z",
            ),
            ev(
                "b",
                "w",
                "Standup",
                "2026-11-16T15:30:00Z",
                "2026-11-16T16:00:00Z",
            ),
            ev(
                "c",
                "w",
                "Standup",
                "2026-11-17T15:30:00Z",
                "2026-11-17T16:00:00Z",
            ),
            ev(
                "d",
                "p",
                "Dentist",
                "2026-11-18T20:00:00Z",
                "2026-11-18T21:00:00Z",
            ),
            ev(
                "f",
                "local-events",
                "Deep work",
                "2026-11-17T17:00:00Z",
                "2026-11-17T19:00:00Z",
            ),
            ev(
                "z",
                "w",
                "Next week",
                "2026-11-24T15:00:00Z",
                "2026-11-24T16:00:00Z",
            ),
        ];
        let focus: HashSet<String> = ["f".to_string()].into();
        let r = build(
            &events,
            &cals,
            &focus,
            "2026-11-16".parse().unwrap(),
            "2026-11-20".parse().unwrap(),
            t("2026-11-20T22:00:00Z"),
            NY,
            &BriefingSettings::default(),
        );
        assert_eq!(r.meetings, 4);
        assert_eq!(r.meeting_minutes, 60 + 30 + 60); // the overlap on Monday once
        assert_eq!(r.focus_minutes, 120);
        assert_eq!(
            r.by_calendar[0],
            Total {
                name: "Work".into(),
                minutes: 120,
                count: 3
            }
        );
        assert_eq!(
            r.by_project,
            vec![Total {
                name: "Acme".into(),
                minutes: 60,
                count: 1
            }]
        );
        assert_eq!(
            r.by_title
                .iter()
                .find(|t| t.name == "Standup")
                .unwrap()
                .count,
            2
        );
        assert_eq!(r.by_day.len(), 5);
        assert_eq!(r.by_day[0].1, 60);
        assert_eq!(r.rows.len(), 5);
        assert!(r.rows.iter().all(|row| row.title != "Next week"));
        assert_eq!(
            r.rows
                .iter()
                .find(|row| row.title == "Deep work")
                .unwrap()
                .kind,
            "focus"
        );
    }

    #[test]
    fn the_csv_quotes_cells_and_never_lets_a_title_run_as_a_formula() {
        let cals: HashMap<String, String> =
            [("w".to_string(), "Work, \"main\"".to_string())].into();
        let events = vec![ev(
            "a",
            "w",
            "=HYPERLINK(\"x\")",
            "2026-11-16T15:00:00Z",
            "2026-11-16T15:45:00Z",
        )];
        let r = build(
            &events,
            &cals,
            &HashSet::new(),
            "2026-11-16".parse().unwrap(),
            "2026-11-16".parse().unwrap(),
            t("2026-11-20T22:00:00Z"),
            NY,
            &BriefingSettings::default(),
        );
        let text = csv(&r);
        let mut lines = text.lines();
        assert_eq!(
            lines.next(),
            Some("Date,Start,End,Minutes,Hours,Title,Calendar,Project,Kind")
        );
        assert_eq!(
            lines.next(),
            Some(
                "2026-11-16,10:00,10:45,45,0.75,\"'=HYPERLINK(\"\"x\"\")\",\"Work, \"\"main\"\"\",\"\",meeting"
            )
        );
    }
}
