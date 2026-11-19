//! Firstlight core: platform-independent calendar logic.
//!
//! This crate must never depend on Tauri. Storage, secrets and network access
//! are reached through traits or plain types so the same logic can later run
//! behind a web front end (see PLAN.md, section 3).

pub mod ai;
pub mod crash;
pub mod erase;
#[cfg(test)]
mod fixture_briefing;
#[cfg(test)]
mod fuzz;
pub mod http;
pub mod invite;
pub mod legacy;
pub mod local;
pub mod mail;
pub mod oauth;
pub mod profile;
pub mod providers;
pub mod secrets;
pub mod settings;
pub mod store;
pub mod sync;
pub mod updates;

pub use firstlight_logic::{Error, Result};
pub use firstlight_logic::{
    ask, briefing, conflict, contain, dedupe, error, ics, join, model, notes, prep, tz, weekahead,
    xml,
};
pub use sync::SyncReport;
