//! "Erase all data": the user's accounts, cached events, history, settings, and the credentials
//! saved for each account.

use crate::Result;
use crate::secrets::SecretStore;
use crate::store::Store;

/// What was removed, for the confirmation message.
#[derive(Debug, PartialEq, Eq)]
pub struct EraseReport {
    pub accounts: usize,
    pub credentials: usize,
}

/// Credentials first: if the keychain refuses, the accounts stay so the user can retry, instead of
/// leaving secrets behind with nothing pointing at them.
pub fn erase_everything(store: &Store, secrets: &dyn SecretStore) -> Result<EraseReport> {
    let accounts = store.list_accounts()?;
    let mut credentials = 0usize;
    for a in &accounts {
        if let Some(r) = &a.secret_ref {
            secrets.delete(r)?;
            credentials += 1;
        }
    }
    // Email-only mailboxes (Yahoo, iCloud) have no account row; their passwords are named in the settings.
    for m in &store.load_settings().email.imap {
        secrets.delete(&crate::mail::imap::secret_key(&m.id))?;
        credentials += 1;
    }
    // The key for sharing through a synced folder (the files in that folder stay; without the key they are noise).
    secrets.delete(crate::folder_sync::KEY_SECRET)?;
    secrets.delete(crate::oauth::apps::GOOGLE_SECRET_KEY)?;
    store.erase_all()?;
    Ok(EraseReport {
        accounts: accounts.len(),
        credentials,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::*;
    use crate::secrets::{DB_KEY_NAME, MemorySecretStore, load_or_create_db_key};
    use crate::{Error, Result};
    use chrono::Utc;

    fn account(id: &str, secret: Option<&str>) -> Account {
        Account {
            id: id.into(),
            provider: ProviderKind::Ics,
            display_name: id.into(),
            email: None,
            status: AccountStatus::Ok,
            secret_ref: secret.map(String::from),
            created_at: Utc::now(),
        }
    }

    #[test]
    fn removes_every_account_credential_and_all_data() {
        let secrets = MemorySecretStore::default();
        let key = load_or_create_db_key(&secrets).unwrap();
        let store = Store::open_in_memory(&key).unwrap();
        for (id, s) in [
            ("a", Some("ics-url:a")),
            ("b", Some("oauth:b")),
            ("sys", None),
        ] {
            store.upsert_account(&account(id, s)).unwrap();
            if let Some(s) = s {
                secrets.set(s, "SECRET-VALUE").unwrap();
            }
        }
        store
            .save_settings(&crate::settings::AppSettings::default())
            .unwrap();

        let report = erase_everything(&store, &secrets).unwrap();
        assert_eq!(
            report,
            EraseReport {
                accounts: 3,
                credentials: 2
            }
        );
        assert_eq!(secrets.get("ics-url:a").unwrap(), None);
        assert_eq!(secrets.get("oauth:b").unwrap(), None);
        assert!(store.list_accounts().unwrap().is_empty());
        // The database key is kept: it only protects the now-empty database.
        assert!(secrets.get(DB_KEY_NAME).unwrap().is_some());
    }

    /// A keychain that refuses to delete.
    struct Stubborn;
    impl SecretStore for Stubborn {
        fn get(&self, _: &str) -> Result<Option<String>> {
            Ok(None)
        }
        fn set(&self, _: &str, _: &str) -> Result<()> {
            Ok(())
        }
        fn delete(&self, _: &str) -> Result<()> {
            Err(Error::Secret("denied".into()))
        }
    }

    #[test]
    fn if_the_keychain_refuses_nothing_is_deleted_so_it_can_be_retried() {
        let key = load_or_create_db_key(&MemorySecretStore::default()).unwrap();
        let store = Store::open_in_memory(&key).unwrap();
        store
            .upsert_account(&account("a", Some("ics-url:a")))
            .unwrap();
        assert!(erase_everything(&store, &Stubborn).is_err());
        assert_eq!(
            store.list_accounts().unwrap().len(),
            1,
            "the account must survive so credentials aren't orphaned"
        );
    }

    #[test]
    fn removes_email_only_mailbox_passwords() {
        let secrets = MemorySecretStore::default();
        let key = load_or_create_db_key(&secrets).unwrap();
        let store = Store::open_in_memory(&key).unwrap();
        let mut settings = crate::settings::AppSettings::default();
        settings.email.imap.push(crate::mail::imap::ImapMailbox {
            id: "m1".into(),
            preset: crate::mail::imap::ImapPreset::Icloud,
            address: "me@icloud.com".into(),
            host: None,
        });
        store.save_settings(&settings).unwrap();
        secrets.set("imap-mail:m1", "APP-PASSWORD").unwrap();

        let report = erase_everything(&store, &secrets).unwrap();
        assert_eq!(report.credentials, 1);
        assert_eq!(secrets.get("imap-mail:m1").unwrap(), None);
        assert!(store.load_settings().email.imap.is_empty());
    }
}
