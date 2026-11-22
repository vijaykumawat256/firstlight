//! Sharing between your own computers through a folder you already sync (iCloud Drive, OneDrive, Dropbox, Google
//! Drive): the rules. Each computer writes only its own file (a snapshot of its records); each reads the others' and
//! merges them record by record against what it saw from that computer last time (the "base"):
//!
//! - same on both sides: nothing to do;
//! - changed only there: take theirs (a record gone there is deleted here);
//! - changed only here: keep ours (the other computer takes it from our file);
//! - changed on both: the newer one wins when the record has a time (tasks, notes), else a fixed rule both computers
//!   agree on (the larger fingerprint), so they always end up the same; a change beats a deletion, so nothing typed is
//!   lost to a conflict.
//!
//! Records are keyed "task:<id>", "note:<key>", "event:<id>", "template:<name>", "timeoff:<from>:<to>". Files, encryption and the database are in
//! `firstlight-core` (`folder_sync`).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// One record: its JSON (canonical, so the same content is the same text everywhere) and, when it has one, the time it
/// was last changed (seconds), to settle a conflict.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rec {
    pub json: String,
    #[serde(default)]
    pub ts: Option<i64>,
}

pub type Records = BTreeMap<String, Rec>;
/// What was seen from one computer last time: record key to fingerprint.
pub type Base = BTreeMap<String, String>;

/// One computer's file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Snapshot {
    /// A random id for the computer (also its file name).
    pub device: String,
    /// The computer's name, to show ("Vijay's MacBook").
    pub name: String,
    pub written_at: DateTime<Utc>,
    pub records: Records,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
    Set(String, Rec),
    Delete(String),
}

/// A short, stable fingerprint of a record's text (64-bit FNV-1a, as hex): the same on every computer and version.
pub fn fingerprint(json: &str) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in json.as_bytes() {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{h:016x}")
}

pub fn base_of(records: &Records) -> Base {
    records
        .iter()
        .map(|(k, r)| (k.clone(), fingerprint(&r.json)))
        .collect()
}

/// The starting point for a device met for the first time, when no copy of it has been merged before.
///
/// It is what this device last published, so a record we published and have since deleted stays deleted instead of
/// coming back from their copy. But a record we still have and they don't is left out: they have simply never seen
/// it (a new phone or computer joining), so it must be kept and handed to them, never taken as deleted there.
pub fn first_contact_base(published: &Base, local: &Records, remote: &Records) -> Base {
    published
        .iter()
        .filter(|(k, _)| remote.contains_key(*k) || !local.contains_key(*k))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect()
}

/// Which side wins when both changed: the newer time, else the larger fingerprint (the same answer on both computers).
fn theirs_win(ours: &Rec, theirs: &Rec) -> bool {
    match (ours.ts, theirs.ts) {
        (Some(a), Some(b)) if a != b => b > a,
        _ => fingerprint(&theirs.json) > fingerprint(&ours.json),
    }
}

/// The changes to make here, given our records, another computer's, and what we saw from it last time.
pub fn merge(local: &Records, remote: &Records, base: &Base) -> Vec<Change> {
    let keys: BTreeSet<&String> = local
        .keys()
        .chain(remote.keys())
        .chain(base.keys())
        .collect();
    let mut out = Vec::new();
    for key in keys {
        let l = local.get(key);
        let r = remote.get(key);
        let b = base.get(key);
        let lf = l.map(|x| fingerprint(&x.json));
        let rf = r.map(|x| fingerprint(&x.json));
        if lf == rf || rf.as_ref() == b {
            // Same, or unchanged there (any change here travels in our file).
            continue;
        }
        let take = if lf.as_ref() == b {
            true // changed only there
        } else {
            match (l, r) {
                (Some(ours), Some(theirs)) => theirs_win(ours, theirs),
                (None, Some(_)) => true, // deleted here, changed there: keep the change
                (Some(_), None) => false, // changed here, deleted there: keep ours
                (None, None) => false,
            }
        };
        if take {
            out.push(match r {
                Some(rec) => Change::Set(key.clone(), rec.clone()),
                None => Change::Delete(key.clone()),
            });
        }
    }
    out
}

/// Applies changes to a set of records (what the database now holds, so the file written next matches it).
pub fn apply(records: &mut Records, changes: &[Change]) {
    for c in changes {
        match c {
            Change::Set(k, r) => {
                records.insert(k.clone(), r.clone());
            }
            Change::Delete(k) => {
                records.remove(k);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(json: &str, ts: Option<i64>) -> Rec {
        Rec {
            json: json.into(),
            ts,
        }
    }
    fn recs(list: &[(&str, &str, Option<i64>)]) -> Records {
        list.iter()
            .map(|(k, j, t)| (k.to_string(), rec(j, *t)))
            .collect()
    }

    #[test]
    fn meeting_a_device_for_the_first_time_keeps_what_only_we_have_and_what_we_deleted_stays_gone()
    {
        // We published task:1 and task:2, then deleted task:2. A new device arrives with task:9 and an old copy of
        // task:2 (it had it from somewhere else).
        let published = base_of(&recs(&[
            ("task:1", "mine", Some(1)),
            ("task:2", "gone", Some(1)),
        ]));
        let local = recs(&[("task:1", "mine", Some(1))]);
        let remote = recs(&[("task:2", "gone", Some(1)), ("task:9", "theirs", Some(2))]);
        let base = first_contact_base(&published, &local, &remote);
        let c = merge(&local, &remote, &base);
        // task:1 is ours and they have never seen it: kept (no change here). task:9 comes over. task:2 stays deleted.
        assert_eq!(c.len(), 1, "{c:?}");
        assert!(
            matches!(&c[0], Change::Set(key, _) if key == "task:9"),
            "{c:?}"
        );
    }

    #[test]
    fn the_fingerprint_is_fixed() {
        assert_eq!(fingerprint(""), "cbf29ce484222325");
        assert_eq!(fingerprint("a"), "af63dc4c8601ec8c");
        assert_ne!(fingerprint("{\"a\":1}"), fingerprint("{\"a\":2}"));
    }

    #[test]
    fn new_and_changed_records_come_over_and_ours_stay() {
        let base = base_of(&recs(&[
            ("task:1", "v1", Some(1)),
            ("task:2", "x", Some(1)),
        ]));
        let local = recs(&[("task:1", "v1", Some(1)), ("task:2", "x-mine", Some(5))]);
        let remote = recs(&[
            ("task:1", "v2", Some(3)),
            ("task:2", "x", Some(1)),
            ("task:3", "new", Some(4)),
        ]);
        let c = merge(&local, &remote, &base);
        assert_eq!(
            c,
            vec![
                Change::Set("task:1".into(), rec("v2", Some(3))),
                Change::Set("task:3".into(), rec("new", Some(4)))
            ]
        );
    }

    #[test]
    fn a_deletion_there_deletes_here_but_never_beats_a_change() {
        let base = base_of(&recs(&[("note:a", "x", Some(1)), ("note:b", "y", Some(1))]));
        let local = recs(&[("note:a", "x", Some(1)), ("note:b", "y-edited", Some(9))]);
        let remote = Records::new();
        let c = merge(&local, &remote, &base);
        assert_eq!(
            c,
            vec![Change::Delete("note:a".into())],
            "a was untouched here, so its deletion comes over; b was edited, so it stays"
        );
        // Deleted here, changed there: the change comes back.
        let local = Records::new();
        let remote = recs(&[("note:a", "x2", Some(5))]);
        assert_eq!(
            merge(&local, &remote, &base),
            vec![Change::Set("note:a".into(), rec("x2", Some(5)))]
        );
        // Deleted here, untouched there: nothing (our file tells them).
        let remote = recs(&[("note:a", "x", Some(1))]);
        assert!(merge(&local, &remote, &base).is_empty());
    }

    #[test]
    fn a_conflict_goes_to_the_newer_and_both_computers_agree() {
        let base = base_of(&recs(&[("task:1", "v0", Some(1))]));
        let a = recs(&[("task:1", "from A", Some(10))]);
        let b = recs(&[("task:1", "from B", Some(20))]);
        assert_eq!(
            merge(&a, &b, &base),
            vec![Change::Set("task:1".into(), rec("from B", Some(20)))]
        );
        assert!(merge(&b, &a, &base).is_empty());
        // Without times (own events), the same fixed rule on both sides.
        let base = base_of(&recs(&[("event:1", "v0", None)]));
        let a = recs(&[("event:1", "A", None)]);
        let b = recs(&[("event:1", "B", None)]);
        let a_takes = !merge(&a, &b, &base).is_empty();
        let b_takes = !merge(&b, &a, &base).is_empty();
        assert!(a_takes != b_takes, "exactly one side gives way");
    }

    #[test]
    fn two_computers_end_up_the_same_after_each_reads_the_other() {
        // A and B start from the same base, both edit, then sync both ways twice.
        let start = recs(&[
            ("task:1", "t1", Some(1)),
            ("task:2", "t2", Some(1)),
            ("note:x", "n", Some(1)),
        ]);
        let mut a = start.clone();
        let mut b = start.clone();
        let (mut base_a_sees_b, mut base_b_sees_a) = (base_of(&start), base_of(&start));
        a.insert("task:1".into(), rec("t1 by A", Some(5)));
        a.remove("note:x");
        b.insert("task:2".into(), rec("t2 by B", Some(6)));
        b.insert("task:3".into(), rec("t3 by B", Some(6)));
        for _ in 0..2 {
            let from_b = b.clone();
            let c = merge(&a, &from_b, &base_a_sees_b);
            apply(&mut a, &c);
            base_a_sees_b = base_of(&from_b);
            let from_a = a.clone();
            let c = merge(&b, &from_a, &base_b_sees_a);
            apply(&mut b, &c);
            base_b_sees_a = base_of(&from_a);
        }
        assert_eq!(a, b);
        assert_eq!(a.get("task:1").unwrap().json, "t1 by A");
        assert_eq!(a.get("task:2").unwrap().json, "t2 by B");
        assert!(a.contains_key("task:3") && !a.contains_key("note:x"));
    }
}
