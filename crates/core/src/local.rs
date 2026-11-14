//! Events the user adds themselves.
//!
//! They live in the same tables as every other event, under one account ("On this computer") and one
//! calendar, so the briefing, notifications, calendar views and "Erase all data" treat them like any
//! other. Nothing here talks to a provider: the entries never leave this computer, and Firstlight
//! still asks for no write access to anyone's calendar.

use crate::model::*;
use crate::store::Store;
use crate::{Error, Result, join};
use chrono::{DateTime, Duration, NaiveDate, Utc};
use chrono_tz::Tz;
use serde::Deserialize;

pub const ACCOUNT_ID: &str = "local-account";
pub const CALENDAR_ID: &str = "local-events";
const MAX_TITLE: usize = 200;
const MAX_LOCATION: usize = 300;
const MAX_NOTES: usize = 4000;
const MAX_LINK: usize = 2000;
const MAX_DAYS: i64 = 366;

/// What the form sends. All-day dates are `YYYY-MM-DD` with `end` the *last* day (inclusive);
/// timed events use RFC 3339 instants.
#[derive(Debug, Clone, Deserialize)]
pub struct EventInput {
    pub title: String,
    pub all_day: bool,
    pub start: String,
    pub end: String,
    pub location: Option<String>,
    pub join_url: Option<String>,
    pub notes: Option<String>,
}

fn clean(s: &Option<String>, max: usize, too_long: &'static str) -> Result<Option<String>> {
    let t = s.as_deref().unwrap_or("").trim();
    if t.chars().count() > max {
        return Err(Error::Invalid(too_long));
    }
    Ok((!t.is_empty()).then(|| t.to_string()))
}

/// Checks the input and builds the event. `remote_id` is `None` for a new event.
pub fn build_event(
    input: &EventInput,
    remote_id: Option<&str>,
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
        let parse = |s: &str| {
            NaiveDate::parse_from_str(s.trim(), "%Y-%m-%d")
                .map_err(|_| Error::Invalid("Choose valid dates."))
        };
        let (start, last) = (parse(&input.start)?, parse(&input.end)?);
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

    let remote = remote_id.map_or_else(new_id, str::to_string);
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

/// Adds an event (`id` is `None`) or replaces one of our own. Returns the saved event.
pub fn save(
    store: &mut Store,
    id: Option<&str>,
    input: &EventInput,
    tz: Tz,
    now: DateTime<Utc>,
) -> Result<Event> {
    let remote = match id {
        None => None,
        Some(id) => Some(own_remote_id(id)?),
    };
    let event = build_event(input, remote, tz, now)?;
    ensure_calendar(store, now)?;
    store.upsert_events(CALENDAR_ID, std::slice::from_ref(&event))?;
    Ok(event)
}

pub fn delete(store: &mut Store, id: &str) -> Result<()> {
    let remote = own_remote_id(id)?.to_string();
    store.delete_events_by_remote_id(CALENDAR_ID, &[remote])
}

/// Only events in our own calendar can be changed or removed, whatever id the page sends.
fn own_remote_id(id: &str) -> Result<&str> {
    id.strip_prefix(&format!("{CALENDAR_ID}:"))
        .filter(|r| !r.is_empty() && r.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-'))
        .ok_or(Error::Invalid("That event can't be changed here."))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::secrets::{MemorySecretStore, load_or_create_db_key};

    fn store() -> Store {
        let key = load_or_create_db_key(&MemorySecretStore::default()).unwrap();
        Store::open_in_memory(&key).unwrap()
    }
    fn timed(title: &str) -> EventInput {
        EventInput {
            title: title.into(),
            all_day: false,
            start: "2026-11-20T15:00:00Z".into(),
            end: "2026-11-20T16:00:00Z".into(),
            location: Some(" Room 4 ".into()),
            join_url: Some("https://meet.google.com/abc-defg-hij".into()),
            notes: None,
        }
    }
    const NY: Tz = chrono_tz::America::New_York;

    #[test]
    fn adds_edits_and_removes_an_event_that_the_briefing_sees() {
        let mut s = store();
        let now = "2026-11-14T12:00:00Z".parse().unwrap();
        let e = save(&mut s, None, &timed("Dentist"), NY, now).unwrap();
        assert_eq!(e.location.as_deref(), Some("Room 4"));
        assert_eq!(s.list_accounts().unwrap().len(), 1);
        let (from, to) = (
            "2026-11-20T00:00:00Z".parse().unwrap(),
            "2026-11-21T00:00:00Z".parse().unwrap(),
        );
        assert_eq!(s.events_between(from, to).unwrap().len(), 1);

        let mut edit = timed("Dentist (moved)");
        edit.start = "2026-11-21T15:00:00Z".into();
        edit.end = "2026-11-21T16:00:00Z".into();
        let moved = save(&mut s, Some(&e.id), &edit, NY, now).unwrap();
        assert_eq!(moved.id, e.id, "same event, not a copy");
        assert_eq!(s.events_between(from, to).unwrap().len(), 0);

        delete(&mut s, &e.id).unwrap();
        let far = "2026-12-01T00:00:00Z".parse().unwrap();
        assert!(s.events_between(from, far).unwrap().is_empty());
    }

    #[test]
    fn all_day_end_is_inclusive_in_the_form_and_exclusive_inside() {
        let input = EventInput {
            all_day: true,
            start: "2026-12-24".into(),
            end: "2026-12-26".into(),
            ..timed("Holiday")
        };
        let e = build_event(&input, None, NY, Utc::now()).unwrap();
        assert_eq!(
            e.when,
            When::AllDay {
                start: "2026-12-24".parse().unwrap(),
                end: "2026-12-27".parse().unwrap()
            }
        );
    }

    #[test]
    fn bad_input_is_refused_with_a_reason() {
        let now = Utc::now();
        let bad = |f: &dyn Fn(&mut EventInput)| {
            let mut i = timed("x");
            f(&mut i);
            build_event(&i, None, NY, now).unwrap_err().to_string()
        };
        assert!(bad(&|i| i.title = "  ".into()).contains("title"));
        assert!(bad(&|i| i.title = "a".repeat(201)).contains("too long"));
        assert!(bad(&|i| i.title = "a\nb".into()).contains("control"));
        assert!(bad(&|i| i.end = i.start.clone()).contains("end after"));
        assert!(bad(&|i| i.start = "tomorrow".into()).contains("valid"));
        assert!(bad(&|i| i.end = "2030-01-01T00:00:00Z".into()).contains("a year"));
        assert!(bad(&|i| i.join_url = Some("http://zoom.us/j/1".into())).contains("https"));
        assert!(bad(&|i| i.join_url = Some("javascript:alert(1)".into())).contains("https"));
        assert!(bad(&|i| i.join_url = Some("https://a.com/x y".into())).contains("single link"));
        assert!(bad(&|i| i.notes = Some("n".repeat(4001))).contains("too long"));
        assert!(
            bad(&|i| {
                i.all_day = true;
                i.start = "2026-12-05".into();
                i.end = "2026-12-01".into()
            })
            .contains("before")
        );
    }

    #[test]
    fn only_our_own_events_can_be_touched() {
        let mut s = store();
        let now = Utc::now();
        for id in [
            "fixture-work:standup",
            "local-events:",
            "local-events:a/b",
            "../local-events:x",
            "",
        ] {
            assert!(
                save(&mut s, Some(id), &timed("x"), NY, now).is_err(),
                "{id}"
            );
            assert!(delete(&mut s, id).is_err(), "{id}");
        }
    }

    #[test]
    fn erasing_the_account_removes_the_events() {
        let mut s = store();
        let e = save(&mut s, None, &timed("Private"), NY, Utc::now()).unwrap();
        s.remove_account(ACCOUNT_ID).unwrap();
        let (a, b) = (
            "2000-01-01T00:00:00Z".parse().unwrap(),
            "2100-01-01T00:00:00Z".parse().unwrap(),
        );
        assert!(s.events_between(a, b).unwrap().iter().all(|x| x.id != e.id));
    }
}
