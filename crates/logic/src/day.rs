//! The person's working day beyond meetings: task reminders, and (later) the end-of-day wrap-up, breaks, lunch and
//! the stop-work reminder. Settings here are part of the one settings document (`settings.day`).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DaySettings {
    /// Notify when a task with a time comes due.
    pub task_reminders: bool,
}

impl Default for DaySettings {
    fn default() -> Self {
        Self {
            task_reminders: true,
        }
    }
}
