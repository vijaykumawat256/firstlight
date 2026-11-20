//! The pure part of Firstlight: no files, no network, no database, no operating system.
//! `firstlight-core` re-exports these modules under their old paths, so nothing else changed.
//! Kept free of anything a browser cannot run, so it can be built to WebAssembly later (docs/PHASE7_DESIGN.md).

pub mod ask;
pub mod briefing;
pub mod conflict;
pub mod contain;
pub mod day;
pub mod dayplan;
pub mod dedupe;
pub mod error;
pub mod followup;
pub mod ics;
pub mod join;
pub mod model;
pub mod notes;
pub mod prep;
pub mod report;
pub mod review;
pub mod tasks;
pub mod tasksync;
pub mod timereport;
pub mod tz;
pub mod weekahead;
pub mod window;
pub mod xml;

pub use error::{Error, Result};
