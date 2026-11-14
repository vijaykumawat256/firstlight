//! Daybrief core: platform-independent calendar logic.
//!
//! This crate must never depend on Tauri. Storage, secrets and network access
//! are reached through traits or plain types so the same logic can later run
//! behind a web front end (see PLAN.md, section 3).

pub mod dedupe;
pub mod error;
pub mod http;
pub mod ics;
pub mod join;
pub mod model;
pub mod providers;
pub mod secrets;
pub mod store;
pub mod sync;
pub mod tz;
pub mod xml;

pub use error::{Error, Result};
pub use sync::SyncReport;
