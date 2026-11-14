//! Encrypted local database (SQLite + SQLCipher).

use crate::model::*;
use crate::secrets::Redacted;
use crate::{Error, Result};
use chrono::{DateTime, NaiveDate, Utc};
use rusqlite::{Connection, OptionalExtension, params};
use std::path::Path;

mod migrations;

pub struct Store {
    conn: Connection,
}

impl Store {
    /// Opens (creating if needed) an encrypted database. `key` is the 64-char hex key.
    pub fn open(path: &Path, key: &Redacted) -> Result<Self> {
        Self::init(Connection::open(path)?, key)
    }

    pub fn open_in_memory(key: &Redacted) -> Result<Self> {
        Self::init(Connection::open_in_memory()?, key)
    }

    fn init(conn: Connection, key: &Redacted) -> Result<Self> {
        let k = key.expose();
        if k.len() != 64 || !k.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(Error::Invalid("database key must be 64 hex characters"));
        }
        // Raw-key syntax: the key is used as-is, no passphrase KDF needed.
        conn.execute_batch(&format!("PRAGMA key = \"x'{k}'\";"))?;
        // A wrong key (or a plaintext/corrupt file) only fails on first read.
        conn.query_row("SELECT count(*) FROM sqlite_master", [], |r| {
            r.get::<_, i64>(0)
        })
        .map_err(|_| Error::DatabaseLocked)?;
        conn.execute_batch("PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL;")?;
        let mut store = Self { conn };
        migrations::run(&mut store.conn)?;
        Ok(store)
    }

    // ---- accounts -------------------------------------------------------

    pub fn upsert_account(&self, a: &Account) -> Result<()> {
        self.conn.execute(
            "INSERT INTO accounts (id, provider, display_name, email, status, secret_ref, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(id) DO UPDATE SET provider=?2, display_name=?3, email=?4, status=?5, secret_ref=?6",
            params![
                a.id,
                a.provider.as_str(),
                a.display_name,
                a.email,
                match a.status {
                    AccountStatus::Ok => "ok",
                    AccountStatus::NeedsAttention => "needs_attention",
                },
                a.secret_ref,
                a.created_at.timestamp()
            ],
        )?;
        Ok(())
    }

    pub fn list_accounts(&self) -> Result<Vec<Account>> {
        let mut st = self.conn.prepare(
            "SELECT id, provider, display_name, email, status, secret_ref, created_at FROM accounts ORDER BY created_at, id",
        )?;
        let rows = st.query_map([], |r| {
            Ok(Account {
                id: r.get(0)?,
                provider: ProviderKind::parse(&r.get::<_, String>(1)?)
                    .unwrap_or(ProviderKind::Fixture),
                display_name: r.get(2)?,
                email: r.get(3)?,
                status: if r.get::<_, String>(4)? == "ok" {
                    AccountStatus::Ok
                } else {
                    AccountStatus::NeedsAttention
                },
                secret_ref: r.get(5)?,
                created_at: DateTime::from_timestamp(r.get(6)?, 0).unwrap_or_default(),
            })
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    // ---- calendars ------------------------------------------------------

    pub fn upsert_calendar(&self, c: &Calendar) -> Result<()> {
        self.conn.execute(
            "INSERT INTO calendars (id, account_id, remote_id, name, colour, visible, read_only)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(id) DO UPDATE SET name=?4, read_only=?7",
            params![
                c.id,
                c.account_id,
                c.remote_id,
                c.name,
                c.colour,
                c.visible,
                c.read_only
            ],
        )?;
        Ok(())
    }

    pub fn list_calendars(&self) -> Result<Vec<Calendar>> {
        let mut st = self
            .conn
            .prepare("SELECT id, account_id, remote_id, name, colour, visible, read_only FROM calendars ORDER BY name, id")?;
        let rows = st.query_map([], |r| {
            Ok(Calendar {
                id: r.get(0)?,
                account_id: r.get(1)?,
                remote_id: r.get(2)?,
                name: r.get(3)?,
                colour: r.get(4)?,
                visible: r.get(5)?,
                read_only: r.get(6)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    // ---- events ---------------------------------------------------------

    /// Replaces every cached event of a calendar atomically.
    pub fn replace_events(&mut self, calendar_id: &str, events: &[Event]) -> Result<()> {
        let tx = self.conn.transaction()?;
        tx.execute("DELETE FROM events WHERE calendar_id = ?1", [calendar_id])?;
        {
            let mut ins = tx.prepare(
                "INSERT INTO events (id, calendar_id, remote_id, ical_uid, title, all_day,
                    start_utc, end_utc, start_tzid, start_date, end_date, location, description,
                    join_url, status, my_response, organizer, recurrence_master_id, updated_at)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19)",
            )?;
            for e in events {
                if e.calendar_id != calendar_id {
                    return Err(Error::Invalid("event belongs to a different calendar"));
                }
                let (all_day, s, en, tz, sd, ed) = match &e.when {
                    When::Timed { start, end, tzid } => (
                        false,
                        Some(start.timestamp()),
                        Some(end.timestamp()),
                        tzid.clone(),
                        None,
                        None,
                    ),
                    When::AllDay { start, end } => (
                        true,
                        None,
                        None,
                        None,
                        Some(start.to_string()),
                        Some(end.to_string()),
                    ),
                };
                ins.execute(params![
                    e.id,
                    e.calendar_id,
                    e.remote_id,
                    e.ical_uid,
                    e.title,
                    all_day,
                    s,
                    en,
                    tz,
                    sd,
                    ed,
                    e.location,
                    e.description,
                    e.join_url,
                    e.status.as_str(),
                    e.my_response.as_str(),
                    e.organizer,
                    e.recurrence_master_id,
                    e.updated_at.timestamp()
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// Events from visible calendars touching `[from, to)`, ordered by start.
    /// Timed events are filtered exactly; all-day events by UTC date (the UI refines).
    pub fn events_between(&self, from: DateTime<Utc>, to: DateTime<Utc>) -> Result<Vec<Event>> {
        if to <= from {
            return Err(Error::Invalid("range end must be after start"));
        }
        let mut st = self.conn.prepare(
            "SELECT e.id, e.calendar_id, e.remote_id, e.ical_uid, e.title, e.all_day, e.start_utc, e.end_utc,
                    e.start_tzid, e.start_date, e.end_date, e.location, e.description, e.join_url,
                    e.status, e.my_response, e.organizer, e.recurrence_master_id, e.updated_at
             FROM events e JOIN calendars c ON c.id = e.calendar_id
             WHERE c.visible = 1
             ORDER BY COALESCE(e.start_utc, CAST(strftime('%s', e.start_date) AS INTEGER)), e.title, e.id",
        )?;
        let rows = st.query_map([], |r| {
            let all_day: bool = r.get(5)?;
            let when = if all_day {
                let parse =
                    |s: String| NaiveDate::parse_from_str(&s, "%Y-%m-%d").unwrap_or_default();
                When::AllDay {
                    start: parse(r.get(9)?),
                    end: parse(r.get(10)?),
                }
            } else {
                When::Timed {
                    start: DateTime::from_timestamp(r.get(6)?, 0).unwrap_or_default(),
                    end: DateTime::from_timestamp(r.get(7)?, 0).unwrap_or_default(),
                    tzid: r.get(8)?,
                }
            };
            Ok(Event {
                id: r.get(0)?,
                calendar_id: r.get(1)?,
                remote_id: r.get(2)?,
                ical_uid: r.get(3)?,
                title: r.get(4)?,
                when,
                location: r.get(11)?,
                description: r.get(12)?,
                join_url: r.get(13)?,
                status: EventStatus::parse(&r.get::<_, String>(14)?)
                    .unwrap_or(EventStatus::Confirmed),
                my_response: Response::parse(&r.get::<_, String>(15)?).unwrap_or(Response::None),
                organizer: r.get(16)?,
                recurrence_master_id: r.get(17)?,
                updated_at: DateTime::from_timestamp(r.get(18)?, 0).unwrap_or_default(),
            })
        })?;
        let mut out = Vec::new();
        for ev in rows {
            let ev = ev?;
            if ev.overlaps(from, to) {
                out.push(ev);
            }
        }
        Ok(out)
    }

    // ---- sync state -----------------------------------------------------

    pub fn set_sync_state(
        &self,
        calendar_id: &str,
        token: Option<&str>,
        error: Option<&str>,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT INTO sync_state (calendar_id, token, last_synced_at, last_error) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(calendar_id) DO UPDATE SET token=?2, last_synced_at=?3, last_error=?4",
            params![calendar_id, token, Utc::now().timestamp(), error],
        )?;
        Ok(())
    }

    pub fn sync_token(&self, calendar_id: &str) -> Result<Option<String>> {
        Ok(self
            .conn
            .query_row(
                "SELECT token FROM sync_state WHERE calendar_id = ?1",
                [calendar_id],
                |r| r.get(0),
            )
            .optional()?
            .flatten())
    }

    // ---- settings -------------------------------------------------------

    pub fn get_setting(&self, key: &str) -> Result<Option<serde_json::Value>> {
        let raw: Option<String> = self
            .conn
            .query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| {
                r.get(0)
            })
            .optional()?;
        Ok(raw.and_then(|s| serde_json::from_str(&s).ok()))
    }

    pub fn set_setting(&self, key: &str, value: &serde_json::Value) -> Result<()> {
        self.conn.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value=?2",
            params![key, value.to_string()],
        )?;
        Ok(())
    }

    // ---- briefing log ---------------------------------------------------

    /// Records that something happened (`kind` is a short code such as "unlock"). Drives the
    /// "first unlock of the day" rule in Phase 3.
    pub fn log_briefing(&self, date: NaiveDate, kind: &str, at: DateTime<Utc>) -> Result<()> {
        self.conn.execute(
            "INSERT INTO briefing_log (date, kind, shown_at) VALUES (?1, ?2, ?3)",
            params![date.to_string(), kind, at.timestamp()],
        )?;
        Ok(())
    }

    pub fn count_briefing_log(&self, date: NaiveDate, kind: &str) -> Result<i64> {
        Ok(self.conn.query_row(
            "SELECT count(*) FROM briefing_log WHERE date = ?1 AND kind = ?2",
            params![date.to_string(), kind],
            |r| r.get(0),
        )?)
    }

    pub fn schema_version(&self) -> Result<i64> {
        Ok(self
            .conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::{Provider, Window, fixture::FixtureProvider};
    use crate::secrets::{MemorySecretStore, load_or_create_db_key};
    use chrono::TimeZone;

    fn key() -> Redacted {
        load_or_create_db_key(&MemorySecretStore::default()).unwrap()
    }

    fn seeded() -> Store {
        let mut s = Store::open_in_memory(&key()).unwrap();
        let today = NaiveDate::from_ymd_opt(2026, 3, 10).unwrap();
        let p = FixtureProvider::new(today, "UTC".parse().unwrap());
        let acct = FixtureProvider::account(Utc::now());
        s.upsert_account(&acct).unwrap();
        let w = Window {
            from: Utc.with_ymd_and_hms(2026, 3, 1, 0, 0, 0).unwrap(),
            to: Utc.with_ymd_and_hms(2026, 4, 1, 0, 0, 0).unwrap(),
        };
        for c in p.list_calendars(&acct).unwrap() {
            s.upsert_calendar(&c).unwrap();
            let r = p.sync_events(&c, None, &w).unwrap();
            s.replace_events(&c.id, &r.changed).unwrap();
        }
        s
    }

    #[test]
    fn migrations_reach_latest_version() {
        assert_eq!(
            seeded().schema_version().unwrap(),
            migrations::latest_version()
        );
    }

    #[test]
    fn events_round_trip_and_order_by_start() {
        let s = seeded();
        let from = Utc.with_ymd_and_hms(2026, 3, 10, 0, 0, 0).unwrap();
        let evs = s.events_between(from, from + Duration::days(1)).unwrap();
        assert!(evs.iter().any(|e| e.remote_id == "design-review"));
        assert!(
            !evs.iter().any(|e| e.remote_id == "flight"),
            "tomorrow excluded"
        );
        let starts: Vec<_> = evs
            .iter()
            .map(|e| match &e.when {
                When::Timed { start, .. } => start.timestamp(),
                When::AllDay { start, .. } => {
                    start.and_hms_opt(0, 0, 0).unwrap().and_utc().timestamp()
                }
            })
            .collect();
        let mut sorted = starts.clone();
        sorted.sort();
        assert_eq!(starts, sorted);
    }

    #[test]
    fn hidden_calendars_are_excluded() {
        let s = seeded();
        s.conn
            .execute(
                "UPDATE calendars SET visible = 0 WHERE id = 'fixture-personal'",
                [],
            )
            .unwrap();
        let from = Utc.with_ymd_and_hms(2026, 3, 10, 0, 0, 0).unwrap();
        let evs = s.events_between(from, from + Duration::days(1)).unwrap();
        assert!(evs.iter().all(|e| e.calendar_id != "fixture-personal"));
        assert!(!evs.is_empty());
    }

    #[test]
    fn replace_is_atomic_and_rejects_foreign_events() {
        let mut s = seeded();
        let from = Utc.with_ymd_and_hms(2026, 3, 10, 0, 0, 0).unwrap();
        let before = s
            .events_between(from, from + Duration::days(2))
            .unwrap()
            .len();
        let mut bad = s
            .events_between(from, from + Duration::days(1))
            .unwrap()
            .remove(0);
        bad.calendar_id = "other".into();
        assert!(s.replace_events("fixture-work", &[bad]).is_err());
        let after = s
            .events_between(from, from + Duration::days(2))
            .unwrap()
            .len();
        assert_eq!(before, after, "failed replace must roll back");
    }

    #[test]
    fn inverted_range_is_rejected() {
        let s = seeded();
        let t = Utc::now();
        assert!(s.events_between(t, t).is_err());
    }

    #[test]
    fn settings_and_sync_state_round_trip() {
        let s = seeded();
        s.set_setting("theme", &serde_json::json!("dark")).unwrap();
        assert_eq!(
            s.get_setting("theme").unwrap(),
            Some(serde_json::json!("dark"))
        );
        assert_eq!(s.get_setting("missing").unwrap(), None);
        assert_eq!(s.sync_token("fixture-work").unwrap(), None);
        s.set_sync_state("fixture-work", Some("tok"), None).unwrap();
        assert_eq!(
            s.sync_token("fixture-work").unwrap().as_deref(),
            Some("tok")
        );
    }

    #[test]
    fn briefing_log_counts_per_day_and_kind() {
        let s = seeded();
        let d = NaiveDate::from_ymd_opt(2026, 3, 10).unwrap();
        s.log_briefing(d, "unlock", Utc::now()).unwrap();
        s.log_briefing(d, "unlock", Utc::now()).unwrap();
        assert_eq!(s.count_briefing_log(d, "unlock").unwrap(), 2);
        assert_eq!(s.count_briefing_log(d, "shown").unwrap(), 0);
    }

    #[test]
    fn database_file_is_unreadable_without_the_key() {
        let dir = std::env::temp_dir().join(format!("daybrief-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("db.sqlite");
        let k = key();
        {
            let s = Store::open(&path, &k).unwrap();
            s.set_setting("marker", &serde_json::json!("PLAINTEXT-MARKER-12345"))
                .unwrap();
        }
        // Correct key reopens fine.
        assert!(Store::open(&path, &k).is_ok());
        // Wrong key is refused.
        assert!(matches!(
            Store::open(&path, &key()),
            Err(Error::DatabaseLocked)
        ));
        // Plain SQLite (no key) cannot read it either.
        let plain = Connection::open(&path).unwrap();
        assert!(
            plain
                .query_row("SELECT count(*) FROM sqlite_master", [], |r| r
                    .get::<_, i64>(0))
                .is_err()
        );
        // And the marker never appears in the raw bytes, including the WAL file.
        for entry in std::fs::read_dir(&dir).unwrap() {
            let bytes = std::fs::read(entry.unwrap().path()).unwrap();
            assert!(!bytes.windows(22).any(|w| w == b"PLAINTEXT-MARKER-12345"));
        }
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn rejects_malformed_keys() {
        assert!(matches!(
            Store::open_in_memory(&Redacted::new("abc")),
            Err(Error::Invalid(_))
        ));
    }

    use chrono::Duration;
}
