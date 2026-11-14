//! Calendar sources. Every source implements [`Provider`].
//!
//! The trait is synchronous on purpose: the shell calls it from worker
//! threads, which keeps `crates/core` free of any async runtime choice.

use crate::Result;
use crate::model::{Account, Calendar, Event, ProviderKind};
use chrono::{DateTime, Utc};

pub mod caldav;
pub mod fixture;
pub mod google;
pub mod ics;
pub mod microsoft;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Capabilities {
    pub incremental_sync: bool,
    pub email: bool,
    pub needs_polling: bool,
}

/// The time range a sync should cover.
#[derive(Debug, Clone, Copy)]
pub struct Window {
    pub from: DateTime<Utc>,
    pub to: DateTime<Utc>,
}

#[derive(Debug)]
pub struct SyncResult {
    /// Events to insert or update.
    pub changed: Vec<Event>,
    /// Remote IDs that no longer exist.
    pub deleted: Vec<String>,
    /// Opaque token to pass to the next sync; `None` if the provider has no incremental sync.
    pub next_state: Option<String>,
    /// True when `changed` is the complete set (a full sync), so anything missing is gone.
    pub full: bool,
}

pub trait Provider: Send + Sync {
    fn kind(&self) -> ProviderKind;
    fn capabilities(&self) -> Capabilities;
    fn list_calendars(&self, account: &Account) -> Result<Vec<Calendar>>;
    fn sync_events(
        &self,
        calendar: &Calendar,
        state: Option<&str>,
        window: &Window,
    ) -> Result<SyncResult>;
}

/// A stable short id for a remote identifier (calendar ids contain characters we'd rather not store in ours).
pub(crate) fn short_hash(s: &str) -> String {
    format!(
        "{:016x}",
        s.bytes()
            .fold(0xcbf2_9ce4_8422_2325u64, |h, b| (h ^ u64::from(b))
                .wrapping_mul(0x0100_0000_01b3))
    )
}
