//! Conflict help: when two meetings overlap, which one is easier to move, and where it could go instead.
//!
//! Pure and local. It never moves anything: it suggests, and the person chooses. The reasons are plain facts
//! about the two events (is it one you made, have you answered it, which is shorter), so the suggestion can always
//! be explained and is easy to overrule.

use crate::briefing::BriefingSettings;
use crate::briefing::slots::find_free_slots;
use crate::model::*;
use crate::tz::local_to_utc;
use chrono::{DateTime, Duration, NaiveDate, Timelike, Utc};
use chrono_tz::Tz;
use serde::Serialize;

/// How many working days ahead alternatives are looked for.
const LOOK_AHEAD_DAYS: i64 = 14;
const MAX_OPTIONS: usize = 3;
/// Suggested start times land on a quarter of an hour.
const STEP_MINUTES: i64 = 15;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Reason {
    /// You created it, so you can change it yourself.
    Yours,
    /// You have not answered the invitation yet.
    NotAnswered,
    /// You said maybe, or the organiser did.
    Tentative,
    /// It is the shorter of the two.
    Shorter,
    /// It is the later of the two.
    Later,
    /// You chose this one.
    YourChoice,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Option_ {
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Help {
    /// The one to move and the one to keep.
    pub moved: Event,
    pub kept: Event,
    pub reasons: Vec<Reason>,
    pub overlap_minutes: i64,
    /// Where the moved one could go instead: free in your working hours, one per day, nearest days first.
    pub options: Vec<Option_>,
    /// Whether Firstlight can move it for you (you created it) rather than only help you ask.
    pub yours: bool,
}

fn span(e: &Event) -> Option<(DateTime<Utc>, DateTime<Utc>)> {
    match &e.when {
        When::Timed { start, end, .. } if end > start => Some((*start, *end)),
        _ => None,
    }
}

/// How much easier `e` is to move than `other`, and why.
fn ease(e: &Event, other: &Event, mine: bool) -> (i32, Vec<Reason>) {
    let mut score = 0;
    let mut why = vec![];
    if mine {
        score += 3;
        why.push(Reason::Yours);
    }
    if e.my_response == Response::NeedsAction && other.my_response != Response::NeedsAction {
        score += 2;
        why.push(Reason::NotAnswered);
    }
    if (e.my_response == Response::Tentative || e.status == EventStatus::Tentative)
        && other.my_response != Response::Tentative
        && other.status != EventStatus::Tentative
    {
        score += 2;
        why.push(Reason::Tentative);
    }
    if let (Some((s1, e1)), Some((s2, e2))) = (span(e), span(other))
        && e1 - s1 < e2 - s2
    {
        score += 1;
        why.push(Reason::Shorter);
    }
    (score, why)
}

fn rounded_up(t: DateTime<Utc>) -> DateTime<Utc> {
    let secs = t.timestamp();
    let step = STEP_MINUTES * 60;
    DateTime::from_timestamp(
        secs.div_euclid(step) * step + if secs.rem_euclid(step) == 0 { 0 } else { step },
        0,
    )
    .unwrap_or(t)
}

/// Help for the overlap of `a` and `b`. `mine` says whether an event is one the person created. `force_move` names
/// the one to move when the person disagrees with the suggestion. `None` if either is not a timed event.
#[allow(clippy::too_many_arguments)]
pub fn help(
    a: &Event,
    b: &Event,
    events: &[Event],
    mine: &dyn Fn(&Event) -> bool,
    force_move: Option<&str>,
    now: DateTime<Utc>,
    tz: Tz,
    s: &BriefingSettings,
) -> Option<Help> {
    let ((sa, ea), (sb, eb)) = (span(a)?, span(b)?);
    let overlap_minutes = (ea.min(eb) - sa.max(sb)).num_minutes();
    if overlap_minutes <= 0 {
        return None;
    }
    let (move_a, reasons) = match force_move {
        Some(id) if id == a.id => (true, vec![Reason::YourChoice]),
        Some(id) if id == b.id => (false, vec![Reason::YourChoice]),
        _ => {
            let (sa_score, ra) = ease(a, b, mine(a));
            let (sb_score, rb) = ease(b, a, mine(b));
            match sa_score.cmp(&sb_score) {
                std::cmp::Ordering::Greater => (true, ra),
                std::cmp::Ordering::Less => (false, rb),
                // Equal: the later one is the one to shift.
                std::cmp::Ordering::Equal => (sa >= sb, vec![Reason::Later]),
            }
        }
    };
    let (moved, kept) = if move_a { (a, b) } else { (b, a) };
    let (ms, me) = span(moved)?;
    let minutes = (me - ms).num_minutes().clamp(5, 12 * 60);

    // Look for room as if the moved one were not there; everything else (the kept one included) is busy.
    let others: Vec<Event> = events
        .iter()
        .filter(|e| e.id != moved.id)
        .cloned()
        .collect();
    let first: NaiveDate = now.with_timezone(&tz).date_naive();
    let slots = find_free_slots(&others, first, LOOK_AHEAD_DAYS, minutes, now, tz, s);

    let want = ms.with_timezone(&tz).time();
    let mut by_day: Vec<(NaiveDate, DateTime<Utc>, i64)> = vec![];
    for slot in &slots {
        let day = slot.start.with_timezone(&tz).date_naive();
        let desired = local_to_utc(tz, day.and_hms_opt(want.hour(), want.minute(), 0)?);
        let latest = slot.end - Duration::minutes(minutes);
        let mut cand = rounded_up(desired.clamp(slot.start, latest.max(slot.start)));
        if cand + Duration::minutes(minutes) > slot.end {
            cand = rounded_up(slot.start);
            if cand + Duration::minutes(minutes) > slot.end {
                continue;
            }
        }
        let distance = (cand - desired).num_minutes().abs();
        match by_day.iter_mut().find(|(d, _, _)| *d == day) {
            Some(best) if best.2 <= distance => {}
            Some(best) => *best = (day, cand, distance),
            None => by_day.push((day, cand, distance)),
        }
    }
    by_day.sort_by_key(|(d, _, _)| *d);
    let options = by_day
        .into_iter()
        .take(MAX_OPTIONS)
        .map(|(_, start, _)| Option_ {
            start,
            end: start + Duration::minutes(minutes),
        })
        .collect();

    Some(Help {
        yours: mine(moved),
        moved: moved.clone(),
        kept: kept.clone(),
        reasons,
        overlap_minutes,
        options,
    })
}

#[cfg(test)]
mod tests;
