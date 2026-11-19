//! Tasks: the person's own to-do list, kept on this computer.
//!
//! A task has a title, optionally a day it is due (with or without a time), an estimate of how long it takes and a
//! note. Nothing is ever moved: a task that was due on an earlier day and is not done simply shows among today's,
//! marked with how many days it has waited ("carried over"). This module holds the rules (checking what was typed,
//! grouping into Today / Upcoming / Someday / Done, which reminders are due); storage is in `firstlight-core`.

use crate::{Error, Result};
use chrono::{DateTime, Duration, NaiveDate, NaiveTime, Utc};
use chrono_tz::Tz;
use serde::{Deserialize, Serialize};

pub const MAX_TITLE: usize = 300;
pub const MAX_NOTES: usize = 4000;
/// Most tasks kept (done ones included) so a runaway script on the page cannot fill the disk.
pub const MAX_TASKS: usize = 5000;
/// Done tasks are listed for this many days.
pub const DONE_DAYS: i64 = 30;
/// A task reminder still shows if the app was asleep, for up to this long after its time.
const REMINDER_GRACE_MINUTES: i64 = 60;

/// When a task is due. A day alone, or a day and a time (stored as the instant).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Due {
    Date { date: NaiveDate },
    At { at: DateTime<Utc> },
}

impl Due {
    /// The local day it falls on.
    pub fn day(&self, tz: Tz) -> NaiveDate {
        match self {
            Due::Date { date } => *date,
            Due::At { at } => at.with_timezone(&tz).date_naive(),
        }
    }
}

/// Where a task came from or is linked to elsewhere. Filled by sync (Google Tasks / Microsoft To Do).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskLink {
    /// The connected account the task is kept in.
    pub account_id: String,
    /// That service's list id and task id.
    pub list_id: String,
    pub remote_id: String,
    /// What the service last said about it (its own change stamp), so a change on either side can be told apart.
    #[serde(default)]
    pub remote_updated: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub notes: Option<String>,
    #[serde(default)]
    pub due: Option<Due>,
    /// How long it is expected to take, in minutes (used when planning it into a free stretch).
    #[serde(default)]
    pub minutes: Option<u32>,
    #[serde(default)]
    pub done_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    /// The meeting it was written down in, when it came from meeting notes (the meeting's title).
    #[serde(default)]
    pub from_meeting: Option<String>,
    /// The event made to do it (an own event id), when it was planned into the calendar.
    #[serde(default)]
    pub planned_event: Option<String>,
    #[serde(default)]
    pub link: Option<TaskLink>,
}

/// What the page sends when a task is added or changed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskInput {
    pub title: String,
    #[serde(default)]
    pub notes: Option<String>,
    /// `YYYY-MM-DD`.
    #[serde(default)]
    pub due_date: Option<String>,
    /// `HH:MM` on `due_date` (local time). Ignored without a date.
    #[serde(default)]
    pub due_time: Option<String>,
    #[serde(default)]
    pub minutes: Option<u32>,
    #[serde(default)]
    pub from_meeting: Option<String>,
}

fn tidy(s: Option<&str>, max: usize, too_long: &'static str) -> Result<Option<String>> {
    let t: String = s
        .unwrap_or("")
        .replace("\r\n", "\n")
        .chars()
        .filter(|c| !c.is_control() || *c == '\n' || *c == '\t')
        .collect();
    let t = t.trim();
    if t.chars().count() > max {
        return Err(Error::Invalid(too_long));
    }
    Ok((!t.is_empty()).then(|| t.to_string()))
}

/// The checked fields of a task: title, notes, due, minutes, meeting.
pub struct Checked {
    pub title: String,
    pub notes: Option<String>,
    pub due: Option<Due>,
    pub minutes: Option<u32>,
    pub from_meeting: Option<String>,
}

/// Checks what was typed. A time needs a day; the time is read in `tz`.
pub fn check(input: &TaskInput, tz: Tz) -> Result<Checked> {
    let title = input.title.split_whitespace().collect::<Vec<_>>().join(" ");
    if title.is_empty() {
        return Err(Error::Invalid("Give the task a title."));
    }
    if title.chars().count() > MAX_TITLE {
        return Err(Error::Invalid("That task title is too long."));
    }
    let notes = tidy(
        input.notes.as_deref(),
        MAX_NOTES,
        "Those notes are too long.",
    )?;
    let from_meeting = tidy(
        input.from_meeting.as_deref(),
        200,
        "That meeting name is too long.",
    )?
    .map(|m| m.replace(['\n', '\t'], " "));
    let date = match input
        .due_date
        .as_deref()
        .map(str::trim)
        .filter(|d| !d.is_empty())
    {
        None => None,
        Some(d) => Some(
            NaiveDate::parse_from_str(d, "%Y-%m-%d")
                .map_err(|_| Error::Invalid("Choose a valid day."))?,
        ),
    };
    let time = match input
        .due_time
        .as_deref()
        .map(str::trim)
        .filter(|t| !t.is_empty())
    {
        None => None,
        Some(t) => Some(
            NaiveTime::parse_from_str(t, "%H:%M")
                .map_err(|_| Error::Invalid("Choose a valid time."))?,
        ),
    };
    let due = match (date, time) {
        (None, Some(_)) => return Err(Error::Invalid("Choose a day for that time.")),
        (None, None) => None,
        (Some(date), None) => Some(Due::Date { date }),
        (Some(date), Some(time)) => Some(Due::At {
            at: crate::tz::local_to_utc(tz, date.and_time(time)),
        }),
    };
    if let Some(m) = input.minutes
        && !(5..=720).contains(&m)
    {
        return Err(Error::Invalid("A task can take 5 minutes to 12 hours."));
    }
    Ok(Checked {
        title,
        notes,
        due,
        minutes: input.minutes,
        from_meeting,
    })
}

/// A task as the lists show it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TaskView {
    #[serde(flatten)]
    pub task: Task,
    /// Days since it was due, for a task carried over from an earlier day (0 otherwise).
    pub days_late: i64,
}

/// Every list the Tasks page and the briefing need, worked out at once.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct TaskLists {
    /// Not done and due today, or on an earlier day (carried over): oldest first, then by time.
    pub today: Vec<TaskView>,
    /// Not done, due after today, soonest first.
    pub upcoming: Vec<TaskView>,
    /// Not done, no day given, newest first.
    pub someday: Vec<TaskView>,
    /// Done in the last 30 days, most recent first.
    pub done: Vec<TaskView>,
    /// How many were done today (for the summary line).
    pub done_today: usize,
}

fn sort_key(t: &Task) -> (i64, DateTime<Utc>) {
    match t.due {
        // Timed ones by their time; a day alone sorts at the start of that day, then by when it was written.
        Some(Due::At { at }) => (at.timestamp(), t.created_at),
        Some(Due::Date { date }) => (
            date.and_hms_opt(0, 0, 0)
                .map(|d| d.and_utc().timestamp())
                .unwrap_or_default(),
            t.created_at,
        ),
        None => (i64::MAX, t.created_at),
    }
}

pub fn lists(tasks: &[Task], now: DateTime<Utc>, tz: Tz) -> TaskLists {
    let today = now.with_timezone(&tz).date_naive();
    let mut out = TaskLists::default();
    for t in tasks {
        let view = |days_late| TaskView {
            task: t.clone(),
            days_late,
        };
        if let Some(done) = t.done_at {
            if done.with_timezone(&tz).date_naive() == today {
                out.done_today += 1;
            }
            if now - done <= Duration::days(DONE_DAYS) {
                out.done.push(view(0));
            }
            continue;
        }
        match t.due.map(|d| d.day(tz)) {
            None => out.someday.push(view(0)),
            Some(day) if day <= today => out.today.push(view((today - day).num_days())),
            Some(_) => out.upcoming.push(view(0)),
        }
    }
    out.today.sort_by_key(|v| sort_key(&v.task));
    out.upcoming.sort_by_key(|v| sort_key(&v.task));
    out.someday
        .sort_by_key(|v| std::cmp::Reverse(v.task.created_at));
    out.done.sort_by_key(|v| std::cmp::Reverse(v.task.done_at));
    out
}

/// A task reminder that is due: the task has a time, it has come (within the last hour), and it is not done.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskReminder {
    /// Unique per task and time, recorded once shown so a restart never repeats it.
    pub id: String,
    pub title: String,
    pub at: DateTime<Utc>,
}

pub fn due_reminders(tasks: &[Task], now: DateTime<Utc>) -> Vec<TaskReminder> {
    tasks
        .iter()
        .filter(|t| t.done_at.is_none())
        .filter_map(|t| match t.due {
            Some(Due::At { at })
                if at <= now && now - at < Duration::minutes(REMINDER_GRACE_MINUTES) =>
            {
                Some(TaskReminder {
                    id: format!("task:{}:{}", t.id, at.timestamp()),
                    title: t.title.clone(),
                    at,
                })
            }
            _ => None,
        })
        .collect()
}

/// The lines of a meeting note that could become tasks: each non-empty line, without list marks ("-", "*", "1.",
/// "[ ]"), at most 300 characters and 30 lines. Lines already ticked ("[x]") are left out.
pub fn note_lines(note: &str) -> Vec<String> {
    note.lines()
        .filter_map(|l| {
            let mut l = l.trim();
            for mark in ["- ", "* ", "• ", "– "] {
                if let Some(rest) = l.strip_prefix(mark) {
                    l = rest.trim();
                }
            }
            if let Some(rest) = l
                .split_once(". ")
                .filter(|(n, _)| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()))
            {
                l = rest.1.trim();
            }
            let lower = l.to_lowercase();
            if lower.starts_with("[x]") {
                return None;
            }
            if let Some(rest) = l.strip_prefix("[ ]").or_else(|| l.strip_prefix("[]")) {
                l = rest.trim();
            }
            (!l.is_empty()).then(|| l.chars().take(MAX_TITLE).collect())
        })
        .take(30)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const NY: Tz = chrono_tz::America::New_York;

    fn t(s: &str) -> DateTime<Utc> {
        s.parse().unwrap()
    }

    fn task(id: &str, due: Option<Due>, created: &str) -> Task {
        Task {
            id: id.into(),
            title: id.into(),
            notes: None,
            due,
            minutes: None,
            done_at: None,
            created_at: t(created),
            updated_at: t(created),
            from_meeting: None,
            planned_event: None,
            link: None,
        }
    }

    fn date(s: &str) -> Option<Due> {
        Some(Due::Date {
            date: s.parse().unwrap(),
        })
    }

    fn input(title: &str) -> TaskInput {
        TaskInput {
            title: title.into(),
            notes: None,
            due_date: None,
            due_time: None,
            minutes: None,
            from_meeting: None,
        }
    }

    #[test]
    fn what_was_typed_is_checked_and_tidied() {
        let c = check(&input("  Call   the bank "), NY).unwrap();
        assert_eq!((c.title.as_str(), c.due), ("Call the bank", None));
        assert!(check(&input("  "), NY).is_err());
        assert!(check(&input(&"x".repeat(MAX_TITLE + 1)), NY).is_err());
        let timed = check(
            &TaskInput {
                due_date: Some("2026-11-20".into()),
                due_time: Some("15:30".into()),
                ..input("Send report")
            },
            NY,
        )
        .unwrap();
        // 15:30 in New York is 20:30 UTC in November.
        assert_eq!(
            timed.due,
            Some(Due::At {
                at: t("2026-11-20T20:30:00Z")
            })
        );
        let no_day = TaskInput {
            due_time: Some("15:30".into()),
            ..input("x")
        };
        assert!(check(&no_day, NY).is_err());
        let bad = TaskInput {
            due_date: Some("2026-02-30".into()),
            ..input("x")
        };
        assert!(check(&bad, NY).is_err());
        let long = TaskInput {
            minutes: Some(721),
            ..input("x")
        };
        assert!(check(&long, NY).is_err());
    }

    #[test]
    fn unfinished_tasks_from_earlier_days_carry_over_to_today() {
        let now = t("2026-11-18T15:00:00Z"); // Wednesday, 10:00 in New York
        let mut done = task("done-today", date("2026-11-18"), "2026-11-10T00:00:00Z");
        done.done_at = Some(t("2026-11-18T14:00:00Z"));
        let mut old_done = task("done-long-ago", None, "2026-09-01T00:00:00Z");
        old_done.done_at = Some(t("2026-09-02T00:00:00Z"));
        let all = vec![
            task(
                "today-late",
                Some(Due::At {
                    at: t("2026-11-18T21:00:00Z"),
                }),
                "2026-11-17T00:00:00Z",
            ),
            task(
                "today-early",
                Some(Due::At {
                    at: t("2026-11-18T14:00:00Z"),
                }),
                "2026-11-17T00:00:00Z",
            ),
            task("three-days", date("2026-11-15"), "2026-11-14T00:00:00Z"),
            task("today-day", date("2026-11-18"), "2026-11-17T00:00:00Z"),
            task("friday", date("2026-11-20"), "2026-11-17T00:00:00Z"),
            task("someday-old", None, "2026-11-01T00:00:00Z"),
            task("someday-new", None, "2026-11-17T00:00:00Z"),
            done,
            old_done,
        ];
        let l = lists(&all, now, NY);
        let ids = |v: &[TaskView]| v.iter().map(|x| x.task.id.clone()).collect::<Vec<_>>();
        assert_eq!(
            ids(&l.today),
            ["three-days", "today-day", "today-early", "today-late"]
        );
        assert_eq!(l.today[0].days_late, 3);
        assert_eq!(l.today[1].days_late, 0);
        assert_eq!(ids(&l.upcoming), ["friday"]);
        assert_eq!(ids(&l.someday), ["someday-new", "someday-old"]);
        assert_eq!(ids(&l.done), ["done-today"]);
        assert_eq!(l.done_today, 1);
    }

    #[test]
    fn a_timed_task_falls_on_its_local_day() {
        // 02:00 UTC on the 19th is still the evening of the 18th in New York.
        let late = task(
            "late",
            Some(Due::At {
                at: t("2026-11-19T02:00:00Z"),
            }),
            "2026-11-17T00:00:00Z",
        );
        let l = lists(&[late], t("2026-11-18T15:00:00Z"), NY);
        assert_eq!(l.today.len(), 1);
    }

    #[test]
    fn reminders_come_once_their_time_has_come_and_not_for_done_tasks() {
        let at = t("2026-11-18T15:00:00Z");
        let mut done = task("done", Some(Due::At { at }), "2026-11-17T00:00:00Z");
        done.done_at = Some(at);
        let all = vec![
            task("timed", Some(Due::At { at }), "2026-11-17T00:00:00Z"),
            task("day-only", date("2026-11-18"), "2026-11-17T00:00:00Z"),
            done,
        ];
        assert!(due_reminders(&all, at - Duration::seconds(1)).is_empty());
        let due = due_reminders(&all, at);
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].id, format!("task:timed:{}", at.timestamp()));
        assert!(due_reminders(&all, at + Duration::minutes(61)).is_empty());
    }

    #[test]
    fn note_lines_become_task_titles_without_list_marks() {
        let note = "- Ask about budget\n\n* Sam sends slides\n1. Book room\n[ ] Draft plan\n[x] Already done\n   plain line  ";
        assert_eq!(
            note_lines(note),
            [
                "Ask about budget",
                "Sam sends slides",
                "Book room",
                "Draft plan",
                "plain line"
            ]
        );
    }
}
