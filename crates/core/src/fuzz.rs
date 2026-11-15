//! Garbage-in tests for every parser that takes data from outside the app. Nothing here may panic: in a real
//! feed or server response a panic would be a way to crash the app (see `contain`). Seeded, so failures reproduce.

use crate::{ai, http, ics, join, mail, settings, xml};
use std::panic::{AssertUnwindSafe, catch_unwind};

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

/// Random text built from fragments that tend to break parsers.
fn garbage(rng: &mut Rng) -> String {
    const PIECES: &[&str] = &[
        "<",
        ">",
        "</",
        "<![CDATA[",
        "]]>",
        "<!DOCTYPE x [<!ENTITY a \"b\">]>",
        "&amp;",
        "&#x110000;",
        "&#99999999999;",
        "&",
        ";",
        "\"",
        "'",
        "\\",
        "\n",
        "\r\n",
        "\t",
        "\u{0}",
        "\u{7}",
        "\u{200b}",
        "\u{feff}",
        "é",
        "😀",
        "日本語",
        "BEGIN:VCALENDAR",
        "BEGIN:VEVENT",
        "END:VEVENT",
        "END:VCALENDAR",
        "DTSTART:",
        "DTEND:",
        "RRULE:FREQ=",
        "UID:",
        ";TZID=",
        ":",
        "=",
        ",",
        "mailto:",
        "<a@b.c>",
        "http://",
        "https://",
        "javascript:",
        "message://",
        "%",
        "%3C",
        "../",
        "{",
        "}",
        "[",
        "]",
        "null",
        "true",
        "9999999999999999999",
        "-1",
        "0",
        "18446744073709551616",
        "\u{1e}",
        "\u{1f}",
        "TOTAL",
        "NOTRUNNING",
        "{\"format\":\"firstlight-settings\",\"settings\":",
        "{\"general\":",
        "\"sync_interval_minutes\":",
        "ICS",
        " ",
    ];
    (0..rng.below(24))
        .map(|_| PIECES[rng.below(PIECES.len())])
        .collect()
}

#[test]
fn no_parser_panics_on_garbage() {
    let mut rng = Rng(0xDEAD_BEEF_CAFE_F00D);
    let window = crate::providers::Window {
        from: chrono::Utc::now(),
        to: chrono::Utc::now() + chrono::Duration::days(30),
    };
    let opts = ics::ParseOptions {
        calendar_id: "c",
        default_tz: chrono_tz::UTC,
        window,
        my_email: Some("me@example.com"),
    };
    let started = std::time::Instant::now();

    for i in 0..30_000 {
        let s = garbage(&mut rng);
        let ics_doc = format!("BEGIN:VCALENDAR\r\n{s}\r\nEND:VCALENDAR");
        let result = catch_unwind(AssertUnwindSafe(|| {
            let _ = xml::parse(&s);
            let _ = ics::events_from_ics(&s, &opts);
            let _ = ics::events_from_ics(&ics_doc, &opts);
            let _ = mail::applemail::parse_output(&s);
            let _ = mail::applemail::message_url(&s);
            let _ = mail::parse_sender(&s);
            let _ = mail::clean_snippet(&s);
            let _ = mail::decode_entities(&s);
            let _ = mail::sender_matches(&s, &s, std::slice::from_ref(&s));
            let _ = mail::subject_looks_like_invitation(&s);
            let _ = join::extract(Some(&s), Some(&s), Some(&s));
            let _ = http::normalise_calendar_url(&s);
            let _ = settings::AppSettings::import(&s);
            let _ = settings::valid_shortcut(&s);
            let _ = settings::valid_local_endpoint(&s);
            let _ = ai::clean_output(&s);
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&s) {
                let _ = settings::AppSettings::from_value(&v);
            }
            let _ = settings::AppSettings::from_value(
                &serde_json::json!({ "general": s.clone(), "email": { "important_senders": [s.clone()] }, "ai": { "model": s.clone(), "endpoint": s.clone() } }),
            );
        }));
        assert!(result.is_ok(), "PANIC at iteration {i} on input: {s:?}");
    }
    assert!(
        started.elapsed() < std::time::Duration::from_secs(120),
        "fuzzing took {:?}",
        started.elapsed()
    );
}
