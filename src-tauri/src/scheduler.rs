//! Background sync: on launch, on unlock, on a timer, and on demand.
//!
//! Runs on its own thread with its own database connection, so a slow server never
//! blocks the UI. Failing accounts back off; accounts whose credentials were revoked
//! are left alone until the user reconnects (a manual refresh retries them once).

use crate::providers::{Deps, provider_for};
use chrono::{DateTime, Duration, Utc};
use firstlight_core::model::AccountStatus;
use firstlight_core::secrets::Redacted;
use firstlight_core::store::Store;
use firstlight_core::sync::{Backoff, default_window, sync_account};
use serde::Serialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::time::Duration as StdDuration;
use tauri::{AppHandle, Emitter};

/// Unlocks within this window of the last sync do not trigger another one.
const UNLOCK_COOLDOWN: Duration = Duration::minutes(2);
const TICK: StdDuration = StdDuration::from_secs(30);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trigger {
    Launch,
    Unlock,
    Manual,
}

/// What the UI is told after each round. Contains no content.
#[derive(Debug, Clone, Serialize)]
pub struct SyncSummary {
    pub ok: bool,
    pub offline: bool,
    pub needs_attention: bool,
}

#[derive(Clone)]
pub struct SyncHandle(Sender<Trigger>);

impl SyncHandle {
    pub fn trigger(&self, t: Trigger) {
        let _ = self.0.send(t);
    }
}

struct Entry {
    backoff: Backoff,
    next_due: DateTime<Utc>,
    last_run: Option<DateTime<Utc>>,
}

pub fn spawn(app: AppHandle, db_path: PathBuf, key: Redacted, deps: Deps) -> SyncHandle {
    let (tx, rx) = mpsc::channel::<Trigger>();
    std::thread::Builder::new()
        .name("sync".into())
        .spawn(move || {
            let mut store = match Store::open(&db_path, &key) {
                Ok(s) => s,
                Err(e) => {
                    log::error!("sync thread cannot open the database: {e}");
                    return;
                }
            };
            let mut entries: HashMap<String, Entry> = HashMap::new();
            loop {
                let trigger = match rx.recv_timeout(TICK) {
                    Ok(t) => Some(t),
                    Err(RecvTimeoutError::Timeout) => None,
                    Err(RecvTimeoutError::Disconnected) => break,
                };
                // A bug triggered by a hostile feed fails this round, never the app (see `contained`).
                let round = firstlight_core::contain::contained(|| {
                    run_round(&mut store, &deps, &mut entries, trigger)
                });
                if round.is_none() {
                    log::error!("a sync round panicked and was contained");
                }
                if let Some(summary) = round.flatten() {
                    let _ = app.emit("sync-finished", summary);
                }
            }
        })
        .expect("failed to start the sync thread");
    SyncHandle(tx)
}

fn interval(store: &Store) -> Duration {
    Duration::minutes(
        store
            .load_settings()
            .general
            .sync_interval_minutes
            .clamp(1, 1440),
    )
}

/// One pass over all accounts. Returns a summary if anything was synced.
fn run_round(
    store: &mut Store,
    deps: &Deps,
    entries: &mut HashMap<String, Entry>,
    trigger: Option<Trigger>,
) -> Option<SyncSummary> {
    let now = Utc::now();
    let normal = interval(store);
    let accounts = store.list_accounts().ok()?;
    entries.retain(|id, _| accounts.iter().any(|a| &a.id == id));

    let mut summary = SyncSummary {
        ok: true,
        offline: false,
        needs_attention: false,
    };
    let mut ran = false;

    for account in accounts {
        let entry = entries.entry(account.id.clone()).or_insert(Entry {
            backoff: Backoff::default(),
            next_due: now,
            last_run: None,
        });
        let due = match trigger {
            Some(Trigger::Launch | Trigger::Manual) => true,
            Some(Trigger::Unlock) => entry.last_run.is_none_or(|t| now - t >= UNLOCK_COOLDOWN),
            None => now >= entry.next_due,
        };
        // Revoked accounts only retry when the user explicitly asks.
        if !due
            || (account.status == AccountStatus::NeedsAttention && trigger != Some(Trigger::Manual))
        {
            continue;
        }
        let Some(provider) = provider_for(account.provider, deps) else {
            continue;
        };

        let report = sync_account(store, provider.as_ref(), &account, &default_window(now));
        ran = true;
        entry.last_run = Some(now);
        entry.backoff.on_result(&report);
        entry.next_due = match entry.backoff.next_delay(normal, report.needs_attention) {
            Some(d) => now + d,
            None => now + Duration::days(3650),
        };
        summary.ok &= report.is_clean();
        summary.offline |= report.offline;
        summary.needs_attention |= report.needs_attention;
        log::info!(
            "synced account {} trigger={:?} ok={} failed={} offline={} attention={}",
            account.id,
            trigger,
            report.calendars_ok,
            report.calendars_failed,
            report.offline,
            report.needs_attention
        );
        if report.offline {
            break; // everything else would fail the same way
        }
    }
    ran.then_some(summary)
}
