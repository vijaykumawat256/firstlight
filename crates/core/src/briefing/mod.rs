//! The briefing engine: turns a list of events and "now" into what the user needs to know.
//!
//! Pure and deterministic: no I/O, no clock, no locale. The caller supplies the time and timezone,
//! and the UI turns the structured result into words in the user's language and time format.

use crate::model::*;
use crate::tz::local_to_utc;
use chrono::{DateTime, Duration, NaiveDate, Timelike, Utc};
use chrono_tz::Tz;
use serde::{Deserialize, Serialize};

pub mod rules;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct BriefingSettings {
    pub show_declined: bool,
    /// Free gaps shorter than this are not worth mentioning.
    pub min_gap_minutes: i64,
    /// Free gaps are only reported inside working hours (local time).
    pub work_start_hour: u32,
    pub work_end_hour: u32,
    /// Tomorrow's first event before this local hour is flagged as an early start.
    pub early_hour: u32,
}

impl Default for BriefingSettings {
    fn default() -> Self {
        Self {
            show_declined: false,
            min_gap_minutes: 30,
            work_start_hour: 9,
            work_end_hour: 18,
            early_hour: 8,
        }
    }
}

/// A timed event with its instants spelled out.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Item {
    pub event: Event,
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NextUp {
    pub event: Event,
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
    pub in_progress: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Conflict {
    pub a: String,
    pub b: String,
    pub overlap_minutes: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Gap {
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
    pub minutes: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Tomorrow {
    pub timed: Vec<Item>,
    pub all_day: Vec<Event>,
    pub early_start: bool,
}

/// How the day looks, so the UI can choose its sentence.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DayState {
    /// Nothing at all today.
    Clear,
    /// Only all-day items.
    AllDayOnly,
    /// Everything has already happened.
    Done,
    /// Something is on right now.
    InProgress,
    /// Something is still ahead.
    Upcoming,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Briefing {
    pub generated_at: DateTime<Utc>,
    /// The user's local date.
    pub date: NaiveDate,
    pub state: DayState,
    pub all_day_today: Vec<Event>,
    pub past: Vec<Item>,
    /// Not yet finished (includes the one in progress), in start order.
    pub remaining: Vec<Item>,
    pub next: Option<NextUp>,
    pub conflicts: Vec<Conflict>,
    /// Runs of three or more meetings with no break between them (ids, in order).
    pub back_to_back_runs: Vec<Vec<String>>,
    pub free_gaps: Vec<Gap>,
    /// Unanswered invitations for the rest of today and tomorrow.
    pub needs_response: Vec<Event>,
    pub tomorrow: Tomorrow,
}

/// `[start, end)` of a local calendar day as UTC instants. Handles 23- and 25-hour days.
pub fn day_bounds(date: NaiveDate, tz: Tz) -> (DateTime<Utc>, DateTime<Utc>) {
    let midnight = |d: NaiveDate| local_to_utc(tz, d.and_hms_opt(0, 0, 0).expect("midnight"));
    (midnight(date), midnight(date + Duration::days(1)))
}

/// Cancelled events never show; declined ones only if the user asks.
pub fn is_shown(e: &Event, s: &BriefingSettings) -> bool {
    e.status != EventStatus::Cancelled && (s.show_declined || e.my_response != Response::Declined)
}

fn timed_items(events: &[&Event], from: DateTime<Utc>, to: DateTime<Utc>) -> Vec<Item> {
    let mut v: Vec<Item> = events
        .iter()
        .filter_map(|e| match &e.when {
            When::Timed { start, end, .. }
                if *start < to && (*end > from || (*start == *end && *start >= from)) =>
            {
                Some(Item {
                    event: (*e).clone(),
                    start: *start,
                    end: *end,
                })
            }
            _ => None,
        })
        .collect();
    v.sort_by(|a, b| {
        a.start
            .cmp(&b.start)
            .then_with(|| a.event.title.cmp(&b.event.title))
            .then_with(|| a.event.id.cmp(&b.event.id))
    });
    v
}

fn all_day_on(events: &[&Event], date: NaiveDate) -> Vec<Event> {
    events
        .iter()
        .filter(|e| matches!(&e.when, When::AllDay { start, end } if *start <= date && date < *end))
        .map(|e| (*e).clone())
        .collect()
}

pub fn build(events: &[Event], now: DateTime<Utc>, tz: Tz, s: &BriefingSettings) -> Briefing {
    let today = now.with_timezone(&tz).date_naive();
    let tomorrow_date = today + Duration::days(1);
    let (day_start, day_end) = day_bounds(today, tz);
    let (_, tomorrow_end) = day_bounds(tomorrow_date, tz);

    let shown: Vec<&Event> = events.iter().filter(|e| is_shown(e, s)).collect();
    let today_items = timed_items(&shown, day_start, day_end);
    let all_day_today = all_day_on(&shown, today);

    let (past, remaining): (Vec<Item>, Vec<Item>) = today_items
        .into_iter()
        .partition(|i| i.end <= now && !(i.start == i.end && i.start > now));
    let next = remaining.first().map(|i| NextUp {
        event: i.event.clone(),
        start: i.start,
        end: i.end,
        in_progress: i.start <= now,
    });

    // Conflicts: overlapping events that are still relevant. Sorted by start, so a later event
    // can only overlap `a` while it starts before `a` ends.
    let mut conflicts = Vec::new();
    for (i, a) in remaining.iter().enumerate() {
        for b in &remaining[i + 1..] {
            if b.start >= a.end {
                break;
            }
            let overlap = (a.end.min(b.end) - b.start).num_minutes();
            conflicts.push(Conflict {
                a: a.event.id.clone(),
                b: b.event.id.clone(),
                overlap_minutes: overlap.max(0),
            });
        }
    }

    // Back-to-back: a chain where each event starts within five minutes of the previous one ending.
    let mut runs: Vec<Vec<String>> = Vec::new();
    let mut run: Vec<&Item> = Vec::new();
    for item in &remaining {
        match run.last() {
            Some(prev)
                if item.start >= prev.end && item.start - prev.end <= Duration::minutes(5) =>
            {
                run.push(item)
            }
            _ => {
                if run.len() >= 3 {
                    runs.push(run.iter().map(|i| i.event.id.clone()).collect());
                }
                run = vec![item];
            }
        }
    }
    if run.len() >= 3 {
        runs.push(run.iter().map(|i| i.event.id.clone()).collect());
    }

    // Free gaps before the last meeting, inside working hours, long enough to use.
    let work = |h: u32| local_to_utc(tz, today.and_hms_opt(h.min(23), 0, 0).expect("hour"));
    let (work_start, work_end) = (work(s.work_start_hour), work(s.work_end_hour));
    let min_gap = Duration::minutes(s.min_gap_minutes.max(1));
    let mut free_gaps = Vec::new();
    let mut cursor = now.max(work_start);
    for item in &remaining {
        let gap_end = item.start.min(work_end);
        if gap_end - cursor >= min_gap {
            free_gaps.push(Gap {
                start: cursor,
                end: gap_end,
                minutes: (gap_end - cursor).num_minutes(),
            });
        }
        cursor = cursor.max(item.end);
    }

    let tomorrow_items = timed_items(&shown, day_end, tomorrow_end);
    let early_start = tomorrow_items
        .iter()
        .any(|i| i.start >= day_end && i.start.with_timezone(&tz).hour() < s.early_hour);

    let mut needs_response: Vec<Event> = remaining
        .iter()
        .chain(tomorrow_items.iter())
        .map(|i| &i.event)
        .filter(|e| e.my_response == Response::NeedsAction)
        .cloned()
        .collect();
    needs_response.dedup_by(|a, b| a.id == b.id);

    let state = if remaining.is_empty() && past.is_empty() {
        if all_day_today.is_empty() {
            DayState::Clear
        } else {
            DayState::AllDayOnly
        }
    } else if next.is_none() {
        DayState::Done
    } else if next.as_ref().is_some_and(|n| n.in_progress) {
        DayState::InProgress
    } else {
        DayState::Upcoming
    };

    Briefing {
        generated_at: now,
        date: today,
        state,
        all_day_today,
        past,
        remaining,
        next,
        conflicts,
        back_to_back_runs: runs,
        free_gaps,
        needs_response,
        tomorrow: Tomorrow {
            timed: tomorrow_items,
            all_day: all_day_on(&shown, tomorrow_date),
            early_start,
        },
    }
}

#[cfg(test)]
mod tests;
