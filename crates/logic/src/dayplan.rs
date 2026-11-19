//! "Plan my day": where today's tasks could go in today's free stretches.
//!
//! Two ways, both only *suggesting*: a simple plan worked out here (tasks that have waited longest first, each in the
//! first free stretch with room), and, when the person turned it on, a plan from their AI service. The AI's answer is
//! checked here line by line (each task once, inside a free stretch, no two overlapping) and anything that does not fit
//! is dropped, so a suggestion can never land on a meeting. Nothing is put in the calendar until the person accepts.

use crate::briefing::slots::Slot;
use crate::tasks::TaskLists;
use chrono::{DateTime, Duration, NaiveDate, NaiveTime, Timelike, Utc};
use chrono_tz::Tz;
use serde::{Deserialize, Serialize};

/// At most this many tasks are considered (and, with AI, sent).
pub const MAX_TASKS: usize = 15;
/// A task without an estimate is planned for this long.
pub const DEFAULT_MINUTES: u32 = 30;

/// A task as the planner sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanTask {
    pub id: String,
    pub title: String,
    pub minutes: u32,
    pub days_late: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Suggestion {
    pub task_id: String,
    pub title: String,
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
}

/// The tasks worth planning today: today's (carried-over ones first, as the list already orders them), then ones with
/// no day. Done tasks and ones that already have time set aside are left out.
pub fn candidates(lists: &TaskLists) -> Vec<PlanTask> {
    lists
        .today
        .iter()
        .chain(lists.someday.iter())
        .filter(|v| v.task.done_at.is_none() && v.task.planned_event.is_none())
        .take(MAX_TASKS)
        .map(|v| PlanTask {
            id: v.task.id.clone(),
            title: v.task.title.clone(),
            minutes: v.task.minutes.unwrap_or(DEFAULT_MINUTES),
            days_late: v.days_late,
        })
        .collect()
}

/// Rounds up to the next five minutes, so suggestions start on tidy times.
fn tidy_start(t: DateTime<Utc>) -> DateTime<Utc> {
    let t = t
        .with_second(0)
        .and_then(|t| t.with_nanosecond(0))
        .unwrap_or(t);
    let over = i64::from(t.minute() % 5);
    if over == 0 {
        t
    } else {
        t + Duration::minutes(5 - over)
    }
}

/// The plan worked out on this computer: each task, in order, goes in the first free stretch that still has room.
pub fn local_plan(tasks: &[PlanTask], slots: &[Slot]) -> Vec<Suggestion> {
    let mut cursors: Vec<(DateTime<Utc>, DateTime<Utc>)> =
        slots.iter().map(|s| (tidy_start(s.start), s.end)).collect();
    let mut out = Vec::new();
    for t in tasks {
        let need = Duration::minutes(i64::from(t.minutes));
        if let Some(c) = cursors.iter_mut().find(|(from, to)| *to - *from >= need) {
            out.push(Suggestion {
                task_id: t.id.clone(),
                title: t.title.clone(),
                start: c.0,
                end: c.0 + need,
            });
            c.0 = tidy_start(c.0 + need);
        }
    }
    out
}

/// One line of the AI's answer: task number (as sent), start "HH:MM", minutes. Kept compact so a whole plan fits in
/// a short reply.
#[derive(Deserialize)]
struct RawPlan {
    plan: Vec<(usize, String, u32)>,
}

/// Reads the AI's answer (`{"plan":[[1,"10:30",30],[3,"14:00",45]]}`, numbered as sent). Tolerates prose or
/// a code fence around the JSON. Every line is checked: a task number that was sent, used once; a real time today;
/// 5 to 480 minutes; wholly inside one free stretch; not overlapping another line. Lines that fail are dropped.
/// `None` means the answer was not a plan at all.
pub fn parse_ai_plan(
    raw: &str,
    tasks: &[PlanTask],
    slots: &[Slot],
    today: NaiveDate,
    tz: Tz,
) -> Option<Vec<Suggestion>> {
    let (from, to) = (raw.find('{')?, raw.rfind('}')?);
    let parsed: RawPlan = serde_json::from_str(raw.get(from..=to)?).ok()?;
    let mut out: Vec<Suggestion> = Vec::new();
    let mut used = std::collections::HashSet::new();
    for (number, at, minutes) in parsed.plan.into_iter().take(MAX_TASKS * 2) {
        let Some(task) = number.checked_sub(1).and_then(|i| tasks.get(i)) else {
            continue;
        };
        if !(5..=480).contains(&minutes) || used.contains(&number) {
            continue;
        }
        let Ok(time) = NaiveTime::parse_from_str(at.trim(), "%H:%M") else {
            continue;
        };
        let start = crate::tz::local_to_utc(tz, today.and_time(time));
        let end = start + Duration::minutes(i64::from(minutes));
        let inside = slots.iter().any(|s| s.start <= start && end <= s.end);
        let clashes = out.iter().any(|o| o.start < end && start < o.end);
        if !inside || clashes {
            continue;
        }
        used.insert(number);
        out.push(Suggestion {
            task_id: task.id.clone(),
            title: task.title.clone(),
            start,
            end,
        });
    }
    out.sort_by_key(|s| s.start);
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const NY: Tz = chrono_tz::America::New_York;

    fn t(s: &str) -> DateTime<Utc> {
        s.parse().unwrap()
    }

    fn slot(a: &str, b: &str) -> Slot {
        let (start, end) = (t(a), t(b));
        Slot {
            start,
            end,
            minutes: (end - start).num_minutes(),
        }
    }

    fn task(id: &str, minutes: u32) -> PlanTask {
        PlanTask {
            id: id.into(),
            title: format!("Task {id}"),
            minutes,
            days_late: 0,
        }
    }

    // Wednesday 18 November 2026 in New York: 10:00 is 15:00 UTC.
    fn slots() -> Vec<Slot> {
        vec![
            slot("2026-11-18T15:03:00Z", "2026-11-18T16:00:00Z"), // 10:03 to 11:00
            slot("2026-11-18T19:00:00Z", "2026-11-18T21:00:00Z"), // 14:00 to 16:00
        ]
    }

    #[test]
    fn the_local_plan_fills_free_stretches_in_order_on_tidy_times() {
        let plan = local_plan(
            &[task("a", 30), task("b", 45), task("c", 20), task("d", 300)],
            &slots(),
        );
        let got: Vec<_> = plan
            .iter()
            .map(|s| (s.task_id.as_str(), s.start, s.end))
            .collect();
        assert_eq!(
            got,
            [
                ("a", t("2026-11-18T15:05:00Z"), t("2026-11-18T15:35:00Z")),
                // 45 minutes no longer fit before 11:00, so it goes after lunch.
                ("b", t("2026-11-18T19:00:00Z"), t("2026-11-18T19:45:00Z")),
                ("c", t("2026-11-18T15:35:00Z"), t("2026-11-18T15:55:00Z")),
            ]
        );
    }

    #[test]
    fn an_ai_plan_is_checked_line_by_line() {
        let tasks = [task("a", 30), task("b", 30), task("c", 30)];
        let today: NaiveDate = "2026-11-18".parse().unwrap();
        let raw = r#"Sure! ```json
        {"plan":[[1,"10:15",30],[1,"14:00",30],[2,"10:30",30],[3,"12:00",30],[9,"14:00",30],[2,"14:30",30],[3,"25:00",30]]}```"#;
        let plan = parse_ai_plan(raw, &tasks, &slots(), today, NY).unwrap();
        let got: Vec<_> = plan.iter().map(|s| (s.task_id.as_str(), s.start)).collect();
        // Task 1 once; task 2's first time overlaps task 1, its second fits; 12:00 is not free; 9 was never sent.
        assert_eq!(
            got,
            [
                ("a", t("2026-11-18T15:15:00Z")),
                ("b", t("2026-11-18T19:30:00Z"))
            ]
        );
        assert_eq!(
            parse_ai_plan("I can't help with that", &tasks, &slots(), today, NY),
            None
        );
    }
}
