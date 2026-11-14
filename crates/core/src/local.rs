//! Events the user adds themselves.
//!
//! What the user typed (title, times, repeat rule, reminder, invitees) is kept as one record in
//! `own_events`. The dated events the app shows are generated from it into the ordinary `events` table,
//! under one account ("On this computer") and one calendar, so the briefing, notifications, calendar
//! views and "Erase all data" treat them like any other. Repeats are expanded by the same tested
//! recurrence code that reads calendar feeds. Nothing here talks to a provider: the entries never leave
//! this computer, and Firstlight still asks for no write access to anyone's calendar.

use crate::invite::{self, IcsExtras};
use crate::model::*;
use crate::store::Store;
use crate::{Error, Result, join};
use chrono::{DateTime, Duration, NaiveDate, Utc};
use chrono_tz::Tz;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub const ACCOUNT_ID: &str = "local-account";
pub const CALENDAR_ID: &str = "local-events";
const MAX_TITLE: usize = 200;
const MAX_LOCATION: usize = 300;
const MAX_NOTES: usize = 4000;
const MAX_LINK: usize = 2000;
const MAX_DAYS: i64 = 366;
const MAX_REPEAT_YEARS: i64 = 20;
const MAX_INSTANCES: usize = 800;
const MAX_REMINDER_MINUTES: u32 = 7 * 24 * 60;

/// How an event repeats. Only these shapes exist; a raw recurrence rule is never accepted from the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepeatInput {
    /// "daily", "weekly", "monthly" or "yearly".
    pub every: String,
    /// Every 1..=30 days, weeks, months or years.
    pub interval: u32,
    /// The last day it can happen (`YYYY-MM-DD`, inclusive). `None` means no end.
    pub until: Option<String>,
}

/// What the form sends. All-day dates are `YYYY-MM-DD` with `end` the *last* day (inclusive);
/// timed events use RFC 3339 instants.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventInput {
    pub title: String,
    pub all_day: bool,
    pub start: String,
    pub end: String,
    #[serde(default)]
    pub location: Option<String>,
    #[serde(default)]
    pub join_url: Option<String>,
    #[serde(default)]
    pub notes: Option<String>,
    #[serde(default)]
    pub repeat: Option<RepeatInput>,
    /// Notify this many minutes before it starts (timed events only).
    #[serde(default)]
    pub reminder_minutes: Option<u32>,
    /// People to share it with. Kept so the invitation can be sent again; nothing is sent by Firstlight.
    #[serde(default)]
    pub invitees: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Stored {
    input: EventInput,
    /// The zone the times were entered in, so a repeating 9:00 stays 9:00 across daylight saving.
    tz: String,
}

/// An own event as the edit form needs it.
#[derive(Debug, Clone, Serialize)]
pub struct OwnEvent {
    /// The event's id (the series id for a repeating one).
    pub id: String,
    pub input: EventInput,
}

fn clean(s: &Option<String>, max: usize, too_long: &'static str) -> Result<Option<String>> {
    let t = s.as_deref().unwrap_or("").trim();
    if t.chars().count() > max {
        return Err(Error::Invalid(too_long));
    }
    Ok((!t.is_empty()).then(|| t.to_string()))
}

fn parse_date(s: &str) -> Result<NaiveDate> {
    NaiveDate::parse_from_str(s.trim(), "%Y-%m-%d")
        .map_err(|_| Error::Invalid("Choose valid dates."))
}

/// Checks the input and builds the event (the first occurrence, for a repeating one).
pub fn build_event(
    input: &EventInput,
    master: Option<&str>,
    tz: Tz,
    now: DateTime<Utc>,
) -> Result<Event> {
    let title = input.title.trim();
    if title.is_empty() {
        return Err(Error::Invalid("Give the event a title."));
    }
    if title.chars().count() > MAX_TITLE {
        return Err(Error::Invalid("That title is too long."));
    }
    if title.chars().any(char::is_control) {
        return Err(Error::Invalid(
            "The title can't contain control characters.",
        ));
    }
    let location = clean(&input.location, MAX_LOCATION, "That location is too long.")?;
    let notes = clean(&input.notes, MAX_NOTES, "Those notes are too long.")?;
    let join_url = match clean(&input.join_url, MAX_LINK, "That link is too long.")? {
        None => None,
        Some(u) => Some(
            join::from_conference_field(&u)
                .filter(|v| !v.chars().any(|c| c.is_whitespace() || c.is_control()))
                .ok_or(Error::Invalid(
                    "The meeting link must start with https:// and be a single link.",
                ))?,
        ),
    };

    let when = if input.all_day {
        let (start, last) = (parse_date(&input.start)?, parse_date(&input.end)?);
        if last < start {
            return Err(Error::Invalid("The event can't end before it starts."));
        }
        if (last - start).num_days() >= MAX_DAYS {
            return Err(Error::Invalid("An event can last at most a year."));
        }
        When::AllDay {
            start,
            end: last + Duration::days(1),
        }
    } else {
        let parse = |s: &str| {
            DateTime::parse_from_rfc3339(s.trim())
                .map(|d| d.with_timezone(&Utc))
                .map_err(|_| Error::Invalid("Choose a valid date and time."))
        };
        let (start, end) = (parse(&input.start)?, parse(&input.end)?);
        if end <= start {
            return Err(Error::Invalid("The event must end after it starts."));
        }
        if end - start > Duration::days(MAX_DAYS) {
            return Err(Error::Invalid("An event can last at most a year."));
        }
        When::Timed {
            start,
            end,
            tzid: Some(tz.name().to_string()),
        }
    };

    let remote = master.map_or_else(new_id, str::to_string);
    Ok(Event {
        id: format!("{CALENDAR_ID}:{remote}"),
        calendar_id: CALENDAR_ID.into(),
        remote_id: remote.clone(),
        ical_uid: format!("{remote}@firstlight.local"),
        title: title.to_string(),
        when,
        location,
        description: notes,
        join_url,
        status: EventStatus::Confirmed,
        my_response: Response::None,
        organizer: None,
        recurrence_master_id: None,
        updated_at: now,
        also_in: vec![],
    })
}

/// Validates the repeat rule, reminder and invitees. Returns the input with the invitees tidied, and
/// the recurrence rule text built from the enum (or `None`).
fn check_extras(input: &EventInput, base: &Event, tz: Tz) -> Result<(EventInput, Option<String>)> {
    let mut out = input.clone();

    out.invitees = invite::parse_list(&input.invitees.join(","))?;

    if let Some(m) = input.reminder_minutes {
        if input.all_day {
            return Err(Error::Invalid("Reminders need a start time."));
        }
        if m == 0 || m > MAX_REMINDER_MINUTES {
            return Err(Error::Invalid("Choose a reminder of up to a week."));
        }
    }

    let rule = match &input.repeat {
        None => None,
        Some(r) => {
            let freq = match r.every.as_str() {
                "daily" => "DAILY",
                "weekly" => "WEEKLY",
                "monthly" => "MONTHLY",
                "yearly" => "YEARLY",
                _ => return Err(Error::Invalid("Choose how often it repeats.")),
            };
            if !(1..=30).contains(&r.interval) {
                return Err(Error::Invalid(
                    "It can repeat every 1 to 30 days, weeks, months or years.",
                ));
            }
            let mut rule = format!("FREQ={freq};INTERVAL={}", r.interval);
            if let Some(until) = &r.until {
                let day = parse_date(until)?;
                let first = match &base.when {
                    When::AllDay { start, .. } => *start,
                    When::Timed { start, .. } => start.with_timezone(&tz).date_naive(),
                };
                if day < first {
                    return Err(Error::Invalid("It can't stop repeating before it starts."));
                }
                if day - first > Duration::days(365 * MAX_REPEAT_YEARS) {
                    return Err(Error::Invalid("Choose an end date within 20 years."));
                }
                if input.all_day {
                    rule.push_str(&format!(";UNTIL={}", day.format("%Y%m%d")));
                } else {
                    // The end of that day where the event happens, as the UTC instant the rule requires.
                    let eod = day.and_hms_opt(23, 59, 59).unwrap_or_default();
                    rule.push_str(&format!(
                        ";UNTIL={}",
                        crate::tz::local_to_utc(tz, eod).format("%Y%m%dT%H%M%SZ")
                    ));
                }
            }
            Some(rule)
        }
    };
    Ok((out, rule))
}

/// The dated events one record produces. A single event is one entry; a repeating one is expanded
/// across the window by the recurrence engine.
fn expand(
    master: &str,
    stored: &Stored,
    window: &crate::providers::Window,
    now: DateTime<Utc>,
) -> Result<Vec<Event>> {
    let tz: Tz = stored.tz.parse().unwrap_or(chrono_tz::UTC);
    let base = build_event(&stored.input, Some(master), tz, now)?;
    let (_, rule) = check_extras(&stored.input, &base, tz)?;
    let Some(rule) = rule else {
        return Ok(vec![base]);
    };
    let text = invite::ics(
        &base,
        &IcsExtras {
            tz,
            rrule: Some(&rule),
            include_notes: true,
            now,
        },
    );
    let opts = crate::ics::ParseOptions {
        calendar_id: CALENDAR_ID,
        default_tz: tz,
        window: *window,
        my_email: None,
    };
    let mut out = Vec::new();
    for mut e in crate::ics::events_from_ics(&text, &opts)?
        .into_iter()
        .take(MAX_INSTANCES)
    {
        let key = match &e.when {
            When::Timed { start, .. } => start.timestamp().to_string(),
            When::AllDay { start, .. } => start.format("%Y%m%d").to_string(),
        };
        e.id = format!("{CALENDAR_ID}:{master}:{key}");
        e.remote_id = format!("{master}:{key}");
        e.recurrence_master_id = Some(master.to_string());
        e.my_response = Response::None;
        out.push(e);
    }
    Ok(out)
}

/// Creates the account and calendar the first time an event is added.
pub fn ensure_calendar(store: &Store, now: DateTime<Utc>) -> Result<()> {
    if store.get_account(ACCOUNT_ID)?.is_some() {
        return Ok(());
    }
    store.upsert_account(&Account {
        id: ACCOUNT_ID.into(),
        provider: ProviderKind::Local,
        display_name: "On this computer".into(),
        email: None,
        status: AccountStatus::Ok,
        secret_ref: None,
        created_at: now,
    })?;
    store.upsert_calendar(&Calendar {
        id: CALENDAR_ID.into(),
        account_id: ACCOUNT_ID.into(),
        remote_id: "events".into(),
        name: "My events".into(),
        colour: "purple".into(),
        visible: true,
        read_only: false,
    })
}

/// Regenerates the dated events from the stored records. Safe to call any time; the app calls it after
/// every change, at launch, and once a day so repeats keep reaching into the future.
pub fn refresh(store: &mut Store, now: DateTime<Utc>) -> Result<()> {
    let rows = store.all_own_events()?;
    if rows.is_empty() && store.get_account(ACCOUNT_ID)?.is_none() {
        return Ok(());
    }
    ensure_calendar(store, now)?;
    let window = crate::sync::default_window(now);
    let mut events = Vec::new();
    for (id, json) in rows {
        // A record we cannot read is skipped, never allowed to hide the others.
        if let Ok(stored) = serde_json::from_str::<Stored>(&json)
            && let Ok(mut e) = expand(&id, &stored, &window, now)
        {
            events.append(&mut e);
        }
    }
    store.replace_events(CALENDAR_ID, &events)
}

/// Adds an event (`id` is `None`) or changes one of our own, for a repeating one the whole series.
pub fn save(
    store: &mut Store,
    id: Option<&str>,
    input: &EventInput,
    tz: Tz,
    now: DateTime<Utc>,
) -> Result<Event> {
    let master = match id {
        None => None,
        Some(id) => {
            let m = own_master_id(id)?;
            if store.get_own_event(m)?.is_none() {
                return Err(Error::Invalid("That event can't be changed here."));
            }
            Some(m)
        }
    };
    let base = build_event(input, master, tz, now)?;
    let (clean_input, _) = check_extras(input, &base, tz)?;
    let stored = Stored {
        input: clean_input,
        tz: tz.name().to_string(),
    };
    let json = serde_json::to_string(&stored)
        .map_err(|_| Error::Invalid("That event couldn't be saved."))?;
    store.put_own_event(&base.remote_id, &json)?;
    refresh(store, now)?;
    Ok(base)
}

/// The record for the edit form. `id` may be the event or any one occurrence of a series.
pub fn get(store: &Store, id: &str) -> Result<Option<OwnEvent>> {
    let master = own_master_id(id)?;
    Ok(store
        .get_own_event(master)?
        .and_then(|j| serde_json::from_str::<Stored>(&j).ok())
        .map(|s| OwnEvent {
            id: format!("{CALENDAR_ID}:{master}"),
            input: s.input,
        }))
}

/// What to put in an invitation file for one of our own events: the first occurrence and, for a
/// repeating one, the recurrence rule, so the people invited get the whole series. `None` if `id`
/// isn't ours.
pub fn share_parts(store: &Store, id: &str) -> Result<Option<(Event, Option<String>)>> {
    let master = own_master_id(id)?;
    let Some(stored) = store
        .get_own_event(master)?
        .and_then(|j| serde_json::from_str::<Stored>(&j).ok())
    else {
        return Ok(None);
    };
    let tz: Tz = stored.tz.parse().unwrap_or(chrono_tz::UTC);
    let base = build_event(&stored.input, Some(master), tz, Utc::now())?;
    let (_, rule) = check_extras(&stored.input, &base, tz)?;
    Ok(Some((base, rule)))
}

/// Deletes an event, or a whole repeating series.
pub fn delete(store: &mut Store, id: &str, now: DateTime<Utc>) -> Result<()> {
    let master = own_master_id(id)?.to_string();
    store.delete_own_event(&master)?;
    refresh(store, now)
}

/// Only events in our own calendar can be changed or removed, whatever id the page sends. Accepts the
/// event id or one occurrence of a series (`local-events:<id>:<occurrence>`).
fn own_master_id(id: &str) -> Result<&str> {
    let rest = id
        .strip_prefix(&format!("{CALENDAR_ID}:"))
        .ok_or(Error::Invalid("That event can't be changed here."))?;
    let (master, occurrence) = rest.split_once(':').unwrap_or((rest, ""));
    let ok = !master.is_empty()
        && master
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-')
        && occurrence.bytes().all(|b| b.is_ascii_digit());
    ok.then_some(master)
        .ok_or(Error::Invalid("That event can't be changed here."))
}

// ---- reminders ---------------------------------------------------------------------------

/// A reminder that is due.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reminder {
    /// Unique per occurrence and lead time; recorded once shown so a restart never repeats it.
    pub id: String,
    pub title: String,
    pub start: DateTime<Utc>,
}

/// Reminder lead times by event (series) id.
pub fn reminder_map(store: &Store) -> Result<HashMap<String, u32>> {
    Ok(store
        .all_own_events()?
        .into_iter()
        .filter_map(|(id, json)| {
            let m = serde_json::from_str::<Stored>(&json)
                .ok()?
                .input
                .reminder_minutes?;
            Some((id, m))
        })
        .collect())
}

fn master_of(e: &Event) -> &str {
    e.recurrence_master_id
        .as_deref()
        .unwrap_or_else(|| e.remote_id.split(':').next().unwrap_or(""))
}

/// Own events that have a reminder of their own. The general "before each event" heads-up skips these,
/// so nobody is told twice.
pub fn has_own_reminder(map: &HashMap<String, u32>, e: &Event) -> bool {
    e.calendar_id == CALENDAR_ID && map.contains_key(master_of(e))
}

/// Reminders whose moment has come: it starts later, and `now` is within the lead time.
pub fn due_reminders(store: &Store, now: DateTime<Utc>) -> Result<Vec<Reminder>> {
    let map = reminder_map(store)?;
    if map.is_empty() {
        return Ok(vec![]);
    }
    let mut out = Vec::new();
    for e in store.events_between(now, now + Duration::days(8))? {
        if e.calendar_id != CALENDAR_ID {
            continue;
        }
        let (When::Timed { start, .. }, Some(minutes)) = (&e.when, map.get(master_of(&e))) else {
            continue;
        };
        if *start > now && now >= *start - Duration::minutes(i64::from(*minutes)) {
            out.push(Reminder {
                id: format!("own-reminder:{}:{minutes}", e.id),
                title: e.title.clone(),
                start: *start,
            });
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests;
