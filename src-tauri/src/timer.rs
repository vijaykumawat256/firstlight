//! The focus timer: 5 to 120 minutes on one task. Kept in memory (a restart forgets a running timer, and the time
//! so far is not counted). While it runs the notifier treats it like focus time. When it ends, or is stopped, the
//! minutes go on the task (`todo::add_spent`) and a short break is suggested.

use crate::AppState;
use crate::commands::{CmdResult, lock_store, user_input_error};
use chrono::{DateTime, Duration, Utc};
use serde::Serialize;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, State};

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct FocusTimer {
    pub task_id: String,
    pub title: String,
    pub started: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
    pub minutes: u32,
}

static TIMER: Mutex<Option<FocusTimer>> = Mutex::new(None);

/// The running timer, if any.
pub(crate) fn current() -> Option<FocusTimer> {
    TIMER.lock().ok().and_then(|t| t.clone())
}

/// Whole minutes worked so far (never more than the timer's length).
fn worked(t: &FocusTimer, now: DateTime<Utc>) -> u32 {
    let m = (now.min(t.ends_at) - t.started).num_minutes().max(0);
    u32::try_from(m).unwrap_or(0).min(t.minutes)
}

/// Takes the timer if it has run out at `now` (the notifier calls this every tick).
pub(crate) fn take_finished(now: DateTime<Utc>) -> Option<FocusTimer> {
    let mut guard = TIMER.lock().ok()?;
    if guard.as_ref().is_some_and(|t| t.ends_at <= now) {
        guard.take()
    } else {
        None
    }
}

/// Puts the worked minutes on the task.
pub(crate) fn credit(store: &firstlight_core::store::Store, t: &FocusTimer, now: DateTime<Utc>) {
    let minutes = worked(t, now);
    if minutes > 0 {
        let _ = firstlight_core::todo::add_spent(store, &t.task_id, minutes, now);
    }
}

#[tauri::command]
pub async fn get_focus_timer() -> CmdResult<Option<FocusTimer>> {
    Ok(current())
}

/// Starts a timer on a task (replacing one that runs, whose time so far is counted).
#[tauri::command]
pub async fn start_focus_timer(
    app: AppHandle,
    state: State<'_, AppState>,
    task_id: String,
    minutes: u32,
) -> CmdResult<FocusTimer> {
    if !(5..=120).contains(&minutes) {
        return Err("A focus timer runs 5 minutes to 2 hours.".into());
    }
    let now = Utc::now();
    let task = {
        let store = lock_store(&state)?;
        firstlight_core::todo::get(&store, &task_id).map_err(user_input_error)?
    };
    if task.done_at.is_some() {
        return Err("That task is already done.".into());
    }
    let timer = FocusTimer {
        task_id,
        title: task.title,
        started: now,
        ends_at: now + Duration::minutes(i64::from(minutes)),
        minutes,
    };
    let old = TIMER
        .lock()
        .map_err(|_| "The timer is busy.".to_string())?
        .replace(timer.clone());
    if let Some(old) = old {
        credit(&*lock_store(&state)?, &old, now);
    }
    log::info!("focus timer started ({minutes} min)");
    let _ = app.emit("timer-changed", ());
    let _ = app.emit("tasks-changed", ());
    Ok(timer)
}

/// Stops the timer early; the minutes so far go on the task.
#[tauri::command]
pub async fn stop_focus_timer(app: AppHandle, state: State<'_, AppState>) -> CmdResult<u32> {
    let now = Utc::now();
    let Some(t) = TIMER
        .lock()
        .map_err(|_| "The timer is busy.".to_string())?
        .take()
    else {
        return Ok(0);
    };
    credit(&*lock_store(&state)?, &t, now);
    let _ = app.emit("timer-changed", ());
    let _ = app.emit("tasks-changed", ());
    Ok(worked(&t, now))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn worked_minutes_never_go_past_the_timer() {
        let started: DateTime<Utc> = "2026-11-19T15:00:00Z".parse().unwrap();
        let t = FocusTimer {
            task_id: "t".into(),
            title: "Write".into(),
            started,
            ends_at: started + Duration::minutes(25),
            minutes: 25,
        };
        assert_eq!(worked(&t, started + Duration::seconds(59)), 0);
        assert_eq!(worked(&t, started + Duration::minutes(10)), 10);
        assert_eq!(worked(&t, started + Duration::hours(3)), 25);
        assert_eq!(worked(&t, started - Duration::minutes(5)), 0);
    }
}
