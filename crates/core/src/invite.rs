//! Sharing an event with other people, without Firstlight ever sending anything.
//!
//! The app prepares the invitation and hands it to the user's own mail app (a `mailto:` link) plus an
//! `.ics` file that any calendar understands. The user reads it and presses Send. No mail permission,
//! no server and no password is involved, so "Firstlight never sends mail" stays true.

use crate::model::{Event, When};
use crate::{Error, Result};
use chrono::{DateTime, Utc};
use chrono_tz::Tz;

pub const MAX_RECIPIENTS: usize = 20;
const MAX_BODY_CHARS: usize = 1500; // mailto links over a few kB are cut off by some mail apps

/// One email address, lower-cased, or `None` if it isn't a plain `name@domain.tld`.
/// Deliberately strict: printable ASCII, no quotes, brackets, commas or spaces.
pub fn parse_address(s: &str) -> Option<String> {
    let s = s.trim();
    let (local, domain) = s.split_once('@')?;
    let ok_local = !local.is_empty()
        && local.len() <= 64
        && local
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._%+-'".contains(&b))
        && !local.starts_with('.')
        && !local.ends_with('.')
        && !local.contains("..");
    let ok_domain = domain.len() <= 253
        && domain.contains('.')
        && domain.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-')
        });
    (ok_local && ok_domain && s.len() <= 254).then(|| s.to_ascii_lowercase())
}

/// Addresses separated by commas, semicolons, spaces or new lines. Duplicates are dropped.
pub fn parse_list(text: &str) -> Result<Vec<String>> {
    let mut out: Vec<String> = Vec::new();
    for part in text.split(|c: char| c == ',' || c == ';' || c.is_whitespace()) {
        if part.is_empty() {
            continue;
        }
        let a = parse_address(part).ok_or(Error::Invalid(
            "One of those email addresses doesn't look right.",
        ))?;
        if !out.contains(&a) {
            out.push(a);
        }
    }
    if out.len() > MAX_RECIPIENTS {
        return Err(Error::Invalid("That's too many people for one invitation."));
    }
    Ok(out)
}

/// The subject and plain-text body of the invitation. `notes` are included only when the caller says so
/// (the user's own events); a connected calendar's description is never copied into an email.
pub fn message(event: &Event, tz: Tz, h24: bool, include_notes: bool) -> (String, String) {
    let clock = if h24 { "%H:%M" } else { "%-I:%M %p" };
    let when = match &event.when {
        When::Timed { start, end, .. } => {
            let (s, e) = (start.with_timezone(&tz), end.with_timezone(&tz));
            let day = s.format("%A %-d %B %Y");
            if s.date_naive() == e.date_naive() {
                format!(
                    "{day}, {} – {} ({})",
                    s.format(clock),
                    e.format(clock),
                    tz.name()
                )
            } else {
                format!(
                    "{day} {} – {} {} ({})",
                    s.format(clock),
                    e.format("%A %-d %B"),
                    e.format(clock),
                    tz.name()
                )
            }
        }
        When::AllDay { start, end } => {
            let last = *end - chrono::Duration::days(1);
            if last <= *start {
                format!("{}, all day", start.format("%A %-d %B %Y"))
            } else {
                format!(
                    "{} – {}, all day",
                    start.format("%A %-d %B"),
                    last.format("%A %-d %B %Y")
                )
            }
        }
    };
    let mut lines = vec![event.title.clone(), String::new(), format!("When: {when}")];
    if let Some(l) = &event.location {
        lines.push(format!("Where: {l}"));
    }
    if let Some(j) = &event.join_url {
        lines.push(format!("Join: {j}"));
    }
    if include_notes && let Some(n) = &event.description {
        lines.push(String::new());
        lines.push(n.clone());
    }
    lines.push(String::new());
    lines.push("The calendar file is attached, so you can add it to your calendar.".into());
    let mut body = lines.join("\n");
    if body.chars().count() > MAX_BODY_CHARS {
        body = body.chars().take(MAX_BODY_CHARS).collect::<String>() + "…";
    }
    (format!("Invitation: {}", event.title), body)
}

fn percent(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3);
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// A `mailto:` link. Recipients must already have passed [`parse_list`]; subject and body are encoded.
pub fn mailto(recipients: &[String], subject: &str, body: &str) -> String {
    format!(
        "mailto:{}?subject={}&body={}",
        recipients
            .iter()
            .map(|r| percent(r).replace("%40", "@"))
            .collect::<Vec<_>>()
            .join(","),
        percent(subject),
        percent(body)
    )
}

// ---- .ics ------------------------------------------------------------------------------

pub(crate) fn escape_text(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            ';' => out.push_str("\\;"),
            ',' => out.push_str("\\,"),
            '\n' => out.push_str("\\n"),
            '\r' => {}
            c if c.is_control() => out.push(' '),
            c => out.push(c),
        }
    }
    out
}

/// Folds a line at 75 bytes, on character boundaries, as RFC 5545 requires.
pub(crate) fn fold(line: &str) -> String {
    let mut out = String::new();
    let mut len = 0;
    for c in line.chars() {
        let n = c.len_utf8();
        if len + n > 74 {
            out.push_str("\r\n ");
            len = 1;
        }
        out.push(c);
        len += n;
    }
    out
}

/// What the `.ics` needs besides the event itself.
pub struct IcsExtras<'a> {
    pub tz: Tz,
    /// A recurrence rule built by us from an enum, never taken from the page: `FREQ=WEEKLY;INTERVAL=2`.
    pub rrule: Option<&'a str>,
    pub include_notes: bool,
    pub now: DateTime<Utc>,
    /// Days removed from a repeating event, as the occurrence keys `local` uses (a start timestamp for a
    /// timed event, `YYYYMMDD` for an all-day one). Written as `EXDATE`. Empty when sharing.
    pub removed: &'a [String],
    /// A reminder before the start, written as an alarm. `None` when sharing: an invitation must not set
    /// alarms on other people's calendars.
    pub alarm_minutes: Option<u32>,
}

/// A one-event calendar file. Timed events are written in the event's own zone when they repeat (so a
/// 9:00 meeting stays at 9:00 across daylight saving) and in UTC otherwise (which every app reads).
pub fn ics(event: &Event, x: &IcsExtras) -> String {
    let mut lines = vec![
        "BEGIN:VCALENDAR".to_string(),
        "VERSION:2.0".into(),
        "PRODID:-//Firstlight//Calendar//EN".into(),
        "CALSCALE:GREGORIAN".into(),
        "METHOD:PUBLISH".into(),
    ];
    lines.extend(vevent(event, x));
    lines.push("END:VCALENDAR".into());
    lines
        .iter()
        .map(|x| fold(x))
        .collect::<Vec<_>>()
        .join("\r\n")
        + "\r\n"
}

/// Several events in one calendar file (the export of everything the user added).
pub fn ics_many(events: &[(Event, IcsExtras)]) -> String {
    let mut lines = vec![
        "BEGIN:VCALENDAR".to_string(),
        "VERSION:2.0".into(),
        "PRODID:-//Firstlight//Calendar//EN".into(),
        "CALSCALE:GREGORIAN".into(),
        "METHOD:PUBLISH".into(),
    ];
    for (event, x) in events {
        lines.extend(vevent(event, x));
    }
    lines.push("END:VCALENDAR".into());
    lines
        .iter()
        .map(|x| fold(x))
        .collect::<Vec<_>>()
        .join("\r\n")
        + "\r\n"
}

/// The lines of one `VEVENT`, unfolded.
fn vevent(event: &Event, x: &IcsExtras) -> Vec<String> {
    let stamp = |d: DateTime<Utc>| d.format("%Y%m%dT%H%M%SZ").to_string();
    let local = |d: DateTime<Utc>| d.with_timezone(&x.tz).format("%Y%m%dT%H%M%S").to_string();
    let mut l: Vec<String> = vec![
        "BEGIN:VEVENT".into(),
        format!("UID:{}", escape_text(&event.ical_uid)),
        format!("DTSTAMP:{}", stamp(x.now)),
    ];
    match &event.when {
        When::Timed { start, end, .. } if x.rrule.is_some() => {
            l.push(format!("DTSTART;TZID={}:{}", x.tz.name(), local(*start)));
            l.push(format!("DTEND;TZID={}:{}", x.tz.name(), local(*end)));
        }
        When::Timed { start, end, .. } => {
            l.push(format!("DTSTART:{}", stamp(*start)));
            l.push(format!("DTEND:{}", stamp(*end)));
        }
        When::AllDay { start, end } => {
            l.push(format!("DTSTART;VALUE=DATE:{}", start.format("%Y%m%d")));
            l.push(format!("DTEND;VALUE=DATE:{}", end.format("%Y%m%d")));
        }
    }
    if let Some(r) = x.rrule {
        l.push(format!("RRULE:{r}"));
        for key in x.removed {
            match &event.when {
                When::Timed { .. } => {
                    if let Some(at) = key
                        .parse::<i64>()
                        .ok()
                        .and_then(|s| DateTime::from_timestamp(s, 0))
                    {
                        l.push(format!("EXDATE;TZID={}:{}", x.tz.name(), local(at)));
                    }
                }
                When::AllDay { .. } => {
                    if key.len() == 8 && key.bytes().all(|b| b.is_ascii_digit()) {
                        l.push(format!("EXDATE;VALUE=DATE:{key}"));
                    }
                }
            }
        }
    }
    l.push(format!("SUMMARY:{}", escape_text(&event.title)));
    if let Some(v) = &event.location {
        l.push(format!("LOCATION:{}", escape_text(v)));
    }
    let mut desc = Vec::new();
    if x.include_notes
        && let Some(n) = &event.description
    {
        desc.push(n.clone());
    }
    if let Some(j) = &event.join_url {
        desc.push(format!("Join: {j}"));
        l.push(format!("URL:{}", escape_text(j)));
    }
    if !desc.is_empty() {
        l.push(format!("DESCRIPTION:{}", escape_text(&desc.join("\n\n"))));
    }
    if let (Some(m), When::Timed { .. }) = (x.alarm_minutes, &event.when) {
        l.push("BEGIN:VALARM".into());
        l.push("ACTION:DISPLAY".into());
        l.push(format!("DESCRIPTION:{}", escape_text(&event.title)));
        l.push(format!("TRIGGER:-PT{m}M"));
        l.push("END:VALARM".into());
    }
    l.push("END:VEVENT".into());
    l
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ics::{ParseOptions, events_from_ics};
    use crate::model::*;
    use crate::providers::Window;

    fn event() -> Event {
        Event {
            id: "local-events:abc".into(),
            calendar_id: "local-events".into(),
            remote_id: "abc".into(),
            ical_uid: "abc@firstlight.local".into(),
            title: "Lunch, with; \"friends\"".into(),
            when: When::Timed {
                start: "2026-11-20T17:00:00Z".parse().unwrap(),
                end: "2026-11-20T18:00:00Z".parse().unwrap(),
                tzid: Some("America/New_York".into()),
            },
            location: Some("Café Nero".into()),
            description: Some("Bring the notes\nsecond line".into()),
            join_url: Some("https://meet.google.com/abc-defg-hij".into()),
            status: EventStatus::Confirmed,
            my_response: Response::None,
            organizer: None,
            recurrence_master_id: None,
            updated_at: Utc::now(),
            also_in: vec![],
        }
    }
    const NY: Tz = chrono_tz::America::New_York;

    #[test]
    fn addresses_are_strict() {
        for ok in ["a@b.co", "Sam.Lee+x@Example.COM", "o'brien@x.org"] {
            assert!(parse_address(ok).is_some(), "{ok}");
        }
        for bad in [
            "",
            "a",
            "a@b",
            "a@@b.co",
            "a b@c.co",
            "a@b..co",
            "<a@b.co>",
            "a@b.co>x",
            "\"a\"@b.co",
            "a@-b.co",
            "a\n@b.co",
            ".a@b.co",
            "é@b.co",
            "a@b.c o",
        ] {
            assert!(parse_address(bad).is_none(), "{bad:?}");
        }
        assert_eq!(
            parse_list("A@b.co, c@d.org;a@B.co\nz@y.io").unwrap(),
            ["a@b.co", "c@d.org", "z@y.io"]
        );
        assert!(parse_list("a@b.co, nonsense").is_err());
        assert!(
            parse_list(
                &(0..21)
                    .map(|i| format!("u{i}@x.org"))
                    .collect::<Vec<_>>()
                    .join(",")
            )
            .is_err()
        );
        assert!(parse_list("  ").unwrap().is_empty());
    }

    #[test]
    fn mailto_encodes_everything_that_could_break_the_link() {
        let (subject, body) = message(&event(), NY, false, true);
        let link = mailto(&["a@b.co".into(), "c@d.org".into()], &subject, &body);
        assert!(link.starts_with("mailto:a@b.co,c@d.org?subject="));
        let query = &link["mailto:a@b.co,c@d.org".len()..];
        assert!(!query.contains(' ') && !query.contains('\n') && !query.contains('"'));
        assert_eq!(
            query.matches('&').count(),
            1,
            "only the separator between subject and body"
        );
        // Hostile text cannot add a header or a recipient.
        let evil = mailto(
            &["a@b.co".into()],
            "x&cc=evil@x.org",
            "y\r\nBcc: evil@x.org",
        );
        assert_eq!(evil.matches("cc=").count(), 0, "{evil}");
        assert!(!evil.contains("\r") && !evil.contains("Bcc: "));
    }

    #[test]
    fn the_message_reads_well_and_keeps_private_text_out() {
        let (subject, body) = message(&event(), NY, false, false);
        assert_eq!(subject, "Invitation: Lunch, with; \"friends\"");
        assert!(
            body.contains("When: Friday 20 November 2026, 12:00 PM – 1:00 PM (America/New_York)")
        );
        assert!(
            body.contains("Where: Café Nero") && body.contains("Join: https://meet.google.com/")
        );
        assert!(
            !body.contains("Bring the notes"),
            "notes are not copied from connected calendars"
        );
        assert!(
            message(&event(), NY, true, true)
                .1
                .contains("12:00 – 13:00")
        );
        assert!(
            message(&event(), NY, true, true)
                .1
                .contains("Bring the notes")
        );
    }

    #[test]
    fn the_calendar_file_reads_back_as_the_same_event() {
        let x = IcsExtras {
            tz: NY,
            rrule: None,
            include_notes: true,
            now: Utc::now(),
            removed: &[],
            alarm_minutes: None,
        };
        let text = ics(&event(), &x);
        assert!(text.contains("\r\n") && !text.contains("\n\n"));
        assert!(text.lines().all(|l| l.len() <= 75), "folded");
        let w = Window {
            from: "2026-01-01T00:00:00Z".parse().unwrap(),
            to: "2027-01-01T00:00:00Z".parse().unwrap(),
        };
        let back = events_from_ics(
            &text,
            &ParseOptions {
                calendar_id: "t",
                default_tz: NY,
                window: w,
                my_email: None,
            },
        )
        .unwrap();
        assert_eq!(back.len(), 1);
        assert_eq!(back[0].title, "Lunch, with; \"friends\"");
        // Same instants (a UTC file carries no zone name).
        let instants = |w: &When| match w {
            When::Timed { start, end, .. } => (*start, *end),
            When::AllDay { .. } => unreachable!(),
        };
        assert_eq!(instants(&back[0].when), instants(&event().when));
        assert_eq!(back[0].location.as_deref(), Some("Café Nero"));
        assert_eq!(
            back[0].join_url.as_deref(),
            Some("https://meet.google.com/abc-defg-hij")
        );

        // All-day, and a repeating one (the rule is ours).
        let mut allday = event();
        allday.when = When::AllDay {
            start: "2026-12-24".parse().unwrap(),
            end: "2026-12-27".parse().unwrap(),
        };
        let t2 = ics(&allday, &x);
        let b2 = events_from_ics(
            &t2,
            &ParseOptions {
                calendar_id: "t",
                default_tz: NY,
                window: w,
                my_email: None,
            },
        )
        .unwrap();
        assert_eq!(b2[0].when, allday.when);
        let rep = IcsExtras {
            rrule: Some("FREQ=WEEKLY;INTERVAL=1;COUNT=3"),
            ..x
        };
        let t3 = ics(&event(), &rep);
        assert_eq!(
            events_from_ics(
                &t3,
                &ParseOptions {
                    calendar_id: "t",
                    default_tz: NY,
                    window: w,
                    my_email: None
                }
            )
            .unwrap()
            .len(),
            3
        );
    }

    #[test]
    fn hostile_text_cannot_inject_calendar_lines() {
        let mut e = event();
        e.title = "x\r\nEND:VEVENT\r\nBEGIN:VEVENT\r\nSUMMARY:evil".into();
        e.location = Some("a\nATTENDEE:mailto:evil@x.org".into());
        let text = ics(
            &e,
            &IcsExtras {
                tz: NY,
                rrule: None,
                include_notes: false,
                now: Utc::now(),
                removed: &[],
                alarm_minutes: None,
            },
        );
        // Real lines only: the hostile text must have become plain text inside SUMMARY and LOCATION.
        let real = |name: &str| text.split("\r\n").filter(|l| *l == name).count();
        assert_eq!(real("BEGIN:VEVENT"), 1);
        assert_eq!(real("END:VEVENT"), 1);
        assert!(!text.split("\r\n").any(|l| l.starts_with("ATTENDEE")));
    }
}
