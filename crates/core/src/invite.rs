//! Sharing an event with other people.
//!
//! By default the app prepares the invitation and hands it to the user's own mail app (a `mailto:` link) plus
//! an `.ics` file that any calendar understands; the user reads it and presses Send. Only if the user turned on
//! "Send invitations from Firstlight" (off by default) can `mail::smtp` send it from a Yahoo or iCloud mailbox
//! they connected, and then only after showing the exact message and on their press of Send.

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

/// "Friday 20 November 2026, 5:00 PM – 6:00 PM (Europe/London)" or the all-day form.
fn when_text(event: &Event, tz: Tz, h24: bool) -> String {
    let clock = if h24 { "%H:%M" } else { "%-I:%M %p" };
    match &event.when {
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
    }
}

fn cut(mut body: String) -> String {
    if body.chars().count() > MAX_BODY_CHARS {
        body = body.chars().take(MAX_BODY_CHARS).collect::<String>() + "…";
    }
    body
}

/// The subject and plain-text body of an invitation sent by `organizer`. `notes` are included only when the
/// caller says so (the user's own events); a connected calendar's description is never copied into an email.
/// The message accompanies a real calendar invitation (`METHOD:REQUEST`), so it says how to answer it.
pub fn message(
    event: &Event,
    tz: Tz,
    h24: bool,
    include_notes: bool,
    organizer: &str,
) -> (String, String) {
    let mut lines = vec![
        event.title.clone(),
        String::new(),
        format!("When: {}", when_text(event, tz, h24)),
    ];
    if let Some(l) = &event.location {
        lines.push(format!("Where: {l}"));
    }
    if let Some(j) = &event.join_url {
        lines.push(format!("Join: {j}"));
    }
    lines.push(format!("Organiser: {organizer}"));
    if include_notes && let Some(n) = &event.description {
        lines.push(String::new());
        lines.push(n.clone());
    }
    lines.push(String::new());
    lines.push(
        "This is a calendar invitation. Accept or decline it in your calendar app and your answer goes back to the organiser. If you don't see buttons for that, open the attached file."
            .into(),
    );
    (
        format!("Invitation: {}", event.title),
        cut(lines.join("\n")),
    )
}

/// The subject and body telling people an event they were invited to is cancelled.
pub fn cancel_message(event: &Event, tz: Tz, h24: bool, organizer: &str) -> (String, String) {
    let lines = [
        format!("{} has been cancelled.", event.title),
        String::new(),
        format!("Was: {}", when_text(event, tz, h24)),
        format!("Organiser: {organizer}"),
        String::new(),
        "Your calendar app should take it off your calendar. If it doesn't, you can delete it yourself."
            .to_string(),
    ];
    (format!("Cancelled: {}", event.title), cut(lines.join("\n")))
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
    lines.extend(zone_blocks(std::iter::once((event, x))));
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
    lines.extend(zone_blocks(events.iter().map(|(e, x)| (e, x))));
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    /// Invite people, or tell them the event changed (a higher `SEQUENCE`).
    Request,
    /// Tell them it is cancelled.
    Cancel,
}

impl Method {
    pub fn name(self) -> &'static str {
        match self {
            Method::Request => "REQUEST",
            Method::Cancel => "CANCEL",
        }
    }
}

/// Who an invitation is from and for.
pub struct Schedule<'a> {
    pub method: Method,
    /// The sender's own address: replies come back here.
    pub organizer: &'a str,
    pub attendees: &'a [String],
    /// Goes up each time the same event is sent again, so calendar apps update their copy instead of adding a new one.
    pub sequence: u32,
}

/// The sequence number for a send made at `now`: minutes since 1970, which only ever grows.
pub fn sequence_at(now: DateTime<Utc>) -> u32 {
    u32::try_from((now.timestamp() / 60).max(0)).unwrap_or(u32::MAX)
}

/// A real calendar invitation (iTIP): the event with an organiser and attendees, so the people who get it can
/// accept or decline in their own calendar app and the answer comes back to `organizer`. Addresses are checked
/// again here: a value that is not a plain address is refused rather than written into the file.
pub fn ics_itip(event: &Event, x: &IcsExtras, sc: &Schedule) -> Result<String> {
    let organizer =
        parse_address(sc.organizer).ok_or(Error::Invalid("Your address isn't valid."))?;
    let mut attendees = Vec::new();
    for a in sc.attendees {
        attendees
            .push(parse_address(a).ok_or(Error::Invalid("One of the addresses isn't valid."))?);
    }
    if attendees.is_empty() {
        return Err(Error::Invalid("Add at least one email address."));
    }
    let mut lines = vec![
        "BEGIN:VCALENDAR".to_string(),
        "VERSION:2.0".into(),
        "PRODID:-//Firstlight//Calendar//EN".into(),
        "CALSCALE:GREGORIAN".into(),
        format!("METHOD:{}", sc.method.name()),
    ];
    lines.extend(zone_blocks(std::iter::once((event, x))));
    let mut body = vevent(event, x);
    body.pop(); // END:VEVENT, put back after the people
    body.push(format!("SEQUENCE:{}", sc.sequence));
    body.push(
        match sc.method {
            Method::Request => "STATUS:CONFIRMED",
            Method::Cancel => "STATUS:CANCELLED",
        }
        .into(),
    );
    body.push(format!("ORGANIZER;CN={organizer}:mailto:{organizer}"));
    for a in &attendees {
        body.push(match sc.method {
            Method::Request => format!(
                "ATTENDEE;CUTYPE=INDIVIDUAL;ROLE=REQ-PARTICIPANT;PARTSTAT=NEEDS-ACTION;RSVP=TRUE;CN={a}:mailto:{a}"
            ),
            Method::Cancel => format!("ATTENDEE;CUTYPE=INDIVIDUAL;ROLE=REQ-PARTICIPANT;CN={a}:mailto:{a}"),
        });
    }
    body.push("END:VEVENT".into());
    lines.extend(body);
    lines.push("END:VCALENDAR".into());
    Ok(lines
        .iter()
        .map(|x| fold(x))
        .collect::<Vec<_>>()
        .join("\r\n")
        + "\r\n")
}

/// A `VTIMEZONE` for each zone a repeating timed event is written in (`TZID=...`), once each, so any
/// calendar program places the series correctly across daylight saving.
fn zone_blocks<'a>(events: impl Iterator<Item = (&'a Event, &'a IcsExtras<'a>)>) -> Vec<String> {
    let mut seen: Vec<(Tz, i32)> = Vec::new();
    let mut out = Vec::new();
    for (event, x) in events {
        if let (When::Timed { start, .. }, Some(_)) = (&event.when, &x.rrule) {
            let key = (x.tz, chrono::Datelike::year(&start.with_timezone(&x.tz)));
            if !seen.contains(&key) {
                seen.push(key);
                out.extend(firstlight_logic::tz::vtimezone_lines(key.0, key.1));
            }
        }
    }
    out
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
        let (subject, body) = message(&event(), NY, false, true, "me@yahoo.com");
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
        let (subject, body) = message(&event(), NY, false, false, "me@yahoo.com");
        assert_eq!(subject, "Invitation: Lunch, with; \"friends\"");
        assert!(
            body.contains("When: Friday 20 November 2026, 12:00 PM – 1:00 PM (America/New_York)")
        );
        assert!(
            body.contains("Where: Café Nero") && body.contains("Join: https://meet.google.com/")
        );
        assert!(body.contains("Organiser: me@yahoo.com"));
        assert!(body.contains("Accept or decline it in your calendar app"));
        assert!(
            !body.contains("Bring the notes"),
            "notes are not copied from connected calendars"
        );
        assert!(
            message(&event(), NY, true, true, "me@yahoo.com")
                .1
                .contains("12:00 – 13:00")
        );
        assert!(
            message(&event(), NY, true, true, "me@yahoo.com")
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

    // ---- real invitations ---------------------------------------------------------------

    fn extras(now: DateTime<Utc>) -> IcsExtras<'static> {
        IcsExtras {
            tz: NY,
            rrule: None,
            include_notes: true,
            now,
            removed: &[],
            alarm_minutes: None,
        }
    }

    fn sched<'a>(method: Method, attendees: &'a [String], seq: u32) -> Schedule<'a> {
        Schedule {
            method,
            organizer: "Me@Yahoo.com",
            attendees,
            sequence: seq,
        }
    }

    #[test]
    fn a_real_invitation_names_the_organiser_and_asks_each_person_to_answer() {
        let now: DateTime<Utc> = "2026-11-14T12:00:00Z".parse().unwrap();
        let to = vec!["Sam@Example.com".to_string(), "kim@example.org".to_string()];
        let f = ics_itip(&event(), &extras(now), &sched(Method::Request, &to, 7)).unwrap();
        assert!(f.contains("METHOD:REQUEST\r\n"));
        assert!(f.contains("ORGANIZER;CN=me@yahoo.com:mailto:me@yahoo.com\r\n"));
        assert_eq!(f.matches("ATTENDEE;").count(), 2);
        let unfolded = f.replace("\r\n ", "");
        assert!(
            unfolded.contains(
                "PARTSTAT=NEEDS-ACTION;RSVP=TRUE;CN=sam@example.com:mailto:sam@example.com"
            )
        );
        assert!(f.contains("SEQUENCE:7\r\n") && f.contains("STATUS:CONFIRMED\r\n"));
        assert!(
            f.contains("UID:abc@firstlight.local\r\n"),
            "the same event keeps its UID"
        );
        assert!(
            !f.contains("VALARM"),
            "an invitation sets no alarms on other people's calendars"
        );
        assert!(f.lines().all(|l| l.len() <= 75), "lines are folded");
        // It is still a calendar file our own reader understands.
        let window = Window {
            from: "2026-11-01T00:00:00Z".parse().unwrap(),
            to: "2026-12-31T00:00:00Z".parse().unwrap(),
        };
        let evs = events_from_ics(
            &f,
            &ParseOptions {
                calendar_id: "c",
                default_tz: chrono_tz::UTC,
                window,
                my_email: None,
            },
        )
        .unwrap();
        assert_eq!(evs.len(), 1);
    }

    #[test]
    fn a_resend_has_a_higher_sequence_and_a_cancellation_says_so() {
        let t1: DateTime<Utc> = "2026-11-14T12:00:00Z".parse().unwrap();
        let t2: DateTime<Utc> = "2026-11-14T12:05:00Z".parse().unwrap();
        assert!(sequence_at(t2) > sequence_at(t1));
        let to = vec!["sam@example.com".to_string()];
        let c = ics_itip(
            &event(),
            &extras(t1),
            &sched(Method::Cancel, &to, sequence_at(t2)),
        )
        .unwrap();
        assert!(c.contains("METHOD:CANCEL\r\n") && c.contains("STATUS:CANCELLED\r\n"));
        assert!(c.contains("UID:abc@firstlight.local\r\n"));
        assert!(!c.contains("RSVP"), "nothing to answer on a cancellation");
    }

    #[test]
    fn nothing_but_a_plain_address_can_reach_the_file() {
        let now: DateTime<Utc> = "2026-11-14T12:00:00Z".parse().unwrap();
        for bad in [
            "x@y.com\r\nATTENDEE:mailto:evil@x.org",
            "a b@c.de",
            "x@y.com;ROLE=CHAIR",
            "\"q\"@x.org",
            "",
        ] {
            let to = vec![bad.to_string()];
            assert!(
                ics_itip(&event(), &extras(now), &sched(Method::Request, &to, 1)).is_err(),
                "{bad:?}"
            );
        }
        let ok = vec!["a@b.co".to_string()];
        let mut s = sched(Method::Request, &ok, 1);
        s.organizer = "me@x.com\r\nORGANIZER:mailto:evil@x.org";
        assert!(ics_itip(&event(), &extras(now), &s).is_err());
        assert!(
            ics_itip(&event(), &extras(now), &sched(Method::Request, &[], 1)).is_err(),
            "nobody to invite"
        );
    }

    #[test]
    fn the_cancellation_message_names_the_event_and_when_it_was() {
        let (subject, body) = cancel_message(&event(), NY, false, "me@yahoo.com");
        assert_eq!(subject, "Cancelled: Lunch, with; \"friends\"");
        assert!(
            body.contains("has been cancelled") && body.contains("Was: Friday 20 November 2026")
        );
    }
}
