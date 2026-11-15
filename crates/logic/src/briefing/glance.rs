//! Two small answers drawn from the briefing: the text for the menu bar ("Design review · in 12 min") and
//! which meeting a "join next meeting" shortcut should open.

use super::{Briefing, Item, NextUp};
use chrono::{DateTime, Duration, Utc};
use chrono_tz::Tz;

const MAX_TITLE_CHARS: usize = 22;

/// The menu-bar text for the next event, or `None` when there is nothing to show. With `hide_titles` the
/// name is replaced by "Meeting", the same privacy switch the notifications use, because a menu bar is
/// visible to anyone looking at the screen (and in a screen share).
pub fn tray_label(
    next: Option<&NextUp>,
    now: DateTime<Utc>,
    hide_titles: bool,
    h24: bool,
    tz: Tz,
) -> Option<String> {
    let next = next?;
    let title = if hide_titles {
        "Meeting".to_string()
    } else {
        short(&next.event.title)
    };
    let when = if next.in_progress || next.start <= now {
        "now".to_string()
    } else {
        let mins = (next.start - now).num_minutes();
        if mins < 1 {
            "now".to_string()
        } else if mins < 60 {
            format!("in {mins} min")
        } else {
            let local = next.start.with_timezone(&tz);
            local
                .format(if h24 { "%H:%M" } else { "%-I:%M %p" })
                .to_string()
        }
    };
    Some(format!("{title} · {when}"))
}

/// One line, no control characters, at most [`MAX_TITLE_CHARS`] characters.
fn short(title: &str) -> String {
    let clean: String = title
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if clean.chars().count() <= MAX_TITLE_CHARS {
        clean
    } else {
        clean.chars().take(MAX_TITLE_CHARS - 1).collect::<String>() + "…"
    }
}

/// The meeting to join: among today's remaining events that have a video link, one that starts within
/// `lead_minutes` (the soonest), otherwise the one already running (the latest to have started).
/// A running meeting is only preferred when nothing is about to start, because someone who is in one
/// meeting and presses the shortcut five minutes before the next almost always means the next.
pub fn join_target(b: &Briefing, now: DateTime<Utc>, lead_minutes: i64) -> Option<&Item> {
    let with_link = || b.remaining.iter().filter(|i| i.event.join_url.is_some());
    with_link()
        .filter(|i| i.start > now && i.start - now <= Duration::minutes(lead_minutes))
        .min_by_key(|i| i.start)
        .or_else(|| {
            with_link()
                .filter(|i| i.start <= now && i.end > now)
                .max_by_key(|i| i.start)
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::briefing::{BriefingSettings, build};
    use crate::model::*;

    const LONDON: Tz = chrono_tz::Europe::London;

    fn t(s: &str) -> DateTime<Utc> {
        s.parse().unwrap()
    }

    fn event(id: &str, title: &str, start: &str, end: &str, link: Option<&str>) -> Event {
        Event {
            id: id.into(),
            calendar_id: "c".into(),
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
            join_url: link.map(String::from),
            status: EventStatus::Confirmed,
            my_response: Response::Accepted,
            organizer: None,
            recurrence_master_id: None,
            updated_at: t("2026-11-01T00:00:00Z"),
            also_in: vec![],
            attendees: vec![],
        }
    }

    fn day(events: &[Event], now: &str) -> Briefing {
        build(events, t(now), LONDON, &BriefingSettings::default())
    }

    const ZOOM: &str = "https://zoom.us/j/1";
    const MEET: &str = "https://meet.google.com/abc-defg-hij";

    #[test]
    fn the_menu_bar_text_counts_down_then_shows_the_time_then_says_now() {
        let e = [event(
            "a",
            "Design review",
            "2026-11-20T10:30:00Z",
            "2026-11-20T11:30:00Z",
            None,
        )];
        let label = |now: &str, hide: bool, h24: bool| {
            let b = day(&e, now);
            tray_label(b.next.as_ref(), t(now), hide, h24, LONDON)
        };
        assert_eq!(
            label("2026-11-20T10:18:00Z", false, false).unwrap(),
            "Design review · in 12 min"
        );
        assert_eq!(
            label("2026-11-20T09:00:00Z", false, false).unwrap(),
            "Design review · 10:30 AM"
        );
        assert_eq!(
            label("2026-11-20T09:00:00Z", false, true).unwrap(),
            "Design review · 10:30"
        );
        assert_eq!(
            label("2026-11-20T10:30:00Z", false, false).unwrap(),
            "Design review · now"
        );
        assert_eq!(
            label("2026-11-20T10:45:00Z", false, false).unwrap(),
            "Design review · now"
        );
        assert_eq!(
            label("2026-11-20T10:18:00Z", true, false).unwrap(),
            "Meeting · in 12 min"
        );
        assert_eq!(
            label("2026-11-20T12:00:00Z", false, false),
            None,
            "nothing left today"
        );
        assert_eq!(
            tray_label(None, t("2026-11-20T09:00:00Z"), false, false, LONDON),
            None
        );
    }

    #[test]
    fn long_and_hostile_titles_are_shortened_to_one_line() {
        let e = [event(
            "a",
            "A very long meeting title that would swallow the menu bar",
            "2026-11-20T10:30:00Z",
            "2026-11-20T11:30:00Z",
            None,
        )];
        let b = day(&e, "2026-11-20T10:00:00Z");
        let s = tray_label(
            b.next.as_ref(),
            t("2026-11-20T10:00:00Z"),
            false,
            false,
            LONDON,
        )
        .unwrap();
        assert!(s.starts_with("A very long meeting t…"), "{s}");
        assert_eq!(s.chars().filter(|c| *c == '…').count(), 1);
        let e = [event(
            "a",
            "line\none\u{7}two\t  three",
            "2026-11-20T10:30:00Z",
            "2026-11-20T11:30:00Z",
            None,
        )];
        let b = day(&e, "2026-11-20T10:00:00Z");
        let s = tray_label(
            b.next.as_ref(),
            t("2026-11-20T10:00:00Z"),
            false,
            false,
            LONDON,
        )
        .unwrap();
        assert!(
            !s.chars().any(|c| c.is_control()) && s.starts_with("line one two three"),
            "{s:?}"
        );
    }

    #[test]
    fn join_picks_the_meeting_about_to_start_over_the_one_running() {
        let e = [
            event(
                "run",
                "Running",
                "2026-11-20T10:00:00Z",
                "2026-11-20T11:00:00Z",
                Some(ZOOM),
            ),
            event(
                "soon",
                "Starting soon",
                "2026-11-20T10:35:00Z",
                "2026-11-20T11:00:00Z",
                Some(MEET),
            ),
        ];
        let b = day(&e, "2026-11-20T10:30:00Z");
        assert_eq!(
            join_target(&b, t("2026-11-20T10:30:00Z"), 15)
                .unwrap()
                .event
                .id,
            "soon"
        );
        // Out of the lead window, the running one is chosen instead.
        assert_eq!(
            join_target(&b, t("2026-11-20T10:10:00Z"), 15)
                .unwrap()
                .event
                .id,
            "run"
        );
    }

    #[test]
    fn join_needs_a_link_and_a_meeting_that_is_near() {
        let e = [
            event(
                "nolink",
                "No link",
                "2026-11-20T10:05:00Z",
                "2026-11-20T11:00:00Z",
                None,
            ),
            event(
                "later",
                "Later",
                "2026-11-20T14:00:00Z",
                "2026-11-20T15:00:00Z",
                Some(ZOOM),
            ),
        ];
        let b = day(&e, "2026-11-20T10:00:00Z");
        assert!(join_target(&b, t("2026-11-20T10:00:00Z"), 15).is_none());
        // Starting in exactly the lead time counts; one minute more does not.
        let b2 = day(&e, "2026-11-20T13:45:00Z");
        assert_eq!(
            join_target(&b2, t("2026-11-20T13:45:00Z"), 15)
                .unwrap()
                .event
                .id,
            "later"
        );
        let b3 = day(&e, "2026-11-20T13:44:00Z");
        assert!(join_target(&b3, t("2026-11-20T13:44:00Z"), 15).is_none());
    }

    #[test]
    fn a_meeting_that_has_ended_or_was_declined_is_never_joined() {
        let mut declined = event(
            "no",
            "Declined",
            "2026-11-20T10:00:00Z",
            "2026-11-20T11:00:00Z",
            Some(ZOOM),
        );
        declined.my_response = Response::Declined;
        let mut cancelled = event(
            "x",
            "Cancelled",
            "2026-11-20T10:00:00Z",
            "2026-11-20T11:00:00Z",
            Some(ZOOM),
        );
        cancelled.status = EventStatus::Cancelled;
        let over = event(
            "over",
            "Over",
            "2026-11-20T08:00:00Z",
            "2026-11-20T09:00:00Z",
            Some(ZOOM),
        );
        let b = day(&[declined, cancelled, over], "2026-11-20T10:30:00Z");
        assert!(join_target(&b, t("2026-11-20T10:30:00Z"), 15).is_none());
    }

    #[test]
    fn the_latest_started_running_meeting_wins_when_none_is_near() {
        let e = [
            event(
                "early",
                "Early",
                "2026-11-20T09:00:00Z",
                "2026-11-20T12:00:00Z",
                Some(ZOOM),
            ),
            event(
                "late",
                "Later start",
                "2026-11-20T10:00:00Z",
                "2026-11-20T11:30:00Z",
                Some(MEET),
            ),
        ];
        let b = day(&e, "2026-11-20T10:30:00Z");
        assert_eq!(
            join_target(&b, t("2026-11-20T10:30:00Z"), 15)
                .unwrap()
                .event
                .id,
            "late"
        );
    }
}
