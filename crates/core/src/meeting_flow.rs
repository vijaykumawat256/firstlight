//! Reads what the follow-up rules (`firstlight_logic::followup`) need from the store: today's events up to now and the
//! private notes kept for them.

use crate::Result;
use crate::briefing::{BriefingSettings, day_bounds};
use crate::followup::{FollowUp, candidates};
use crate::store::Store;
use chrono::{DateTime, Duration, Utc};
use chrono_tz::Tz;
use std::collections::HashMap;

pub fn follow_ups(
    store: &Store,
    now: DateTime<Utc>,
    tz: Tz,
    bs: &BriefingSettings,
) -> Result<Vec<FollowUp>> {
    let (from, _) = day_bounds(now.with_timezone(&tz).date_naive(), tz);
    let events = crate::dedupe::dedupe(store.events_between(from, now + Duration::seconds(1))?);
    let mut notes = HashMap::new();
    for e in &events {
        let key = crate::notes::meeting_key(e);
        if !notes.contains_key(&key)
            && let Some(n) = store.get_meeting_note(&key)?
        {
            notes.insert(key, n);
        }
    }
    Ok(candidates(&events, &notes, now, tz, bs))
}

/// This week so far next to the same days of last week (see `review`).
pub fn week_review(
    store: &Store,
    now: DateTime<Utc>,
    tz: Tz,
    bs: &BriefingSettings,
) -> Result<crate::review::WeekReview> {
    let from = now - Duration::days(15);
    let events = crate::dedupe::dedupe(store.events_between(from, now + Duration::seconds(1))?);
    let focus = crate::local::focus_event_ids(store, from, now + Duration::seconds(1))?;
    let tasks = crate::todo::all(store)?;
    Ok(crate::review::review(&events, &focus, &tasks, now, tz, bs))
}

/// The standup or weekly update, written from the tasks and the calendar (8 days back, 14 ahead).
pub fn status_report(
    store: &Store,
    kind: crate::report::ReportKind,
    now: DateTime<Utc>,
    tz: Tz,
    bs: &BriefingSettings,
) -> Result<crate::report::Report> {
    let (from, to) = (now - Duration::days(8), now + Duration::days(15));
    let events = crate::dedupe::dedupe(store.events_between(from, to)?);
    let focus = crate::local::focus_event_ids(store, from, to)?;
    let tasks = crate::todo::all(store)?;
    Ok(crate::report::build(
        kind, &events, &focus, &tasks, now, tz, bs,
    ))
}

/// Where the time went between two local days (at most `timereport::MAX_DAYS` apart).
pub fn time_report(
    store: &Store,
    from: chrono::NaiveDate,
    to: chrono::NaiveDate,
    now: DateTime<Utc>,
    tz: Tz,
    bs: &BriefingSettings,
) -> Result<crate::timereport::TimeReport> {
    if to < from || (to - from).num_days() > crate::timereport::MAX_DAYS {
        return Err(crate::Error::Invalid("Choose up to three months."));
    }
    let (start, _) = crate::briefing::day_bounds(from, tz);
    let (_, end) = crate::briefing::day_bounds(to, tz);
    let events = crate::dedupe::dedupe(store.events_between(start, end)?);
    let focus = crate::local::focus_event_ids(store, start, end)?;
    let calendars = store
        .list_calendars()?
        .into_iter()
        .map(|c| (c.id, c.name))
        .collect();
    Ok(crate::timereport::build(
        &events, &calendars, &focus, from, to, now, tz, bs,
    ))
}
