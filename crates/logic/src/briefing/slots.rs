//! "Find a time": the free stretches in working hours that are long enough for something.
//!
//! Pure, like the rest of the briefing. Weekends are skipped, only timed events that are shown
//! (not cancelled, not declined) count as busy, and a stretch already in the past is cut short.

use super::{BriefingSettings, day_bounds, is_shown};
use crate::model::*;
use crate::tz::local_to_utc;
use chrono::{DateTime, Datelike, Duration, NaiveDate, Utc, Weekday};
use chrono_tz::Tz;
use serde::Serialize;

/// Most days one search may cover.
pub const MAX_DAYS: i64 = 21;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Slot {
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
    pub minutes: i64,
}

/// Free stretches of at least `minutes` on `days` days starting at `first`.
pub fn find_free_slots(
    events: &[Event],
    first: NaiveDate,
    days: i64,
    minutes: i64,
    now: DateTime<Utc>,
    tz: Tz,
    s: &BriefingSettings,
) -> Vec<Slot> {
    let need = Duration::minutes(minutes.clamp(5, 12 * 60));
    let mut busy: Vec<(DateTime<Utc>, DateTime<Utc>)> = events
        .iter()
        .filter(|e| is_shown(e, s))
        .filter_map(|e| match &e.when {
            When::Timed { start, end, .. } if end > start => Some((*start, *end)),
            _ => None,
        })
        .collect();
    busy.sort();
    let mut out = Vec::new();
    for i in 0..days.clamp(1, MAX_DAYS) {
        let date = first + Duration::days(i);
        if matches!(date.weekday(), Weekday::Sat | Weekday::Sun) {
            continue;
        }
        let (day_start, _) = day_bounds(date, tz);
        let at = |h: u32| {
            local_to_utc(tz, date.and_hms_opt(h.min(23), 0, 0).expect("hour")).max(day_start)
        };
        let (open, close) = (at(s.work_start_hour).max(now), at(s.work_end_hour));
        let mut cursor = open;
        for (bs, be) in busy.iter().filter(|(bs, be)| *bs < close && *be > open) {
            if *bs - cursor >= need {
                out.push(slot(cursor, *bs));
            }
            cursor = cursor.max(*be);
        }
        if close - cursor >= need {
            out.push(slot(cursor, close));
        }
    }
    out
}

fn slot(start: DateTime<Utc>, end: DateTime<Utc>) -> Slot {
    Slot {
        start,
        end,
        minutes: (end - start).num_minutes(),
    }
}
