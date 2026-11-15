//! Moving a previous installation's data under the app's current names.
//!
//! The app used to be called "Daybrief" and stored its database and secrets under that name
//! (`com.daybrief.app`). When it became Firstlight the identity changed everywhere, so on the first start the
//! data is moved across: the database file, and every secret that belongs to it (the database key, account
//! sign-ins, AI keys, mailbox passwords) from the old keychain service to the new one.
//!
//! The move is careful rather than quick: nothing old is deleted until the copy has been opened and checked,
//! and any failure puts things back exactly as they were. The old names live only in this file.

use crate::mail::imap;
use crate::secrets::{DB_KEY_NAME, Redacted, SecretStore};
use crate::settings::AiProvider;
use crate::store::Store;
use crate::{Error, Result};
use std::path::{Path, PathBuf};

/// The keychain service name and data folder the app used before it was renamed.
pub const OLD_KEYCHAIN_BASE: &str = "com.daybrief.app";
pub const OLD_APP_DIR: &str = "com.daybrief.app";

/// The database file name a profile used before the rename (`daybrief.db`, `daybrief-<profile>.db`).
pub fn old_db_file_name(profile: Option<&str>) -> String {
    match profile {
        Some(p) => format!("daybrief-{p}.db"),
        None => "daybrief.db".to_string(),
    }
}

/// Where the old and the new data are.
pub struct Paths {
    pub old_dir: PathBuf,
    pub old_db: String,
    pub new_dir: PathBuf,
    pub new_db: String,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    /// The current database already holds the user's data (or there was never an old one to bring).
    NotNeeded,
    /// Nothing from a previous installation was found.
    NothingToMigrate,
    /// A previous installation exists but could not be read this time (for example the keychain asked and was
    /// refused). It is left untouched and tried again at the next start.
    Skipped(&'static str),
    Migrated {
        accounts: usize,
        secrets: usize,
    },
}

/// The files that make up one database (the write-ahead log and index file only exist while it is open).
fn db_files(dir: &Path, name: &str) -> Vec<PathBuf> {
    ["", "-wal", "-shm"]
        .iter()
        .map(|suffix| dir.join(format!("{name}{suffix}")))
        .collect()
}

fn remove_db(dir: &Path, name: &str) {
    for f in db_files(dir, name) {
        let _ = std::fs::remove_file(f);
    }
}

/// A database is "fresh" if the app created it and nothing was ever added: no accounts and no events of the
/// user's own. A fresh one never holds data worth keeping over a previous installation's.
fn is_fresh(path: &Path, key: &Redacted) -> bool {
    match Store::open(path, key) {
        Ok(s) => {
            s.list_accounts().map(|a| a.is_empty()).unwrap_or(false)
                && s.all_own_events().map(|e| e.is_empty()).unwrap_or(false)
        }
        Err(_) => false,
    }
}

/// Moves the previous installation's data to the current names. Safe to call at every start.
pub fn migrate(p: &Paths, old: &dyn SecretStore, new: &dyn SecretStore) -> Result<Outcome> {
    let old_path = p.old_dir.join(&p.old_db);
    let new_path = p.new_dir.join(&p.new_db);
    if !old_path.exists() {
        return Ok(Outcome::NothingToMigrate);
    }
    if new_path.exists() {
        // Already moved, or the app has been used under its new name. Only a database that is still
        // untouched (a start that created an empty one before the move could happen) gets replaced.
        let fresh = new
            .get(DB_KEY_NAME)
            .ok()
            .flatten()
            .is_some_and(|k| is_fresh(&new_path, &Redacted::new(k)));
        if !fresh {
            return Ok(Outcome::NotNeeded);
        }
    }

    // Without the old key the old database cannot be read. Leave everything as it is.
    let old_key = match old.get(DB_KEY_NAME) {
        Ok(Some(k)) if !k.is_empty() => k,
        Ok(_) => return Ok(Outcome::Skipped("the previous installation has no key")),
        Err(_) => return Ok(Outcome::Skipped("the previous keychain could not be read")),
    };

    // Which secrets belong to this data: found by reading the old database, never by guessing.
    let mut names: Vec<String> = vec![DB_KEY_NAME.to_string()];
    let accounts;
    {
        let store = Store::open(&old_path, &Redacted::new(old_key.clone()))
            .map_err(|_| Error::Provider("the previous database could not be opened".into()))?;
        let list = store.list_accounts()?;
        accounts = list.len();
        names.extend(list.into_iter().filter_map(|a| a.secret_ref));
        let settings = store.load_settings();
        names.extend(settings.email.imap.iter().map(|m| imap::secret_key(&m.id)));
        // The old database is closed here (its write-ahead log is folded in) before it is copied.
    }
    names.extend(
        AiProvider::ALL
            .iter()
            .filter_map(|p| p.key_name())
            .map(String::from),
    );
    names.sort();
    names.dedup();

    // Copy the secrets, remembering which names did not exist in the new keychain so a failure can undo them.
    let mut added: Vec<String> = Vec::new();
    let mut moved: Vec<String> = Vec::new();
    let undo = |added: &[String]| {
        for n in added {
            let _ = new.delete(n);
        }
    };
    for name in &names {
        let value = match old.get(name) {
            Ok(Some(v)) => v,
            Ok(None) => continue,
            Err(e) => {
                undo(&added);
                return Err(e);
            }
        };
        let existed = new.get(name).ok().flatten().is_some();
        if let Err(e) = new.set(name, &value) {
            undo(&added);
            return Err(e);
        }
        if !existed {
            added.push(name.clone());
        }
        moved.push(name.clone());
    }

    // Copy the database next to its new name, then prove the copy opens and matches before touching the old.
    let copy_result = (|| -> Result<()> {
        std::fs::create_dir_all(&p.new_dir)
            .map_err(|_| Error::Provider("could not create the new data folder".into()))?;
        remove_db(&p.new_dir, &p.new_db); // a fresh, empty one being replaced
        for (from, to) in db_files(&p.old_dir, &p.old_db)
            .into_iter()
            .zip(db_files(&p.new_dir, &p.new_db))
        {
            if from.exists() {
                std::fs::copy(&from, &to)
                    .map_err(|_| Error::Provider("could not copy the database".into()))?;
            }
        }
        let check = Store::open(&new_path, &Redacted::new(old_key.clone()))
            .map_err(|_| Error::Provider("the copied database did not open".into()))?;
        if check.list_accounts()?.len() != accounts {
            return Err(Error::Provider("the copied database did not match".into()));
        }
        Ok(())
    })();
    if let Err(e) = copy_result {
        remove_db(&p.new_dir, &p.new_db);
        undo(&added);
        return Err(e);
    }

    // Verified: now the old copies can go, so no trace of the old name stays behind.
    for name in &moved {
        let _ = old.delete(name);
    }
    remove_db(&p.old_dir, &p.old_db);
    let _ = std::fs::remove_dir(&p.old_dir); // only if it is now empty
    Ok(Outcome::Migrated {
        accounts,
        secrets: moved.len(),
    })
}

#[cfg(test)]
mod tests;
