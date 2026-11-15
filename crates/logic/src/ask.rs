//! "Ask your calendar": a typed question becomes a small, checked query that runs on this computer.
//!
//! The AI only ever sees the question (plus today's date and the time zone). It answers with a plan, a handful of
//! dates and words, and this module validates that plan and runs it against the person's own events locally.
//! No event, title, place or name is ever sent to the AI, and nothing the AI says is shown as fact: the answer
//! shown is always built from the person's own calendar.

use crate::briefing::slots::{MAX_DAYS, Slot, find_free_slots};
use crate::briefing::{BriefingSettings, day_bounds, is_shown};
use crate::model::*;
use chrono::{DateTime, Duration, NaiveDate, Utc};
use chrono_tz::Tz;
use serde::{Deserialize, Serialize};

/// The longest stretch a question may cover.
pub const MAX_SPAN_DAYS: i64 = 92;
/// How far from today a date may be (a model's guess a year out is more likely a mistake than a question).
const MAX_DISTANCE_DAYS: i64 = 400;
const MAX_KEYWORDS: usize = 5;
const MAX_KEYWORD_CHARS: usize = 40;
/// Most events one answer lists.
pub const MAX_EVENTS: usize = 50;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    /// List or find events.
    Events,
    /// Find free time.
    Free,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    pub kind: Kind,
    pub from: NaiveDate,
    /// Inclusive.
    pub to: NaiveDate,
    /// Lower-case words that must all appear in an event's title, place or organiser. Empty lists everything.
    pub keywords: Vec<String>,
    /// How long a free stretch must be (Free only).
    pub minutes: i64,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Understood {
    Plan(Plan),
    /// The question is not about the calendar.
    NotACalendarQuestion,
}

#[derive(Deserialize)]
struct Raw {
    #[serde(default)]
    kind: String,
    #[serde(default)]
    from: String,
    #[serde(default)]
    to: String,
    #[serde(default)]
    keywords: Vec<serde_json::Value>,
    #[serde(default)]
    minutes: Option<i64>,
}

fn word(v: &serde_json::Value) -> Option<String> {
    let s = v.as_str()?;
    let s: String = s
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '\'' || *c == '-' || *c == ' ')
        .collect::<String>()
        .trim()
        .to_lowercase();
    (!s.is_empty() && s.chars().count() <= MAX_KEYWORD_CHARS).then_some(s)
}

/// Reads the model's reply. It must be one JSON object (extra prose or a code fence around it is tolerated) whose
/// every field passes the checks here; anything else is `None` and nothing is run.
pub fn parse_plan(reply: &str, today: NaiveDate) -> Option<Understood> {
    let (a, b) = (reply.find('{')?, reply.rfind('}')?);
    let raw: Raw = serde_json::from_str(reply.get(a..=b)?).ok()?;
    let kind = match raw.kind.trim().to_lowercase().as_str() {
        "events" => Kind::Events,
        "free" => Kind::Free,
        "none" => return Some(Understood::NotACalendarQuestion),
        _ => return None,
    };
    let from = NaiveDate::parse_from_str(raw.from.trim(), "%Y-%m-%d").ok()?;
    let mut to = NaiveDate::parse_from_str(raw.to.trim(), "%Y-%m-%d").unwrap_or(from);
    if to < from || (to - from).num_days() >= MAX_SPAN_DAYS {
        return None;
    }
    if (from - today).num_days().abs() > MAX_DISTANCE_DAYS
        || (to - today).num_days().abs() > MAX_DISTANCE_DAYS
    {
        return None;
    }
    if kind == Kind::Free {
        to = to.min(from + Duration::days(MAX_DAYS - 1));
    }
    let mut keywords: Vec<String> = raw.keywords.iter().filter_map(word).collect();
    keywords.dedup();
    keywords.truncate(MAX_KEYWORDS);
    Some(Understood::Plan(Plan {
        kind,
        from,
        to,
        keywords,
        minutes: raw.minutes.unwrap_or(30).clamp(5, 12 * 60),
    }))
}

#[derive(Debug, Serialize)]
pub struct Answer {
    pub kind: Kind,
    pub from: NaiveDate,
    pub to: NaiveDate,
    pub keywords: Vec<String>,
    pub minutes: i64,
    /// How many events matched (the list holds at most [`MAX_EVENTS`]).
    pub count: usize,
    pub events: Vec<Event>,
    pub slots: Vec<Slot>,
}

fn haystack(e: &Event) -> String {
    format!(
        "{} {} {}",
        e.title,
        e.location.as_deref().unwrap_or(""),
        e.organizer.as_deref().unwrap_or("")
    )
    .to_lowercase()
}

fn start_of(e: &Event, tz: Tz) -> DateTime<Utc> {
    match &e.when {
        When::Timed { start, .. } => *start,
        When::AllDay { start, .. } => day_bounds(*start, tz).0,
    }
}

/// Runs the plan over the person's own events.
pub fn answer(
    plan: &Plan,
    events: &[Event],
    now: DateTime<Utc>,
    tz: Tz,
    s: &BriefingSettings,
) -> Answer {
    let (lo, _) = day_bounds(plan.from, tz);
    let (_, hi) = day_bounds(plan.to, tz);
    let mut out = Answer {
        kind: plan.kind,
        from: plan.from,
        to: plan.to,
        keywords: plan.keywords.clone(),
        minutes: plan.minutes,
        count: 0,
        events: vec![],
        slots: vec![],
    };
    match plan.kind {
        Kind::Free => {
            let days = (plan.to - plan.from).num_days() + 1;
            out.slots = find_free_slots(events, plan.from, days, plan.minutes, now, tz, s);
        }
        Kind::Events => {
            let mut hits: Vec<&Event> = events
                .iter()
                .filter(|e| is_shown(e, s))
                .filter(|e| match &e.when {
                    When::Timed { start, end, .. } => *start < hi && (*end > lo || *start >= lo),
                    When::AllDay { start, end } => *start <= plan.to && *end > plan.from,
                })
                .filter(|e| {
                    let h = haystack(e);
                    plan.keywords.iter().all(|k| h.contains(k.as_str()))
                })
                .collect();
            hits.sort_by_key(|e| (start_of(e, tz), e.title.clone(), e.id.clone()));
            out.count = hits.len();
            out.events = hits.into_iter().take(MAX_EVENTS).cloned().collect();
        }
    }
    out
}

#[cfg(test)]
mod tests;
