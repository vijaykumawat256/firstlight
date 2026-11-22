//! Sharing between your own computers through a synced folder: files, encryption and the database. The merge rules
//! are in `firstlight_logic::foldersync`.
//!
//! Each computer writes `<folder>/Firstlight/<device>.flsync`: "FLSYNC1", a random 24-byte nonce, then the snapshot
//! (JSON) sealed with XChaCha20-Poly1305 under a 32-byte key. The key is made on the first computer and shown as a
//! sync code the person types on the others; it lives in each computer's keychain. Without it the files are noise,
//! so the cloud service holding the folder cannot read them.
//!
//! What is shared: tasks (deletion markers too, so a task removed while kept in step with Google goes everywhere),
//! meeting notes, events of your own kept on this computer (not ones saved into the Calendar app or CalDAV: those
//! belong to that calendar), your templates and your days off.

use crate::foldersync::{self, Change, Rec, Records, Snapshot};
use crate::store::Store;
use crate::tasks::Task;
use crate::{Error, Result};
use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use chrono::{DateTime, Utc};
use std::path::{Path, PathBuf};

/// Keychain entry holding the key.
pub const KEY_SECRET: &str = "folder-sync-key";
const MAGIC: &[u8] = b"FLSYNC1";
const SUBFOLDER: &str = "Firstlight";
const EXT: &str = "flsync";
/// Biggest file read (a runaway or foreign file is skipped).
const MAX_FILE: u64 = 32 * 1024 * 1024;
const DEVICE_KEY: &str = "folder_sync_device";
const WRITTEN_KEY: &str = "folder_sync_written";
/// Fingerprints of what this computer last wrote to the folder.
const PUBLISHED_KEY: &str = "folder_sync_published";

/// A new random key and the sync code that carries it.
pub fn new_code() -> Result<(String, [u8; 32])> {
    let mut key = [0u8; 32];
    getrandom::fill(&mut key).map_err(|_| Error::Invalid("Couldn't make a sync code."))?;
    Ok((code_of(&key), key))
}

const B32: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";

/// "FL1-" and the key in base32 (letters and digits 2 to 7, easy to read out and type), in groups of four.
pub fn code_of(key: &[u8; 32]) -> String {
    let mut bits: u64 = 0;
    let mut n = 0;
    let mut out = String::new();
    for b in key {
        bits = (bits << 8) | u64::from(*b);
        n += 8;
        while n >= 5 {
            n -= 5;
            out.push(B32[((bits >> n) & 31) as usize] as char);
        }
    }
    if n > 0 {
        out.push(B32[((bits << (5 - n)) & 31) as usize] as char);
    }
    let groups: Vec<String> = out
        .as_bytes()
        .chunks(4)
        .map(|c| String::from_utf8_lossy(c).into_owned())
        .collect();
    format!("FL1-{}", groups.join("-"))
}

/// Reads a sync code as typed: any case, spaces and dashes ignored, "FL1" optional.
pub fn parse_code(code: &str) -> Result<[u8; 32]> {
    let bad =
        || Error::Invalid("That sync code isn't right. Copy it again from the other computer.");
    let upper: String = code
        .trim()
        .to_uppercase()
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '-')
        .collect();
    let body = upper.strip_prefix("FL1").unwrap_or(&upper);
    let mut bits: u64 = 0;
    let mut n = 0;
    let mut out = Vec::with_capacity(32);
    for c in body.bytes() {
        let v = B32.iter().position(|x| *x == c).ok_or_else(bad)? as u64;
        bits = (bits << 5) | v;
        n += 5;
        if n >= 8 {
            n -= 8;
            out.push(((bits >> n) & 0xff) as u8);
        }
    }
    out.try_into().map_err(|_| bad())
}

pub fn seal(key: &[u8; 32], plain: &[u8]) -> Result<Vec<u8>> {
    let cipher = XChaCha20Poly1305::new(key.into());
    let mut nonce = [0u8; 24];
    getrandom::fill(&mut nonce).map_err(|_| Error::Invalid("Couldn't encrypt."))?;
    let sealed = cipher
        .encrypt(XNonce::from_slice(&nonce), plain)
        .map_err(|_| Error::Invalid("Couldn't encrypt."))?;
    Ok([MAGIC, &nonce, &sealed].concat())
}

pub fn open(key: &[u8; 32], data: &[u8]) -> Result<Vec<u8>> {
    let wrong = Error::Invalid("A file in the folder was made with a different sync code.");
    let rest = data.strip_prefix(MAGIC).ok_or(Error::Invalid(
        "A file in the folder isn't one of Firstlight's.",
    ))?;
    if rest.len() < 24 + 16 {
        return Err(wrong);
    }
    let (nonce, sealed) = rest.split_at(24);
    XChaCha20Poly1305::new(key.into())
        .decrypt(XNonce::from_slice(nonce), sealed)
        .map_err(|_| wrong)
}

/// Canonical JSON (keys sorted) so the same content is the same text on every computer.
fn canonical(json: &str) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(json)
        .ok()
        .map(|v| v.to_string())
}

/// This computer's shareable records.
pub fn local_records(store: &Store) -> Result<Records> {
    let mut out = Records::new();
    for t in crate::todo::all_records(store)? {
        if let Ok(json) = serde_json::to_string(&t) {
            out.insert(
                format!("task:{}", t.id),
                Rec {
                    json: canonical(&json).unwrap_or(json),
                    ts: Some(t.updated_at.timestamp()),
                },
            );
        }
    }
    for (key, text, at) in store.all_meeting_notes()? {
        let json = serde_json::json!({ "text": text }).to_string();
        out.insert(
            format!("note:{key}"),
            Rec {
                json,
                ts: Some(at.timestamp()),
            },
        );
    }
    for (id, json) in store.all_own_events()? {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&json) else {
            continue;
        };
        if !v.get("remote").is_none_or(serde_json::Value::is_null) {
            continue; // lives in the Calendar app or a CalDAV calendar: not ours to share
        }
        out.insert(
            format!("event:{id}"),
            Rec {
                json: v.to_string(),
                ts: None,
            },
        );
    }
    // Templates and days off one by one, so two computers' lists combine instead of one replacing the other.
    let s = store.load_settings();
    for t in &s.templates {
        if let Ok(json) = serde_json::to_string(t) {
            out.insert(
                format!("template:{}", t.name.to_lowercase()),
                Rec {
                    json: canonical(&json).unwrap_or(json),
                    ts: None,
                },
            );
        }
    }
    for t in &s.day.time_off {
        if let Ok(json) = serde_json::to_string(t) {
            out.insert(
                format!("timeoff:{}:{}", t.from, t.to),
                Rec {
                    json: canonical(&json).unwrap_or(json),
                    ts: None,
                },
            );
        }
    }
    Ok(out)
}

/// Makes the changes in the database. A record that cannot be read is skipped (never allowed to stop the rest).
pub fn apply_changes(store: &mut Store, changes: &[Change], now: DateTime<Utc>) -> Result<usize> {
    let mut applied = 0;
    let mut events_changed = false;
    for c in changes {
        let (key, rec) = match c {
            Change::Set(k, r) => (k.as_str(), Some(r)),
            Change::Delete(k) => (k.as_str(), None),
        };
        let done = if let Some(id) = key.strip_prefix("task:") {
            match rec {
                Some(r) => match serde_json::from_str::<Task>(&r.json) {
                    Ok(t) if t.id == id && task_within_limits(store, &t) => {
                        crate::todo::put(store, &t).is_ok()
                    }
                    _ => false,
                },
                None => store.delete_task(id).is_ok(),
            }
        } else if let Some(k) = key.strip_prefix("note:") {
            match rec {
                Some(r) => {
                    let text = serde_json::from_str::<serde_json::Value>(&r.json)
                        .ok()
                        .and_then(|v| v["text"].as_str().map(str::to_string));
                    let at =
                        r.ts.and_then(|t| DateTime::from_timestamp(t, 0))
                            .unwrap_or(now);
                    match text.map(|t| crate::notes::clean_note(&t)) {
                        Some(Ok(Some(t))) => store.set_meeting_note(k, Some(&t), at).is_ok(),
                        Some(Ok(None)) => store.set_meeting_note(k, None, now).is_ok(),
                        _ => false,
                    }
                }
                None => store.set_meeting_note(k, None, now).is_ok(),
            }
        } else if let Some(id) = key.strip_prefix("event:") {
            // Never touch a record here that lives in the Calendar app or CalDAV.
            let here_remote = store
                .get_own_event(id)?
                .and_then(|j| serde_json::from_str::<serde_json::Value>(&j).ok())
                .is_some_and(|v| !v.get("remote").is_none_or(serde_json::Value::is_null));
            if here_remote {
                false
            } else {
                events_changed = true;
                match rec {
                    Some(r) if crate::local::valid_shared_record(&r.json, now) => {
                        store.put_own_event(id, &r.json).is_ok()
                    }
                    Some(_) => false,
                    None => store.delete_own_event(id).is_ok(),
                }
            }
        } else if let Some(name) = key.strip_prefix("template:") {
            let mut st = store.load_settings();
            st.templates.retain(|t| t.name.to_lowercase() != name);
            let ok = match rec {
                Some(r) => match serde_json::from_str::<crate::settings::EventTemplate>(&r.json) {
                    Ok(t) => {
                        st.templates.push(t);
                        true
                    }
                    Err(_) => false,
                },
                None => true,
            };
            st.sanitize();
            ok && store.save_settings(&st).is_ok()
        } else if key.starts_with("timeoff:") {
            let mut st = store.load_settings();
            let ok = match rec {
                Some(r) => match serde_json::from_str::<crate::day::TimeOff>(&r.json) {
                    Ok(t) => {
                        st.day
                            .time_off
                            .retain(|x| !(x.from == t.from && x.to == t.to));
                        st.day.time_off.push(t);
                        true
                    }
                    Err(_) => false,
                },
                None => {
                    st.day
                        .time_off
                        .retain(|x| key != format!("timeoff:{}:{}", x.from, x.to));
                    true
                }
            };
            st.sanitize();
            ok && store.save_settings(&st).is_ok()
        } else {
            false
        };
        applied += usize::from(done);
    }
    if events_changed {
        crate::local::refresh(store, now)?;
    }
    Ok(applied)
}

/// This computer's id (made once, kept in the database; never exported).
pub fn device_id(store: &Store) -> Result<String> {
    if let Some(id) = store
        .get_setting(DEVICE_KEY)?
        .and_then(|v| v.as_str().map(str::to_string))
    {
        return Ok(id);
    }
    let mut b = [0u8; 8];
    getrandom::fill(&mut b).map_err(|_| Error::Invalid("Couldn't name this computer."))?;
    let id: String = b.iter().map(|x| format!("{x:02x}")).collect();
    store.set_setting(DEVICE_KEY, &serde_json::Value::String(id.clone()))?;
    Ok(id)
}

/// Another computer seen in the folder.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Peer {
    pub name: String,
    pub written_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct Report {
    /// Changes made here from the other computers.
    pub applied: usize,
    pub peers: Vec<Peer>,
    /// Files that could not be read (made with another sync code, damaged).
    pub unreadable: usize,
}

fn folder(base: &Path) -> PathBuf {
    base.join(SUBFOLDER)
}

/// The same limits as a task typed here (title, notes, the size of the list), for one from another computer.
fn task_within_limits(store: &Store, t: &Task) -> bool {
    use crate::tasks::{MAX_NOTES, MAX_TASKS, MAX_TITLE};
    let title = t.title.trim();
    let fits = !title.is_empty()
        && title.chars().count() <= MAX_TITLE
        && !title.chars().any(char::is_control)
        && t.notes
            .as_ref()
            .is_none_or(|n| n.chars().count() <= MAX_NOTES);
    let room = store.get_task(&t.id).ok().flatten().is_some()
        || store
            .all_tasks()
            .map(|a| a.len() < MAX_TASKS)
            .unwrap_or(false);
    fits && room
}

/// Whether the folder already holds a file this key cannot open (someone else's code, or this computer is joining).
pub fn needs_code(base: &Path, key: Option<&[u8; 32]>, own: &str) -> bool {
    let Ok(entries) = std::fs::read_dir(folder(base)) else {
        return false;
    };
    entries
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == EXT))
        .filter(|p| p.file_stem().is_some_and(|s| s != own))
        .any(|p| {
            if std::fs::metadata(&p).map_or(true, |m| m.len() > MAX_FILE) {
                return false;
            }
            let Ok(data) = std::fs::read(&p) else {
                return false;
            };
            key.is_none_or(|k| open(k, &data).is_err())
        })
}

/// One round: read every other computer's file, merge it in, then write ours if it changed.
pub fn sync(
    store: &mut Store,
    base: &Path,
    key: &[u8; 32],
    name: &str,
    now: DateTime<Utc>,
) -> Result<Report> {
    let dir = folder(base);
    std::fs::create_dir_all(&dir)
        .map_err(|_| Error::Invalid("Couldn't use that folder. Check it still exists."))?;
    let me = device_id(store)?;
    let mut local = local_records(store)?;
    let mut report = Report::default();
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .map_err(|_| Error::Invalid("Couldn't read that folder."))?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.extension().is_some_and(|x| x == EXT)
                && p.file_stem().is_some_and(|s| s != me.as_str())
        })
        .collect();
    files.sort();
    for path in files {
        let ok_size = std::fs::metadata(&path)
            .map(|m| m.len() <= MAX_FILE)
            .unwrap_or(false);
        let snap = ok_size
            .then(|| std::fs::read(&path).ok())
            .flatten()
            .and_then(|data| open(key, &data).ok())
            .and_then(|plain| serde_json::from_slice::<Snapshot>(&plain).ok());
        let Some(snap) = snap else {
            report.unreadable += 1;
            continue;
        };
        let base_key = format!("folder_sync_base:{}", snap.device);
        // A computer read for the first time is compared with what this one last put in the folder: a record we
        // published and have since deleted stays deleted, instead of coming back from their copy of it.
        let seen: foldersync::Base = store
            .get_setting(&base_key)?
            .or(store.get_setting(PUBLISHED_KEY)?)
            .and_then(|v| serde_json::from_value(v).ok())
            .unwrap_or_default();
        let changes = foldersync::merge(&local, &snap.records, &seen);
        if !changes.is_empty() {
            report.applied += apply_changes(store, &changes, now)?;
            // What the database holds now (a record it refused stays as it was).
            local = local_records(store)?;
        }
        store.set_setting(
            &base_key,
            &serde_json::to_value(foldersync::base_of(&snap.records)).unwrap_or_default(),
        )?;
        report.peers.push(Peer {
            name: snap.name.chars().take(80).collect(),
            written_at: snap.written_at,
        });
    }
    // Ours, only when it changed (so the cloud service is not asked to upload the same thing every few minutes).
    let body = serde_json::to_string(&local)
        .map_err(|_| Error::Invalid("Couldn't write the sync file."))?;
    let print = foldersync::fingerprint(&body);
    let last = store
        .get_setting(WRITTEN_KEY)?
        .and_then(|v| v.as_str().map(str::to_string));
    let own_path = dir.join(format!("{me}.{EXT}"));
    if last.as_deref() != Some(print.as_str()) || !own_path.exists() {
        let snap = Snapshot {
            device: me.clone(),
            name: name.chars().take(80).collect(),
            written_at: now,
            records: local,
        };
        let plain = serde_json::to_vec(&snap)
            .map_err(|_| Error::Invalid("Couldn't write the sync file."))?;
        let sealed = seal(key, &plain)?;
        let tmp = dir.join(format!(".{me}.tmp"));
        std::fs::write(&tmp, sealed)
            .and_then(|_| std::fs::rename(&tmp, &own_path))
            .map_err(|_| Error::Invalid("Couldn't write to that folder."))?;
        store.set_setting(WRITTEN_KEY, &serde_json::Value::String(print))?;
        store.set_setting(
            PUBLISHED_KEY,
            &serde_json::to_value(foldersync::base_of(&snap.records)).unwrap_or_default(),
        )?;
    }
    Ok(report)
}

#[cfg(test)]
mod tests;
