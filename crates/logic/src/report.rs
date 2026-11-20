//! Standup and status updates, written from the person's own tasks and calendar. No AI: plain rules, so it is
//! instant, works offline and says only what happened.
//!
//! Daily: Yesterday (the last working day: tasks finished, meetings held), Today (tasks due, meetings), Blockers
//! (tasks waiting on someone). Weekly: Done this week, Meetings (count and time), Next week (tasks and meetings),
//! Waiting on.

use crate::briefing::{BriefingSettings, day_bounds, is_shown};
use crate::model::{Event, When};
use crate::review::covered;
use crate::tasks::{Due, Task};
use chrono::{DateTime, Datelike, Duration, NaiveDate, Utc, Weekday};
use chrono_tz::Tz;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReportKind {
    Daily,
    Weekly,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Section {
    pub heading: String,
    pub lines: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Report {
    /// A subject line for an email ("Update for Thu 19 Nov", "Weekly update: 16 to 20 Nov").
    pub subject: String,
    pub sections: Vec<Section>,
    /// The whole update as plain text, ready to paste.
    pub text: String,
}

/// Most lines listed in a section; the rest are counted ("and 4 more").
const MAX_LINES: usize = 8;

fn is_workday(d: NaiveDate) -> bool {
    !matches!(d.weekday(), Weekday::Sat | Weekday::Sun)
}

/// The working day before `today` (Friday for a Monday).
pub fn previous_workday(today: NaiveDate) -> NaiveDate {
    let mut d = today - Duration::days(1);
    while !is_workday(d) {
        d -= Duration::days(1);
    }
    d
}

fn done_on(tasks: &[Task], from: DateTime<Utc>, to: DateTime<Utc>) -> Vec<String> {
    let mut done: Vec<&Task> = tasks
        .iter()
        .filter(|t| t.deleted_at.is_none())
        .filter(|t| t.done_at.is_some_and(|d| d >= from && d < to))
        .collect();
    done.sort_by_key(|t| t.done_at);
    done.iter().map(|t| t.title.clone()).collect()
}

/// Meeting titles (timed, shown, not focus time) starting in `[from, to)`, in order, one per title.
fn meetings(
    events: &[Event],
    focus: &HashSet<String>,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
    bs: &BriefingSettings,
) -> Vec<(DateTime<Utc>, DateTime<Utc>, String)> {
    let mut seen = HashSet::new();
    let mut out: Vec<_> = events
        .iter()
        .filter(|e| is_shown(e, bs) && !focus.contains(&e.id))
        .filter_map(|e| match e.when {
            When::Timed { start, end, .. } if start >= from && start < to => {
                Some((start, end, e.title.trim().to_string()))
            }
            _ => None,
        })
        .filter(|(_, _, t)| !t.is_empty())
        .collect();
    out.sort();
    out.retain(|(_, _, t)| seen.insert(t.to_lowercase()));
    out
}

/// How many meetings (timed, shown, not focus time) start in `[from, to)`: every one, repeats included.
fn meeting_count(
    events: &[Event],
    focus: &HashSet<String>,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
    bs: &BriefingSettings,
) -> usize {
    let mut seen = HashSet::new();
    events
        .iter()
        .filter(|e| is_shown(e, bs) && !focus.contains(&e.id))
        .filter(|e| matches!(e.when, When::Timed { start, .. } if start >= from && start < to))
        .filter(|e| seen.insert(e.id.clone()))
        .count()
}

fn open_due(tasks: &[Task], tz: Tz, from: NaiveDate, to: NaiveDate) -> Vec<String> {
    let mut list: Vec<&Task> = tasks
        .iter()
        .filter(|t| t.done_at.is_none() && t.deleted_at.is_none() && t.waiting_on.is_none())
        .filter(|t| t.due.is_some_and(|d| d.day(tz) >= from && d.day(tz) <= to))
        .collect();
    list.sort_by_key(|t| match t.due {
        Some(Due::At { at }) => (at.with_timezone(&tz).date_naive(), Some(at)),
        Some(Due::Date { date }) => (date, None),
        None => (NaiveDate::MAX, None),
    });
    list.iter().map(|t| t.title.clone()).collect()
}

fn waiting(tasks: &[Task], now: DateTime<Utc>) -> Vec<String> {
    let mut list: Vec<&Task> = tasks
        .iter()
        .filter(|t| t.done_at.is_none() && t.deleted_at.is_none() && t.waiting_on.is_some())
        .collect();
    list.sort_by_key(|t| t.waiting_since.unwrap_or(t.created_at));
    list.iter()
        .map(|t| {
            let days = (now - t.waiting_since.unwrap_or(t.created_at)).num_days();
            let who = t.waiting_on.as_deref().unwrap_or_default();
            if days >= 1 {
                format!(
                    "{}: waiting on {who} ({days} {})",
                    t.title,
                    if days == 1 { "day" } else { "days" }
                )
            } else {
                format!("{}: waiting on {who}", t.title)
            }
        })
        .collect()
}

fn section(heading: &str, mut lines: Vec<String>, empty: &str) -> Section {
    if lines.is_empty() {
        lines.push(empty.to_string());
    } else if lines.len() > MAX_LINES {
        let more = lines.len() - MAX_LINES;
        lines.truncate(MAX_LINES);
        lines.push(format!("and {more} more"));
    }
    Section {
        heading: heading.to_string(),
        lines,
    }
}

fn hours(minutes: i64) -> String {
    match (minutes / 60, minutes % 60) {
        (0, m) => format!("{m} min"),
        (h, 0) => format!("{h} h"),
        (h, m) => format!("{h} h {m} min"),
    }
}

/// Writes the update. `focus` holds the ids of focus-time events (left out of meetings).
pub fn build(
    kind: ReportKind,
    events: &[Event],
    focus: &HashSet<String>,
    tasks: &[Task],
    now: DateTime<Utc>,
    tz: Tz,
    bs: &BriefingSettings,
) -> Report {
    let today = now.with_timezone(&tz).date_naive();
    let fmt = |d: NaiveDate| d.format("%a %-d %b").to_string();
    let (sections, subject) = match kind {
        ReportKind::Daily => {
            let yesterday = previous_workday(today);
            let (y_from, _) = day_bounds(yesterday, tz);
            let (t_from, t_to) = day_bounds(today, tz);
            let mut done = done_on(tasks, y_from, t_from);
            done.extend(
                meetings(events, focus, y_from, t_from, bs)
                    .into_iter()
                    .map(|(_, _, t)| format!("Met: {t}")),
            );
            let mut plan = open_due(tasks, tz, NaiveDate::MIN, today);
            plan.extend(
                meetings(events, focus, t_from, t_to, bs)
                    .into_iter()
                    .map(|(_, _, t)| format!("Meeting: {t}")),
            );
            let label = if yesterday == today - Duration::days(1) {
                "Yesterday".to_string()
            } else {
                format!("Last working day ({})", yesterday.format("%A"))
            };
            (
                vec![
                    section(&label, done, "Nothing recorded"),
                    section("Today", plan, "Nothing planned yet"),
                    section("Blockers", waiting(tasks, now), "None"),
                ],
                format!("Update for {}", fmt(today)),
            )
        }
        ReportKind::Weekly => {
            let monday = today - Duration::days(i64::from(today.weekday().num_days_from_monday()));
            let next_monday = monday + Duration::days(7);
            let (w_from, _) = day_bounds(monday, tz);
            let (n_from, _) = day_bounds(next_monday, tz);
            let (n_to, _) = day_bounds(next_monday + Duration::days(7), tz);
            let held = meeting_count(events, focus, w_from, now, bs);
            let spans: Vec<_> = events
                .iter()
                .filter(|e| is_shown(e, bs) && !focus.contains(&e.id))
                .filter_map(|e| match e.when {
                    When::Timed { start, end, .. } if start < now && end > w_from => {
                        Some((start.max(w_from), end.min(now)))
                    }
                    _ => None,
                })
                .collect();
            let meeting_line = format!(
                "{} {} this week, {} in all",
                held,
                if held == 1 { "meeting" } else { "meetings" },
                hours(covered(spans))
            );
            let mut next = open_due(tasks, tz, next_monday, next_monday + Duration::days(6));
            let coming = meeting_count(events, focus, n_from, n_to, bs);
            if coming > 0 {
                next.push(format!(
                    "{} {} in the calendar",
                    coming,
                    if coming == 1 { "meeting" } else { "meetings" }
                ));
            }
            let last = today.min(monday + Duration::days(4)).max(monday);
            (
                vec![
                    section(
                        "Done this week",
                        done_on(tasks, w_from, now + Duration::seconds(1)),
                        "No tasks ticked off",
                    ),
                    section("Meetings", vec![meeting_line], ""),
                    section("Next week", next, "Nothing planned yet"),
                    section("Waiting on", waiting(tasks, now), "No one"),
                ],
                format!(
                    "Weekly update: {} to {}",
                    monday.format("%-d %b"),
                    last.format("%-d %b")
                ),
            )
        }
    };
    let mut text = String::new();
    for s in &sections {
        text.push_str(&s.heading);
        text.push('\n');
        for l in &s.lines {
            text.push_str("- ");
            text.push_str(l);
            text.push('\n');
        }
        text.push('\n');
    }
    Report {
        subject,
        sections,
        text: text.trim_end().to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Event, EventStatus, Response};

    const NY: Tz = chrono_tz::America::New_York;

    fn t(s: &str) -> DateTime<Utc> {
        s.parse().unwrap()
    }

    fn meeting(id: &str, title: &str, start: &str, end: &str) -> Event {
        Event {
            id: id.into(),
            calendar_id: "cal".into(),
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

    fn task(title: &str) -> Task {
        Task {
            id: title.into(),
            title: title.into(),
            created_at: t("2026-11-10T12:00:00Z"),
            updated_at: t("2026-11-10T12:00:00Z"),
            ..Default::default()
        }
    }

    #[test]
    fn monday_standup_looks_back_to_friday() {
        assert_eq!(
            previous_workday("2026-11-23".parse().unwrap()),
            "2026-11-20".parse::<NaiveDate>().unwrap()
        );
        assert_eq!(
            previous_workday("2026-11-19".parse().unwrap()),
            "2026-11-18".parse::<NaiveDate>().unwrap()
        );
    }

    #[test]
    fn the_daily_update_lists_yesterday_today_and_blockers() {
        let now = t("2026-11-19T14:00:00Z"); // Thursday 9:00 in New York
        let mut done = task("Send the budget");
        done.done_at = Some(t("2026-11-18T20:00:00Z"));
        let mut old = task("Ancient");
        old.done_at = Some(t("2026-11-10T20:00:00Z"));
        let mut today = task("Review slides");
        today.due = Some(Due::Date {
            date: "2026-11-19".parse().unwrap(),
        });
        let mut later = task("Book flights");
        later.due = Some(Due::Date {
            date: "2026-11-25".parse().unwrap(),
        });
        let mut wait = task("Budget numbers");
        wait.waiting_on = Some("Priya".into());
        wait.waiting_since = Some(t("2026-11-16T12:00:00Z"));
        let events = vec![
            meeting(
                "a",
                "Design review",
                "2026-11-18T15:00:00Z",
                "2026-11-18T16:00:00Z",
            ),
            meeting(
                "b",
                "Standup",
                "2026-11-19T15:00:00Z",
                "2026-11-19T15:15:00Z",
            ),
            meeting(
                "c",
                "Focus block",
                "2026-11-19T17:00:00Z",
                "2026-11-19T19:00:00Z",
            ),
        ];
        let focus: HashSet<String> = ["c".to_string()].into();
        let r = build(
            ReportKind::Daily,
            &events,
            &focus,
            &[done, old, today, later, wait],
            now,
            NY,
            &BriefingSettings::default(),
        );
        assert_eq!(r.subject, "Update for Thu 19 Nov");
        assert_eq!(r.sections[0].heading, "Yesterday");
        assert_eq!(
            r.sections[0].lines,
            ["Send the budget", "Met: Design review"]
        );
        assert_eq!(r.sections[1].lines, ["Review slides", "Meeting: Standup"]);
        assert_eq!(
            r.sections[2].lines,
            ["Budget numbers: waiting on Priya (3 days)"]
        );
        assert!(r.text.starts_with("Yesterday\n- Send the budget\n"));
        assert!(
            !r.text.contains("Focus block")
                && !r.text.contains("Ancient")
                && !r.text.contains("Book flights")
        );
    }

    #[test]
    fn the_weekly_update_counts_meetings_and_looks_at_next_week() {
        let now = t("2026-11-20T21:00:00Z"); // Friday 16:00
        let mut done = task("Ship the release");
        done.done_at = Some(t("2026-11-17T20:00:00Z"));
        let mut next = task("Quarterly plan");
        next.due = Some(Due::Date {
            date: "2026-11-24".parse().unwrap(),
        });
        let events = vec![
            meeting(
                "a",
                "Planning",
                "2026-11-16T15:00:00Z",
                "2026-11-16T16:00:00Z",
            ),
            meeting(
                "b",
                "Overlap",
                "2026-11-16T15:30:00Z",
                "2026-11-16T16:30:00Z",
            ),
            meeting(
                "c",
                "Kickoff",
                "2026-11-24T15:00:00Z",
                "2026-11-24T16:00:00Z",
            ),
        ];
        let r = build(
            ReportKind::Weekly,
            &events,
            &HashSet::new(),
            &[done, next],
            now,
            NY,
            &BriefingSettings::default(),
        );
        assert_eq!(r.subject, "Weekly update: 16 Nov to 20 Nov");
        assert_eq!(r.sections[0].lines, ["Ship the release"]);
        assert_eq!(
            r.sections[1].lines,
            ["2 meetings this week, 1 h 30 min in all"]
        );
        assert_eq!(
            r.sections[2].lines,
            ["Quarterly plan", "1 meeting in the calendar"]
        );
        assert_eq!(r.sections[3].lines, ["No one"]);
    }

    #[test]
    fn a_long_section_is_cut_with_a_count() {
        let s = section(
            "Done",
            (0..12).map(|i| format!("Task {i}")).collect(),
            "None",
        );
        assert_eq!(s.lines.len(), 9);
        assert_eq!(s.lines[8], "and 4 more");
    }

    #[test]
    fn repeated_meetings_are_each_counted_in_the_weekly_update() {
        let now = t("2026-11-20T21:00:00Z");
        let events = vec![
            meeting(
                "s1",
                "Standup",
                "2026-11-16T14:00:00Z",
                "2026-11-16T14:15:00Z",
            ),
            meeting(
                "s2",
                "Standup",
                "2026-11-17T14:00:00Z",
                "2026-11-17T14:15:00Z",
            ),
            meeting(
                "s3",
                "Standup",
                "2026-11-18T14:00:00Z",
                "2026-11-18T14:15:00Z",
            ),
            meeting(
                "r",
                "Review",
                "2026-11-19T15:00:00Z",
                "2026-11-19T16:00:00Z",
            ),
        ];
        let r = build(
            ReportKind::Weekly,
            &events,
            &HashSet::new(),
            &[],
            now,
            NY,
            &BriefingSettings::default(),
        );
        assert_eq!(
            r.sections[1].lines,
            ["4 meetings this week, 1 h 45 min in all"]
        );
    }
}
