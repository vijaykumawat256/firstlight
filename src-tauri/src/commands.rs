//! The only functions the UI may call. Inputs are validated here, errors returned to the UI are
//! short and content-free, and anything that can touch the network or disk runs off the main thread.

use crate::AppState;
use crate::providers::provider_for;
use crate::scheduler::Trigger;
use chrono::{Duration, Utc};
use daybrief_core::dedupe::dedupe;
use daybrief_core::http::normalise_calendar_url;
use daybrief_core::model::*;
use daybrief_core::providers::caldav::{CaldavProvider, CaldavSecret, validate_server};
use daybrief_core::providers::ics::IcsProvider;
use daybrief_core::store::Store;
use daybrief_core::sync::{default_window, sync_account};
use daybrief_core::{Error, SyncReport};
use tauri::{AppHandle, Emitter, State};
use tauri_plugin_autostart::ManagerExt;

type CmdResult<T> = Result<T, String>;

/// Longest range the UI may ask for in one call.
const MAX_RANGE_DAYS: i64 = 62;

fn internal<E: std::fmt::Debug>(what: &'static str) -> impl Fn(E) -> String {
    // Log the detail (never content), return a generic message to the UI.
    move |e| {
        log::error!("{what}: {e:?}");
        format!("{what} failed")
    }
}

fn lock(state: &AppState) -> CmdResult<std::sync::MutexGuard<'_, Store>> {
    state
        .store
        .lock()
        .map_err(|_| "state unavailable".to_string())
}

// ---- reading ---------------------------------------------------------------

#[tauri::command]
pub async fn get_events(
    state: State<'_, AppState>,
    start: String,
    end: String,
) -> CmdResult<Vec<Event>> {
    let parse = |s: &str| {
        chrono::DateTime::parse_from_rfc3339(s)
            .map(|d| d.with_timezone(&Utc))
            .map_err(|_| "invalid date".to_string())
    };
    let (start, end) = (parse(&start)?, parse(&end)?);
    if end <= start {
        return Err("end must be after start".into());
    }
    if end - start > Duration::days(MAX_RANGE_DAYS) {
        return Err("range too large".into());
    }
    let events = lock(&state)?
        .events_between(start, end)
        .map_err(internal("loading events"))?;
    Ok(dedupe(events))
}

#[tauri::command]
pub async fn get_calendars(state: State<'_, AppState>) -> CmdResult<Vec<Calendar>> {
    lock(&state)?
        .list_calendars()
        .map_err(internal("loading calendars"))
}

#[tauri::command]
pub async fn list_accounts(state: State<'_, AppState>) -> CmdResult<Vec<AccountView>> {
    lock(&state)?
        .account_views()
        .map_err(internal("loading accounts"))
}

// ---- adding and removing accounts ----------------------------------------------

/// Plain-language reason a first sync failed, without any content from the remote.
fn explain(kind: ProviderKind, report: &SyncReport) -> &'static str {
    match (kind, report.needs_attention, report.offline) {
        (ProviderKind::Ics, true, _) => {
            "That link needs a sign-in or has been reset. Copy a fresh link from your calendar's settings."
        }
        (ProviderKind::Caldav, true, _) => {
            "The server rejected that username or password. Use an app-specific password, not your normal one."
        }
        (_, _, true) => "Couldn't reach the server. Check your connection and try again.",
        (ProviderKind::Ics, ..) => {
            "Couldn't read a calendar from that link. Check the address and try again."
        }
        (ProviderKind::Caldav, ..) => {
            "Couldn't find calendars on that server. Check the server address and try again."
        }
        _ => "Couldn't connect that account.",
    }
}

fn user_input_error(e: Error) -> String {
    match e {
        Error::Invalid(m) => m.to_string(),
        _ => "invalid input".to_string(),
    }
}

/// Everything needed to connect one account.
struct NewAccount {
    kind: ProviderKind,
    name: String,
    email: Option<String>,
    secret_ref: String,
    secret_value: String,
}

/// Saves the credential in the keychain, runs a first sync, and rolls everything back if it fails,
/// so a mistyped password never leaves a half-connected account behind.
async fn connect(
    app: &AppHandle,
    state: &AppState,
    id: String,
    new: NewAccount,
) -> CmdResult<AccountView> {
    let (deps, db, key) = (state.deps.clone(), state.db_path.clone(), state.key.clone());
    let view = tauri::async_runtime::spawn_blocking(move || -> CmdResult<AccountView> {
        // The credential is a secret: keychain only, never the database.
        deps.secrets
            .set(&new.secret_ref, &new.secret_value)
            .map_err(internal("saving the credentials"))?;
        let account = Account {
            id: id.clone(),
            provider: new.kind,
            display_name: new.name,
            email: new.email,
            status: AccountStatus::Ok,
            secret_ref: Some(new.secret_ref.clone()),
            created_at: Utc::now(),
        };
        let rollback = |store: Option<&Store>| {
            if let Some(s) = store {
                let _ = s.remove_account(&id);
            }
            let _ = deps.secrets.delete(&new.secret_ref);
        };
        let mut store = Store::open(&db, &key)
            .inspect_err(|_| rollback(None))
            .map_err(internal("opening the database"))?;
        store
            .upsert_account(&account)
            .inspect_err(|_| rollback(Some(&store)))
            .map_err(internal("saving the account"))?;
        let provider = provider_for(new.kind, &deps).ok_or("unsupported")?;
        let report = sync_account(
            &mut store,
            provider.as_ref(),
            &account,
            &default_window(Utc::now()),
        );
        if !report.is_clean() {
            rollback(Some(&store));
            return Err(explain(new.kind, &report).into());
        }
        store
            .account_views()
            .map_err(internal("loading accounts"))?
            .into_iter()
            .find(|a| a.id == id)
            .ok_or_else(|| "account missing".to_string())
    })
    .await
    .map_err(|_| "add failed".to_string())??;
    let _ = app.emit(
        "sync-finished",
        crate::scheduler::SyncSummary {
            ok: true,
            offline: false,
            needs_attention: false,
        },
    );
    Ok(view)
}

fn clean_name(name: &str, fallback: &str) -> CmdResult<String> {
    let name = name.trim();
    if name.chars().count() > 80 {
        return Err("name must be 80 characters or fewer".into());
    }
    Ok(if name.is_empty() { fallback } else { name }.to_string())
}

#[tauri::command]
pub async fn add_ics_account(
    app: AppHandle,
    state: State<'_, AppState>,
    name: String,
    url: String,
) -> CmdResult<AccountView> {
    let url = normalise_calendar_url(&url).map_err(user_input_error)?;
    let id = new_id();
    let new = NewAccount {
        kind: ProviderKind::Ics,
        name: clean_name(&name, "Subscribed calendar")?,
        email: None,
        secret_ref: IcsProvider::secret_key(&id),
        secret_value: url,
    };
    connect(&app, &state, id, new).await
}

#[tauri::command]
pub async fn add_caldav_account(
    app: AppHandle,
    state: State<'_, AppState>,
    name: String,
    server: String,
    username: String,
    password: String,
) -> CmdResult<AccountView> {
    let server = validate_server(&server).map_err(user_input_error)?;
    let username = username.trim().to_string();
    if username.is_empty() || username.chars().count() > 254 {
        return Err("enter your username or email address".into());
    }
    if password.is_empty() || password.chars().count() > 256 {
        return Err("enter the app password".into());
    }
    let id = new_id();
    let secret = serde_json::to_string(&CaldavSecret {
        server,
        username: username.clone(),
        password,
    })
    .map_err(internal("saving the credentials"))?;
    let new = NewAccount {
        kind: ProviderKind::Caldav,
        name: clean_name(&name, "Calendar")?,
        email: username.contains('@').then_some(username),
        secret_ref: CaldavProvider::secret_key(&id),
        secret_value: secret,
    };
    connect(&app, &state, id, new).await
}

#[tauri::command]
pub async fn add_demo_account(
    app: AppHandle,
    state: State<'_, AppState>,
) -> CmdResult<AccountView> {
    let (deps, db, key) = (state.deps.clone(), state.db_path.clone(), state.key.clone());
    let view = tauri::async_runtime::spawn_blocking(move || -> CmdResult<AccountView> {
        let mut store = Store::open(&db, &key).map_err(internal("opening the database"))?;
        let account = daybrief_core::providers::fixture::FixtureProvider::account(Utc::now());
        store
            .upsert_account(&account)
            .map_err(internal("saving the account"))?;
        let provider = provider_for(ProviderKind::Fixture, &deps).ok_or("unsupported")?;
        let report = sync_account(
            &mut store,
            provider.as_ref(),
            &account,
            &default_window(Utc::now()),
        );
        if !report.is_clean() {
            return Err("Couldn't load sample data".into());
        }
        store
            .account_views()
            .map_err(internal("loading accounts"))?
            .into_iter()
            .find(|a| a.id == account.id)
            .ok_or_else(|| "account missing".to_string())
    })
    .await
    .map_err(|_| "add failed".to_string())??;
    let _ = app.emit(
        "sync-finished",
        crate::scheduler::SyncSummary {
            ok: true,
            offline: false,
            needs_attention: false,
        },
    );
    Ok(view)
}

#[tauri::command]
pub async fn remove_account(state: State<'_, AppState>, id: String) -> CmdResult<()> {
    let store = lock(&state)?;
    let Some(account) = store
        .get_account(&id)
        .map_err(internal("loading the account"))?
    else {
        return Ok(()); // already gone
    };
    // Credentials first: if the keychain refuses, keep the account so the user can retry.
    if let Some(secret) = &account.secret_ref {
        state
            .deps
            .secrets
            .delete(secret)
            .map_err(internal("removing saved credentials"))?;
    }
    store
        .remove_account(&id)
        .map_err(internal("removing the account"))
}

#[tauri::command]
pub async fn rename_account(state: State<'_, AppState>, id: String, name: String) -> CmdResult<()> {
    lock(&state)?
        .rename_account(&id, &name)
        .map_err(|e| match e {
            Error::Invalid(m) => m.to_string(),
            other => internal("renaming")(other),
        })
}

#[tauri::command]
pub async fn set_calendar_visible(
    state: State<'_, AppState>,
    calendar_id: String,
    visible: bool,
) -> CmdResult<()> {
    lock(&state)?
        .set_calendar_visible(&calendar_id, visible)
        .map_err(internal("updating the calendar"))
}

#[tauri::command]
pub async fn set_calendar_colour(
    state: State<'_, AppState>,
    calendar_id: String,
    colour: String,
) -> CmdResult<()> {
    lock(&state)?
        .set_calendar_colour(&calendar_id, &colour)
        .map_err(|e| match e {
            Error::Invalid(m) => m.to_string(),
            other => internal("updating the calendar")(other),
        })
}

#[tauri::command]
pub async fn refresh_now(state: State<'_, AppState>) -> CmdResult<()> {
    state.sync.trigger(Trigger::Manual);
    Ok(())
}

// ---- shell settings ------------------------------------------------------------

#[tauri::command]
pub async fn get_autostart(app: AppHandle) -> CmdResult<bool> {
    app.autolaunch()
        .is_enabled()
        .map_err(internal("reading autostart"))
}

#[tauri::command]
pub async fn set_autostart(app: AppHandle, enabled: bool) -> CmdResult<()> {
    let launcher = app.autolaunch();
    if enabled {
        launcher.enable()
    } else {
        launcher.disable()
    }
    .map_err(internal("changing autostart"))
}

#[tauri::command]
pub async fn get_unlock_count_today(state: State<'_, AppState>) -> CmdResult<i64> {
    let today = Utc::now().with_timezone(&state.tz).date_naive();
    lock(&state)?
        .count_briefing_log(today, "unlock")
        .map_err(internal("reading unlock log"))
}
