//! The only functions the UI may call. Inputs are validated here, errors returned to the UI are
//! short and content-free, and anything that can touch the network or disk runs off the main thread.

use crate::AppState;
use crate::providers::provider_for;
use crate::scheduler::Trigger;
use chrono::{Duration, Utc};
use daybrief_core::dedupe::dedupe;
use daybrief_core::http::normalise_calendar_url;
use daybrief_core::model::*;
use daybrief_core::oauth::loopback::Loopback;
use daybrief_core::oauth::{
    self, OAuthConfig, Pkce, authorization_url, exchange_code, random_state,
};
use daybrief_core::providers::caldav::{CaldavProvider, CaldavSecret, validate_server};
use daybrief_core::providers::ics::IcsProvider;
use daybrief_core::providers::system::{Access, SYSTEM_ACCOUNT_ID};
use daybrief_core::providers::{google, microsoft};
use daybrief_core::store::Store;
use daybrief_core::sync::{default_window, sync_account};
use daybrief_core::{Error, SyncReport};
use tauri::{AppHandle, Emitter, State};
use tauri_plugin_autostart::ManagerExt;

pub(crate) type CmdResult<T> = Result<T, String>;

/// Longest range the UI may ask for in one call.
const MAX_RANGE_DAYS: i64 = 62;

pub(crate) fn internal<E: std::fmt::Debug>(what: &'static str) -> impl Fn(E) -> String {
    // Log the detail (never content), return a generic message to the UI.
    move |e| {
        log::error!("{what}: {e:?}");
        format!("{what} failed")
    }
}

pub(crate) fn lock_store(state: &AppState) -> CmdResult<std::sync::MutexGuard<'_, Store>> {
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
    let events = lock_store(&state)?
        .events_between(start, end)
        .map_err(internal("loading events"))?;
    Ok(dedupe(events))
}

#[tauri::command]
pub async fn get_calendars(state: State<'_, AppState>) -> CmdResult<Vec<Calendar>> {
    lock_store(&state)?
        .list_calendars()
        .map_err(internal("loading calendars"))
}

#[tauri::command]
pub async fn list_accounts(state: State<'_, AppState>) -> CmdResult<Vec<AccountView>> {
    lock_store(&state)?
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
        (ProviderKind::Google | ProviderKind::Microsoft, true, _) => {
            "Access was refused. Make sure calendar access is allowed for this app, then sign in again."
        }
        (ProviderKind::System, true, _) => {
            "Calendar access is turned off for Daybrief. Allow it in System Settings, then try again."
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
    /// Providers that need no credential (the Calendar app) leave both `None`.
    secret_ref: Option<String>,
    secret_value: Option<String>,
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
        if let (Some(r), Some(v)) = (&new.secret_ref, &new.secret_value) {
            deps.secrets
                .set(r, v)
                .map_err(internal("saving the credentials"))?;
        }
        let account = Account {
            id: id.clone(),
            provider: new.kind,
            display_name: new.name,
            email: new.email,
            status: AccountStatus::Ok,
            secret_ref: new.secret_ref.clone(),
            created_at: Utc::now(),
        };
        let rollback = |store: Option<&Store>| {
            if let Some(s) = store {
                let _ = s.remove_account(&id);
            }
            if let Some(r) = &new.secret_ref {
                let _ = deps.secrets.delete(r);
            }
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
        secret_ref: Some(IcsProvider::secret_key(&id)),
        secret_value: Some(url),
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
        secret_ref: Some(CaldavProvider::secret_key(&id)),
        secret_value: Some(secret),
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
    let store = lock_store(&state)?;
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
    lock_store(&state)?
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
    lock_store(&state)?
        .set_calendar_visible(&calendar_id, visible)
        .map_err(internal("updating the calendar"))
}

#[tauri::command]
pub async fn set_calendar_colour(
    state: State<'_, AppState>,
    calendar_id: String,
    colour: String,
) -> CmdResult<()> {
    lock_store(&state)?
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
    let today = Utc::now().with_timezone(&crate::local_tz()).date_naive();
    lock_store(&state)?
        .count_briefing_log(today, "unlock")
        .map_err(internal("reading unlock log"))
}

// ---- OAuth sign-in (Google, Microsoft) -------------------------------------------

#[derive(serde::Serialize)]
pub struct OAuthAvailability {
    google: bool,
    microsoft: bool,
}

/// Whether this build carries a client ID for each provider (see `.env.example`).
#[tauri::command]
pub async fn oauth_available(state: State<'_, AppState>) -> CmdResult<OAuthAvailability> {
    Ok(OAuthAvailability {
        google: state.deps.oauth.google.is_some(),
        microsoft: state.deps.oauth.microsoft.is_some(),
    })
}

/// Aborts a sign-in that is waiting for the browser.
#[tauri::command]
pub async fn cancel_oauth(state: State<'_, AppState>) -> CmdResult<()> {
    state
        .oauth_cancel
        .store(true, std::sync::atomic::Ordering::Relaxed);
    Ok(())
}

/// Only these sign-in pages may be opened in the browser.
const SIGN_IN_HOSTS: [&str; 2] = ["accounts.google.com", "login.microsoftonline.com"];
const SIGN_IN_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(300);

pub(crate) fn oauth_error(e: Error) -> String {
    match e {
        // Messages from the loopback/cancel path are already plain language.
        Error::Provider(m) if m.starts_with("sign-in") => {
            let mut c = m.chars();
            c.next()
                .map(|f| f.to_uppercase().collect::<String>() + c.as_str())
                .unwrap_or(m)
                + "."
        }
        Error::Offline => {
            "Couldn't reach the sign-in server. Check your connection and try again.".into()
        }
        Error::AuthRevoked => "The sign-in was refused. Please try again.".into(),
        _ => "Couldn't complete sign-in. Please try again.".into(),
    }
}

pub(crate) fn config_for(state: &AppState, kind: ProviderKind) -> CmdResult<OAuthConfig> {
    match kind {
        ProviderKind::Google => state.deps.oauth.google.clone(),
        ProviderKind::Microsoft => state.deps.oauth.microsoft.clone(),
        _ => None,
    }
    .ok_or_else(|| "Sign-in for this provider isn't set up in this build yet.".to_string())
}

/// Opens the provider's sign-in page in the system browser and waits for the user to finish.
/// Used for adding an account and for granting extra permissions (such as mail) later.
pub(crate) async fn browser_sign_in(
    app: &AppHandle,
    state: &AppState,
    kind: ProviderKind,
    cfg: OAuthConfig,
) -> CmdResult<(oauth::TokenSet, String)> {
    use tauri_plugin_opener::OpenerExt;
    let (deps, cancel, lock) = (
        state.deps.clone(),
        state.oauth_cancel.clone(),
        state.oauth_lock.clone(),
    );
    let opener_app = app.clone();
    tauri::async_runtime::spawn_blocking(move || -> CmdResult<(oauth::TokenSet, String)> {
        let _one_at_a_time = lock
            .try_lock()
            .map_err(|_| "A sign-in is already in progress.".to_string())?;
        cancel.store(false, std::sync::atomic::Ordering::Relaxed);

        let listener = Loopback::bind().map_err(oauth_error)?;
        // Microsoft only registers "localhost"; Google recommends the literal loopback address.
        let host = if kind == ProviderKind::Microsoft {
            "localhost"
        } else {
            "127.0.0.1"
        };
        let redirect = listener.redirect_uri(host);
        let (state_token, pkce) = (random_state(), Pkce::generate());
        let extra: &[(&str, &str)] = if kind == ProviderKind::Google {
            &google::AUTH_EXTRA
        } else {
            &[]
        };
        let url = authorization_url(&cfg, &redirect, &state_token, &pkce.challenge, extra)
            .map_err(oauth_error)?;

        let parsed =
            daybrief_core::http::Url::parse(&url).map_err(|_| "bad sign-in address".to_string())?;
        if parsed.scheme() != "https"
            || !parsed
                .host_str()
                .is_some_and(|h| SIGN_IN_HOSTS.contains(&h))
        {
            return Err("Refusing to open an unexpected sign-in address.".into());
        }
        opener_app
            .opener()
            .open_url(url, None::<&str>)
            .map_err(|_| "Couldn't open your browser.".to_string())?;

        let code = listener
            .wait_for_code(&state_token, SIGN_IN_TIMEOUT, &cancel)
            .map_err(oauth_error)?;
        let tokens = exchange_code(
            deps.http.as_ref(),
            &cfg,
            &code,
            &pkce.verifier,
            &redirect,
            Utc::now(),
        )
        .map_err(oauth_error)?;
        let email = match kind {
            ProviderKind::Google => google::GoogleProvider::fetch_email(
                deps.http.as_ref(),
                google::USERINFO_URL,
                &tokens.access_token,
            ),
            _ => microsoft::MicrosoftProvider::fetch_email(
                deps.http.as_ref(),
                microsoft::API_BASE,
                &tokens.access_token,
            ),
        }
        .map_err(oauth_error)?;
        Ok((tokens, email))
    })
    .await
    .map_err(|_| "sign-in failed".to_string())?
}

#[tauri::command]
pub async fn add_oauth_account(
    app: AppHandle,
    state: State<'_, AppState>,
    provider: String,
) -> CmdResult<AccountView> {
    let kind = match provider.as_str() {
        "google" => ProviderKind::Google,
        "microsoft" => ProviderKind::Microsoft,
        _ => return Err("unknown provider".into()),
    };
    let cfg = config_for(&state, kind)?;
    let (tokens, email) = browser_sign_in(&app, &state, kind, cfg).await?;

    let secret_value = serde_json::to_string(&tokens).map_err(internal("saving the sign-in"))?;

    // Signing in again with an account we already have refreshes it (this is also how "Reconnect" works).
    let existing = lock_store(&state)?
        .list_accounts()
        .map_err(internal("loading accounts"))?
        .into_iter()
        .find(|a| {
            a.provider == kind
                && a.email
                    .as_deref()
                    .is_some_and(|e| e.eq_ignore_ascii_case(&email))
        });
    if let Some(account) = existing {
        let secret_ref = account
            .secret_ref
            .clone()
            .unwrap_or_else(|| oauth::secret_key(&account.id));
        state
            .deps
            .secrets
            .set(&secret_ref, &secret_value)
            .map_err(internal("saving the sign-in"))?;
        let store = lock_store(&state)?;
        store
            .set_account_status(&account.id, AccountStatus::Ok)
            .map_err(internal("updating the account"))?;
        state.sync.trigger(Trigger::Manual);
        return store
            .account_views()
            .map_err(internal("loading accounts"))?
            .into_iter()
            .find(|a| a.id == account.id)
            .ok_or_else(|| "account missing".to_string());
    }

    let id = new_id();
    let new = NewAccount {
        kind,
        name: email.clone(),
        email: Some(email),
        secret_ref: Some(oauth::secret_key(&id)),
        secret_value: Some(secret_value),
    };
    connect(&app, &state, id, new).await
}

// ---- the Calendar app (macOS) ----------------------------------------------------

#[derive(serde::Serialize)]
pub struct SystemStatus {
    supported: bool,
    /// "granted" | "not_determined" | "denied" | "restricted" | "unsupported"
    access: &'static str,
}

fn access_name(a: Access) -> &'static str {
    match a {
        Access::Granted => "granted",
        Access::NotDetermined => "not_determined",
        Access::Denied => "denied",
        Access::Restricted => "restricted",
        Access::Unsupported => "unsupported",
    }
}

/// Whether the Calendar app can be used and whether the user has allowed it. Never shows a prompt.
#[tauri::command]
pub async fn system_calendar_status(state: State<'_, AppState>) -> CmdResult<SystemStatus> {
    let access = state.deps.system.access();
    Ok(SystemStatus {
        supported: access != Access::Unsupported,
        access: access_name(access),
    })
}

/// Connects the Calendar app. Shows the macOS permission prompt the first time.
#[tauri::command]
pub async fn add_system_account(
    app: AppHandle,
    state: State<'_, AppState>,
) -> CmdResult<AccountView> {
    let source = state.deps.system.clone();
    // The prompt blocks until the user answers, so keep it off the UI thread.
    let access = tauri::async_runtime::spawn_blocking(move || source.request_access())
        .await
        .map_err(|_| "permission request failed".to_string())?
        .map_err(|e| match e {
            Error::Provider(m) if m.contains("not answered") => {
                "The permission prompt wasn't answered. Try again.".to_string()
            }
            other => internal("requesting calendar access")(other),
        })?;
    match access {
        Access::Granted => {}
        Access::NotDetermined => return Err("The permission prompt wasn't answered. Try again.".into()),
        Access::Denied => return Err("Calendar access is turned off for Daybrief. Open System Settings, switch Daybrief on under Calendars, then try again.".into()),
        Access::Restricted => return Err("This Mac's settings don't allow apps to read calendars.".into()),
        Access::Unsupported => return Err("The Calendar app can't be used here. It needs macOS 14 or later.".into()),
    }
    let new = NewAccount {
        kind: ProviderKind::System,
        name: "Calendar app".into(),
        email: None,
        secret_ref: None,
        secret_value: None,
    };
    connect(&app, &state, SYSTEM_ACCOUNT_ID.to_string(), new).await
}

/// Opens the relevant System Settings pane so the user can allow access.
#[tauri::command]
pub async fn open_privacy_settings(app: AppHandle, pane: String) -> CmdResult<()> {
    use tauri_plugin_opener::OpenerExt;
    let url = match pane.as_str() {
        "calendars" => "x-apple.systempreferences:com.apple.preference.security?Privacy_Calendars",
        "automation" => {
            "x-apple.systempreferences:com.apple.preference.security?Privacy_Automation"
        }
        _ => return Err("unknown settings pane".into()),
    };
    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|_| "Couldn't open System Settings.".to_string())
}

// ---- the briefing ------------------------------------------------------------------

/// Today and tomorrow, worked out by the briefing engine (pure; see daybrief-core).
#[tauri::command]
pub async fn get_briefing(
    state: State<'_, AppState>,
) -> CmdResult<daybrief_core::briefing::Briefing> {
    use daybrief_core::briefing::{build, day_bounds};
    let now = Utc::now();
    let tz = crate::local_tz();
    let store = lock_store(&state)?;
    let settings = store.load_settings().briefing;
    let today = now.with_timezone(&tz).date_naive();
    let (from, _) = day_bounds(today, tz);
    let (_, to) = day_bounds(today + Duration::days(1), tz);
    let events = dedupe(
        store
            .events_between(from, to)
            .map_err(internal("loading events"))?,
    );
    Ok(build(&events, now, tz, &settings))
}

/// Hides the small "up next" panel.
#[tauri::command]
pub async fn dismiss_up_next(app: AppHandle) -> CmdResult<()> {
    crate::hide_up_next(&app);
    Ok(())
}

/// Brings the full briefing to the front (from the small panel or the tray).
#[tauri::command]
pub async fn open_briefing(app: AppHandle) -> CmdResult<()> {
    crate::hide_up_next(&app);
    crate::show_main(&app);
    Ok(())
}

// ---- settings --------------------------------------------------------------------------

use daybrief_core::settings::AppSettings;

#[tauri::command]
pub async fn get_settings(state: State<'_, AppState>) -> CmdResult<AppSettings> {
    Ok(lock_store(&state)?.load_settings())
}

/// Checks, applies and saves new settings, and returns them as stored (out-of-range values are corrected).
fn apply_and_save(app: &AppHandle, state: &AppState, mut s: AppSettings) -> CmdResult<AppSettings> {
    s.validate()?;
    // Consent to send data to an AI service is recorded only by the explicit enable action, never by a save.
    let stored = lock_store(state)?.load_settings();
    s.ai.consented = stored.ai.consented;
    // Mailboxes change only through add/remove, which keep their keychain passwords in step.
    s.email.imap = stored.email.imap;
    s.sanitize();
    // Try the shortcut first: if the system refuses it, nothing is saved and the user is told why.
    crate::apply_shortcut(app, s.general.global_shortcut.as_deref())?;
    lock_store(state)?
        .save_settings(&s)
        .map_err(internal("saving settings"))?;
    crate::crash_reports::set_enabled(s.privacy.crash_reports);
    let _ = app.emit("settings-changed", ());
    Ok(s)
}

#[tauri::command]
pub async fn set_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    settings: AppSettings,
) -> CmdResult<AppSettings> {
    apply_and_save(&app, &state, settings)
}

/// The settings as a portable JSON document. Contains no accounts and no secrets.
#[tauri::command]
pub async fn export_settings(state: State<'_, AppState>) -> CmdResult<String> {
    Ok(lock_store(&state)?.load_settings().export())
}

/// Writes the export to the Downloads folder and returns where it went.
#[tauri::command]
pub async fn save_settings_export(app: AppHandle, state: State<'_, AppState>) -> CmdResult<String> {
    use tauri::Manager;
    let text = lock_store(&state)?.load_settings().export();
    let dir = app
        .path()
        .download_dir()
        .map_err(|_| "Couldn't find your Downloads folder.".to_string())?;
    let mut path = dir.join("daybrief-settings.json");
    // Never overwrite something the user already has.
    for n in 2..100 {
        if !path.exists() {
            break;
        }
        path = dir.join(format!("daybrief-settings ({n}).json"));
    }
    std::fs::write(&path, text).map_err(|_| "Couldn't save the file.".to_string())?;
    Ok(path.display().to_string())
}

#[tauri::command]
pub async fn import_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    text: String,
) -> CmdResult<AppSettings> {
    let imported = AppSettings::import(&text)?;
    apply_and_save(&app, &state, imported)
}

/// Deletes all accounts, cached events, history and settings, plus every saved credential.
/// The caller must have asked the user to confirm.
#[tauri::command]
pub async fn erase_all_data(app: AppHandle, state: State<'_, AppState>) -> CmdResult<()> {
    use tauri_plugin_autostart::ManagerExt;
    let report = {
        let store = lock_store(&state)?;
        daybrief_core::erase::erase_everything(&store, state.deps.secrets.as_ref())
            .map_err(internal("erasing data"))?
    };
    log::info!(
        "erased {} accounts and {} credentials",
        report.accounts,
        report.credentials
    );
    let _ = app.autolaunch().disable();
    let _ = crate::apply_shortcut(&app, None);
    let _ = app.emit("settings-changed", ());
    let _ = app.emit(
        "sync-finished",
        crate::scheduler::SyncSummary {
            ok: true,
            offline: false,
            needs_attention: false,
        },
    );
    Ok(())
}

// ---- about, updates, crash reports ------------------------------------------------------

#[derive(serde::Serialize)]
pub struct AppInfo {
    name: &'static str,
    version: &'static str,
    os: &'static str,
    arch: &'static str,
    /// Whether this package can update itself.
    updates_configured: bool,
}

#[tauri::command]
pub async fn get_app_info() -> CmdResult<AppInfo> {
    Ok(AppInfo {
        name: "Daybrief",
        version: env!("CARGO_PKG_VERSION"),
        os: std::env::consts::OS,
        arch: std::env::consts::ARCH,
        updates_configured: cfg!(feature = "updater")
            && crate::updates::configured_base().is_some(),
    })
}

#[tauri::command]
pub async fn check_for_update(
    app: AppHandle,
    state: State<'_, AppState>,
) -> CmdResult<crate::updates::UpdateStatus> {
    let channel = lock_store(&state)?.load_settings().general.update_channel;
    Ok(crate::updates::check(&app, channel).await)
}

#[tauri::command]
pub async fn install_update(app: AppHandle, state: State<'_, AppState>) -> CmdResult<()> {
    let channel = lock_store(&state)?.load_settings().general.update_channel;
    crate::updates::install(&app, channel).await
}

#[tauri::command]
pub async fn get_last_crash_report(app: AppHandle) -> CmdResult<Option<String>> {
    use tauri::Manager;
    Ok(app
        .path()
        .app_log_dir()
        .ok()
        .and_then(|d| crate::crash_reports::latest(&d)))
}

#[tauri::command]
pub async fn clear_crash_reports(app: AppHandle) -> CmdResult<()> {
    use tauri::Manager;
    if let Ok(d) = app.path().app_log_dir() {
        crate::crash_reports::clear(&d);
    }
    Ok(())
}
