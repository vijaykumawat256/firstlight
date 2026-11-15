//! The time range a sync should cover.

use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Copy)]
pub struct Window {
    pub from: DateTime<Utc>,
    pub to: DateTime<Utc>,
}
