//! Keeping tasks in step with Google Tasks and Microsoft To Do: the permission, the commands, and the run after each
//! calendar sync. The rules are in `firstlight_logic::tasksync`; the service calls in `firstlight_core::remote_tasks`.

use crate::AppState;
use crate::commands::{CmdResult, internal, lock_store};
use crate::providers::Deps;
use chrono::Utc;
use firstlight_core::Error;
use firstlight_core::model::{Account, ProviderKind};
use firstlight_core::oauth::TokenManager;
use firstlight_core::remote_tasks::{self, GoogleTasks, MicrosoftTodo, TaskService};
use firstlight_core::settings::AppSettings;
use firstlight_core::store::Store;
use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

/// The task permission this account holds, if task sync is on for it.
pub(crate) fn scopes_held(settings: &AppSettings, account: &Account) -> Vec<&'static str> {
    if !settings
        .tasks
        .sync_accounts
        .iter()
        .any(|a| a == &account.id)
    {
        return vec![];
    }
    match account.provider {
        ProviderKind::Google => vec![remote_tasks::GOOGLE_TASKS_SCOPE],
        ProviderKind::Microsoft => vec![remote_tasks::MS_TASKS_SCOPE],
        _ => vec![],
    }
}

fn service(deps: &Deps, account: &Account) -> Option<Box<dyn TaskService>> {
    let cfg = match account.provider {
        ProviderKind::Google => deps.oauth.google(),
        ProviderKind::Microsoft => deps.oauth.microsoft(),
        _ => None,
    }?;
    let tokens = TokenManager {
        http: deps.http.clone(),
        secrets: deps.secrets.clone(),
        cfg,
    };
    Some(match account.provider {
        ProviderKind::Google => Box::new(GoogleTasks::new(
            deps.http.clone(),
            tokens,
            &account.id,
            remote_tasks::GOOGLE_TASKS_API,
        )),
        _ => Box::new(MicrosoftTodo::new(
            deps.http.clone(),
            tokens,
            &account.id,
            remote_tasks::GRAPH_API,
        )),
    })
}

/// Every task list sync is on for, in order (the first takes tasks made here): its id, a name to show, the service.
fn targets(store: &Store, deps: &Deps) -> Vec<(String, String, Box<dyn TaskService>)> {
    let settings = store.load_settings();
    settings
        .tasks
        .sync_accounts
        .iter()
        .filter_map(|id| {
            if id == crate::reminders::ID {
                return crate::reminders::AVAILABLE.then(|| {
                    (
                        id.clone(),
                        "Reminders".to_string(),
                        Box::new(crate::reminders::RemindersService) as Box<dyn TaskService>,
                    )
                });
            }
            let account = store.get_account(id).ok().flatten()?;
            let svc = service(deps, &account)?;
            Some((account.id.clone(), account.display_name.clone(), svc))
        })
        .collect()
}

/// Syncs every account task sync is on for. Returns whether anything changed here. Errors are logged (without
/// content) and the next round tries again.
pub(crate) fn run(store: &Store, deps: &Deps) -> bool {
    let mut changed = false;
    for (i, (id, _, svc)) in targets(store, deps).into_iter().enumerate() {
        match remote_tasks::sync_account(store, svc.as_ref(), &id, i == 0, deps.tz(), Utc::now()) {
            Ok(done) => {
                changed |= done.changed_here > 0;
                if done.changed_here + done.changed_there > 0 {
                    log::info!(
                        "tasks synced for {}: {} here, {} there",
                        id,
                        done.changed_here,
                        done.changed_there
                    );
                }
            }
            Err(e) => log::warn!("task sync for {} failed: {}", id, kind(&e)),
        }
    }
    changed
}

/// A content-free word for an error, for the log and the page.
fn kind(e: &Error) -> &'static str {
    match e {
        Error::AuthRevoked => "signed out",
        Error::PermissionNeeded => "not allowed",
        Error::Offline => "offline",
        _ => "failed",
    }
}

/// Asks the account (in the browser) for its task permission, then keeps your tasks in step with it. Tasks you have
/// here are sent to the first account task sync is on for.
#[tauri::command]
pub async fn enable_task_sync(
    app: AppHandle,
    state: State<'_, AppState>,
    account_id: String,
) -> CmdResult<AppSettings> {
    let account = lock_store(&state)?
        .get_account(&account_id)
        .map_err(internal("loading the account"))?
        .ok_or("That account no longer exists.")?;
    let scope = match account.provider {
        ProviderKind::Google => remote_tasks::GOOGLE_TASKS_SCOPE,
        ProviderKind::Microsoft => remote_tasks::MS_TASKS_SCOPE,
        _ => return Err("Only Google and Microsoft accounts have task lists.".into()),
    };
    crate::mail_ai::upgrade_grant(&app, &state, &account, scope).await?;
    let saved = {
        let store = lock_store(&state)?;
        let mut s = store.load_settings();
        if !s.tasks.sync_accounts.contains(&account.id) {
            s.tasks.sync_accounts.push(account.id.clone());
        }
        store
            .save_settings(&s)
            .map_err(internal("saving settings"))?;
        s
    };
    let _ = app.emit("settings-changed", ());
    sync_tasks_now(app, state).await?;
    Ok(saved)
}

/// Stops keeping tasks in step with that account. Its tasks stay here as ordinary tasks.
#[tauri::command]
pub async fn disable_task_sync(
    app: AppHandle,
    state: State<'_, AppState>,
    account_id: String,
) -> CmdResult<AppSettings> {
    let store = lock_store(&state)?;
    remote_tasks::unlink_account(&store, &account_id).map_err(internal("updating tasks"))?;
    let mut s = store.load_settings();
    s.tasks.sync_accounts.retain(|a| a != &account_id);
    store
        .save_settings(&s)
        .map_err(internal("saving settings"))?;
    drop(store);
    let _ = app.emit("settings-changed", ());
    let _ = app.emit("tasks-changed", ());
    Ok(s)
}

#[derive(Serialize)]
pub struct TaskSyncReply {
    /// "signed out" | "not allowed" | "offline" | "failed" for an account that could not be synced.
    problems: Vec<(String, &'static str)>,
}

/// Syncs tasks now (the "Sync now" button, and right after turning it on). Runs on its own database connection so the
/// network calls never hold up the rest of the app.
#[tauri::command]
pub async fn sync_tasks_now(
    app: AppHandle,
    state: State<'_, AppState>,
) -> CmdResult<TaskSyncReply> {
    let (db, key, deps) = (state.db_path.clone(), state.key.clone(), state.deps.clone());
    let problems = tauri::async_runtime::spawn_blocking(
        move || -> Result<Vec<(String, &'static str)>, String> {
            let store =
                Store::open(&db, &key).map_err(|_| "Couldn't open your data.".to_string())?;
            let mut problems = Vec::new();
            for (i, (id, name, svc)) in targets(&store, &deps).into_iter().enumerate() {
                if let Err(e) = remote_tasks::sync_account(
                    &store,
                    svc.as_ref(),
                    &id,
                    i == 0,
                    deps.tz(),
                    Utc::now(),
                ) {
                    problems.push((name, kind(&e)));
                }
            }
            Ok(problems)
        },
    )
    .await
    .map_err(|_| "Syncing was interrupted.".to_string())??;
    let _ = app.emit("tasks-changed", ());
    Ok(TaskSyncReply { problems })
}

/// Keeps tasks in step with Apple Reminders (Mac): asks for Reminders access the first time, then syncs.
#[tauri::command]
pub async fn enable_reminders_sync(
    app: AppHandle,
    state: State<'_, AppState>,
) -> CmdResult<AppSettings> {
    if !crate::reminders::AVAILABLE {
        return Err("Reminders is on Macs only.".into());
    }
    let allowed = tauri::async_runtime::spawn_blocking(crate::reminders::request)
        .await
        .map_err(|_| "Asking for Reminders was interrupted.".to_string())?
        .map_err(|_| "Reminders needs macOS 14 or later.".to_string())?;
    if !allowed {
        return Err("Firstlight isn't allowed to use Reminders. Allow it in System Settings, Privacy & Security, Reminders.".into());
    }
    let saved = {
        let store = lock_store(&state)?;
        let mut s = store.load_settings();
        if !s
            .tasks
            .sync_accounts
            .iter()
            .any(|a| a == crate::reminders::ID)
        {
            s.tasks.sync_accounts.push(crate::reminders::ID.to_string());
        }
        store
            .save_settings(&s)
            .map_err(internal("saving settings"))?;
        s
    };
    let _ = app.emit("settings-changed", ());
    sync_tasks_now(app, state).await?;
    Ok(saved)
}

/// Whether Reminders can be offered here (Mac) and whether access is already granted.
#[tauri::command]
pub async fn reminders_status() -> CmdResult<(bool, bool)> {
    Ok((crate::reminders::AVAILABLE, crate::reminders::granted()))
}
