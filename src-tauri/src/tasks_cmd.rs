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
    let t = todo::set_done(&store, &id, done, crate::local_tz(), Utc::now())
        .map_err(user_input_error)?;
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

/// Moves tasks to another day (`YYYY-MM-DD`), keeping each one's time of day.
#[tauri::command]
pub async fn move_tasks(
    app: AppHandle,
    state: State<'_, AppState>,
    ids: Vec<String>,
    date: String,
) -> CmdResult<()> {
    let date = chrono::NaiveDate::parse_from_str(date.trim(), "%Y-%m-%d")
        .map_err(|_| "Choose a valid day.".to_string())?;
    if ids.is_empty() || ids.len() > 200 {
        return Err("Choose some tasks to move.".into());
    }
    let (store, tz, now) = (lock_store(&state)?, crate::local_tz(), Utc::now());
    for id in &ids {
        todo::move_to(&store, id, date, tz, now).map_err(user_input_error)?;
    }
    drop(store);
    changed(&app);
    Ok(())
}

#[derive(serde::Serialize)]
pub struct DayStatus {
    /// The wrap-up moment has come (the briefing shows its wrap-up card until midnight).
    wrap_up: bool,
    /// Meetings leave too little room for lunch today.
    lunch: Option<firstlight_core::day::LunchWarning>,
    /// In-person meetings the meeting before leaves too little time to get to.
    travel: Vec<firstlight_core::day::TravelWarning>,
    /// It is Friday afternoon or the weekend: the briefing shows the week's review.
    review: bool,
    /// Trip times from Apple Maps can be offered (macOS).
    maps_available: bool,
}

/// What the briefing needs to know about the rest of the day beyond meetings.
#[tauri::command]
pub async fn get_day_status(state: State<'_, AppState>) -> CmdResult<DayStatus> {
    use firstlight_core::briefing::day_bounds;
    let (now, tz) = (Utc::now(), crate::local_tz());
    let store = lock_store(&state)?;
    let settings = store.load_settings();
    let (from, to) = day_bounds(now.with_timezone(&tz).date_naive(), tz);
    let events = firstlight_core::dedupe::dedupe(
        store
            .events_between(from, to)
            .map_err(internal("loading events"))?,
    );
    Ok(DayStatus {
        wrap_up: firstlight_core::day::wrap_up_time(now, tz, &settings.day),
        lunch: firstlight_core::day::lunch_check(
            &events,
            now,
            tz,
            &settings.day,
            &settings.briefing,
        ),
        travel: firstlight_core::day::travel_warnings(
            &events,
            now,
            tz,
            &settings.day,
            &settings.briefing,
            // Only trip times already known: the page never waits on Apple Maps.
            &crate::maps::trip_times(&events, now, tz, &settings.day, &settings.briefing, false),
        ),
        maps_available: crate::maps::AVAILABLE,
        review: settings.day.weekly_review && {
            use chrono::{Datelike, Timelike};
            let local = now.with_timezone(&tz);
            matches!(local.weekday(), chrono::Weekday::Sat | chrono::Weekday::Sun)
                || (local.weekday() == chrono::Weekday::Fri && local.hour() >= 12)
        },
    })
}

/// Meetings that ended in the last three hours with notes written for them, to follow up on.
#[tauri::command]
pub async fn get_follow_ups(
    state: State<'_, AppState>,
) -> CmdResult<Vec<firstlight_core::followup::FollowUp>> {
    let store = lock_store(&state)?;
    let bs = store.load_settings().briefing;
    firstlight_core::meeting_flow::follow_ups(&store, Utc::now(), crate::local_tz(), &bs)
        .map_err(internal("loading follow-ups"))
}

/// This week so far (Monday to now) next to the same days last week: meeting time, focus time, tasks done.
#[tauri::command]
pub async fn get_week_review(
    state: State<'_, AppState>,
) -> CmdResult<firstlight_core::review::WeekReview> {
    let store = lock_store(&state)?;
    let bs = store.load_settings().briefing;
    firstlight_core::meeting_flow::week_review(&store, Utc::now(), crate::local_tz(), &bs)
        .map_err(internal("working out the week"))
}

/// A standup ("daily") or weekly update, written from your tasks and calendar. Nothing is sent anywhere.
#[tauri::command]
pub async fn get_status_report(
    state: State<'_, AppState>,
    kind: firstlight_core::report::ReportKind,
) -> CmdResult<firstlight_core::report::Report> {
    let store = lock_store(&state)?;
    let bs = store.load_settings().briefing;
    firstlight_core::meeting_flow::status_report(&store, kind, Utc::now(), crate::local_tz(), &bs)
        .map_err(internal("writing the update"))
}

fn day_arg(s: &str) -> Result<chrono::NaiveDate, String> {
    chrono::NaiveDate::parse_from_str(s.trim(), "%Y-%m-%d")
        .map_err(|_| "Choose a valid day.".to_string())
}

/// Meeting and focus time between two days (`YYYY-MM-DD`, at most three months).
#[tauri::command]
pub async fn get_time_report(
    state: State<'_, AppState>,
    from: String,
    to: String,
) -> CmdResult<firstlight_core::timereport::TimeReport> {
    let (from, to) = (day_arg(&from)?, day_arg(&to)?);
    let store = lock_store(&state)?;
    let bs = store.load_settings().briefing;
    firstlight_core::meeting_flow::time_report(&store, from, to, Utc::now(), crate::local_tz(), &bs)
        .map_err(user_input_error)
}

/// Saves the time report as a spreadsheet file (CSV) in Downloads and shows it in the file manager.
#[tauri::command]
pub async fn export_time_report(
    app: AppHandle,
    state: State<'_, AppState>,
    from: String,
    to: String,
) -> CmdResult<String> {
    use tauri::Manager;
    use tauri_plugin_opener::OpenerExt;
    let (from_d, to_d) = (day_arg(&from)?, day_arg(&to)?);
    let report = {
        let store = lock_store(&state)?;
        let bs = store.load_settings().briefing;
        firstlight_core::meeting_flow::time_report(
            &store,
            from_d,
            to_d,
            Utc::now(),
            crate::local_tz(),
            &bs,
        )
        .map_err(user_input_error)?
    };
    let dir = app
        .path()
        .download_dir()
        .map_err(|_| "Couldn't find your Downloads folder.".to_string())?;
    let path =
        crate::commands::unique_path(&dir, &format!("firstlight-time-{from_d}-to-{to_d}"), "csv");
    std::fs::write(&path, firstlight_core::timereport::csv(&report))
        .map_err(|_| "Couldn't save the file.".to_string())?;
    let _ = app.opener().reveal_item_in_dir(&path);
    Ok(path.display().to_string())
}
