//! Follow-ups: after a meeting you wrote notes in, a nudge to turn them into tasks or send a short recap.
//!
//! A meeting counts when it has ended today, it is a timed meeting that was shown (not cancelled or declined), and its
//! private meeting note (`notes::meeting_key`) was written or changed from an hour before it started onwards, so the
//! standing note of a repeating meeting does not prompt every week by itself.

use crate::briefing::{BriefingSettings, is_shown};
use crate::model::{Event, When};
use chrono::{DateTime, Duration, Utc};
use chrono_tz::Tz;
use serde::Serialize;
use std::collections::HashMap;

/// How long after a meeting ends the briefing still offers its follow-up.
pub const OFFER_HOURS: i64 = 3;
/// The notification comes within this long of the end (if the app was asleep it is not shown late).
pub const NOTIFY_MINUTES: i64 = 15;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FollowUp {
    pub event_id: String,
    pub title: String,
    pub end: DateTime<Utc>,
    /// The note as written.
    pub note: String,
    /// Everyone on the guest list (organiser first), for a recap draft. Empty when the calendar gives none.
    pub people: Vec<String>,
}

/// Meetings that ended in the last three hours (today) and have a note written for them, most recent first.
/// `notes` maps a meeting key to the note text and when it was last changed.
pub fn candidates(
    events: &[Event],
    notes: &HashMap<String, (String, DateTime<Utc>)>,
    now: DateTime<Utc>,
    tz: Tz,
    bs: &BriefingSettings,
) -> Vec<FollowUp> {
    let today = now.with_timezone(&tz).date_naive();
    let mut out: Vec<FollowUp> = events
        .iter()
        .filter(|e| is_shown(e, bs))
        .filter_map(|e| {
            let When::Timed { start, end, .. } = e.when else {
                return None;
            };
            if end > now
                || now - end > Duration::hours(OFFER_HOURS)
                || end.with_timezone(&tz).date_naive() != today
            {
                return None;
            }
            let (note, changed) = notes.get(&crate::notes::meeting_key(e))?;
            if *changed < start - Duration::hours(1) {
                return None;
            }
            let mut people: Vec<String> = Vec::new();
            for a in e
                .attendees
                .iter()
                .filter(|a| a.organizer)
                .chain(e.attendees.iter().filter(|a| !a.organizer))
            {
                if !people.contains(&a.email) {
                    people.push(a.email.clone());
                }
            }
            Some(FollowUp {
                event_id: e.id.clone(),
                title: e.title.clone(),
                end,
                note: note.clone(),
                people,
            })
        })
        .collect();
    out.sort_by_key(|f| std::cmp::Reverse(f.end));
    out.dedup_by(|a, b| a.event_id == b.event_id);
    out
}

/// The ones whose notification is due now (within 15 minutes of the end).
pub fn to_notify(followups: &[FollowUp], now: DateTime<Utc>) -> Vec<&FollowUp> {
    followups
        .iter()
        .filter(|f| now - f.end <= Duration::minutes(NOTIFY_MINUTES))
        .collect()
}

/// A recap message: the title and the note, cut to fit a mail link (the person reads and edits it before sending).
pub fn recap(f: &FollowUp, max_chars: usize) -> (String, String) {
    let subject = format!("Notes from {}", f.title);
    let mut body = format!("Hi all,\n\nNotes from {}:\n\n{}\n", f.title, f.note.trim());
    if body.chars().count() > max_chars {
        body = body
            .chars()
            .take(max_chars.saturating_sub(1))
            .collect::<String>();
        body.push('…');
    }
    (subject, body)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::*;

    const NY: Tz = chrono_tz::America::New_York;

    fn t(s: &str) -> DateTime<Utc> {
        s.parse().unwrap()
    }

    fn ev(id: &str, title: &str, start: &str, end: &str) -> Event {
        Event {
            id: id.into(),
            calendar_id: "cal".into(),
            remote_id: id.into(),
            ical_uid: id.into(),
            title: title.into(),
            when: When::Timed {
                start: t(start),
                end: t(end),
                tzid: None,
            },
            location: None,
            description: None,
            join_url: None,
            status: EventStatus::Confirmed,
            my_response: Response::Accepted,
            organizer: None,
            recurrence_master_id: None,
            updated_at: t(start),
            also_in: vec![],
            attendees: vec![],
        }
    }

    #[test]
    fn a_meeting_with_fresh_notes_gets_a_follow_up_for_three_hours() {
        let mut review = ev(
            "r",
            "Design review",
            "2026-11-18T15:00:00Z",
            "2026-11-18T16:00:00Z",
        );
        review.attendees = vec![
            Attendee {
                email: "kim@example.com".into(),
                name: None,
                response: Response::Accepted,
                organizer: false,
            },
            Attendee {
                email: "priya@example.com".into(),
                name: None,
                response: Response::Accepted,
                organizer: true,
            },
        ];
        let weekly = ev(
            "w",
            "Weekly sync",
            "2026-11-18T14:00:00Z",
            "2026-11-18T14:30:00Z",
        );
        let no_notes = ev(
            "n",
            "Budget",
            "2026-11-18T13:00:00Z",
            "2026-11-18T13:30:00Z",
        );
        let notes: HashMap<String, (String, DateTime<Utc>)> = [
            (
                crate::notes::meeting_key(&review),
                ("Sam sends slides".to_string(), t("2026-11-18T15:20:00Z")),
            ),
            // Written last week: the standing note of a repeating meeting does not count.
            (
                crate::notes::meeting_key(&weekly),
                ("Old".to_string(), t("2026-11-11T14:10:00Z")),
            ),
        ]
        .into();
        let events = [review, weekly, no_notes];
        let bs = BriefingSettings::default();
        assert!(candidates(&events, &notes, t("2026-11-18T15:59:00Z"), NY, &bs).is_empty()); // not over yet
        let f = candidates(&events, &notes, t("2026-11-18T16:05:00Z"), NY, &bs);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].people, ["priya@example.com", "kim@example.com"]);
        assert_eq!(to_notify(&f, t("2026-11-18T16:05:00Z")).len(), 1);
        assert!(to_notify(&f, t("2026-11-18T16:16:00Z")).is_empty());
        assert!(candidates(&events, &notes, t("2026-11-18T19:01:00Z"), NY, &bs).is_empty());
    }

    #[test]
    fn a_recap_is_cut_to_fit() {
        let f = FollowUp {
            event_id: "r".into(),
            title: "Design review".into(),
            end: t("2026-11-18T16:00:00Z"),
            note: "x".repeat(5000),
            people: vec![],
        };
        let (subject, body) = recap(&f, 1500);
        assert_eq!(subject, "Notes from Design review");
        assert_eq!(body.chars().count(), 1500);
        assert!(body.ends_with('…'));
    }
}
