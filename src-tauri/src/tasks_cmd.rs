//! Commands for the to-do list. Everything is stored on this computer; nothing here touches the network.

use crate::AppState;
use crate::commands::{CmdResult, internal, lock_store, user_input_error};
use chrono::{DateTime, Utc};
use firstlight_core::tasks::{Task, TaskInput, TaskLists};
use firstlight_core::todo;
use tauri::{AppHandle, Emitter, State};

fn changed(app: &AppHandle) {
    let _ = app.emit("tasks-changed", ());
}

/// Today (with tasks carried over from earlier days), upcoming, someday and recently done.
#[tauri::command]
pub async fn list_tasks(state: State<'_, AppState>) -> CmdResult<TaskLists> {
    let store = lock_store(&state)?;
    todo::lists(&store, Utc::now(), crate::local_tz()).map_err(internal("loading tasks"))
}

#[tauri::command]
pub async fn add_task(
    app: AppHandle,
    state: State<'_, AppState>,
    task: TaskInput,
) -> CmdResult<Task> {
    let store = lock_store(&state)?;
    let t = todo::add(&store, &task, crate::local_tz(), Utc::now()).map_err(user_input_error)?;
    changed(&app);
    Ok(t)
}

#[tauri::command]
pub async fn update_task(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    task: TaskInput,
) -> CmdResult<Task> {
    let store = lock_store(&state)?;
    let t = todo::update(&store, &id, &task, crate::local_tz(), Utc::now())
        .map_err(user_input_error)?;
    changed(&app);
    Ok(t)
}

#[tauri::command]
pub async fn set_task_done(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    done: bool,
) -> CmdResult<Task> {
    let store = lock_store(&state)?;
    let t = todo::set_done(&store, &id, done, Utc::now()).map_err(user_input_error)?;
    changed(&app);
    Ok(t)
}

#[tauri::command]
pub async fn delete_task(app: AppHandle, state: State<'_, AppState>, id: String) -> CmdResult<()> {
    let store = lock_store(&state)?;
    todo::delete(&store, &id).map_err(user_input_error)?;
    changed(&app);
    Ok(())
}

/// Sets time aside for the task: an event of your own from `start` to `end` (kept on this computer).
#[tauri::command]
pub async fn plan_task(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    start: String,
    end: String,
) -> CmdResult<Task> {
    let parse = |s: &str| {
        DateTime::parse_from_rfc3339(s)
            .map(|d| d.with_timezone(&Utc))
            .map_err(|_| "Choose a valid time.".to_string())
    };
    let (start, end) = (parse(&start)?, parse(&end)?);
    let mut store = lock_store(&state)?;
    let t = todo::plan(&mut store, &id, start, end, crate::local_tz(), Utc::now())
        .map_err(user_input_error)?;
    drop(store);
    changed(&app);
    let _ = app.emit("events-changed", ());
    Ok(t)
}

/// The lines of a meeting note that could become tasks.
#[tauri::command]
pub async fn note_task_lines(text: String) -> CmdResult<Vec<String>> {
    Ok(firstlight_core::tasks::note_lines(&text))
}

/// Adds the chosen lines of a meeting note as tasks, marked with the meeting they came from.
#[tauri::command]
pub async fn add_tasks_from_note(
    app: AppHandle,
    state: State<'_, AppState>,
    titles: Vec<String>,
    meeting: Option<String>,
) -> CmdResult<Vec<Task>> {
    let store = lock_store(&state)?;
    let made = todo::add_many(
        &store,
        &titles,
        meeting.as_deref(),
        crate::local_tz(),
        Utc::now(),
    )
    .map_err(user_input_error)?;
    changed(&app);
    Ok(made)
}
