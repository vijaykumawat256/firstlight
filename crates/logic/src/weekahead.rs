//! The week ahead: how heavy each of the next days is, and what is worth knowing before it starts.
//!
//! Pure and local, like the rest of the briefing. Only the numbers and a few all-day titles are produced;
//! the wording is the caller's. "Busy" time counts overlapping meetings once.

use crate::briefing::slots::find_free_slots;
use crate::briefing::{BriefingSettings, day_bounds, is_shown};
use crate::model::*;
use chrono::{DateTime, Datelike, Duration, NaiveDate, Timelike, Utc, Weekday};
use chrono_tz::Tz;
use serde::Serialize;

/// A day with at least this much meeting time is called heavy.
pub const HEAVY_MINUTES: i64 = 5 * 60;
/// The shortest stretch worth calling a block for focus.
pub const FOCUS_MINUTES: i64 = 120;
const MAX_ALL_DAY_TITLES: usize = 3;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Day {
    pub date: NaiveDate,
    pub weekend: bool,
    /// Timed meetings that start this day.
    pub meetings: usize,
    /// Time in meetings, overlaps counted once.
    pub minutes: i64,
    pub first_start: Option<DateTime<Utc>>,
    pub last_end: Option<DateTime<Utc>>,
    /// The first meeting starts before the "early start" hour.
    pub early_start: bool,
    /// Runs of three or more meetings with no real break between them.
    pub back_to_back_runs: usize,
    /// Pairs of meetings that overlap.
    pub conflicts: usize,
    pub heavy: bool,
    /// Titles of all-day events (holidays, trips, birthdays), a few at most.
    pub all_day: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Focus {
    pub date: NaiveDate,
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
    pub minutes: i64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct WeekAhead {
    pub days: Vec<Day>,
    pub meetings: usize,
    pub minutes: i64,
    /// The day with the most meeting time (none if the whole stretch is empty).
    pub busiest: Option<NaiveDate>,
    /// The lightest working day, when there are at least two working days and it differs from the busiest.
    pub lightest: Option<NaiveDate>,
    /// The longest free stretch of two hours or more inside working hours.
    pub focus: Option<Focus>,
    /// Invitations you have not answered in this stretch.
    pub needs_response: usize,
    pub conflicts: usize,
    pub early_starts: Vec<NaiveDate>,
}

fn union_minutes(mut spans: Vec<(DateTime<Utc>, DateTime<Utc>)>) -> i64 {
    spans.sort();
    let mut total = Duration::zero();
    let mut cur: Option<(DateTime<Utc>, DateTime<Utc>)> = None;
    for (s, e) in spans {
        match cur {
            Some((cs, ce)) if s <= ce => cur = Some((cs, ce.max(e))),
            Some((cs, ce)) => {
                total += ce - cs;
                cur = Some((s, e));
            }
            None => cur = Some((s, e)),
        }
    }
    if let Some((cs, ce)) = cur {
        total += ce - cs;
    }
    total.num_minutes()
}

/// The `days` days beginning at `start` (clamped to 1..=14).
pub fn build(
    events: &[Event],
    start: NaiveDate,
    days: i64,
    tz: Tz,
    s: &BriefingSettings,
) -> WeekAhead {
    let days = days.clamp(1, 14);
    let shown: Vec<&Event> = events.iter().filter(|e| is_shown(e, s)).collect();
    let mut out = Vec::new();
    for i in 0..days {
        let date = start + Duration::days(i);
        let (lo, hi) = day_bounds(date, tz);
        let mut timed: Vec<(DateTime<Utc>, DateTime<Utc>)> = shown
            .iter()
            .filter_map(|e| match &e.when {
                When::Timed { start, end, .. } if end > start && *start >= lo && *start < hi => {
                    Some((*start, *end))
                }
                _ => None,
            })
            .collect();
        timed.sort();
        // Time inside this day only: a late meeting that runs past midnight counts to midnight.
        let clipped: Vec<_> = timed.iter().map(|(a, b)| (*a, (*b).min(hi))).collect();
        let minutes = union_minutes(clipped);

        let mut runs = 0;
        let mut run_len = 0;
        let mut prev_end: Option<DateTime<Utc>> = None;
        let mut conflicts = 0;
        for (idx, (st, en)) in timed.iter().enumerate() {
            match prev_end {
                Some(pe) if *st >= pe && *st - pe <= Duration::minutes(5) => run_len += 1,
                _ => {
                    if run_len >= 3 {
                        runs += 1;
                    }
                    run_len = 1;
                }
            }
            prev_end = Some(prev_end.map_or(*en, |pe| if *en > pe { *en } else { pe }));
            conflicts += timed[..idx].iter().filter(|(_, pe)| *pe > *st).count();
        }
        if run_len >= 3 {
            runs += 1;
        }

        let all_day: Vec<String> = shown
            .iter()
            .filter(
                |e| matches!(&e.when, When::AllDay { start, end } if *start <= date && date < *end),
            )
            .map(|e| e.title.clone())
            .take(MAX_ALL_DAY_TITLES)
            .collect();
        out.push(Day {
            date,
            weekend: matches!(date.weekday(), Weekday::Sat | Weekday::Sun),
            meetings: timed.len(),
            minutes,
            first_start: timed.first().map(|t| t.0),
            last_end: timed.iter().map(|t| t.1).max(),
            early_start: timed
                .first()
                .is_some_and(|t| t.0.with_timezone(&tz).hour() < s.early_hour),
            back_to_back_runs: runs,
            conflicts,
            heavy: minutes >= HEAVY_MINUTES,
            all_day,
        });
    }

    let busiest = out
        .iter()
        .filter(|d| d.minutes > 0)
        .max_by_key(|d| (d.minutes, std::cmp::Reverse(d.date)))
        .map(|d| d.date);
    let working: Vec<&Day> = out.iter().filter(|d| !d.weekend).collect();
    // Only worth saying when the working days really differ.
    let spread = working.iter().map(|d| d.minutes).max() != working.iter().map(|d| d.minutes).min();
    let lightest = if working.len() >= 2 && spread {
        working
            .iter()
            .min_by_key(|d| (d.minutes, d.date))
            .map(|d| d.date)
            .filter(|d| Some(*d) != busiest)
    } else {
        None
    };

    // Counting from the start of the first day, so nothing is cut off as "already past".
    let from = day_bounds(start, tz).0;
    let focus = find_free_slots(events, start, days, FOCUS_MINUTES, from, tz, s)
        .into_iter()
        .max_by_key(|f| (f.minutes, std::cmp::Reverse(f.start)))
        .map(|f| Focus {
            date: f.start.with_timezone(&tz).date_naive(),
            start: f.start,
            end: f.end,
            minutes: f.minutes,
        });

    let (lo, hi) = (
        day_bounds(start, tz).0,
        day_bounds(start + Duration::days(days - 1), tz).1,
    );
    let needs_response = shown
        .iter()
        .filter(|e| e.my_response == Response::NeedsAction)
        .filter(|e| match &e.when {
            When::Timed { start, .. } => *start >= lo && *start < hi,
            When::AllDay { start, .. } => {
                *start >= start_date(lo, tz) && *start < start_date(hi, tz)
            }
        })
        .count();

    WeekAhead {
        meetings: out.iter().map(|d| d.meetings).sum(),
        minutes: out.iter().map(|d| d.minutes).sum(),
        conflicts: out.iter().map(|d| d.conflicts).sum(),
        early_starts: out
            .iter()
            .filter(|d| d.early_start)
            .map(|d| d.date)
            .collect(),
        days: out,
        busiest,
        lightest,
        focus,
        needs_response,
    }
}

fn start_date(t: DateTime<Utc>, tz: Tz) -> NaiveDate {
    t.with_timezone(&tz).date_naive()
}

#[cfg(test)]
mod tests;
