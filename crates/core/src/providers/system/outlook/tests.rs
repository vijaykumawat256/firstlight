use super::*;
use std::sync::Mutex;

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
        r#"WARNING: banner
[{"store":"00AB","entry":"01CD","title":"Calendar","source":"me@work.com","exchange":true},
 {"store":"00EF","entry":"02AA","title":"","source":"Personal.pst","exchange":false}]"#,
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
    let s = OutlookSource::new(Fake::new(true, json));
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
    let fake = Fake::new(true, "[]");
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
    for junk in ["", "Outlook is not installed", "{\"a\":1}", "[not json]"] {
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
