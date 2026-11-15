use super::*;
use crate::model::*;
use crate::secrets::{MemorySecretStore, load_or_create_db_key};
use chrono::Utc;
use std::sync::Mutex;

fn scratch() -> PathBuf {
    let d = std::env::temp_dir().join(format!("firstlight-legacy-test-{}", new_id()));
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn paths(root: &Path) -> Paths {
    Paths {
        old_dir: root.join("old"),
        old_db: "daybrief.db".into(),
        new_dir: root.join("new"),
        new_db: "firstlight.db".into(),
    }
}

fn account(id: &str, secret: &str) -> Account {
    Account {
        id: id.into(),
        provider: ProviderKind::Ics,
        display_name: format!("Calendar {id}"),
        email: None,
        status: AccountStatus::Ok,
        secret_ref: Some(secret.into()),
        created_at: Utc::now(),
    }
}

/// A previous installation: a database with two accounts, an own event and a mailbox, and secrets for each.
fn previous_install(root: &Path) -> (MemorySecretStore, Redacted) {
    let old = MemorySecretStore::default();
    let key = load_or_create_db_key(&old).unwrap();
    std::fs::create_dir_all(root.join("old")).unwrap();
    let store = Store::open(&root.join("old").join("daybrief.db"), &key).unwrap();
    store.upsert_account(&account("a", "ics-url:a")).unwrap();
    store.upsert_account(&account("b", "oauth:b")).unwrap();
    let mut s = crate::settings::AppSettings::default();
    s.email.imap.push(imap::ImapMailbox {
        id: "m1".into(),
        preset: imap::ImapPreset::Icloud,
        address: "me@icloud.com".into(),
        host: None,
    });
    store.save_settings(&s).unwrap();
    store.put_own_event("own1", "{}").unwrap();
    drop(store);
    old.set("ics-url:a", "https://example.com/a.ics").unwrap();
    old.set("oauth:b", "TOKEN-B").unwrap();
    old.set("imap-mail:m1", "MAIL-PASSWORD").unwrap();
    old.set("ai-key-anthropic", "sk-ant-KEY").unwrap();
    old.set("ai-key-openai", "sk-OPENAI").unwrap();
    old.set("unrelated-secret", "NOT-OURS").unwrap();
    (old, key)
}

#[test]
fn a_previous_installation_is_moved_across_whole_and_nothing_old_is_left() {
    let root = scratch();
    let (old, key) = previous_install(&root);
    let new = MemorySecretStore::default();
    let p = paths(&root);

    let out = migrate(&p, &old, &new).unwrap();
    assert_eq!(
        out,
        Outcome::Migrated {
            accounts: 2,
            secrets: 6
        }
    );

    // The new database opens with the same key and holds the same things.
    assert_eq!(new.get(DB_KEY_NAME).unwrap().as_deref(), Some(key.expose()));
    let s = Store::open(&root.join("new").join("firstlight.db"), &key).unwrap();
    assert_eq!(s.list_accounts().unwrap().len(), 2);
    assert_eq!(s.all_own_events().unwrap().len(), 1);
    assert_eq!(s.load_settings().email.imap.len(), 1);
    // Every secret that belonged to it came too, and only those.
    assert_eq!(new.get("oauth:b").unwrap().as_deref(), Some("TOKEN-B"));
    assert_eq!(
        new.get("imap-mail:m1").unwrap().as_deref(),
        Some("MAIL-PASSWORD")
    );
    assert_eq!(
        new.get("ai-key-anthropic").unwrap().as_deref(),
        Some("sk-ant-KEY")
    );
    assert_eq!(
        new.get("ai-key-openai").unwrap().as_deref(),
        Some("sk-OPENAI")
    );
    assert_eq!(new.get("unrelated-secret").unwrap(), None);
    // Nothing is left under the old name.
    assert_eq!(old.get(DB_KEY_NAME).unwrap(), None);
    assert_eq!(old.get("oauth:b").unwrap(), None);
    assert_eq!(
        old.get("unrelated-secret").unwrap().as_deref(),
        Some("NOT-OURS"),
        "not ours: untouched"
    );
    assert!(!root.join("old").join("daybrief.db").exists());
    assert!(
        !root.join("old").exists(),
        "the empty old folder is removed"
    );

    // Running again does nothing, and the data is still intact.
    assert_eq!(migrate(&p, &old, &new).unwrap(), Outcome::NothingToMigrate);
    let s = Store::open(&root.join("new").join("firstlight.db"), &key).unwrap();
    assert_eq!(s.list_accounts().unwrap().len(), 2);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn nothing_to_move_and_already_moved_are_both_quiet() {
    let root = scratch();
    let p = paths(&root);
    let (old, new) = (MemorySecretStore::default(), MemorySecretStore::default());
    assert_eq!(migrate(&p, &old, &new).unwrap(), Outcome::NothingToMigrate);
    assert!(!root.join("new").exists(), "nothing is created for nothing");

    // A new database that already has the user's data is never replaced by an old one.
    let (old, _) = previous_install(&root);
    let key = load_or_create_db_key(&new).unwrap();
    std::fs::create_dir_all(root.join("new")).unwrap();
    let store = Store::open(&root.join("new").join("firstlight.db"), &key).unwrap();
    store
        .upsert_account(&account("keep", "oauth:keep"))
        .unwrap();
    drop(store);
    assert_eq!(migrate(&p, &old, &new).unwrap(), Outcome::NotNeeded);
    assert!(
        root.join("old").join("daybrief.db").exists(),
        "the old data is left alone"
    );
    assert_eq!(old.get("oauth:b").unwrap().as_deref(), Some("TOKEN-B"));
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn an_empty_database_made_too_early_is_replaced_by_the_real_data() {
    let root = scratch();
    let (old, key) = previous_install(&root);
    let new = MemorySecretStore::default();
    // A start that created a brand-new empty database under the new name before the move could happen.
    let fresh_key = load_or_create_db_key(&new).unwrap();
    std::fs::create_dir_all(root.join("new")).unwrap();
    drop(Store::open(&root.join("new").join("firstlight.db"), &fresh_key).unwrap());

    let out = migrate(&paths(&root), &old, &new).unwrap();
    assert!(matches!(out, Outcome::Migrated { accounts: 2, .. }));
    assert_eq!(
        new.get(DB_KEY_NAME).unwrap().as_deref(),
        Some(key.expose()),
        "the real key wins"
    );
    let s = Store::open(&root.join("new").join("firstlight.db"), &key).unwrap();
    assert_eq!(s.list_accounts().unwrap().len(), 2);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn an_unreadable_or_keyless_previous_install_is_left_untouched() {
    struct Denied;
    impl SecretStore for Denied {
        fn get(&self, _: &str) -> Result<Option<String>> {
            Err(Error::Secret("access denied".into()))
        }
        fn set(&self, _: &str, _: &str) -> Result<()> {
            Ok(())
        }
        fn delete(&self, _: &str) -> Result<()> {
            Ok(())
        }
    }
    let root = scratch();
    let (_old, _) = previous_install(&root);
    let new = MemorySecretStore::default();
    let out = migrate(&paths(&root), &Denied, &new).unwrap();
    assert!(matches!(out, Outcome::Skipped(_)));
    assert!(root.join("old").join("daybrief.db").exists());
    assert!(!root.join("new").join("firstlight.db").exists());

    let keyless = MemorySecretStore::default();
    assert!(matches!(
        migrate(&paths(&root), &keyless, &new).unwrap(),
        Outcome::Skipped(_)
    ));
    assert!(root.join("old").join("daybrief.db").exists());
    let _ = std::fs::remove_dir_all(&root);
}

/// A keychain that refuses to save one particular secret, to prove a failure puts everything back.
struct Failing {
    inner: MemorySecretStore,
    fail_on: &'static str,
    sets: Mutex<Vec<String>>,
}
impl SecretStore for Failing {
    fn get(&self, k: &str) -> Result<Option<String>> {
        self.inner.get(k)
    }
    fn set(&self, k: &str, v: &str) -> Result<()> {
        if k == self.fail_on {
            return Err(Error::Secret("keychain full".into()));
        }
        self.sets.lock().unwrap().push(k.to_string());
        self.inner.set(k, v)
    }
    fn delete(&self, k: &str) -> Result<()> {
        self.inner.delete(k)
    }
}

#[test]
fn a_failure_part_way_puts_everything_back_exactly_as_it_was() {
    let root = scratch();
    let (old, _) = previous_install(&root);
    let new = Failing {
        inner: MemorySecretStore::default(),
        fail_on: "oauth:b",
        sets: Mutex::new(vec![]),
    };
    let err = migrate(&paths(&root), &old, &new).unwrap_err();
    assert!(err.to_string().to_lowercase().contains("keychain") || matches!(err, Error::Secret(_)));
    // Whatever was copied before the failure is removed again, and the old data is untouched.
    assert!(
        !new.sets.lock().unwrap().is_empty(),
        "some secrets had been copied"
    );
    for name in new.sets.lock().unwrap().iter() {
        assert_eq!(new.inner.get(name).unwrap(), None, "{name} was left behind");
    }
    assert!(!root.join("new").join("firstlight.db").exists());
    assert!(root.join("old").join("daybrief.db").exists());
    assert_eq!(old.get("oauth:b").unwrap().as_deref(), Some("TOKEN-B"));
    assert!(old.get(DB_KEY_NAME).unwrap().is_some());

    // A later attempt with a healthy keychain still succeeds.
    let healthy = MemorySecretStore::default();
    assert!(matches!(
        migrate(&paths(&root), &old, &healthy).unwrap(),
        Outcome::Migrated { .. }
    ));
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn the_old_names_are_spelled_out_in_one_place() {
    assert_eq!(OLD_KEYCHAIN_BASE, "com.daybrief.app");
    assert_eq!(OLD_APP_DIR, "com.daybrief.app");
    assert_eq!(old_db_file_name(None), "daybrief.db");
    assert_eq!(old_db_file_name(Some("qa")), "daybrief-qa.db");
}
