//! The same meeting often appears in several connected accounts (you were invited on
//! your work and personal addresses). Show it once and remember where else it lives.

use crate::model::*;
use std::collections::HashMap;

fn key(e: &Event) -> Option<(String, String)> {
    if e.ical_uid.is_empty() {
        return None;
    }
    let when = match &e.when {
        When::Timed { start, .. } => start.timestamp().to_string(),
        When::AllDay { start, .. } => start.to_string(),
    };
    Some((e.ical_uid.clone(), when))
}

/// Merges duplicates. The first event stays primary unless a later one carries RSVP
/// information the first lacks; every other calendar id lands in `also_in`.
/// Input order (and therefore sort order) is preserved.
pub fn dedupe(events: Vec<Event>) -> Vec<Event> {
    let mut out: Vec<Event> = Vec::with_capacity(events.len());
    let mut seen: HashMap<(String, String), usize> = HashMap::new();
    for e in events {
        let Some(k) = key(&e) else {
            out.push(e);
            continue;
        };
        match seen.get(&k) {
            None => {
                seen.insert(k, out.len());
                out.push(e);
            }
            Some(&i) => {
                let primary = &mut out[i];
                if e.calendar_id == primary.calendar_id {
                    continue; // exact repeat within one calendar
                }
                let mut others = std::mem::take(&mut primary.also_in);
                if primary.my_response == Response::None && e.my_response != Response::None {
                    // The later copy knows our RSVP: it becomes primary.
                    others.push(primary.calendar_id.clone());
                    *primary = e;
                } else if !others.contains(&e.calendar_id) {
                    others.push(e.calendar_id.clone());
                }
                others.sort();
                others.dedup();
                primary.also_in = others;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};

    fn ev(cal: &str, uid: &str, hour: u32, resp: Response) -> Event {
        Event {
            id: format!("{cal}:{uid}:{hour}"),
            calendar_id: cal.into(),
            remote_id: uid.into(),
            ical_uid: uid.into(),
            title: "Sync".into(),
            when: When::Timed {
                start: Utc.with_ymd_and_hms(2026, 3, 10, hour, 0, 0).unwrap(),
                end: Utc.with_ymd_and_hms(2026, 3, 10, hour + 1, 0, 0).unwrap(),
                tzid: None,
            },
            location: None,
            description: None,
            join_url: None,
            status: EventStatus::Confirmed,
            my_response: resp,
            organizer: None,
            recurrence_master_id: None,
            updated_at: Utc::now(),
            also_in: vec![],
        }
    }

    #[test]
    fn same_uid_and_start_in_two_calendars_shows_once() {
        let out = dedupe(vec![
            ev("work", "u1", 9, Response::Accepted),
            ev("personal", "u1", 9, Response::Accepted),
        ]);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].calendar_id, "work");
        assert_eq!(out[0].also_in, ["personal"]);
    }

    #[test]
    fn different_start_or_uid_is_not_a_duplicate() {
        let out = dedupe(vec![
            ev("work", "u1", 9, Response::None),
            ev("personal", "u1", 10, Response::None), // same series, different occurrence
            ev("personal", "u2", 9, Response::None),
        ]);
        assert_eq!(out.len(), 3);
        assert!(out.iter().all(|e| e.also_in.is_empty()));
    }

    #[test]
    fn copy_with_an_rsvp_becomes_primary() {
        let out = dedupe(vec![
            ev("feed", "u1", 9, Response::None),
            ev("work", "u1", 9, Response::Declined),
        ]);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].calendar_id, "work");
        assert_eq!(out[0].my_response, Response::Declined);
        assert_eq!(out[0].also_in, ["feed"]);
    }

    #[test]
    fn three_way_duplicates_and_order_are_preserved() {
        let out = dedupe(vec![
            ev("a", "u1", 9, Response::None),
            ev("a", "other", 11, Response::None),
            ev("b", "u1", 9, Response::None),
            ev("c", "u1", 9, Response::None),
        ]);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].also_in, ["b", "c"]);
        assert_eq!(out[1].ical_uid, "other");
    }

    #[test]
    fn events_without_a_uid_are_never_merged() {
        let out = dedupe(vec![
            ev("a", "", 9, Response::None),
            ev("b", "", 9, Response::None),
        ]);
        assert_eq!(out.len(), 2);
    }
}
