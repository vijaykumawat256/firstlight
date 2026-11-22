//! Automatic backups of the database, so a bad update or a mistake never costs the person their tasks, notes and
//! events. Copies stay encrypted with the same key as the database (in the system keychain), in
//! `<data folder>/backups/<database name>/`:
//!
//! - before a new version of the app opens the database for the first time (`before_open`, a plain file copy taken
//!   while nothing has the database open, after folding the write-ahead log into it), so a format change can be
//!   undone;
//! - once a day while the app runs (`daily`, through SQLCipher's export, safe while the database is in use);
//! - right before a restore replaces the database (so a restore can itself be undone).
//!
//! The newest `KEEP` are kept. A restore is asked for while the app runs (`request_restore`) and done at the next start,
//! before the database is opened (`before_open`), because the open database cannot be swapped underneath the app.

use crate::secrets::Redacted;
use crate::store::Store;
use crate::{Error, Result};
use chrono::{DateTime, NaiveDateTime, Utc};
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};

pub const KEEP: usize = 10;
const REQUEST: &str = "restore-request";
const LAST_VERSION: &str = "last-version";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Reason {
    Daily,
    Update,
    BeforeRestore,
}

impl Reason {
    fn tag(self) -> &'static str {
        match self {
            Reason::Daily => "daily",
            Reason::Update => "update",
            Reason::BeforeRestore => "restore",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BackupInfo {
    pub name: String,
    pub taken_at: DateTime<Utc>,
    pub reason: Reason,
    pub bytes: u64,
}

/// Where the backups of one database live.
pub fn dir_for(data_dir: &Path, db_file: &str) -> PathBuf {
    let stem = db_file.strip_suffix(".db").unwrap_or(db_file);
    data_dir.join("backups").join(stem)
}

fn name_for(at: DateTime<Utc>, reason: Reason) -> String {
    format!(
        "firstlight-{}-{}.db",
        at.format("%Y%m%d-%H%M%S"),
        reason.tag()
    )
}

/// Reads a backup's name back; anything else in the folder is not ours and is ignored (and never restored).
fn parse_name(name: &str) -> Option<(DateTime<Utc>, Reason)> {
    let rest = name.strip_prefix("firstlight-")?.strip_suffix(".db")?;
    let (stamp, tag) = rest.rsplit_once('-')?;
    let reason = match tag {
        "daily" => Reason::Daily,
        "update" => Reason::Update,
        "restore" => Reason::BeforeRestore,
        _ => return None,
    };
    if stamp.len() != 15 || !stamp.bytes().all(|b| b.is_ascii_digit() || b == b'-') {
        return None;
    }
    let at = NaiveDateTime::parse_from_str(stamp, "%Y%m%d-%H%M%S").ok()?;
    Some((at.and_utc(), reason))
}

/// The backups, newest first.
pub fn list(dir: &Path) -> Vec<BackupInfo> {
    let mut out: Vec<BackupInfo> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_str()?.to_string();
            let (taken_at, reason) = parse_name(&name)?;
            let meta = e.metadata().ok().filter(|m| m.is_file())?;
            Some(BackupInfo {
                name,
                taken_at,
                reason,
                bytes: meta.len(),
            })
        })
        .collect();
    out.sort_by(|a, b| b.taken_at.cmp(&a.taken_at).then(b.name.cmp(&a.name)));
    out
}

fn prune(dir: &Path) {
    for old in list(dir).into_iter().skip(KEEP) {
        let _ = fs::remove_file(dir.join(old.name));
    }
}

/// A file error, without its path (a path can carry the person's name).
fn io(e: std::io::Error) -> Error {
    Error::Database(format!("backup file: {:?}", e.kind()))
}

fn copy_file(from: &Path, to: &Path) -> Result<()> {
    let tmp = to.with_extension("partial");
    fs::copy(from, &tmp).map_err(io)?;
    fs::rename(&tmp, to).map_err(io)?;
    Ok(())
}

/// Folds the write-ahead log into the database file, so the file alone is the whole database.
fn checkpoint(db: &Path, key: &Redacted) -> Result<()> {
    let conn = rusqlite::Connection::open(db)?;
    conn.execute_batch(&format!("PRAGMA key = \"x'{}'\";", key.expose()))?;
    conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()))
        .map_err(|_| Error::DatabaseLocked)?;
    Ok(())
}

fn remove_sidecars(db: &Path) {
    for ext in ["db-wal", "db-shm"] {
        let _ = fs::remove_file(db.with_extension(ext));
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Startup {
    /// The backup put back, if a restore was asked for.
    pub restored: Option<String>,
    /// The backup taken because this version had not opened the database before.
    pub update_backup: Option<String>,
}

/// Runs before the database is opened: a pending restore first, then a copy if this version of the app has not
/// opened the database before. Nothing here may stop the app from starting; failures are returned for logging.
pub fn before_open(
    data_dir: &Path,
    db_file: &str,
    key: &Redacted,
    app_version: &str,
    now: DateTime<Utc>,
) -> Result<Startup> {
    let db = data_dir.join(db_file);
    let dir = dir_for(data_dir, db_file);
    let mut out = Startup::default();
    if !db.exists() {
        return Ok(out);
    }
    fs::create_dir_all(&dir).map_err(io)?;

    if let Ok(wanted) = fs::read_to_string(dir.join(REQUEST)) {
        let _ = fs::remove_file(dir.join(REQUEST));
        let wanted = wanted.trim();
        if list(&dir).iter().any(|b| b.name == wanted) {
            checkpoint(&db, key)?;
            copy_file(&db, &dir.join(name_for(now, Reason::BeforeRestore)))?;
            remove_sidecars(&db);
            copy_file(&dir.join(wanted), &db)?;
            out.restored = Some(wanted.to_string());
        }
    }

    let last = fs::read_to_string(dir.join(LAST_VERSION)).unwrap_or_default();
    if last.trim() != app_version {
        checkpoint(&db, key)?;
        let name = name_for(now, Reason::Update);
        copy_file(&db, &dir.join(&name))?;
        fs::write(dir.join(LAST_VERSION), app_version).map_err(io)?;
        out.update_backup = Some(name);
    }
    prune(&dir);
    Ok(out)
}

/// Takes the day's backup if there is none from the last 20 hours. Returns its name when one was taken.
pub fn daily(
    store: &Store,
    key: &Redacted,
    dir: &Path,
    now: DateTime<Utc>,
) -> Result<Option<String>> {
    let recent = list(dir)
        .iter()
        .any(|b| now.signed_duration_since(b.taken_at) < chrono::Duration::hours(20));
    if recent {
        return Ok(None);
    }
    fs::create_dir_all(dir).map_err(io)?;
    let name = name_for(now, Reason::Daily);
    let tmp = dir.join(format!("{name}.partial"));
    let _ = fs::remove_file(&tmp);
    store.export_copy(&tmp, key)?;
    fs::rename(&tmp, dir.join(&name)).map_err(io)?;
    prune(dir);
    Ok(Some(name))
}

/// Asks for `name` to be put back at the next start. Only a backup in the list can be asked for.
pub fn request_restore(dir: &Path, name: &str) -> Result<()> {
    if !list(dir).iter().any(|b| b.name == name) {
        return Err(Error::Invalid("no such backup"));
    }
    fs::write(dir.join(REQUEST), name).map_err(io)?;
    Ok(())
}

#[cfg(test)]
mod tests;
