use super::*;
use crate::secrets::{MemorySecretStore, load_or_create_db_key};
use crate::tasks::TaskInput;
use chrono_tz::Tz;

const NY: Tz = chrono_tz::America::New_York;

fn store() -> Store {
    let key = load_or_create_db_key(&MemorySecretStore::default()).unwrap();
    Store::open_in_memory(&key).unwrap()
}

fn t(s: &str) -> DateTime<Utc> {
    s.parse().unwrap()
}

fn tmp(name: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("fl-folder-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    p
}

fn task(title: &str) -> TaskInput {
    TaskInput {
        title: title.into(),
        ..Default::default()
    }
}

#[test]
fn the_sync_code_carries_the_key_and_forgives_typing() {
    let (code, key) = new_code().unwrap();
    assert!(code.starts_with("FL1-"));
    assert_eq!(parse_code(&code).unwrap(), key);
    assert_eq!(
        parse_code(&code.to_lowercase().replace('-', " ")).unwrap(),
        key
    );
    assert!(parse_code("FL1-ABCD").is_err());
    assert!(parse_code("not a code!").is_err());
}

#[test]
fn a_file_opens_only_with_its_key_and_is_not_readable_text() {
    let (_, key) = new_code().unwrap();
    let (_, other) = new_code().unwrap();
    let sealed = seal(&key, b"{\"task\":\"Call the bank\"}").unwrap();
    assert!(!String::from_utf8_lossy(&sealed).contains("Call the bank"));
    assert_eq!(
        open(&key, &sealed).unwrap(),
        b"{\"task\":\"Call the bank\"}"
    );
    assert!(open(&other, &sealed).is_err());
    let mut tampered = sealed.clone();
    *tampered.last_mut().unwrap() ^= 1;
    assert!(open(&key, &tampered).is_err());
}

#[test]
fn two_computers_share_tasks_notes_events_and_days_off_and_deletions_follow() {
    let dir = tmp("two");
    let (_, key) = new_code().unwrap();
    let (mut mac, mut pc) = (store(), store());
    let now = t("2026-11-19T15:00:00Z");

    // The Mac has a task, a note, an event of its own and a day off.
    let bank = crate::todo::add(&mac, &task("Call the bank"), NY, now).unwrap();
    mac.set_meeting_note("design review", Some("Ask about the budget"), now)
        .unwrap();
    let ev = crate::local::save(
        &mut mac,
        None,
        &crate::local::EventInput {
            title: "Dentist".into(),
            all_day: false,
            start: "2026-11-20T15:00:00Z".into(),
            end: "2026-11-20T16:00:00Z".into(),
            location: None,
            join_url: None,
            notes: None,
            repeat: None,
            reminder_minutes: None,
            invitees: vec![],
            focus: false,
        },
        NY,
        now,
    )
    .unwrap();
    let mut s = mac.load_settings();
    s.day.time_off = vec![crate::day::TimeOff {
        from: "2026-12-24".parse().unwrap(),
        to: "2026-12-31".parse().unwrap(),
        label: "Winter".into(),
    }];
    mac.save_settings(&s).unwrap();

    sync(&mut mac, &dir, &key, "Mac", now).unwrap();
    let r = sync(&mut pc, &dir, &key, "PC", now).unwrap();
    assert!(r.applied >= 4, "{r:?}");
    assert_eq!(r.peers[0].name, "Mac");
    assert_eq!(
        crate::todo::get(&pc, &bank.id).unwrap().title,
        "Call the bank"
    );
    assert_eq!(
        pc.get_meeting_note("design review").unwrap().unwrap().0,
        "Ask about the budget"
    );
    assert!(
        pc.get_event(&ev.id).unwrap().is_some(),
        "the event's dated rows are made on the PC too"
    );
    assert_eq!(pc.load_settings().day.time_off.len(), 1);

    // The PC ticks the task off and adds one; the Mac deletes the note.
    let later = t("2026-11-19T16:00:00Z");
    crate::todo::set_done(&pc, &bank.id, true, NY, later).unwrap();
    let milk = crate::todo::add(&pc, &task("Buy milk"), NY, later).unwrap();
    mac.set_meeting_note("design review", None, later).unwrap();
    sync(&mut pc, &dir, &key, "PC", later).unwrap();
    sync(&mut mac, &dir, &key, "Mac", later).unwrap();
    sync(&mut pc, &dir, &key, "PC", later).unwrap();
    assert!(crate::todo::get(&mac, &bank.id).unwrap().done_at.is_some());
    assert_eq!(crate::todo::get(&mac, &milk.id).unwrap().title, "Buy milk");
    assert!(pc.get_meeting_note("design review").unwrap().is_none());

    // A deleted task goes everywhere.
    crate::todo::delete(&mac, &milk.id).unwrap();
    sync(&mut mac, &dir, &key, "Mac", later).unwrap();
    sync(&mut pc, &dir, &key, "PC", later).unwrap();
    assert!(crate::todo::get(&pc, &milk.id).is_err());

    // Nothing changed: our file is not written again.
    let own = dir
        .join("Firstlight")
        .join(format!("{}.flsync", device_id(&mac).unwrap()));
    let before = std::fs::metadata(&own).unwrap().modified().unwrap();
    std::thread::sleep(std::time::Duration::from_millis(20));
    sync(&mut mac, &dir, &key, "Mac", later).unwrap();
    assert_eq!(std::fs::metadata(&own).unwrap().modified().unwrap(), before);
    assert_eq!(
        std::fs::read_dir(dir.join("Firstlight")).unwrap().count(),
        2
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_file_made_with_another_code_is_reported_and_changes_nothing() {
    let dir = tmp("other");
    let (_, mine) = new_code().unwrap();
    let (_, theirs) = new_code().unwrap();
    let (mut a, mut b) = (store(), store());
    let now = t("2026-11-19T15:00:00Z");
    crate::todo::add(&b, &task("Secret"), NY, now).unwrap();
    sync(&mut b, &dir, &theirs, "Stranger", now).unwrap();
    assert!(needs_code(&dir, Some(&mine), &device_id(&a).unwrap()));
    assert!(!needs_code(&dir, Some(&theirs), &device_id(&a).unwrap()));
    let r = sync(&mut a, &dir, &mine, "Mine", now).unwrap();
    assert_eq!((r.unreadable, r.applied), (1, 0));
    assert!(crate::todo::all(&a).unwrap().is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}
