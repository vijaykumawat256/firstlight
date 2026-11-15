//! Meeting prep: what is worth knowing just before a meeting, worked out from the person's own calendar.
//!
//! Pure and local: it looks at earlier occurrences of the same meeting (and what was written in them), the next
//! one, what comes straight before and after, and anything that overlaps. No AI and no network. Only the events
//! the app already keeps are used (about 30 days back), so "last time" means the last time inside that window.

use crate::briefing::{BriefingSettings, day_bounds, is_shown};
use crate::model::*;
use chrono::{DateTime, Duration, Utc};
use chrono_tz::Tz;
use serde::Serialize;

const MAX_EARLIER: usize = 3;
const MAX_NOTES_CHARS: usize = 300;
/// A neighbour further away than this is not "right before" or "right after".
const NEIGHBOUR_HOURS: i64 = 4;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Earlier {
    pub id: String,
    pub title: String,
    pub when: When,
    /// What was written in that event (agenda, notes), tidied: one paragraph, no links, cut short.
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Neighbour {
    pub id: String,
    pub title: String,
    /// Minutes between the two (0 = back to back).
    pub gap_minutes: i64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Overlap {
    pub id: String,
    pub title: String,
    pub minutes: i64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct NextTime {
    pub id: String,
    pub when: When,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct Prep {
    /// Earlier occurrences of the same meeting, newest first.
    pub earlier: Vec<Earlier>,
    pub next_time: Option<NextTime>,
    pub before: Option<Neighbour>,
    pub after: Option<Neighbour>,
    pub overlaps: Vec<Overlap>,
}

impl Prep {
    pub fn is_empty(&self) -> bool {
        self.earlier.is_empty()
            && self.next_time.is_none()
            && self.before.is_none()
            && self.after.is_none()
            && self.overlaps.is_empty()
    }
}

fn norm(t: &str) -> String {
    t.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// The same meeting again: the same series or calendar entry, or (for meetings made one by one) the same title.
fn same_meeting(a: &Event, b: &Event) -> bool {
    if a.id == b.id {
        return false;
    }
    if !a.ical_uid.is_empty() && a.ical_uid == b.ical_uid {
        return true;
    }
    if a.recurrence_master_id.is_some() && a.recurrence_master_id == b.recurrence_master_id {
        return true;
    }
    let t = norm(&a.title);
    t.chars().count() >= 4 && t == norm(&b.title)
}

/// One tidy paragraph: control characters and links removed, cut on a character boundary.
pub fn clean_notes(s: &str) -> Option<String> {
    let words: Vec<&str> = s
        .split_whitespace()
        .filter(|w| !w.to_lowercase().starts_with("http"))
        .collect();
    let text: String = words
        .join(" ")
        .chars()
        .filter(|c| !c.is_control())
        .collect();
    let text = text.trim().to_string();
    if text.is_empty() {
        return None;
    }
    if text.chars().count() <= MAX_NOTES_CHARS {
        return Some(text);
    }
    let cut: String = text.chars().take(MAX_NOTES_CHARS - 1).collect();
    Some(format!("{}…", cut.trim_end()))
}

fn span(e: &Event) -> Option<(DateTime<Utc>, DateTime<Utc>)> {
    match &e.when {
        When::Timed { start, end, .. } if end > start => Some((*start, *end)),
        _ => None,
    }
}

fn first_instant(e: &Event, tz: Tz) -> DateTime<Utc> {
    match &e.when {
        When::Timed { start, .. } => *start,
        When::AllDay { start, .. } => day_bounds(*start, tz).0,
    }
}

/// What to know before `target`, from the events the app holds. Meetings that are not timed have no prep.
pub fn prepare(target: &Event, events: &[Event], tz: Tz, s: &BriefingSettings) -> Prep {
    let Some((start, end)) = span(target) else {
        return Prep::default();
    };
    let shown: Vec<&Event> = events
        .iter()
        .filter(|e| e.id != target.id && is_shown(e, s))
        .collect();

    let mut earlier: Vec<&Event> = shown
        .iter()
        .copied()
        .filter(|e| same_meeting(target, e) && first_instant(e, tz) < start)
        .collect();
    earlier.sort_by_key(|e| std::cmp::Reverse(first_instant(e, tz)));
    let earlier = earlier
        .into_iter()
        .take(MAX_EARLIER)
        .map(|e| Earlier {
            id: e.id.clone(),
            title: e.title.clone(),
            when: e.when.clone(),
            notes: e.description.as_deref().and_then(clean_notes),
        })
        .collect();

    let next_time = shown
        .iter()
        .copied()
        .filter(|e| same_meeting(target, e) && first_instant(e, tz) >= end)
        .min_by_key(|e| first_instant(e, tz))
        .map(|e| NextTime {
            id: e.id.clone(),
            when: e.when.clone(),
        });

    let window = Duration::hours(NEIGHBOUR_HOURS);
    let neighbour = |e: &Event, gap: Duration| Neighbour {
        id: e.id.clone(),
        title: e.title.clone(),
        gap_minutes: gap.num_minutes(),
    };
    let before = shown
        .iter()
        .copied()
        .filter_map(|e| span(e).map(|(_, en)| (e, en)))
        .filter(|(_, en)| *en <= start && start - *en <= window)
        .max_by_key(|(_, en)| *en)
        .map(|(e, en)| neighbour(e, start - en));
    let after = shown
        .iter()
        .copied()
        .filter_map(|e| span(e).map(|(st, _)| (e, st)))
        .filter(|(_, st)| *st >= end && *st - end <= window)
        .min_by_key(|(_, st)| *st)
        .map(|(e, st)| neighbour(e, st - end));

    let mut overlaps: Vec<Overlap> = shown
        .iter()
        .copied()
        .filter_map(|e| span(e).map(|(st, en)| (e, st, en)))
        .filter(|(_, st, en)| *st < end && *en > start)
        .map(|(e, st, en)| Overlap {
            id: e.id.clone(),
            title: e.title.clone(),
            minutes: (en.min(end) - st.max(start)).num_minutes(),
        })
        .collect();
    overlaps.sort_by(|a, b| a.title.cmp(&b.title).then(a.id.cmp(&b.id)));

    Prep {
        earlier,
        next_time,
        before,
        after,
        overlaps,
    }
}

#[cfg(test)]
mod tests;
