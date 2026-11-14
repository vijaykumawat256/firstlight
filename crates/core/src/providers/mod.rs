//! Calendar sources. Every source implements [`Provider`].
//!
//! The trait is synchronous on purpose: the shell calls it from worker
//! threads, which keeps `crates/core` free of any async runtime choice.

use crate::Result;
use crate::model::{Account, Calendar, Event, ProviderKind};
use chrono::{DateTime, Utc};

pub mod fixture;
pub mod ics;

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
