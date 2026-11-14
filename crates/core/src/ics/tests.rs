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
