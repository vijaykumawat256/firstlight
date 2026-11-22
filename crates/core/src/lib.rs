//! Firstlight core: platform-independent calendar logic.
//!
//! This crate must never depend on Tauri. Storage, secrets and network access
//! are reached through traits or plain types so the same logic can later run
//! behind a web front end (see PLAN.md, section 3).

pub mod ai;
pub mod backup;
pub mod crash;
pub mod erase;
#[cfg(test)]
mod fixture_briefing;
pub mod folder_sync;
#[cfg(test)]
mod fuzz;
pub mod http;
pub mod invite;
pub mod legacy;
pub mod local;
pub mod mail;
pub mod meeting_flow;
pub mod oauth;
pub mod profile;
pub mod providers;
pub mod remote_tasks;
pub mod secrets;
pub mod settings;
pub mod store;
pub mod sync;
pub mod todo;
pub mod updates;

pub use firstlight_logic::{Error, Result};
pub use firstlight_logic::{
    ask, briefing, conflict, contain, day, dayplan, dedupe, error, foldersync, followup, ics, join,
    model, notes, prep, report, review, tasks, tasksync, timeoff, timereport, tz, weekahead, xml,
};
pub use sync::SyncReport;
