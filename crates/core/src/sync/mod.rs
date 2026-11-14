//! Keeps the local cache in step with the providers.
//!
//! [`sync_account`] does one account's sync and applies the failure rules from PLAN.md:
//! revoked credentials mark the account "needs attention" and stop (no retry loop),
//! offline failures keep the cache, and an expired sync token triggers one clean full resync.
//! [`Backoff`] decides when to try again.

use crate::model::*;
use crate::providers::{Provider, Window};
use crate::store::Store;
use crate::{Error, Result};
use chrono::{DateTime, Duration, Utc};

/// How far back and forward the cache reaches (PLAN.md section 3).
pub const DAYS_BACK: i64 = 30;
pub const DAYS_FORWARD: i64 = 90;

pub fn default_window(now: DateTime<Utc>) -> Window {
    Window {
        from: now - Duration::days(DAYS_BACK),
        to: now + Duration::days(DAYS_FORWARD),
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct SyncReport {
    pub calendars_ok: usize,
    pub calendars_failed: usize,
    /// Credentials were rejected: the user must reconnect.
    pub needs_attention: bool,
    /// Failed because the network was unreachable (cache kept).
    pub offline: bool,
}

impl SyncReport {
    pub fn is_clean(&self) -> bool {
        self.calendars_failed == 0 && !self.needs_attention && !self.offline
    }
}

/// Short, content-free codes stored in `sync_state.last_error`.
fn code(e: &Error) -> &'static str {
    match e {
        Error::AuthRevoked => "auth",
        Error::Offline => "offline",
        Error::SyncTokenExpired => "token_expired",
        _ => "failed",
    }
}

/// Syncs every calendar of one account into the store.
pub fn sync_account(
    store: &mut Store,
    provider: &dyn Provider,
    account: &Account,
    window: &Window,
) -> SyncReport {
    let mut report = SyncReport::default();

    let calendars = match provider.list_calendars(account) {
        Ok(c) => c,
        Err(e) => {
            record_account_failure(store, account, &e, &mut report);
            return report;
        }
    };

    // Remote calendars that vanished (deleted or unshared) leave the cache.
    if let Ok(existing) = store.calendars_for_account(&account.id) {
        for old in existing
            .iter()
            .filter(|c| !calendars.iter().any(|n| n.id == c.id))
        {
            let _ = store.remove_calendar(&old.id);
        }
    }

    for cal in &calendars {
        if store.upsert_calendar(cal).is_err() {
            report.calendars_failed += 1;
            continue;
        }
        match sync_calendar(store, provider, cal, window) {
            Ok(()) => report.calendars_ok += 1,
            Err(e) => {
                report.calendars_failed += 1;
                let _ = store.set_sync_error(&cal.id, code(&e));
                match e {
                    Error::AuthRevoked => {
                        // Stop: every other calendar of this account shares the credential.
                        let _ =
                            store.set_account_status(&account.id, AccountStatus::NeedsAttention);
                        report.needs_attention = true;
                        return report;
                    }
                    Error::Offline => {
                        report.offline = true;
                        return report; // the rest would fail the same way
                    }
                    _ => {}
                }
            }
        }
    }

    if report.calendars_failed == 0 && account.status == AccountStatus::NeedsAttention {
        let _ = store.set_account_status(&account.id, AccountStatus::Ok);
    }
    report
}

fn record_account_failure(store: &Store, account: &Account, e: &Error, report: &mut SyncReport) {
    report.calendars_failed += 1;
    match e {
        Error::AuthRevoked => {
            let _ = store.set_account_status(&account.id, AccountStatus::NeedsAttention);
            report.needs_attention = true;
        }
        Error::Offline => report.offline = true,
        _ => {}
    }
    if let Ok(cals) = store.calendars_for_account(&account.id) {
        for c in cals {
            let _ = store.set_sync_error(&c.id, code(e));
        }
    }
}

fn sync_calendar(
    store: &mut Store,
    provider: &dyn Provider,
    cal: &Calendar,
    window: &Window,
) -> Result<()> {
    let token = store.sync_token(&cal.id)?;
    let result = match provider.sync_events(cal, token.as_deref(), window) {
        Err(Error::SyncTokenExpired) => provider.sync_events(cal, None, window)?, // one clean full resync
        other => other?,
    };

    if result.full {
        store.replace_events(&cal.id, &result.changed)?;
    } else {
        store.upsert_events(&cal.id, &result.changed)?;
        store.delete_events_by_remote_id(&cal.id, &result.deleted)?;
    }
    store.set_sync_state(&cal.id, result.next_state.as_deref(), None)?;
    Ok(())
}

/// Exponential backoff for failing accounts: 1, 2, 4, ... minutes, capped at 30.
/// Successful syncs reset to the normal interval.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Backoff {
    pub consecutive_failures: u32,
}

impl Backoff {
    pub fn on_result(&mut self, report: &SyncReport) {
        if report.is_clean() {
            self.consecutive_failures = 0;
        } else {
            self.consecutive_failures = self.consecutive_failures.saturating_add(1);
        }
    }

    /// Delay until the next attempt. A revoked account is not retried until the user acts,
    /// signalled by `None`.
    pub fn next_delay(&self, normal: Duration, needs_attention: bool) -> Option<Duration> {
        if needs_attention {
            return None;
        }
        if self.consecutive_failures == 0 {
            return Some(normal);
        }
        let mins = 1i64 << (self.consecutive_failures - 1).min(5);
        Some(Duration::minutes(mins.min(30)).max(Duration::minutes(1)))
    }
}

#[cfg(test)]
mod tests;
