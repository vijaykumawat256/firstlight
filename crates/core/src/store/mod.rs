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
        // WAL lets the UI read while a background sync writes; busy_timeout covers brief write overlaps.
        conn.execute_batch(
            "PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL; PRAGMA busy_timeout = 5000;",
        )?;
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
        for e in events {
            if e.calendar_id != calendar_id {
                return Err(Error::Invalid("event belongs to a different calendar"));
            }
            insert_event(&tx, e)?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Events from visible calendars touching `[from, to)`, ordered by start.
    /// Timed events are filtered exactly; all-day events by UTC date (the UI refines).
    /// One event by id, whether or not its calendar is shown.
    pub fn get_event(&self, id: &str) -> Result<Option<Event>> {
        let sql = format!("SELECT {EVENT_COLUMNS} FROM events e WHERE e.id = ?1");
        match self.conn.query_row(&sql, [id], event_from_row) {
            Ok(e) => Ok(Some(e)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    pub fn events_between(&self, from: DateTime<Utc>, to: DateTime<Utc>) -> Result<Vec<Event>> {
        if to <= from {
            return Err(Error::Invalid("range end must be after start"));
        }
        let mut st = self.conn.prepare(
            "SELECT e.id, e.calendar_id, e.remote_id, e.ical_uid, e.title, e.all_day, e.start_utc, e.end_utc,
                    e.start_tzid, e.start_date, e.end_date, e.location, e.description, e.join_url,
                    e.status, e.my_response, e.organizer, e.recurrence_master_id, e.updated_at, e.attendees
             FROM events e JOIN calendars c ON c.id = e.calendar_id
             WHERE c.visible = 1
             ORDER BY COALESCE(e.start_utc, CAST(strftime('%s', e.start_date) AS INTEGER)), e.title, e.id",
        )?;
        let rows = st.query_map([], event_from_row)?;
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

    /// When something of this kind last appeared, if ever.
    pub fn last_log_at(&self, kind: &str) -> Result<Option<DateTime<Utc>>> {
        let t: Option<i64> = self.conn.query_row(
            "SELECT max(shown_at) FROM briefing_log WHERE kind = ?1",
            [kind],
            |r| r.get(0),
        )?;
        Ok(t.and_then(|t| DateTime::from_timestamp(t, 0)))
    }

    /// The state the unlock rules need: the date of the last full briefing and the last automatic appearance.
    pub fn rule_state(&self, tz: chrono_tz::Tz) -> Result<crate::briefing::rules::RuleState> {
        let full = self.last_log_at("full")?;
        let up_next = self.last_log_at("upnext")?;
        Ok(crate::briefing::rules::RuleState {
            last_full_date: full.map(|t| t.with_timezone(&tz).date_naive()),
            last_shown_at: full.max(up_next),
        })
    }

    pub fn count_briefing_log(&self, date: NaiveDate, kind: &str) -> Result<i64> {
        Ok(self.conn.query_row(
            "SELECT count(*) FROM briefing_log WHERE date = ?1 AND kind = ?2",
            params![date.to_string(), kind],
            |r| r.get(0),
        )?)
    }

    /// Ids of notifications already shown, so a restart never repeats one.
    pub fn sent_notifications(&self) -> Result<std::collections::HashSet<String>> {
        let mut st = self.conn.prepare("SELECT id FROM notifications_sent")?;
        let rows = st.query_map([], |r| r.get::<_, String>(0))?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    pub fn mark_notification_sent(&self, id: &str, at: DateTime<Utc>) -> Result<()> {
        self.conn.execute(
            "INSERT OR IGNORE INTO notifications_sent (id, sent_at) VALUES (?1, ?2)",
            params![id, at.timestamp()],
        )?;
        Ok(())
    }

    /// Forgets old records; ids embed dates and start times, so they can never match again.
    pub fn prune_notifications(&self, older_than: DateTime<Utc>) -> Result<()> {
        self.conn.execute(
            "DELETE FROM notifications_sent WHERE sent_at < ?1",
            [older_than.timestamp()],
        )?;
        Ok(())
    }

    pub fn schema_version(&self) -> Result<i64> {
        Ok(self
            .conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))?)
    }
}

/// Cached sync bookkeeping for one calendar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncInfo {
    pub token: Option<String>,
    pub last_synced_at: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
}

const EVENT_COLUMNS: &str =
    "e.id, e.calendar_id, e.remote_id, e.ical_uid, e.title, e.all_day, e.start_utc, e.end_utc,
                    e.start_tzid, e.start_date, e.end_date, e.location, e.description, e.join_url,
                    e.status, e.my_response, e.organizer, e.recurrence_master_id, e.updated_at, e.attendees";

/// One row of `EVENT_COLUMNS` as an [`Event`].
fn event_from_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Event> {
    let all_day: bool = r.get(5)?;
    let when = if all_day {
        let parse = |s: String| NaiveDate::parse_from_str(&s, "%Y-%m-%d").unwrap_or_default();
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
        status: EventStatus::parse(&r.get::<_, String>(14)?).unwrap_or(EventStatus::Confirmed),
        my_response: Response::parse(&r.get::<_, String>(15)?).unwrap_or(Response::None),
        organizer: r.get(16)?,
        recurrence_master_id: r.get(17)?,
        updated_at: DateTime::from_timestamp(r.get(18)?, 0).unwrap_or_default(),
        also_in: Vec::new(),
        attendees: r
            .get::<_, Option<String>>(19)?
            .and_then(|j| serde_json::from_str(&j).ok())
            .unwrap_or_default(),
    })
}

/// The guest list as stored: JSON, or nothing when there is none.
fn attendees_json(a: &[Attendee]) -> Option<String> {
    if a.is_empty() {
        return None;
    }
    serde_json::to_string(&a[..a.len().min(MAX_ATTENDEES)]).ok()
}

fn insert_event(conn: &Connection, e: &Event) -> Result<()> {
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
    conn.execute(
        "INSERT OR REPLACE INTO events (id, calendar_id, remote_id, ical_uid, title, all_day,
            start_utc, end_utc, start_tzid, start_date, end_date, location, description,
            join_url, status, my_response, organizer, recurrence_master_id, updated_at, attendees)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20)",
        params![
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
            e.updated_at.timestamp(),
            attendees_json(&e.attendees),
        ],
    )?;
    Ok(())
}

impl Store {
    /// Every account with its calendars and the freshest sync state, for the Accounts screen.
    pub fn account_views(&self) -> Result<Vec<AccountView>> {
        let calendars = self.list_calendars()?;
        let mut out = Vec::new();
        for a in self.list_accounts()? {
            let cals: Vec<Calendar> = calendars
                .iter()
                .filter(|c| c.account_id == a.id)
                .cloned()
                .collect();
            let mut last_synced_at = None;
            let mut last_error = None;
            for c in &cals {
                if let Some(info) = self.sync_info(&c.id)? {
                    last_synced_at = last_synced_at.max(info.last_synced_at);
                    last_error = last_error.or(info.last_error);
                }
            }
            out.push(AccountView {
                id: a.id,
                provider: a.provider,
                display_name: a.display_name,
                email: a.email,
                status: a.status,
                calendars: cals,
                last_synced_at,
                last_error,
            });
        }
        Ok(out)
    }

    pub fn get_account(&self, id: &str) -> Result<Option<Account>> {
        Ok(self.list_accounts()?.into_iter().find(|a| a.id == id))
    }

    pub fn set_account_status(&self, id: &str, status: AccountStatus) -> Result<()> {
        let s = match status {
            AccountStatus::Ok => "ok",
            AccountStatus::NeedsAttention => "needs_attention",
        };
        self.conn.execute(
            "UPDATE accounts SET status = ?2 WHERE id = ?1",
            params![id, s],
        )?;
        Ok(())
    }

    pub fn rename_account(&self, id: &str, name: &str) -> Result<()> {
        let name = name.trim();
        if name.is_empty() || name.chars().count() > 80 {
            return Err(Error::Invalid("name must be 1-80 characters"));
        }
        self.conn.execute(
            "UPDATE accounts SET display_name = ?2 WHERE id = ?1",
            params![id, name],
        )?;
        Ok(())
    }

    /// Deletes an account and, by cascade, its calendars, cached events and sync state.
    pub fn remove_account(&self, id: &str) -> Result<()> {
        self.conn
            .execute("DELETE FROM accounts WHERE id = ?1", [id])?;
        if id == crate::local::ACCOUNT_ID {
            // Their own events live in two places; removing the account removes both.
            self.conn.execute("DELETE FROM own_events", [])?;
        }
        Ok(())
    }

    // ---- the user's own events (see `local`) --------------------------------------------

    pub fn put_own_event(&self, id: &str, json: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO own_events (id, json) VALUES (?1, ?2)
             ON CONFLICT(id) DO UPDATE SET json = excluded.json",
            params![id, json],
        )?;
        Ok(())
    }

    pub fn get_own_event(&self, id: &str) -> Result<Option<String>> {
        match self
            .conn
            .query_row("SELECT json FROM own_events WHERE id = ?1", [id], |r| {
                r.get(0)
            }) {
            Ok(v) => Ok(Some(v)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    pub fn all_own_events(&self) -> Result<Vec<(String, String)>> {
        let mut st = self
            .conn
            .prepare("SELECT id, json FROM own_events ORDER BY id")?;
        let rows = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// For tests of failure handling: makes every later write of an own event fail.
    #[cfg(test)]
    pub fn drop_own_events_table_for_tests(&self) {
        self.conn.execute_batch("DROP TABLE own_events").unwrap();
    }

    pub fn delete_own_event(&self, id: &str) -> Result<()> {
        self.conn
            .execute("DELETE FROM own_events WHERE id = ?1", [id])?;
        Ok(())
    }

    // ---- the to-do list (see `todo`) ----------------------------------------------------

    pub fn put_task(&self, id: &str, json: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO tasks (id, json) VALUES (?1, ?2)
             ON CONFLICT(id) DO UPDATE SET json = excluded.json",
            params![id, json],
        )?;
        Ok(())
    }

    pub fn get_task(&self, id: &str) -> Result<Option<String>> {
        Ok(self
            .conn
            .query_row("SELECT json FROM tasks WHERE id = ?1", [id], |r| r.get(0))
            .optional()?)
    }

    pub fn all_tasks(&self) -> Result<Vec<(String, String)>> {
        let mut st = self
            .conn
            .prepare("SELECT id, json FROM tasks ORDER BY id")?;
        let rows = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    pub fn delete_task(&self, id: &str) -> Result<()> {
        self.conn.execute("DELETE FROM tasks WHERE id = ?1", [id])?;
        Ok(())
    }

    // ---- private meeting notes (see `notes`) ---------------------------------------------

    /// The note kept for a meeting, and when it was last changed.
    pub fn get_meeting_note(&self, key: &str) -> Result<Option<(String, DateTime<Utc>)>> {
        match self.conn.query_row(
            "SELECT text, updated_at FROM meeting_notes WHERE key = ?1",
            [key],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)),
        ) {
            Ok((text, at)) => Ok(Some((
                text,
                DateTime::from_timestamp(at, 0).unwrap_or_default(),
            ))),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Keeps a note for a meeting; an empty note (`None`) removes it.
    pub fn set_meeting_note(
        &self,
        key: &str,
        text: Option<&str>,
        now: DateTime<Utc>,
    ) -> Result<()> {
        match text {
            Some(text) => self.conn.execute(
                "INSERT INTO meeting_notes (key, text, updated_at) VALUES (?1, ?2, ?3)
                 ON CONFLICT(key) DO UPDATE SET text = excluded.text, updated_at = excluded.updated_at",
                params![key, text, now.timestamp()],
            )?,
            None => self
                .conn
                .execute("DELETE FROM meeting_notes WHERE key = ?1", [key])?,
        };
        Ok(())
    }

    /// Every meeting note: key, text, when it was last changed.
    pub fn all_meeting_notes(&self) -> Result<Vec<(String, String, DateTime<Utc>)>> {
        let mut stmt = self
            .conn
            .prepare("SELECT key, text, updated_at FROM meeting_notes")?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
            ))
        })?;
        Ok(rows
            .filter_map(|r| r.ok())
            .map(|(k, t, at)| (k, t, DateTime::from_timestamp(at, 0).unwrap_or_default()))
            .collect())
    }

    pub fn remove_calendar(&self, id: &str) -> Result<()> {
        self.conn
            .execute("DELETE FROM calendars WHERE id = ?1", [id])?;
        Ok(())
    }

    pub fn calendars_for_account(&self, account_id: &str) -> Result<Vec<Calendar>> {
        Ok(self
            .list_calendars()?
            .into_iter()
            .filter(|c| c.account_id == account_id)
            .collect())
    }

    pub fn set_calendar_visible(&self, id: &str, visible: bool) -> Result<()> {
        self.conn.execute(
            "UPDATE calendars SET visible = ?2 WHERE id = ?1",
            params![id, visible],
        )?;
        Ok(())
    }

    pub fn set_calendar_colour(&self, id: &str, colour: &str) -> Result<()> {
        if !crate::model::CALENDAR_COLOURS.contains(&colour) {
            return Err(Error::Invalid("unknown colour"));
        }
        self.conn.execute(
            "UPDATE calendars SET colour = ?2 WHERE id = ?1",
            params![id, colour],
        )?;
        Ok(())
    }

    /// Inserts or updates events without touching the rest of the calendar (incremental sync).
    pub fn upsert_events(&mut self, calendar_id: &str, events: &[Event]) -> Result<()> {
        let tx = self.conn.transaction()?;
        for e in events {
            if e.calendar_id != calendar_id {
                return Err(Error::Invalid("event belongs to a different calendar"));
            }
            insert_event(&tx, e)?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn delete_events_by_remote_id(
        &mut self,
        calendar_id: &str,
        remote_ids: &[String],
    ) -> Result<()> {
        let tx = self.conn.transaction()?;
        for r in remote_ids {
            tx.execute(
                "DELETE FROM events WHERE calendar_id = ?1 AND remote_id = ?2",
                params![calendar_id, r],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Records a failed sync without disturbing the stored token (so the next try can resume).
    pub fn set_sync_error(&self, calendar_id: &str, error: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO sync_state (calendar_id, token, last_synced_at, last_error) VALUES (?1, NULL, NULL, ?2)
             ON CONFLICT(calendar_id) DO UPDATE SET last_error = ?2",
            params![calendar_id, error],
        )?;
        Ok(())
    }

    pub fn sync_info(&self, calendar_id: &str) -> Result<Option<SyncInfo>> {
        Ok(self
            .conn
            .query_row(
                "SELECT token, last_synced_at, last_error FROM sync_state WHERE calendar_id = ?1",
                [calendar_id],
                |r| {
                    Ok(SyncInfo {
                        token: r.get(0)?,
                        last_synced_at: r
                            .get::<_, Option<i64>>(1)?
                            .and_then(|t| DateTime::from_timestamp(t, 0)),
                        last_error: r.get(2)?,
                    })
                },
            )
            .optional()?)
    }

    /// The user's settings. Never fails: missing, damaged or old data falls back to defaults, and the
    /// scattered keys used before the unified document existed are migrated on first read.
    pub fn load_settings(&self) -> crate::settings::AppSettings {
        use crate::settings::{AppSettings, SETTINGS_KEY};
        if let Ok(Some(v)) = self.get_setting(SETTINGS_KEY) {
            return AppSettings::from_value(&v);
        }
        let get = |k: &str| self.get_setting(k).ok().flatten();
        let mut legacy = serde_json::Map::new();
        for (old, new) in [
            ("briefing", "briefing"),
            ("unlock_rules", "unlock"),
            ("notify", "notify"),
        ] {
            if let Some(v) = get(old) {
                legacy.insert(new.into(), v);
            }
        }
        if let Some(m) = get("sync_interval_minutes") {
            legacy.insert(
                "general".into(),
                serde_json::json!({ "sync_interval_minutes": m }),
            );
        }
        AppSettings::from_value(&serde_json::Value::Object(legacy))
    }

    pub fn save_settings(&self, s: &crate::settings::AppSettings) -> Result<()> {
        let v =
            serde_json::to_value(s).map_err(|_| Error::Invalid("settings could not be saved"))?;
        self.set_setting(crate::settings::SETTINGS_KEY, &v)
    }

    /// Deletes every account, calendar, event, log entry and setting, then compacts the file so the
    /// removed rows are physically gone. The (empty) database and its key remain.
    pub fn erase_all(&self) -> Result<()> {
        self.conn.execute_batch(
            "DELETE FROM accounts;      -- cascades to calendars, events and sync state
             DELETE FROM briefing_log;
             DELETE FROM notifications_sent;
             DELETE FROM own_events;
             DELETE FROM meeting_notes;
             DELETE FROM tasks;
             DELETE FROM settings;",
        )?;
        self.conn
            .execute_batch("VACUUM; PRAGMA wal_checkpoint(TRUNCATE);")?;
        Ok(())
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
    fn a_guest_list_survives_the_database_and_an_event_without_one_reads_as_empty() {
        let mut s = seeded();
        let mut e = s
            .events_between(
                Utc.with_ymd_and_hms(2026, 3, 1, 0, 0, 0).unwrap(),
                Utc.with_ymd_and_hms(2026, 4, 1, 0, 0, 0).unwrap(),
            )
            .unwrap()
            .remove(0);
        assert!(e.attendees.is_empty(), "the fixture has no guest list");
        let mut list = Vec::new();
        add_guest(
            &mut list,
            "Priya@Example.com",
            Some("Priya"),
            Response::Accepted,
            true,
        );
        add_guest(
            &mut list,
            "sam@example.com",
            None,
            Response::NeedsAction,
            false,
        );
        e.attendees = list.clone();
        let cal = e.calendar_id.clone();
        s.upsert_events(&cal, std::slice::from_ref(&e)).unwrap();
        assert_eq!(s.get_event(&e.id).unwrap().unwrap().attendees, list);
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
        let dir = std::env::temp_dir().join(format!("firstlight-test-{}", uuid::Uuid::new_v4()));
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
    fn account_views_combine_calendars_and_sync_state() {
        let s = seeded();
        s.set_sync_state("fixture-work", None, None).unwrap();
        s.set_sync_error("fixture-personal", "offline").unwrap();
        let views = s.account_views().unwrap();
        assert_eq!(views.len(), 1);
        assert_eq!(views[0].calendars.len(), 2);
        assert!(views[0].last_synced_at.is_some());
        assert_eq!(views[0].last_error.as_deref(), Some("offline"));
    }

    #[test]
    fn removing_an_account_cascades_to_everything() {
        let s = seeded();
        s.remove_account("fixture-account").unwrap();
        assert!(s.list_calendars().unwrap().is_empty());
        let from = Utc.with_ymd_and_hms(2026, 3, 1, 0, 0, 0).unwrap();
        assert!(
            s.events_between(from, from + Duration::days(60))
                .unwrap()
                .is_empty()
        );
        assert!(s.sync_info("fixture-work").unwrap().is_none());
    }

    #[test]
    fn colour_and_name_validation() {
        let s = seeded();
        assert!(s.set_calendar_colour("fixture-work", "pink").is_ok());
        assert!(s.set_calendar_colour("fixture-work", "#ff0000").is_err());
        assert!(s.rename_account("fixture-account", "  ").is_err());
        assert!(
            s.rename_account("fixture-account", &"x".repeat(81))
                .is_err()
        );
        assert!(s.rename_account("fixture-account", "Mine").is_ok());
    }

    #[test]
    fn settings_load_defaults_save_and_reload() {
        use crate::settings::{AppSettings, DefaultView};
        let s = seeded();
        assert_eq!(s.load_settings(), AppSettings::default());
        let mut custom = AppSettings::default();
        custom.general.default_view = DefaultView::Week;
        custom.notify.before_minutes = 20;
        s.save_settings(&custom).unwrap();
        assert_eq!(s.load_settings(), custom);
    }

    #[test]
    fn settings_from_the_earlier_scattered_keys_are_migrated() {
        let s = seeded();
        s.set_setting(
            "briefing",
            &serde_json::json!({"show_declined": true, "min_gap_minutes": 45}),
        )
        .unwrap();
        s.set_setting(
            "unlock_rules",
            &serde_json::json!({"min_interval_minutes": 90}),
        )
        .unwrap();
        s.set_setting("notify", &serde_json::json!({"before_minutes": 5}))
            .unwrap();
        s.set_setting("sync_interval_minutes", &serde_json::json!(3))
            .unwrap();
        let m = s.load_settings();
        assert!(m.briefing.show_declined);
        assert_eq!(m.briefing.min_gap_minutes, 45);
        assert_eq!(m.unlock.min_interval_minutes, 90);
        assert_eq!(m.notify.before_minutes, 5);
        assert_eq!(m.general.sync_interval_minutes, 3);
        // Once the new document exists it wins over the old keys.
        s.save_settings(&crate::settings::AppSettings::default())
            .unwrap();
        assert!(!s.load_settings().briefing.show_declined);
    }

    #[test]
    fn a_corrupt_stored_document_loads_as_defaults_instead_of_failing() {
        let s = seeded();
        s.set_setting("app_settings", &serde_json::json!("garbage"))
            .unwrap();
        assert_eq!(s.load_settings(), crate::settings::AppSettings::default());
        s.set_setting("app_settings", &serde_json::json!({"general": {"sync_interval_minutes": "NaN"}, "unlock": {"first_unlock_after_hour": 7}})).unwrap();
        let l = s.load_settings();
        assert_eq!(l.general.sync_interval_minutes, 10);
        assert_eq!(l.unlock.first_unlock_after_hour, 7);
    }

    #[test]
    fn meeting_notes_are_kept_changed_and_removed() {
        let s = seeded();
        let (t1, t2) = (
            DateTime::from_timestamp(1_800_000_000, 0).unwrap(),
            DateTime::from_timestamp(1_800_000_600, 0).unwrap(),
        );
        assert_eq!(s.get_meeting_note("title:standup").unwrap(), None);
        s.set_meeting_note("title:standup", Some("Ask about the budget"), t1)
            .unwrap();
        s.set_meeting_note("title:standup", Some("Budget agreed"), t2)
            .unwrap();
        assert_eq!(
            s.get_meeting_note("title:standup").unwrap(),
            Some(("Budget agreed".into(), t2))
        );
        assert_eq!(s.get_meeting_note("title:other").unwrap(), None);
        s.set_meeting_note("title:standup", None, t2).unwrap();
        assert_eq!(s.get_meeting_note("title:standup").unwrap(), None);
    }

    #[test]
    fn erase_all_removes_everything() {
        let s = seeded();
        s.save_settings(&crate::settings::AppSettings::default())
            .unwrap();
        s.log_briefing(
            NaiveDate::from_ymd_opt(2026, 3, 10).unwrap(),
            "full",
            Utc::now(),
        )
        .unwrap();
        s.mark_notification_sent("x", Utc::now()).unwrap();
        s.set_meeting_note("title:standup", Some("private"), Utc::now())
            .unwrap();
        s.put_task("t1", "{}").unwrap();
        s.erase_all().unwrap();
        for table in [
            "accounts",
            "calendars",
            "events",
            "sync_state",
            "settings",
            "briefing_log",
            "notifications_sent",
            "own_events",
            "meeting_notes",
            "tasks",
        ] {
            let n: i64 = s
                .conn
                .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
                .unwrap();
            assert_eq!(n, 0, "{table} should be empty");
        }
        assert_eq!(s.load_settings(), crate::settings::AppSettings::default());
        // The database is still usable afterwards.
        s.upsert_account(&crate::providers::fixture::FixtureProvider::account(
            Utc::now(),
        ))
        .unwrap();
    }

    #[test]
    fn rule_state_reflects_the_log() {
        let s = seeded();
        let tz = chrono_tz::Asia::Tokyo;
        assert_eq!(
            s.rule_state(tz).unwrap(),
            crate::briefing::rules::RuleState::default()
        );
        // 23:30 UTC on the 9th is already the 10th in Tokyo.
        let full = Utc.with_ymd_and_hms(2026, 3, 9, 23, 30, 0).unwrap();
        s.log_briefing(NaiveDate::from_ymd_opt(2026, 3, 10).unwrap(), "full", full)
            .unwrap();
        let later = full + Duration::hours(3);
        s.log_briefing(
            NaiveDate::from_ymd_opt(2026, 3, 10).unwrap(),
            "upnext",
            later,
        )
        .unwrap();
        s.log_briefing(
            NaiveDate::from_ymd_opt(2026, 3, 10).unwrap(),
            "unlock",
            later + Duration::hours(9),
        )
        .unwrap();
        let st = s.rule_state(tz).unwrap();
        assert_eq!(
            st.last_full_date,
            Some(NaiveDate::from_ymd_opt(2026, 3, 10).unwrap())
        );
        assert_eq!(
            st.last_shown_at,
            Some(later),
            "plain unlock log entries do not count as appearances"
        );
    }

    #[test]
    fn sent_notifications_persist_and_old_ones_are_pruned() {
        let s = seeded();
        let now = Utc.with_ymd_and_hms(2026, 3, 10, 12, 0, 0).unwrap();
        s.mark_notification_sent("a", now - Duration::days(5))
            .unwrap();
        s.mark_notification_sent("b", now).unwrap();
        s.mark_notification_sent("b", now + Duration::hours(1))
            .unwrap(); // marking twice is harmless
        assert_eq!(s.sent_notifications().unwrap().len(), 2);
        s.prune_notifications(now - Duration::days(3)).unwrap();
        let left: Vec<_> = s.sent_notifications().unwrap().into_iter().collect();
        assert_eq!(left, ["b"]);
    }

    #[test]
    fn new_ids_are_unique_hex() {
        let (a, b) = (new_id(), new_id());
        assert_ne!(a, b);
        assert_eq!(a.len(), 32);
        assert!(a.bytes().all(|c| c.is_ascii_hexdigit()));
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
