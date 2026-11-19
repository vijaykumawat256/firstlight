use crate::Result;
use rusqlite::Connection;

/// Append-only. Never edit a released migration; add a new one.
const MIGRATIONS: &[&str] = &[
    // 1: initial schema
    "CREATE TABLE accounts (
        id TEXT PRIMARY KEY,
        provider TEXT NOT NULL,
        display_name TEXT NOT NULL,
        email TEXT,
        status TEXT NOT NULL DEFAULT 'ok',
        secret_ref TEXT,
        created_at INTEGER NOT NULL
     );
     CREATE TABLE calendars (
        id TEXT PRIMARY KEY,
        account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
        remote_id TEXT NOT NULL,
        name TEXT NOT NULL,
        colour TEXT NOT NULL,
        visible INTEGER NOT NULL DEFAULT 1,
        read_only INTEGER NOT NULL DEFAULT 1
     );
     CREATE TABLE events (
        id TEXT PRIMARY KEY,
        calendar_id TEXT NOT NULL REFERENCES calendars(id) ON DELETE CASCADE,
        remote_id TEXT NOT NULL,
        ical_uid TEXT NOT NULL,
        title TEXT NOT NULL,
        all_day INTEGER NOT NULL,
        start_utc INTEGER,
        end_utc INTEGER,
        start_tzid TEXT,
        start_date TEXT,
        end_date TEXT,
        location TEXT,
        description TEXT,
        join_url TEXT,
        status TEXT NOT NULL,
        my_response TEXT NOT NULL,
        organizer TEXT,
        recurrence_master_id TEXT,
        updated_at INTEGER NOT NULL
     );
     CREATE INDEX events_calendar ON events(calendar_id);
     CREATE INDEX events_start ON events(start_utc);
     CREATE TABLE sync_state (
        calendar_id TEXT PRIMARY KEY REFERENCES calendars(id) ON DELETE CASCADE,
        token TEXT,
        last_synced_at INTEGER,
        last_error TEXT
     );
     CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);
     CREATE TABLE briefing_log (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        date TEXT NOT NULL,
        kind TEXT NOT NULL,
        shown_at INTEGER NOT NULL
     );
     CREATE INDEX briefing_log_date ON briefing_log(date, kind);",
    // 2: notifications already sent, so a restart never repeats one
    "CREATE TABLE notifications_sent (
        id TEXT PRIMARY KEY,
        sent_at INTEGER NOT NULL
     );",
    // 3: events the user adds themselves, as the record they typed (repeat rule, reminder, invitees); the
    // dated instances the app shows are generated from it into `events`
    "CREATE TABLE own_events (
        id TEXT PRIMARY KEY,
        json TEXT NOT NULL
     );",
    // 4: the guest list of an event, as the calendar reported it (JSON; names and addresses stay in the encrypted file)
    "ALTER TABLE events ADD COLUMN attendees TEXT;",
    // 5: private meeting notes, one per meeting (see `notes::meeting_key`); kept only in this encrypted file
    "CREATE TABLE meeting_notes (
        key TEXT PRIMARY KEY,
        text TEXT NOT NULL,
        updated_at INTEGER NOT NULL
     );",
];

pub fn run(conn: &mut Connection) -> Result<()> {
    let current: usize =
        conn.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))? as usize;
    for (i, sql) in MIGRATIONS.iter().enumerate().skip(current) {
        let tx = conn.transaction()?;
        tx.execute_batch(sql)?;
        tx.execute_batch(&format!("PRAGMA user_version = {};", i + 1))?;
        tx.commit()?;
    }
    Ok(())
}

#[cfg(test)]
pub fn latest_version() -> i64 {
    MIGRATIONS.len() as i64
}
