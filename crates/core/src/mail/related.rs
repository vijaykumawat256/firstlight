//! Mail that is probably about a given meeting, picked from the unread digest already fetched (nothing is
//! fetched for this, and no message text is read): from the organiser, or with the meeting's name in the subject.

use super::{MailDigest, MailMessage};
use crate::model::Event;

const MAX: usize = 3;
/// Words too common in meeting names to say anything on their own.
const COMMON: &[&str] = &[
    "meeting", "call", "sync", "weekly", "daily", "review", "catch", "with", "check", "update",
    "team", "monthly",
];

fn norm(s: &str) -> String {
    s.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// Name without a trailing "(PST)" style note.
fn plain_name(s: &str) -> String {
    norm(s.split('(').next().unwrap_or(s))
}

fn significant(title: &str) -> Vec<String> {
    norm(title)
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.chars().count() >= 4 && !COMMON.contains(w))
        .map(str::to_string)
        .collect()
}

fn is_about(m: &MailMessage, e: &Event) -> bool {
    let subject = norm(&m.subject);
    if let Some(org) = e
        .organizer
        .as_deref()
        .map(plain_name)
        .filter(|o| o.chars().count() >= 3)
        && (org == plain_name(&m.sender_name) || org == m.sender_address.to_lowercase())
    {
        return true;
    }
    let title = norm(&e.title);
    if title.chars().count() >= 5 && subject.contains(&title) {
        return true;
    }
    let words = significant(&e.title);
    // One distinctive word is enough only when it is the whole point of the title ("Offsite").
    let hits = words
        .iter()
        .filter(|w| subject.contains(w.as_str()))
        .count();
    hits >= 2 || (title.split_whitespace().count() == 1 && hits == 1)
}

/// Up to three unread messages (newest first) that look to be about `event`.
pub fn related(digest: &MailDigest, event: &Event) -> Vec<MailMessage> {
    let mut found: Vec<&MailMessage> = digest
        .important
        .iter()
        .chain(digest.invitations.iter().map(|i| &i.message))
        .filter(|m| is_about(m, event))
        .collect();
    found.sort_by(|a, b| b.received.cmp(&a.received).then(a.id.cmp(&b.id)));
    found.dedup_by(|a, b| a.id == b.id);
    found.into_iter().take(MAX).cloned().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mail::OpenTarget;
    use crate::model::{EventStatus, Response, When};
    use chrono::{TimeZone, Utc};

    fn event(title: &str, organizer: Option<&str>) -> Event {
        let s = Utc.with_ymd_and_hms(2026, 3, 10, 10, 0, 0).unwrap();
        Event {
            id: "e".into(),
            calendar_id: "c".into(),
            remote_id: "e".into(),
            ical_uid: "e".into(),
            title: title.into(),
            when: When::Timed {
                start: s,
                end: s,
                tzid: None,
            },
            location: None,
            description: None,
            join_url: None,
            status: EventStatus::Confirmed,
            my_response: Response::Accepted,
            organizer: organizer.map(Into::into),
            recurrence_master_id: None,
            updated_at: s,
            also_in: vec![],
            attendees: vec![],
        }
    }

    fn mail(id: &str, from: &str, addr: &str, subject: &str, hour: u32) -> MailMessage {
        MailMessage {
            id: id.into(),
            sender_name: from.into(),
            sender_address: addr.into(),
            subject: subject.into(),
            received: Utc.with_ymd_and_hms(2026, 3, 10, hour, 0, 0).unwrap(),
            snippet: "private words".into(),
            open: OpenTarget::None,
        }
    }

    fn digest(m: Vec<MailMessage>) -> MailDigest {
        MailDigest {
            important: m,
            ..Default::default()
        }
    }

    #[test]
    fn mail_from_the_organiser_or_naming_the_meeting_is_found_newest_first() {
        let d = digest(vec![
            mail("1", "Priya Shah", "priya@x.test", "Slides", 7),
            mail("2", "Dana", "dana@x.test", "Re: Design review agenda", 8),
            mail("3", "Lee", "lee@x.test", "Lunch?", 9),
            mail("4", "PRIYA SHAH", "p2@x.test", "Notes", 9),
        ]);
        let got = related(&d, &event("Design review", Some("Priya Shah (PST)")));
        assert_eq!(
            got.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(),
            ["4", "2", "1"]
        );
    }

    #[test]
    fn generic_words_alone_do_not_make_mail_related() {
        let d = digest(vec![mail(
            "1",
            "A",
            "a@x.test",
            "Weekly sync notes from the team call",
            8,
        )]);
        assert!(related(&d, &event("Weekly team sync", None)).is_empty());
    }

    #[test]
    fn one_distinctive_title_word_is_enough_for_a_one_word_title() {
        let d = digest(vec![mail("1", "A", "a@x.test", "Offsite logistics", 8)]);
        assert_eq!(related(&d, &event("Offsite", None)).len(), 1);
        assert!(related(&d, &event("Dentist", None)).is_empty());
    }

    #[test]
    fn a_single_word_from_a_longer_title_is_not_enough() {
        let d = digest(vec![mail(
            "1",
            "A",
            "a@x.test",
            "Design tokens question",
            8,
        )]);
        assert!(related(&d, &event("Design review", None)).is_empty());
    }

    #[test]
    fn at_most_three_and_a_short_organiser_name_never_matches() {
        let many: Vec<MailMessage> = (0..8)
            .map(|i| mail(&i.to_string(), "Sam Lee", "s@x.test", "x", 1 + i))
            .collect();
        assert_eq!(
            related(&digest(many), &event("Anything at all", Some("Sam Lee"))).len(),
            3
        );
        let d = digest(vec![mail("1", "Al", "al@x.test", "x", 8)]);
        assert!(related(&d, &event("Anything at all", Some("Al"))).is_empty());
    }
}
