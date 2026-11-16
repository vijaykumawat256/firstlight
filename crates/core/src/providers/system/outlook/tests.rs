use super::*;
use std::sync::Mutex;

/// What a script prints when it works: the JSON, base64, behind a marker.
fn ok(json: &str) -> String {
    use base64::Engine;
    format!(
        "FLJSON:{}",
        base64::engine::general_purpose::STANDARD.encode(json)
    )
}

/// What a script prints when Outlook or Windows refuses.
fn failed(message: &str) -> String {
    use base64::Engine;
    format!(
        "FLERR:{}",
        base64::engine::general_purpose::STANDARD.encode(message)
    )
}

struct Fake {
    installed: bool,
    answer: String,
    scripts: Mutex<Vec<String>>,
}

impl Fake {
    fn new(installed: bool, answer: &str) -> Self {
        Self {
            installed,
            answer: answer.into(),
            scripts: Mutex::default(),
        }
    }
}

impl ScriptRunner for Fake {
    fn installed(&self) -> bool {
        self.installed
    }
    fn run(&self, script: &str) -> Result<String> {
        self.scripts.lock().unwrap().push(script.to_string());
        Ok(self.answer.clone())
    }
}

#[test]
fn it_is_offered_only_when_classic_outlook_is_installed() {
    assert_eq!(
        OutlookSource::new(Fake::new(true, "[]")).access(),
        Access::Granted
    );
    assert_eq!(
        OutlookSource::new(Fake::new(false, "[]")).access(),
        Access::Unsupported
    );
    assert_eq!(OutlookSource::new(Fake::new(true, "[]")).name(), "Outlook");
}

#[test]
fn calendars_come_from_every_store_and_exchange_ones_are_marked() {
    let s = OutlookSource::new(Fake::new(
        true,
        &format!(
            "WARNING: banner\n{}",
            ok(
                r#"[{"store":"00AB","entry":"01CD","title":"Calendar","source":"me@work.com","exchange":true},
 {"store":"00EF","entry":"02AA","title":"","source":"Personal.pst","exchange":false}]"#
            )
        ),
    ));
    let c = s.calendars().unwrap();
    assert_eq!(c.len(), 2);
    assert_eq!(c[0].id, "00AB:01CD");
    assert_eq!(c[0].kind, SysCalendarKind::Exchange);
    assert_eq!(
        c[1].title, "Calendar",
        "an unnamed folder still gets a name"
    );
    assert_eq!(c[1].kind, SysCalendarKind::Local);
    assert!(c.iter().all(|x| x.read_only));
}

#[test]
fn events_are_mapped_with_response_status_and_cancellation() {
    let json = r#"[
 {"id":"AA11","uid":"U1","title":"Design review","start":1773136800,"end":1773140400,"all_day":false,
  "location":"Room 4","notes":"Agenda","organizer":"Sam","busy":2,"meeting":1,"response":5,"recurring":true,"modified":1773000000},
 {"id":"AA22","uid":"","title":"Offsite","start":1773100800,"end":1773187200,"all_day":true,
  "location":"","notes":"","organizer":"","busy":1,"meeting":5,"response":3,"recurring":false,"modified":0}]"#;
    let s = OutlookSource::new(Fake::new(true, &ok(json)));
    let from = DateTime::from_timestamp(1773000000, 0).unwrap();
    let e = s
        .events("00AB:01CD", from, from + chrono::Duration::days(1))
        .unwrap();
    assert_eq!(e[0].title, "Design review");
    assert_eq!(e[0].response, SysResponse::Pending);
    assert_eq!(e[0].status, SysStatus::Confirmed);
    assert!(e[0].is_recurring);
    assert_eq!(e[0].location.as_deref(), Some("Room 4"));
    assert_eq!(e[0].start.timestamp(), 1773136800);
    assert_eq!(
        e[1].status,
        SysStatus::Canceled,
        "a cancelled meeting is marked"
    );
    assert_eq!(e[1].response, SysResponse::Accepted);
    assert!(
        e[1].all_day && e[1].uid.is_none() && e[1].location.is_none() && e[1].modified.is_none()
    );
}

#[test]
fn the_script_asks_for_the_window_in_unix_seconds_and_only_for_that_calendar() {
    let fake = Fake::new(true, &ok("[]"));
    let s = OutlookSource::new(fake);
    let from = DateTime::from_timestamp(1_800_000_000, 0).unwrap();
    s.events("00AB:01CD", from, from + chrono::Duration::days(2))
        .unwrap();
    let scripts = s.runner.scripts.lock().unwrap();
    let sc = &scripts[0];
    assert!(sc.contains("GetFolderFromID('01CD','00AB')"), "{sc}");
    assert!(
        sc.contains("FromUnixTimeSeconds(1800000000)")
            && sc.contains("FromUnixTimeSeconds(1800172800)")
    );
    assert!(sc.contains("IncludeRecurrences=$true"));
    // Reading only: nothing that creates, changes or removes.
    for banned in [
        ".Save(",
        ".Delete(",
        "CreateItem",
        ".Send(",
        ".Move(",
        "Remove-",
    ] {
        assert!(!sc.contains(banned), "script must not contain {banned}");
    }
}

#[test]
fn an_id_that_is_not_hexadecimal_never_reaches_a_script() {
    let fake = Fake::new(true, "[]");
    let s = OutlookSource::new(fake);
    let t = DateTime::from_timestamp(1_800_000_000, 0).unwrap();
    for bad in [
        "",
        "nocolon",
        "00AB:",
        ":01CD",
        "00AB:01CD');Remove-Item x;('",
        "zz:01",
        "00AB:01CD:02",
    ] {
        assert!(s.events(bad, t, t).is_err(), "{bad:?}");
    }
    assert!(s.runner.scripts.lock().unwrap().is_empty());
}

#[test]
fn an_answer_that_is_not_json_is_a_plain_error_not_a_panic() {
    for junk in [
        "",
        "Outlook is not installed",
        "{\"a\":1}",
        "[not json]",
        "FLJSON:!!!not base64",
        &ok("[not json]"),
        &ok("{\"a\":1}"),
    ] {
        let s = OutlookSource::new(Fake::new(true, junk));
        assert!(s.calendars().is_err(), "{junk:?}");
    }
}

#[test]
fn the_calendar_listing_script_is_read_only_too() {
    let sc = calendars_script();
    assert!(sc.contains("GetDefaultFolder(9)"));
    for banned in [".Save(", ".Delete(", "CreateItem", ".Send("] {
        assert!(!sc.contains(banned));
    }
}

#[test]
fn a_failure_reported_by_the_script_reaches_the_person_with_a_hint_and_what_windows_said() {
    let cases = [
        (
            "Retrieving the COM class factory for component with CLSID {0006F03A-0000-0000-C000-000000000046} failed due to the following error: 80040154 Class not registered (Exception from HRESULT: 0x80040154 (REGDB_E_CLASSNOTREG)).",
            "Classic Outlook isn't installed",
        ),
        (
            "Server execution failed (Exception from HRESULT: 0x80080005 (CO_E_SERVER_EXEC_FAILURE))",
            "wouldn't start",
        ),
        (
            "Cannot complete the operation. You are not connected. Profile missing",
            "waiting for you to choose a profile",
        ),
        ("Something nobody expected", "Outlook couldn't be read."),
    ];
    for (said, hint) in cases {
        let s = OutlookSource::new(Fake::new(true, &failed(said)));
        let Err(Error::Provider(m)) = s.calendars() else {
            panic!("expected a provider error for {said}")
        };
        assert!(m.contains(hint), "{m}");
        assert!(m.contains("Windows said:"), "{m}");
    }
}

#[test]
fn what_windows_said_is_one_short_clean_line() {
    let long = format!("bad\u{7}\n\tthing {}", "x".repeat(2000));
    let s = OutlookSource::new(Fake::new(true, &failed(&long)));
    let Err(Error::Provider(m)) = s.calendars() else {
        panic!()
    };
    assert!(!m.chars().any(char::is_control), "{m:?}");
    assert!(m.chars().count() < 700, "{}", m.chars().count());
}

#[test]
fn the_script_is_handed_to_powershell_as_one_encoded_command() {
    // "Write-Output 1" as UTF-16LE, base64: the form PowerShell's -EncodedCommand expects.
    assert_eq!(
        encode_command("Write-Output 1"),
        "VwByAGkAdABlAC0ATwB1AHQAcAB1AHQAIAAxAA=="
    );
    // Both real scripts fit comfortably where a command line allows (about 32 000 characters).
    for sc in [calendars_script(), events_script("00AB", "01CD", 1, 2)] {
        assert!(
            encode_command(&sc).len() < 12_000,
            "{}",
            encode_command(&sc).len()
        );
    }
}

#[test]
fn the_scripts_are_balanced_and_always_answer() {
    for sc in [
        calendars_script(),
        events_script("00AB", "01CD", 1_773_000_000, 1_773_172_800),
    ] {
        assert_eq!(
            sc.matches('{').count(),
            sc.matches('}').count(),
            "braces: {sc}"
        );
        assert_eq!(
            sc.matches('(').count(),
            sc.matches(')').count(),
            "parentheses: {sc}"
        );
        assert!(
            sc.contains("FLJSON:") && sc.contains("FLERR:"),
            "both ways out are there"
        );
        assert!(sc.starts_with("$ErrorActionPreference='Stop'"));
        assert!(
            !sc.contains('@') || !sc.contains("@ENTRY@"),
            "no placeholder is left in"
        );
        assert!(
            !sc.contains("OutputEncoding"),
            "nothing depends on the console's code page"
        );
    }
}

#[test]
fn long_store_ids_are_accepted_and_the_default_calendar_is_a_fallback() {
    // Exchange and IMAP store ids run to many hundreds of characters.
    assert!(is_hex(&"0A".repeat(900)));
    assert!(!is_hex(&"0A".repeat(3000)));
    let sc = calendars_script();
    assert!(sc.contains("$ns.GetDefaultFolder(9)"));
    assert!(sc.contains("No calendar folder found"));
}

#[test]
fn the_scripts_never_ask_for_a_field_outlook_guards() {
    // Each of these makes Outlook wait on a security prompt, which stalls the whole read.
    let events = events_script("0A", "0B", 0, 1);
    for sc in [calendars_script(), events] {
        for guarded in [
            ".Body",
            ".Organizer",
            ".Recipients",
            "PropertyAccessor",
            ".RequiredAttendees",
            ".OptionalAttendees",
        ] {
            assert!(!sc.contains(guarded), "{guarded} must not be asked for");
        }
    }
}
