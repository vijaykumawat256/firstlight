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
