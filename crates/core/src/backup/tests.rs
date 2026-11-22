use super::*;
use crate::secrets::{MemorySecretStore, load_or_create_db_key};
use crate::tasks::TaskInput;

const NY: chrono_tz::Tz = chrono_tz::America::New_York;
const DB: &str = "firstlight.db";

fn tmp(name: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("fl-backup-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&p);
    fs::create_dir_all(&p).unwrap();
    p
}

fn t(s: &str) -> DateTime<Utc> {
    s.parse().unwrap()
}

fn add(store: &Store, title: &str) {
    let input = TaskInput {
        title: title.into(),
        ..Default::default()
    };
    crate::todo::add(store, &input, NY, t("2026-11-21T10:00:00Z")).unwrap();
}

fn titles(store: &Store) -> Vec<String> {
    let mut v: Vec<String> = crate::todo::all(store)
        .unwrap()
        .into_iter()
        .map(|t| t.title)
        .collect();
    v.sort();
    v
}

#[test]
fn names_are_read_back_and_strangers_ignored() {
    let at = t("2026-11-21T09:08:07Z");
    let n = name_for(at, Reason::Update);
    assert_eq!(n, "firstlight-20261121-090807-update.db");
    assert_eq!(parse_name(&n), Some((at, Reason::Update)));
    for bad in [
        "firstlight.db",
        "firstlight-20261121-090807-daily.db-wal",
        "firstlight-2026112-090807-daily.db",
        "firstlight-20261121-090807-other.db",
        "../firstlight-20261121-090807-daily.db",
    ] {
        assert_eq!(parse_name(bad), None, "{bad}");
    }
}

#[test]
fn a_new_version_backs_up_first_and_the_copy_opens_with_the_same_key() {
    let data = tmp("update");
    let key = load_or_create_db_key(&MemorySecretStore::default()).unwrap();
    {
        let s = Store::open(&data.join(DB), &key).unwrap();
        add(&s, "Call the bank");
    } // closed without a checkpoint: the task may still be in the write-ahead log

    let now = t("2026-11-21T10:00:00Z");
    let first = before_open(&data, DB, &key, "0.1.0", now).unwrap();
    let name = first
        .update_backup
        .expect("the first start of a version backs up");
    // The same version again: no new copy.
    assert_eq!(
        before_open(&data, DB, &key, "0.1.0", now).unwrap(),
        Startup::default()
    );
    let dir = dir_for(&data, DB);
    let copy = Store::open(&dir.join(&name), &key).unwrap();
    assert_eq!(titles(&copy), ["Call the bank"]);
    // Another key cannot read it.
    let other = load_or_create_db_key(&MemorySecretStore::default()).unwrap();
    assert!(Store::open(&dir.join(&name), &other).is_err());
    // The next version backs up again.
    assert!(
        before_open(&data, DB, &key, "0.2.0", t("2026-11-22T10:00:00Z"))
            .unwrap()
            .update_backup
            .is_some()
    );
    let _ = fs::remove_dir_all(&data);
}

#[test]
fn the_daily_copy_is_taken_once_a_day_while_in_use() {
    let data = tmp("daily");
    let key = load_or_create_db_key(&MemorySecretStore::default()).unwrap();
    let s = Store::open(&data.join(DB), &key).unwrap();
    add(&s, "Water the plants");
    let dir = dir_for(&data, DB);
    let name = daily(&s, &key, &dir, t("2026-11-21T10:00:00Z"))
        .unwrap()
        .unwrap();
    assert_eq!(
        daily(&s, &key, &dir, t("2026-11-21T20:00:00Z")).unwrap(),
        None
    );
    assert!(
        daily(&s, &key, &dir, t("2026-11-22T10:00:00Z"))
            .unwrap()
            .is_some()
    );
    let copy = Store::open(&dir.join(&name), &key).unwrap();
    assert_eq!(titles(&copy), ["Water the plants"]);
    assert_eq!(copy.schema_version().unwrap(), s.schema_version().unwrap());
    let _ = fs::remove_dir_all(&data);
}

#[test]
fn a_restore_puts_the_backup_back_at_the_next_start_and_can_itself_be_undone() {
    let data = tmp("restore");
    let key = load_or_create_db_key(&MemorySecretStore::default()).unwrap();
    let dir = dir_for(&data, DB);
    let first;
    {
        let s = Store::open(&data.join(DB), &key).unwrap();
        add(&s, "Kept");
        first = daily(&s, &key, &dir, t("2026-11-20T10:00:00Z"))
            .unwrap()
            .unwrap();
        add(&s, "Added after the backup");
        assert!(request_restore(&dir, "firstlight.db").is_err());
        assert!(request_restore(&dir, "../firstlight.db").is_err());
        request_restore(&dir, &first).unwrap();
    }
    fs::write(dir.join(LAST_VERSION), "0.1.0").unwrap();
    let r = before_open(&data, DB, &key, "0.1.0", t("2026-11-21T10:00:00Z")).unwrap();
    assert_eq!(r.restored.as_deref(), Some(first.as_str()));
    let s = Store::open(&data.join(DB), &key).unwrap();
    assert_eq!(titles(&s), ["Kept"]);
    // What was there before the restore was kept.
    let undo = list(&dir)
        .into_iter()
        .find(|b| b.reason == Reason::BeforeRestore)
        .unwrap();
    let before = Store::open(&dir.join(&undo.name), &key).unwrap();
    assert_eq!(titles(&before), ["Added after the backup", "Kept"]);
    // The request is used once.
    assert_eq!(
        before_open(&data, DB, &key, "0.1.0", t("2026-11-21T11:00:00Z"))
            .unwrap()
            .restored,
        None
    );
    let _ = fs::remove_dir_all(&data);
}

#[test]
fn only_the_newest_are_kept() {
    let data = tmp("prune");
    let key = load_or_create_db_key(&MemorySecretStore::default()).unwrap();
    let s = Store::open(&data.join(DB), &key).unwrap();
    let dir = dir_for(&data, DB);
    for day in 1..=14 {
        daily(&s, &key, &dir, t(&format!("2026-11-{day:02}T10:00:00Z"))).unwrap();
    }
    let kept = list(&dir);
    assert_eq!(kept.len(), KEEP);
    assert_eq!(kept[0].taken_at, t("2026-11-14T10:00:00Z"));
    let _ = fs::remove_dir_all(&data);
}
