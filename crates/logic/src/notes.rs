//! Meeting notes: one private note per meeting that comes back every time the meeting does.
//!
//! The note belongs to the *meeting*, not to one day of it: every occurrence of a repeating meeting shares it,
//! and so do meetings made one by one under the same title (the same rule meeting prep uses to find "last
//! time"). Notes are kept only on this computer, in the encrypted database, and are never part of an event,
//! so they are never sent to an AI service or a calendar provider.

use crate::model::Event;
use crate::{Error, Result};

/// Longest note kept, in characters.
pub const MAX_NOTE_CHARS: usize = 10_000;
/// Titles shorter than this ("1:1", "Call") are too common to tie a note to; those notes stay with that event.
const MIN_TITLE_CHARS: usize = 4;

fn norm(t: &str) -> String {
    t.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// Which note an event shares. A repeating meeting uses its series (the same for every occurrence, in every
/// provider); a single meeting uses its title, so the next one with that name finds the note; a single event
/// with a very short title keeps a note of its own.
pub fn meeting_key(e: &Event) -> String {
    if let Some(master) = e.recurrence_master_id.as_deref().filter(|m| !m.is_empty()) {
        return format!("series:{}:{master}", e.calendar_id);
    }
    let title = norm(&e.title);
    if title.chars().count() >= MIN_TITLE_CHARS {
        return format!("title:{title}");
    }
    let own = if e.ical_uid.is_empty() {
        &e.id
    } else {
        &e.ical_uid
    };
    format!("event:{own}")
}

/// Tidies what was typed: line breaks and tabs stay, other control characters go, Windows line ends become
/// plain ones, and surrounding blank space is trimmed. `None` means the note is empty (and is removed).
pub fn clean_note(text: &str) -> Result<Option<String>> {
    let tidy: String = text
        .replace("\r\n", "\n")
        .chars()
        .filter(|c| !c.is_control() || *c == '\n' || *c == '\t')
        .collect();
    let tidy = tidy.trim();
    if tidy.chars().count() > MAX_NOTE_CHARS {
        return Err(Error::Invalid("That note is too long."));
    }
    Ok((!tidy.is_empty()).then(|| tidy.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::*;
    use chrono::Utc;

    fn ev(id: &str, title: &str) -> Event {
        let now = Utc::now();
        Event {
            id: id.into(),
            calendar_id: "cal".into(),
            remote_id: id.into(),
            ical_uid: format!("{id}@x"),
            title: title.into(),
            when: When::Timed {
                start: now,
                end: now,
                tzid: None,
            },
            location: None,
            description: None,
            join_url: None,
            status: EventStatus::Confirmed,
            my_response: Response::None,
            organizer: None,
            recurrence_master_id: None,
            updated_at: now,
            also_in: vec![],
            attendees: vec![],
        }
    }

    #[test]
    fn every_occurrence_of_a_series_shares_one_note() {
        let mut a = ev("a", "Standup");
        let mut b = ev("b", "Standup (moved)");
        a.recurrence_master_id = Some("series1".into());
        b.recurrence_master_id = Some("series1".into());
        assert_eq!(meeting_key(&a), meeting_key(&b));
        // Another calendar's series with the same id is a different meeting.
        let mut c = ev("c", "Standup");
        c.calendar_id = "other".into();
        c.recurrence_master_id = Some("series1".into());
        assert_ne!(meeting_key(&a), meeting_key(&c));
    }

    #[test]
    fn single_meetings_with_the_same_title_share_one_note() {
        let a = ev("a", "Design  Review");
        let b = ev("b", "design review");
        assert_eq!(meeting_key(&a), meeting_key(&b));
        assert_ne!(meeting_key(&a), meeting_key(&ev("c", "Budget review")));
    }

    #[test]
    fn a_very_short_title_keeps_its_note_to_itself() {
        let a = ev("a", "1:1");
        let b = ev("b", "1:1");
        assert_ne!(meeting_key(&a), meeting_key(&b));
        assert_eq!(meeting_key(&a), "event:a@x");
    }

    #[test]
    fn notes_are_tidied_and_limited() {
        assert_eq!(clean_note("  \n ").unwrap(), None);
        assert_eq!(
            clean_note("Ask about\r\nthe budget\u{7}\tnow ").unwrap(),
            Some("Ask about\nthe budget\tnow".into())
        );
        assert!(clean_note(&"x".repeat(MAX_NOTE_CHARS)).is_ok());
        assert!(clean_note(&"x".repeat(MAX_NOTE_CHARS + 1)).is_err());
    }
}
