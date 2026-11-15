use super::*;
use chrono::TimeZone;

fn window(from: (i32, u32, u32), to: (i32, u32, u32)) -> Window {
    Window {
        from: Utc
            .with_ymd_and_hms(from.0, from.1, from.2, 0, 0, 0)
            .unwrap(),
        to: Utc.with_ymd_and_hms(to.0, to.1, to.2, 0, 0, 0).unwrap(),
    }
}

fn parse(body: &str, w: Window) -> Vec<Event> {
    parse_in(body, w, chrono_tz::Europe::London, None)
}

fn parse_in(body: &str, w: Window, tz: Tz, me: Option<&str>) -> Vec<Event> {
    let ics = format!("BEGIN:VCALENDAR\r\nVERSION:2.0\r\n{body}\r\nEND:VCALENDAR\r\n");
    events_from_ics(
        &ics,
        &ParseOptions {
            calendar_id: "cal",
            default_tz: tz,
            window: w,
            my_email: me,
        },
    )
    .unwrap()
}

fn start_utc(e: &Event) -> String {
    match &e.when {
        When::Timed { start, .. } => start.format("%Y-%m-%d %H:%M").to_string(),
        When::AllDay { start, .. } => format!("{start} (all-day)"),
    }
}

const MARCH: ((i32, u32, u32), (i32, u32, u32)) = ((2026, 3, 1), (2026, 4, 30));

#[test]
fn simple_utc_event_with_escaped_text_and_folding() {
    let evs = parse(
        "BEGIN:VEVENT\r\nUID:a1\r\nDTSTART:20260310T090000Z\r\nDTEND:20260310T100000Z\r\n\
         SUMMARY:Budget\\, Q2\\; review\r\nDESCRIPTION:Line one\\nLine two with a very long te\r\n xt that was folded\r\n\
         LOCATION:Room 4\r\nEND:VEVENT",
        window(MARCH.0, MARCH.1),
    );
    assert_eq!(evs.len(), 1);
    let e = &evs[0];
    assert_eq!(e.title, "Budget, Q2; review");
    assert_eq!(
        e.description.as_deref(),
        Some("Line one\nLine two with a very long text that was folded")
    );
    assert_eq!(start_utc(e), "2026-03-10 09:00");
    assert!(e.recurrence_master_id.is_none());
}

#[test]
fn tzid_windows_names_and_floating_times() {
    let evs = parse(
        "BEGIN:VEVENT\r\nUID:ny\r\nDTSTART;TZID=America/New_York:20260310T090000\r\nDTEND;TZID=America/New_York:20260310T100000\r\nSUMMARY:NY\r\nEND:VEVENT\r\n\
         BEGIN:VEVENT\r\nUID:win\r\nDTSTART;TZID=Pacific Standard Time:20260310T090000\r\nDTEND;TZID=Pacific Standard Time:20260310T100000\r\nSUMMARY:LA\r\nEND:VEVENT\r\n\
         BEGIN:VEVENT\r\nUID:float\r\nDTSTART:20260310T090000\r\nDTEND:20260310T100000\r\nSUMMARY:Floating\r\nEND:VEVENT\r\n\
         BEGIN:VEVENT\r\nUID:bad\r\nDTSTART;TZID=Nowhere/Land:20260310T090000\r\nDTEND;TZID=Nowhere/Land:20260310T100000\r\nSUMMARY:Unknown zone\r\nEND:VEVENT",
        window(MARCH.0, MARCH.1),
    );
    let by = |uid: &str| evs.iter().find(|e| e.ical_uid == uid).unwrap();
    assert_eq!(start_utc(by("ny")), "2026-03-10 13:00"); // EDT, UTC-4
    assert_eq!(start_utc(by("win")), "2026-03-10 16:00"); // PDT, UTC-7
    assert_eq!(start_utc(by("float")), "2026-03-10 09:00"); // London default, GMT
    assert_eq!(start_utc(by("bad")), "2026-03-10 09:00"); // falls back to default zone
    assert!(matches!(&by("ny").when, When::Timed { tzid: Some(t), .. } if t == "America/New_York"));
}

#[test]
fn all_day_single_and_multi_day_with_exclusive_end() {
    let evs = parse(
        "BEGIN:VEVENT\r\nUID:d1\r\nDTSTART;VALUE=DATE:20260310\r\nSUMMARY:One day\r\nEND:VEVENT\r\n\
         BEGIN:VEVENT\r\nUID:d3\r\nDTSTART;VALUE=DATE:20260310\r\nDTEND;VALUE=DATE:20260313\r\nSUMMARY:Three days\r\nEND:VEVENT",
        window(MARCH.0, MARCH.1),
    );
    let by = |uid: &str| evs.iter().find(|e| e.ical_uid == uid).unwrap();
    let day = |y, m, d| NaiveDate::from_ymd_opt(y, m, d).unwrap();
    assert_eq!(
        by("d1").when,
        When::AllDay {
            start: day(2026, 3, 10),
            end: day(2026, 3, 11)
        }
    );
    assert_eq!(
        by("d3").when,
        When::AllDay {
            start: day(2026, 3, 10),
            end: day(2026, 3, 13)
        }
    );
}

#[test]
fn duration_instead_of_dtend() {
    let evs = parse(
        "BEGIN:VEVENT\r\nUID:dur\r\nDTSTART:20260310T090000Z\r\nDURATION:PT1H30M\r\nSUMMARY:x\r\nEND:VEVENT\r\n\
         BEGIN:VEVENT\r\nUID:wk\r\nDTSTART;VALUE=DATE:20260310\r\nDURATION:P1W\r\nSUMMARY:y\r\nEND:VEVENT",
        window(MARCH.0, MARCH.1),
    );
    let dur = evs.iter().find(|e| e.ical_uid == "dur").unwrap();
    assert!(
        matches!(&dur.when, When::Timed { start, end, .. } if (*end - *start) == Duration::minutes(90))
    );
    let wk = evs.iter().find(|e| e.ical_uid == "wk").unwrap();
    assert!(matches!(&wk.when, When::AllDay { start, end } if (*end - *start).num_days() == 7));
}

#[test]
fn weekly_meeting_keeps_local_time_across_a_dst_change() {
    // Every Monday 09:00 London. UK clocks go forward on 2026-03-29: UTC offset changes 0 -> +1.
    let evs = parse(
        "BEGIN:VEVENT\r\nUID:wk\r\nDTSTART;TZID=Europe/London:20260316T090000\r\nDTEND;TZID=Europe/London:20260316T093000\r\n\
         RRULE:FREQ=WEEKLY;BYDAY=MO\r\nSUMMARY:Standup\r\nEND:VEVENT",
        window((2026, 3, 15), (2026, 4, 13)),
    );
    let starts: Vec<_> = evs.iter().map(start_utc).collect();
    assert_eq!(
        starts,
        [
            "2026-03-16 09:00",
            "2026-03-23 09:00",
            "2026-03-30 08:00",
            "2026-04-06 08:00"
        ],
        "09:00 local stays 09:00 local: 09:00 UTC in GMT, 08:00 UTC in BST"
    );
    assert!(
        evs.iter()
            .all(|e| e.recurrence_master_id.as_deref() == Some("wk"))
    );
    // Each instance has a distinct id and remote id.
    let mut ids: Vec<_> = evs.iter().map(|e| e.id.clone()).collect();
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), 4);
}

#[test]
fn exdate_removes_one_occurrence() {
    let evs = parse(
        "BEGIN:VEVENT\r\nUID:x\r\nDTSTART:20260302T090000Z\r\nDTEND:20260302T100000Z\r\nRRULE:FREQ=DAILY;COUNT=5\r\n\
         EXDATE:20260304T090000Z\r\nSUMMARY:Daily\r\nEND:VEVENT",
        window(MARCH.0, MARCH.1),
    );
    let starts: Vec<_> = evs.iter().map(start_utc).collect();
    assert_eq!(
        starts,
        [
            "2026-03-02 09:00",
            "2026-03-03 09:00",
            "2026-03-05 09:00",
            "2026-03-06 09:00"
        ]
    );
}

#[test]
fn exdate_with_tzid_matches_the_local_occurrence() {
    let evs = parse(
        "BEGIN:VEVENT\r\nUID:x\r\nDTSTART;TZID=America/New_York:20260302T090000\r\nDTEND;TZID=America/New_York:20260302T100000\r\n\
         RRULE:FREQ=DAILY;COUNT=3\r\nEXDATE;TZID=America/New_York:20260303T090000\r\nSUMMARY:x\r\nEND:VEVENT",
        window(MARCH.0, MARCH.1),
    );
    assert_eq!(evs.len(), 2);
}

#[test]
fn override_moves_one_occurrence_and_cancel_removes_one() {
    let evs = parse(
        "BEGIN:VEVENT\r\nUID:s\r\nDTSTART:20260302T090000Z\r\nDTEND:20260302T100000Z\r\nRRULE:FREQ=DAILY;COUNT=4\r\nSUMMARY:Series\r\nEND:VEVENT\r\n\
         BEGIN:VEVENT\r\nUID:s\r\nRECURRENCE-ID:20260303T090000Z\r\nDTSTART:20260303T140000Z\r\nDTEND:20260303T150000Z\r\nSUMMARY:Series (moved)\r\nEND:VEVENT\r\n\
         BEGIN:VEVENT\r\nUID:s\r\nRECURRENCE-ID:20260304T090000Z\r\nDTSTART:20260304T090000Z\r\nDTEND:20260304T100000Z\r\nSTATUS:CANCELLED\r\nSUMMARY:Series\r\nEND:VEVENT",
        window(MARCH.0, MARCH.1),
    );
    let got: Vec<_> = evs
        .iter()
        .map(|e| format!("{} {}", start_utc(e), e.title))
        .collect();
    assert_eq!(
        got,
        [
            "2026-03-02 09:00 Series",
            "2026-03-03 14:00 Series (moved)",
            "2026-03-05 09:00 Series"
        ]
    );
}

#[test]
fn rrule_is_limited_to_the_window_and_by_until_and_count() {
    let infinite = "BEGIN:VEVENT\r\nUID:i\r\nDTSTART:20250101T090000Z\r\nDTEND:20250101T100000Z\r\nRRULE:FREQ=DAILY\r\nSUMMARY:x\r\nEND:VEVENT";
    let evs = parse(infinite, window((2026, 3, 10), (2026, 3, 13)));
    assert_eq!(
        evs.len(),
        3,
        "an endless daily series only yields what is in the window"
    );

    let until = "BEGIN:VEVENT\r\nUID:u\r\nDTSTART:20260301T090000Z\r\nDTEND:20260301T100000Z\r\nRRULE:FREQ=DAILY;UNTIL=20260303T235959Z\r\nSUMMARY:x\r\nEND:VEVENT";
    assert_eq!(parse(until, window(MARCH.0, MARCH.1)).len(), 3);
}

#[test]
fn all_day_recurrence_stays_on_the_same_dates() {
    let evs = parse(
        "BEGIN:VEVENT\r\nUID:b\r\nDTSTART;VALUE=DATE:20260310\r\nRRULE:FREQ=WEEKLY;COUNT=3\r\nSUMMARY:Bin day\r\nEND:VEVENT",
        window(MARCH.0, MARCH.1),
    );
    let starts: Vec<_> = evs.iter().map(start_utc).collect();
    assert_eq!(
        starts,
        [
            "2026-03-10 (all-day)",
            "2026-03-17 (all-day)",
            "2026-03-24 (all-day)"
        ]
    );
}

#[test]
fn rdate_adds_extra_occurrences() {
    let evs = parse(
        "BEGIN:VEVENT\r\nUID:r\r\nDTSTART:20260302T090000Z\r\nDTEND:20260302T100000Z\r\nRDATE:20260310T090000Z,20260311T090000Z\r\nSUMMARY:x\r\nEND:VEVENT",
        window(MARCH.0, MARCH.1),
    );
    assert_eq!(evs.len(), 3);
}

#[test]
fn event_crossing_midnight_and_window_edges() {
    let body = "BEGIN:VEVENT\r\nUID:n\r\nDTSTART:20260310T230000Z\r\nDTEND:20260311T010000Z\r\nSUMMARY:Night\r\nEND:VEVENT";
    assert_eq!(
        parse(body, window((2026, 3, 11), (2026, 3, 12))).len(),
        1,
        "in window by its tail"
    );
    assert_eq!(parse(body, window((2026, 3, 12), (2026, 3, 13))).len(), 0);
    assert_eq!(parse(body, window((2026, 3, 9), (2026, 3, 10))).len(), 0);
}

#[test]
fn alarms_do_not_leak_into_the_event() {
    let evs = parse(
        "BEGIN:VEVENT\r\nUID:al\r\nDTSTART:20260310T090000Z\r\nDTEND:20260310T100000Z\r\nSUMMARY:Real title\r\n\
         BEGIN:VALARM\r\nACTION:DISPLAY\r\nDESCRIPTION:Reminder text\r\nTRIGGER:-PT10M\r\nEND:VALARM\r\nEND:VEVENT",
        window(MARCH.0, MARCH.1),
    );
    assert_eq!(evs.len(), 1);
    assert_eq!(evs[0].title, "Real title");
    assert_eq!(evs[0].description, None);
}

#[test]
fn status_and_rsvp_are_read() {
    let body = "BEGIN:VEVENT\r\nUID:p\r\nDTSTART:20260310T090000Z\r\nDTEND:20260310T100000Z\r\nSTATUS:TENTATIVE\r\n\
                ORGANIZER;CN=Priya:mailto:priya@example.com\r\nATTENDEE;PARTSTAT=DECLINED:mailto:Me@Example.com\r\nSUMMARY:x\r\nEND:VEVENT";
    let w = window(MARCH.0, MARCH.1);
    let e = &parse_in(body, w, chrono_tz::UTC, Some("me@example.com"))[0];
    assert_eq!(e.status, EventStatus::Tentative);
    assert_eq!(e.my_response, Response::Declined);
    assert_eq!(e.organizer.as_deref(), Some("Priya"));
    assert_eq!(
        parse_in(body, w, chrono_tz::UTC, None)[0].my_response,
        Response::None,
        "anonymous feeds have no RSVP"
    );
    let mine = body.replace("priya@example.com", "me@example.com");
    assert_eq!(
        parse_in(&mine, w, chrono_tz::UTC, Some("me@example.com"))[0].my_response,
        Response::None,
        "organiser"
    );
}

#[test]
fn join_link_comes_from_conference_field_location_or_description() {
    let body = "BEGIN:VEVENT\r\nUID:j1\r\nDTSTART:20260310T090000Z\r\nDTEND:20260310T100000Z\r\nSUMMARY:a\r\n\
                DESCRIPTION:Join https://acme.zoom.us/j/123 now\r\nEND:VEVENT\r\n\
                BEGIN:VEVENT\r\nUID:j2\r\nDTSTART:20260310T110000Z\r\nDTEND:20260310T120000Z\r\nSUMMARY:b\r\n\
                X-GOOGLE-CONFERENCE:https://meet.google.com/abc-defg-hij\r\nLOCATION:Room 1\r\nEND:VEVENT\r\n\
                BEGIN:VEVENT\r\nUID:j3\r\nDTSTART:20260310T130000Z\r\nDTEND:20260310T140000Z\r\nSUMMARY:c\r\n\
                DESCRIPTION:Dial http://zoom.us/j/9 only\r\nEND:VEVENT";
    let evs = parse(body, window(MARCH.0, MARCH.1));
    let by = |u: &str| {
        evs.iter()
            .find(|e| e.ical_uid == u)
            .unwrap()
            .join_url
            .clone()
    };
    assert_eq!(by("j1").as_deref(), Some("https://acme.zoom.us/j/123"));
    assert_eq!(
        by("j2").as_deref(),
        Some("https://meet.google.com/abc-defg-hij")
    );
    assert_eq!(by("j3"), None, "plain http is rejected");
}

#[test]
fn malformed_events_are_skipped_not_fatal() {
    let evs = parse(
        "BEGIN:VEVENT\r\nUID:ok\r\nDTSTART:20260310T090000Z\r\nDTEND:20260310T100000Z\r\nSUMMARY:fine\r\nEND:VEVENT\r\n\
         BEGIN:VEVENT\r\nUID:nodate\r\nSUMMARY:no start\r\nEND:VEVENT\r\n\
         BEGIN:VEVENT\r\nDTSTART:20260310T090000Z\r\nSUMMARY:no uid\r\nEND:VEVENT\r\n\
         BEGIN:VEVENT\r\nUID:garbage\r\nDTSTART:not-a-date\r\nSUMMARY:bad date\r\nEND:VEVENT\r\n\
         BEGIN:VEVENT\r\nUID:badrule\r\nDTSTART:20260311T090000Z\r\nDTEND:20260311T100000Z\r\nRRULE:FREQ=NONSENSE\r\nSUMMARY:rule\r\nEND:VEVENT",
        window(MARCH.0, MARCH.1),
    );
    let titles: Vec<_> = evs.iter().map(|e| e.title.as_str()).collect();
    assert_eq!(
        titles,
        ["fine", "rule"],
        "bad rule keeps the first occurrence"
    );
}

#[test]
fn non_calendar_input_is_an_error() {
    let opts = ParseOptions {
        calendar_id: "c",
        default_tz: chrono_tz::UTC,
        window: window(MARCH.0, MARCH.1),
        my_email: None,
    };
    assert!(events_from_ics("<html>Not found</html>", &opts).is_err());
    assert!(events_from_ics("", &opts).is_err());
    // An empty but valid calendar is fine.
    assert!(
        events_from_ics("BEGIN:VCALENDAR\r\nEND:VCALENDAR", &opts)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn byte_order_mark_and_bare_newlines_are_tolerated() {
    let ics = "\u{feff}BEGIN:VCALENDAR\nBEGIN:VEVENT\nUID:b\nDTSTART:20260310T090000Z\nDTEND:20260310T100000Z\nSUMMARY:ok\nEND:VEVENT\nEND:VCALENDAR\n";
    let opts = ParseOptions {
        calendar_id: "c",
        default_tz: chrono_tz::UTC,
        window: window(MARCH.0, MARCH.1),
        my_email: None,
    };
    assert_eq!(events_from_ics(ics, &opts).unwrap().len(), 1);
}

#[test]
fn durations_parse() {
    assert_eq!(parse_duration("PT45M"), Some(Duration::minutes(45)));
    assert_eq!(parse_duration("P1DT2H"), Some(Duration::hours(26)));
    assert_eq!(parse_duration("-PT15M"), Some(-Duration::minutes(15)));
    assert_eq!(parse_duration("P2W"), Some(Duration::weeks(2)));
    assert_eq!(parse_duration("PT"), Some(Duration::zero()));
    for bad in ["45M", "PT5", "P1H", "PTxM", ""] {
        assert_eq!(parse_duration(bad), None, "{bad}");
    }
}

/// A feed from the internet is untrusted. A pathological rule must never stall the sync thread.
#[test]
fn hostile_recurrence_rules_finish_quickly_and_keep_the_event() {
    let started = std::time::Instant::now();
    for rule in [
        "FREQ=SECONDLY",
        "FREQ=MINUTELY;INTERVAL=1",
        "FREQ=SECONDLY;INTERVAL=1;COUNT=999999999",
        "FREQ=DAILY;BYHOUR=0,1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19,20,21,22,23;BYMINUTE=0,5,10,15,20,25,30,35,40,45,50,55;BYSECOND=0,10,20,30,40,50",
        "FREQ=SECONDLY;BYSECOND=0,1,2,3",
    ] {
        let body = format!(
            "BEGIN:VEVENT\r\nUID:h\r\nDTSTART:19900101T090000Z\r\nDTEND:19900101T090100Z\r\nRRULE:{rule}\r\nSUMMARY:hostile\r\nEND:VEVENT"
        );
        let evs = parse(&body, window((2026, 3, 1), (2026, 4, 30)));
        assert!(evs.len() <= 1000, "{rule}: {} events", evs.len());
    }
    assert!(
        started.elapsed() < std::time::Duration::from_secs(5),
        "took {:?}",
        started.elapsed()
    );
}

#[test]
fn a_hostile_rule_still_shows_the_first_occurrence_when_it_is_in_the_window() {
    let body = "BEGIN:VEVENT\r\nUID:h\r\nDTSTART:20260310T090000Z\r\nDTEND:20260310T090100Z\r\nRRULE:FREQ=SECONDLY\r\nSUMMARY:fast\r\nEND:VEVENT";
    // Starting inside the window, the step estimate is tiny, so it expands (and is capped at the instance limit).
    let evs = parse(body, window((2026, 3, 10), (2026, 3, 11)));
    assert!(!evs.is_empty() && evs.len() <= 1000);
}

#[test]
fn ordinary_long_running_rules_still_expand_correctly() {
    // Daily since 1990, weekly since 2000, hourly since 2020: realistic, and well under the step budget.
    for (rule, dtstart, expected_min) in [
        ("FREQ=DAILY", "19900101T090000Z", 30),
        ("FREQ=WEEKLY;BYDAY=MO,WE,FR", "20000103T090000Z", 6),
        ("FREQ=HOURLY;INTERVAL=6", "20200101T000000Z", 100),
        ("FREQ=MONTHLY;BYMONTHDAY=1,15", "19950101T090000Z", 2),
        ("FREQ=YEARLY", "19000310T090000Z", 0),
    ] {
        let body = format!(
            "BEGIN:VEVENT\r\nUID:r\r\nDTSTART:{dtstart}\r\nDTEND:{dtstart}\r\nRRULE:{rule}\r\nSUMMARY:r\r\nEND:VEVENT"
        );
        let evs = parse(&body, window((2026, 3, 1), (2026, 4, 30)));
        assert!(
            evs.len() >= expected_min,
            "{rule} from {dtstart}: only {} events",
            evs.len()
        );
    }
}

#[test]
fn the_step_estimate_separates_real_rules_from_hostile_ones() {
    let t = |y| Utc.with_ymd_and_hms(y, 1, 1, 0, 0, 0).unwrap();
    let steps = |rule, from| estimated_steps(rule, from, t(2026));
    assert!(steps("FREQ=DAILY", t(1990)).unwrap() < 20_000);
    assert!(steps("FREQ=HOURLY", t(2000)).unwrap() < MAX_EXPANSION_STEPS);
    assert!(steps("FREQ=SECONDLY", t(2025)).unwrap() > MAX_EXPANSION_STEPS);
    assert!(steps("FREQ=MINUTELY", t(2020)).unwrap() > MAX_EXPANSION_STEPS);
    assert!(steps("FREQ=DAILY;BYHOUR=0,1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19,20,21,22,23;BYMINUTE=0,5,10,15,20,25,30,35,40,45,50,55", t(1990)).unwrap() > MAX_EXPANSION_STEPS);
    assert_eq!(steps("FREQ=NONSENSE", t(2020)), None);
    assert_eq!(steps("not a rule", t(2020)), None);
    assert!(
        steps("FREQ=DAILY;INTERVAL=0", t(1990)).is_some(),
        "a zero interval cannot divide by zero"
    );
    assert_eq!(
        steps("FREQ=DAILY", t(2030)).unwrap(),
        0,
        "a start after the window needs no scanning"
    );
}

/// With `panic = "abort"` in release builds, one panic on untrusted input would crash the whole app at every
/// launch (the sync thread runs at startup). Extreme values must produce an error or a skipped event, never a panic.
#[test]
fn extreme_values_in_a_feed_never_panic() {
    let wrap = |body: &str| {
        format!(
            "BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nUID:x\r\n{body}\r\nSUMMARY:t\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n"
        )
    };
    let cases = [
        "DTSTART:20260310T090000Z\r\nDURATION:P99999999999999W",
        "DTSTART:20260310T090000Z\r\nDURATION:P9223372036854775807D",
        "DTSTART:20260310T090000Z\r\nDURATION:PT9223372036854775807S",
        "DTSTART:20260310T090000Z\r\nDURATION:-P9223372036854775807W",
        "DTSTART;VALUE=DATE:20260310\r\nDURATION:P99999999999999D",
        "DTSTART:99991231T235959Z\r\nDTEND:99991231T235959Z",
        "DTSTART:99991231T235959Z\r\nDURATION:P1000W",
        "DTSTART;VALUE=DATE:99991231\r\nDTEND;VALUE=DATE:99991231",
        "DTSTART;VALUE=DATE:99991231\r\nRRULE:FREQ=DAILY;COUNT=5",
        "DTSTART:00010101T000000Z\r\nDTEND:99991231T235959Z",
        "DTSTART:00010101T000000Z\r\nRRULE:FREQ=YEARLY",
        "DTSTART:20260310T090000Z\r\nDTEND:20260310T100000Z\r\nRRULE:FREQ=DAILY;INTERVAL=99999999999",
        "DTSTART:20260310T090000Z\r\nDTEND:20260310T100000Z\r\nRRULE:FREQ=DAILY;COUNT=99999999999999999999",
        "DTSTART:20260310T090000Z\r\nDTEND:20260310T100000Z\r\nRRULE:FREQ=YEARLY;UNTIL=99991231T235959Z",
        "DTSTART:20260310T090000Z\r\nDTEND:20260310T100000Z\r\nRRULE:FREQ=WEEKLY;BYDAY=MO;INTERVAL=0",
        "DTSTART:20260310T090000Z\r\nDTEND:20260310T100000Z\r\nRDATE:99991231T235959Z,00010101T000000Z",
        "DTSTART:20260310T090000Z\r\nDTEND:20260310T100000Z\r\nEXDATE:99991231T235959Z",
        "DTSTART:20260310T090000Z\r\nDTEND:20260309T090000Z",
        "DTSTART:20260310T250000Z",
        "DTSTART:20261310T090000Z",
        "DTSTART:20260230T090000Z",
        "DTSTART;TZID=Europe/London:20260329T013000\r\nDTEND;TZID=Europe/London:20260329T020000",
        "DTSTART;TZID=Pacific/Apia:20111230T120000",
        "DTSTART:20260310T090000Z\r\nRECURRENCE-ID:99991231T235959Z",
        "DTSTART:20260310T090000Z\r\nDTEND:\r\nRRULE:",
        "DTSTART:\r\nDTEND:",
        "DTSTART:2026\r\nDTEND:1",
        "DTSTART:20260310T090000Z\r\nDTEND:20260310T100000Z\r\nRRULE:FREQ=DAILY;BYSETPOS=9999999999",
        "DTSTART:20260310T090000Z\r\nDTEND:20260310T100000Z\r\nRRULE:FREQ=MONTHLY;BYMONTHDAY=99999999999",
    ];
    let w = window((2026, 3, 1), (2026, 4, 30));
    for body in cases {
        let ics = wrap(body);
        let result = std::panic::catch_unwind(|| {
            events_from_ics(
                &ics,
                &ParseOptions {
                    calendar_id: "c",
                    default_tz: chrono_tz::Europe::London,
                    window: w,
                    my_email: None,
                },
            )
        });
        assert!(result.is_ok(), "PANIC on: {body:?}");
    }
}

/// A small deterministic fuzzer: random extreme tokens in every date/recurrence field. It cannot prove the
/// absence of bugs, but it covers far more combinations than hand-picked cases. Seeded, so failures reproduce.
#[test]
#[allow(clippy::manual_is_multiple_of)]
fn fuzzing_extreme_feed_values_never_panics_or_stalls() {
    struct Rng(u64);
    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0
        }
        fn pick<'a>(&mut self, items: &[&'a str]) -> &'a str {
            items[(self.next() % items.len() as u64) as usize]
        }
    }
    let numbers = [
        "0",
        "1",
        "-1",
        "7",
        "99",
        "86400",
        "2147483647",
        "4294967296",
        "9223372036854775807",
        "99999999999999999999",
        "-9223372036854775808",
        "",
    ];
    let dates = [
        "00010101T000000Z",
        "19000101T000000Z",
        "19700101T000000Z",
        "20260310T090000Z",
        "20260329T013000",
        "99991231T235959Z",
        "20260230T000000Z",
        "20261331T000000Z",
        "2026",
        "garbage",
        "",
    ];
    let day_dates = ["00010101", "20260310", "99991231", "20260230", "x"];
    let freqs = [
        "SECONDLY",
        "MINUTELY",
        "HOURLY",
        "DAILY",
        "WEEKLY",
        "MONTHLY",
        "YEARLY",
        "FORTNIGHTLY",
        "",
    ];
    let units = ["W", "D", "H", "M", "S", "X"];
    let zones = [
        "UTC",
        "Europe/London",
        "Pacific/Apia",
        "Mars/Base",
        "Eastern Standard Time",
        "",
    ];

    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    let w = window((2026, 3, 1), (2026, 4, 30));
    let started = std::time::Instant::now();
    for i in 0..4000 {
        let mut lines = vec![format!("UID:f{i}")];
        let all_day = rng.next() % 4 == 0;
        let zone = rng.pick(&zones);
        if all_day {
            lines.push(format!("DTSTART;VALUE=DATE:{}", rng.pick(&day_dates)));
            if rng.next() % 2 == 0 {
                lines.push(format!("DTEND;VALUE=DATE:{}", rng.pick(&day_dates)));
            }
        } else {
            lines.push(format!("DTSTART;TZID={zone}:{}", rng.pick(&dates)));
            match rng.next() % 3 {
                0 => lines.push(format!("DTEND;TZID={zone}:{}", rng.pick(&dates))),
                1 => lines.push(format!(
                    "DURATION:P{}{}T{}{}",
                    rng.pick(&numbers),
                    rng.pick(&units),
                    rng.pick(&numbers),
                    rng.pick(&units)
                )),
                _ => {}
            }
        }
        if rng.next() % 2 == 0 {
            let mut rule = format!("FREQ={}", rng.pick(&freqs));
            for key in [
                "INTERVAL",
                "COUNT",
                "BYSETPOS",
                "BYMONTHDAY",
                "BYHOUR",
                "BYWEEKNO",
            ] {
                if rng.next() % 3 == 0 {
                    rule += &format!(";{key}={}", rng.pick(&numbers));
                }
            }
            if rng.next() % 3 == 0 {
                rule += &format!(";UNTIL={}", rng.pick(&dates));
            }
            lines.push(format!("RRULE:{rule}"));
        }
        for key in ["EXDATE", "RDATE", "RECURRENCE-ID"] {
            if rng.next() % 4 == 0 {
                lines.push(format!("{key}:{},{}", rng.pick(&dates), rng.pick(&dates)));
            }
        }
        let ics = format!(
            "BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\n{}\r\nSUMMARY:fuzz\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n",
            lines.join("\r\n")
        );
        let res = std::panic::catch_unwind(|| {
            events_from_ics(
                &ics,
                &ParseOptions {
                    calendar_id: "c",
                    default_tz: chrono_tz::UTC,
                    window: w,
                    my_email: None,
                },
            )
        });
        assert!(res.is_ok(), "PANIC (iteration {i}) on:\n{ics}");
        if let Ok(Ok(evs)) = res {
            assert!(evs.len() <= 1100, "iteration {i}: {} events", evs.len());
        }
    }
    assert!(
        started.elapsed() < std::time::Duration::from_secs(60),
        "fuzzing took {:?}",
        started.elapsed()
    );
}

/// Regression: an all-day recurring event that ends on a date used to show only its first day.
#[test]
fn all_day_repeat_with_a_date_until_expands_fully() {
    let text = "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nBEGIN:VEVENT\r\nUID:a@b\r\nDTSTAMP:20261114T000000Z\r\nDTSTART;VALUE=DATE:20261116\r\nDTEND;VALUE=DATE:20261117\r\nRRULE:FREQ=DAILY;UNTIL=20261119\r\nSUMMARY:Holiday\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";
    let now: chrono::DateTime<chrono::Utc> = "2026-11-14T12:00:00Z".parse().unwrap();
    let opts = ParseOptions {
        calendar_id: "t",
        default_tz: chrono_tz::UTC,
        window: crate::window::Window {
            from: now - chrono::Duration::days(30),
            to: now + chrono::Duration::days(90),
        },
        my_email: None,
    };
    let days: Vec<String> = events_from_ics(text, &opts)
        .unwrap()
        .iter()
        .filter_map(|e| match &e.when {
            crate::model::When::AllDay { start, .. } => Some(start.to_string()),
            _ => None,
        })
        .collect();
    assert_eq!(
        days,
        ["2026-11-16", "2026-11-17", "2026-11-18", "2026-11-19"],
        "UNTIL is inclusive"
    );
}

// ---- guest lists --------------------------------------------------------------------------

fn guests(vevent_lines: &str) -> Vec<Attendee> {
    let text = format!(
        "BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nUID:g1\r\nDTSTART:20260310T100000Z\r\nDTEND:20260310T110000Z\r\nSUMMARY:x\r\n{vevent_lines}END:VEVENT\r\nEND:VCALENDAR\r\n"
    );
    let opts = ParseOptions {
        calendar_id: "c",
        default_tz: chrono_tz::UTC,
        window: crate::window::Window {
            from: "2026-03-01T00:00:00Z".parse().unwrap(),
            to: "2026-04-01T00:00:00Z".parse().unwrap(),
        },
        my_email: None,
    };
    events_from_ics(&text, &opts).unwrap().remove(0).attendees
}

#[test]
fn the_guest_list_has_the_organiser_first_then_each_person_with_their_answer() {
    let g = guests(
        "ORGANIZER;CN=Priya Shah:mailto:Priya@Example.com\r\n\
         ATTENDEE;CN=Priya Shah;PARTSTAT=ACCEPTED:mailto:priya@example.com\r\n\
         ATTENDEE;CN=Sam;PARTSTAT=NEEDS-ACTION:mailto:sam@example.com\r\n\
         ATTENDEE;PARTSTAT=DECLINED:mailto:kim@example.com\r\n\
         ATTENDEE;PARTSTAT=TENTATIVE;CN=Lee:mailto:lee@example.com\r\n",
    );
    let line = |a: &Attendee| (a.email.clone(), a.name.clone(), a.response, a.organizer);
    assert_eq!(g.len(), 4, "the organiser listed again is one entry");
    assert_eq!(
        line(&g[0]),
        (
            "priya@example.com".into(),
            Some("Priya Shah".into()),
            Response::Accepted,
            true
        )
    );
    assert_eq!(
        line(&g[1]),
        (
            "sam@example.com".into(),
            Some("Sam".into()),
            Response::NeedsAction,
            false
        )
    );
    assert_eq!(
        line(&g[2]),
        ("kim@example.com".into(), None, Response::Declined, false)
    );
    assert_eq!(g[3].response, Response::Tentative);
}

#[test]
fn rooms_and_entries_without_an_address_are_not_people_and_the_list_is_capped() {
    let g = guests(
        "ATTENDEE;CUTYPE=ROOM;CN=Room 4:mailto:room4@example.com\r\n\
         ATTENDEE;CN=Everyone:urn:group\r\n\
         ATTENDEE:mailto:ok@example.com\r\n",
    );
    assert_eq!(
        g.iter().map(|a| a.email.as_str()).collect::<Vec<_>>(),
        ["ok@example.com"]
    );
    let many: String = (0..300)
        .map(|i| format!("ATTENDEE:mailto:p{i}@example.com\r\n"))
        .collect();
    assert_eq!(guests(&many).len(), MAX_ATTENDEES);
}

#[test]
fn an_event_with_no_guests_has_an_empty_list() {
    assert!(guests("").is_empty());
}
