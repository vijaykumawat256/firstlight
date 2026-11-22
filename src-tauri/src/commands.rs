//! The only functions the UI may call. Inputs are validated here, errors returned to the UI are
//! short and content-free, and anything that can touch the network or disk runs off the main thread.

use crate::AppState;
use crate::providers::provider_for;
use crate::scheduler::Trigger;
use chrono::{Duration, Utc};
use firstlight_core::dedupe::dedupe;
use firstlight_core::http::normalise_calendar_url;
use firstlight_core::model::*;
use firstlight_core::oauth::loopback::Loopback;
use firstlight_core::oauth::{
    self, OAuthConfig, Pkce, authorization_url, exchange_code, random_state,
};
use firstlight_core::providers::caldav::{
    CaldavProvider, CaldavSecret, CaldavWriter, validate_server,
};
use firstlight_core::providers::ics::IcsProvider;
use firstlight_core::providers::system::{
    Access, CalendarWriter, SYSTEM_ACCOUNT_ID, SysNewEvent, SystemSource,
};
use firstlight_core::providers::{google, microsoft};
use firstlight_core::store::Store;
use firstlight_core::sync::{default_window, sync_account};
use firstlight_core::{Error, SyncReport};
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
            "Calendar access is turned off for Firstlight. Allow it in System Settings, then try again."
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

/// Why Outlook couldn't be read, in the words Windows and Outlook used (no calendar content is ever in them):
/// asks again, first for the list of calendars and then for the events of the first one, to find which step fails.
fn explain_outlook(deps: &crate::providers::Deps) -> Option<String> {
    let calendars = match deps.system.calendars() {
        Err(Error::Provider(m)) => return Some(m),
        Err(_) => return None,
        Ok(c) => c,
    };
    let first = calendars.first()?;
    let now = Utc::now();
    match deps.system.events(&first.id, now, now + Duration::days(1)) {
        Err(Error::Provider(m)) => Some(format!(
            "Outlook's calendar list was read, but its events couldn't be: {m}"
        )),
        _ => None,
    }
}

pub(crate) fn user_input_error(e: Error) -> String {
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
        // Outlook can hold several kinds of calendar (archives, shared ones): one that can't be read must not stop
        // the others from being added, so it counts as connected as soon as one calendar worked.
        let outlook = new.kind == ProviderKind::System && deps.system.name() == "Outlook";
        let partly_ok = outlook && report.calendars_ok > 0 && !report.needs_attention;
        if !report.is_clean() && !partly_ok {
            rollback(Some(&store));
            if outlook && let Some(why) = explain_outlook(&deps) {
                return Err(why);
            }
            return Err(explain(new.kind, &report).into());
        }
        if outlook && report.calendars_ok == 0 {
            rollback(Some(&store));
            return Err("Outlook answered, but it has no calendar Firstlight can read. Open Outlook and make sure an account is set up there. (The new Outlook can't be read this way: use Sign in with Microsoft instead.)".into());
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
        let account = firstlight_core::providers::fixture::FixtureProvider::account(Utc::now());
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
    // Its tasks stay here as ordinary tasks; nothing more is sent to it.
    let _ = firstlight_core::remote_tasks::unlink_account(&store, &id);
    store
        .remove_account(&id)
        .map_err(internal("removing the account"))?;
    let mut s = store.load_settings();
    s.tasks.sync_accounts.retain(|a| a != &id);
    if s.tasks.cloud_sync.as_deref() == Some(id.as_str()) {
        s.tasks.cloud_sync = None;
    }
    s.email.send_accounts.retain(|a| a != &id);
    s.email.autoreply_accounts.retain(|a| a != &id);
    store.save_settings(&s).map_err(internal("saving settings"))
}

/// Adds an event of the user's own. With `calendar_id` (one of `writable_calendars`) it is saved into that
/// calendar in the Calendar app, which needs the setting to be on; otherwise it stays only on this computer.
#[tauri::command]
pub async fn create_event(
    app: AppHandle,
    state: State<'_, AppState>,
    event: firstlight_core::local::EventInput,
    calendar_id: Option<String>,
) -> CmdResult<Event> {
    match calendar_id {
        None => save_own_event(&app, &state, None, event),
        Some(cal) => save_calendar_event(&app, &state, None, &cal, event),
    }
}

#[tauri::command]
pub async fn update_event(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    event: firstlight_core::local::EventInput,
) -> CmdResult<Event> {
    // An event saved in the Calendar app is changed there; one kept only here is changed here.
    let saved_to = {
        let store = lock_store(&state)?;
        firstlight_core::local::get(&store, &id)
            .map_err(user_input_error)?
            .and_then(|o| o.saved_to)
    };
    match saved_to {
        Some(cal) => save_calendar_event(&app, &state, Some(&id), &cal, event),
        None => save_own_event(&app, &state, Some(&id), event),
    }
}

/// Removes one of the user's own events (and, if it was saved in the Calendar app or a CalDAV account, from
/// there too). Events Firstlight did not create can never be removed here.
#[tauri::command]
pub async fn delete_event(app: AppHandle, state: State<'_, AppState>, id: String) -> CmdResult<()> {
    // The slow part (the Calendar app or a server) runs with the store unlocked, so the rest of the app keeps working.
    let (master, remote, writer) = {
        let store = lock_store(&state)?;
        let (master, remote) =
            firstlight_core::local::prepare_delete(&store, &id).map_err(user_input_error)?;
        // Without permission no writer is handed in, so an event that lives elsewhere is refused.
        let writer: Option<Box<dyn CalendarWriter>> = match &remote {
            Some(r) => writer_for_existing(&state, &store, r).ok(),
            None => None,
        };
        (master, remote, writer)
    };
    firstlight_core::local::delete_remote(remote.as_ref(), writer.as_deref())
        .map_err(user_input_error)?;
    {
        let mut store = lock_store(&state)?;
        firstlight_core::local::finish_delete(&mut store, &master, Utc::now())
            .map_err(user_input_error)?;
    }
    state.sync.trigger(Trigger::Manual);
    let _ = app.emit("events-changed", ());
    Ok(())
}

const SYSTEM_WRITE_OFF: &str =
    "Turn on “Let Firstlight add events to the Calendar app” in Settings first.";
const SYSTEM_ACCESS_OFF: &str = "Allow Firstlight to use your calendars in System Settings first.";

/// Whether Firstlight may write to the Calendar app right now: the user turned it on, and the system
/// allowed access to calendars. Both are checked at the moment of every write.
fn calendar_writing_allowed(state: &AppState) -> CmdResult<()> {
    if !lock_store(state)?
        .load_settings()
        .general
        .write_to_calendar_app
    {
        return Err(SYSTEM_WRITE_OFF.into());
    }
    if state.deps.system.access() != Access::Granted {
        return Err(SYSTEM_ACCESS_OFF.into());
    }
    Ok(())
}

/// A calendar events can be added to: one in the Calendar app, or one on a CalDAV account the user allowed.
#[derive(serde::Serialize)]
pub struct WritableCalendar {
    id: String,
    name: String,
    source: String,
}

fn writable_list(state: &AppState) -> CmdResult<Vec<WritableCalendar>> {
    use firstlight_core::providers::system::SysCalendarKind;
    Ok(state
        .deps
        .system
        .calendars()
        .map_err(internal("reading the calendars"))?
        .into_iter()
        // Birthdays and subscribed calendars are read-only by nature; anything the system says it will not let
        // anyone change is left out too.
        .filter(|c| {
            !c.read_only
                && !matches!(
                    c.kind,
                    SysCalendarKind::Birthday | SysCalendarKind::Subscription
                )
        })
        .map(|c| WritableCalendar {
            id: c.id,
            name: c.title,
            source: c.source,
        })
        .collect())
}

/// The calendars of the CalDAV accounts the user allowed, that the server says may be changed.
fn caldav_writable_list(state: &AppState) -> CmdResult<Vec<WritableCalendar>> {
    let store = lock_store(state)?;
    let allowed = store.load_settings().general.writable_accounts;
    let mut out = Vec::new();
    for acct in store
        .list_accounts()
        .map_err(internal("reading accounts"))?
    {
        if acct.provider != ProviderKind::Caldav || !allowed.contains(&acct.id) {
            continue;
        }
        for c in store
            .calendars_for_account(&acct.id)
            .map_err(internal("reading calendars"))?
            .into_iter()
            .filter(|c| !c.read_only)
        {
            out.push(WritableCalendar {
                id: c.id,
                name: c.name,
                source: acct.display_name.clone(),
            });
        }
    }
    Ok(out)
}

/// The calendars the "Save to" menu offers. Empty unless the user opted in somewhere, so the form shows
/// nothing extra to anyone who has not.
#[tauri::command]
pub async fn writable_calendars(state: State<'_, AppState>) -> CmdResult<Vec<WritableCalendar>> {
    let mut out = if calendar_writing_allowed(&state).is_ok() {
        writable_list(&state)?
    } else {
        vec![]
    };
    out.extend(caldav_writable_list(&state)?);
    Ok(out)
}

/// Where a new event goes: a CalDAV calendar (account, address) when `calendar_id` is one of ours, else the
/// Calendar app.
fn caldav_calendar(store: &Store, calendar_id: &str) -> Option<(Account, Calendar)> {
    let cal = store
        .list_calendars()
        .ok()?
        .into_iter()
        .find(|c| c.id == calendar_id)?;
    let acct = store.get_account(&cal.account_id).ok()??;
    (acct.provider == ProviderKind::Caldav).then_some((acct, cal))
}

/// The writer for a CalDAV calendar, after checking the user allowed writing to that account and the server
/// lets them change that calendar.
fn caldav_writer(
    state: &AppState,
    store: &Store,
    acct: &Account,
    cal: &Calendar,
) -> CmdResult<Box<dyn CalendarWriter>> {
    if !store
        .load_settings()
        .general
        .writable_accounts
        .contains(&acct.id)
    {
        return Err(format!(
            "Turn on “Let Firstlight add events to {}” in Settings first.",
            acct.display_name
        ));
    }
    if cal.read_only {
        return Err("That calendar can't be added to.".into());
    }
    Ok(Box::new(CaldavWriter::new(
        state.deps.http.clone(),
        state.deps.secrets.clone(),
        &acct.id,
        &cal.remote_id,
    )))
}

/// The writer for an event we saved before, found through where it was saved.
fn writer_for_existing(
    state: &AppState,
    store: &Store,
    remote: &firstlight_core::local::RemoteRef,
) -> CmdResult<Box<dyn CalendarWriter>> {
    if remote.account_id.is_none() {
        calendar_writing_allowed(state)?;
        return Ok(Box::new(SystemWriter(state.deps.system.clone())));
    }
    let (acct, cal) = caldav_calendar(store, &remote.calendar_id)
        .ok_or("That calendar isn't connected any more.")?;
    caldav_writer(state, store, &acct, &cal)
}

/// Lets the Calendar app be handed around as a `CalendarWriter`.
struct SystemWriter(std::sync::Arc<dyn SystemSource>);

impl CalendarWriter for SystemWriter {
    fn save_event(
        &self,
        calendar_id: &str,
        existing: Option<&str>,
        event: &SysNewEvent,
        tz: chrono_tz::Tz,
    ) -> firstlight_core::Result<String> {
        self.0.save_event(calendar_id, existing, event, tz)
    }
    fn delete_event(&self, event_id: &str) -> firstlight_core::Result<()> {
        self.0.delete_event(event_id)
    }
}

fn save_calendar_event(
    app: &AppHandle,
    state: &AppState,
    id: Option<&str>,
    calendar_id: &str,
    event: firstlight_core::local::EventInput,
) -> CmdResult<Event> {
    // Which account the event goes to: where it already is, or the calendar the form named.
    let (writer, account, calendar_id, place): (
        Box<dyn CalendarWriter>,
        Option<String>,
        String,
        &str,
    ) = {
        let store = lock_store(state)?;
        let existing = match id {
            Some(id) => Some(
                firstlight_core::local::remote_of(&store, id)
                    .map_err(user_input_error)?
                    .ok_or("That event is stored only on this computer.")?,
            ),
            None => None,
        };
        let target = match &existing {
            Some(r) => r.calendar_id.clone(),
            None => calendar_id.to_string(),
        };
        let caldav = match &existing {
            Some(r) => r.account_id.is_some(),
            None => caldav_calendar(&store, &target).is_some(),
        };
        if caldav {
            let (acct, cal) = caldav_calendar(&store, &target)
                .ok_or("That calendar isn't connected any more.")?;
            let w = caldav_writer(state, &store, &acct, &cal)?;
            (w, Some(acct.id), target, "The server")
        } else {
            drop(store);
            calendar_writing_allowed(state)?;
            // A new event may only go to a calendar the system lists as changeable (an edit keeps its own).
            if id.is_none() && !writable_list(state)?.iter().any(|c| c.id == target) {
                return Err("That calendar can't be added to.".into());
            }
            (
                Box::new(SystemWriter(state.deps.system.clone())),
                None,
                target,
                "The Calendar app",
            )
        }
    };
    let fail = |e: Error| match e {
        Error::Invalid(m) => m.to_string(),
        Error::PermissionNeeded => SYSTEM_ACCESS_OFF.to_string(),
        Error::AuthRevoked => {
            "Firstlight couldn't sign in to that account. Reconnect it in Settings, then try again."
                .to_string()
        }
        Error::Offline => format!("{place} couldn't be reached. Nothing was changed."),
        Error::Provider(m) if account.is_some() => {
            format!("{place} couldn't save that ({m}). Nothing was changed.")
        }
        _ => format!("{place} couldn't save that. Nothing was changed."),
    };
    let (tz, now) = (crate::local_tz(), Utc::now());
    // Check under the lock, write to the Calendar app or server with the store free (it can take up to 30 s),
    // then record the result under the lock again.
    let prepared = {
        let store = lock_store(state)?;
        firstlight_core::local::prepare_save(&store, id, &calendar_id, &event, tz, now)
            .map_err(fail)?
    };
    let event_id = prepared.write(writer.as_ref(), tz).map_err(fail)?;
    let saved = {
        let mut store = lock_store(state)?;
        firstlight_core::local::commit_save(
            &mut store,
            writer.as_ref(),
            account.as_deref(),
            prepared,
            event_id,
            tz,
            now,
        )
        .map_err(fail)?
    };
    // Bring the saved event back through the normal sync so it appears like any other event.
    state.sync.trigger(Trigger::Manual);
    let _ = app.emit("events-changed", ());
    Ok(saved)
}

/// `dir/stem.ext`, or `dir/stem (2).ext` and so on: never overwrites something the user already has.
pub(crate) fn unique_path(dir: &std::path::Path, stem: &str, ext: &str) -> std::path::PathBuf {
    let mut path = dir.join(format!("{stem}.{ext}"));
    for n in 2..100 {
        if !path.exists() {
            break;
        }
        path = dir.join(format!("{stem} ({n}).{ext}"));
    }
    path
}

#[derive(serde::Serialize)]
pub struct ExportResult {
    path: String,
    count: usize,
}

/// Saves everything the user added as one calendar file in Downloads and shows it in Finder.
#[tauri::command]
pub async fn export_own_events(
    app: AppHandle,
    state: State<'_, AppState>,
) -> CmdResult<ExportResult> {
    use tauri::Manager;
    use tauri_plugin_opener::OpenerExt;
    let exported = {
        let store = lock_store(&state)?;
        firstlight_core::local::export_all(&store, Utc::now())
            .map_err(internal("exporting events"))?
    };
    let Some((text, count)) = exported else {
        return Err("You haven't added any events of your own yet.".into());
    };
    let dir = app
        .path()
        .download_dir()
        .map_err(|_| "Couldn't find your Downloads folder.".to_string())?;
    let path = unique_path(&dir, "firstlight-events", "ics");
    std::fs::write(&path, text).map_err(|_| "Couldn't save the file.".to_string())?;
    let _ = app.opener().reveal_item_in_dir(&path);
    Ok(ExportResult {
        path: path.display().to_string(),
        count,
    })
}

/// An own event as the edit form needs it (with its repeat rule, reminder and invitees).
#[tauri::command]
pub async fn get_own_event(
    state: State<'_, AppState>,
    id: String,
) -> CmdResult<Option<firstlight_core::local::OwnEvent>> {
    let store = lock_store(&state)?;
    firstlight_core::local::get(&store, &id).map_err(user_input_error)
}

/// Removes one day of a repeating event; the rest of the series stays.
#[tauri::command]
pub async fn skip_event_occurrence(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> CmdResult<()> {
    {
        let mut store = lock_store(&state)?;
        firstlight_core::local::skip_occurrence(&mut store, &id, Utc::now())
            .map_err(user_input_error)?;
    }
    let _ = app.emit("events-changed", ());
    Ok(())
}

/// The macOS accent colour as one of our palette names, or `None` for the default blue, graphite,
/// yellow (too pale for text) and every other system.
#[tauri::command]
pub async fn get_system_accent() -> Option<String> {
    system_accent().map(String::from)
}

fn system_accent() -> Option<&'static str> {
    #[cfg(target_os = "macos")]
    {
        let out = std::process::Command::new("defaults")
            .args(["read", "-g", "AppleAccentColor"])
            .output()
            .ok()?;
        accent_name(String::from_utf8_lossy(&out.stdout).trim())
    }
    #[cfg(not(target_os = "macos"))]
    None
}

/// macOS stores the accent as a number; absent means the default multicolour blue. Only called on macOS
/// (and by its test, which runs everywhere), so other systems do not see it as dead code.
#[cfg(any(target_os = "macos", test))]
pub(crate) fn accent_name(value: &str) -> Option<&'static str> {
    match value {
        "0" => Some("red"),
        "1" => Some("orange"),
        "3" => Some("green"),
        "5" => Some("purple"),
        "6" => Some("pink"),
        _ => None, // 4 blue, 2 yellow, -1 graphite, anything unknown
    }
}

/// The exact message the "Review before sending" window shows, so nothing leaves that the person has not read.
#[derive(serde::Serialize)]
pub struct InvitePreview {
    subject: String,
    body: String,
}

/// A mailbox invitations can be sent from.
#[derive(serde::Serialize)]
pub struct Sender {
    id: String,
    address: String,
    label: String,
}

/// Mailboxes the user may send from: none unless they turned sending on, and only Yahoo and iCloud have a
/// built-in outgoing server. When there are none, the app offers no way to invite anyone.
#[tauri::command]
pub async fn invite_senders(state: State<'_, AppState>) -> CmdResult<Vec<Sender>> {
    let settings = lock_store(&state)?.load_settings();
    if !settings.email.send_invitations {
        return Ok(vec![]);
    }
    Ok(settings
        .email
        .imap
        .iter()
        .filter(|m| firstlight_core::mail::smtp::server_for(m.preset).is_some())
        .map(|m| Sender {
            id: m.id.clone(),
            address: m.address.clone(),
            label: m.preset.label().to_string(),
        })
        .collect())
}

/// Everything about an invitation except the people: the event (only ever one of the user's own), its
/// repeat rule, the settings and the mailbox it would come from.
struct InviteBasis {
    settings: firstlight_core::settings::AppSettings,
    event: Event,
    rule: Option<String>,
    mailbox: firstlight_core::mail::imap::ImapMailbox,
}

fn invite_basis(state: &AppState, id: &str, mailbox_id: &str) -> CmdResult<InviteBasis> {
    let (settings, event, rule) = {
        let store = lock_store(state)?;
        let Some((event, rule)) = firstlight_core::local::share_parts(&store, id)
            .ok()
            .flatten()
        else {
            // Only the organiser invites. An event from a connected calendar belongs to someone else.
            return Err("You can only invite people to events you created here.".into());
        };
        (store.load_settings(), event, rule)
    };
    if !settings.email.send_invitations {
        return Err("Turn on “Send invitations from Firstlight” in Settings first.".into());
    }
    let mailbox = settings
        .email
        .imap
        .iter()
        .find(|m| m.id == mailbox_id)
        .ok_or("That mailbox isn't connected.")?
        .clone();
    if firstlight_core::mail::smtp::server_for(mailbox.preset).is_none() {
        return Err("Sending isn't available for that mailbox.".into());
    }
    Ok(InviteBasis {
        settings,
        event,
        rule,
        mailbox,
    })
}

/// "request" (invite, or tell people it changed) or "cancel".
fn invite_method(kind: &str) -> CmdResult<firstlight_core::invite::Method> {
    match kind {
        "request" => Ok(firstlight_core::invite::Method::Request),
        "cancel" => Ok(firstlight_core::invite::Method::Cancel),
        _ => Err("That kind of message can't be sent.".into()),
    }
}

fn invite_text(b: &InviteBasis, method: firstlight_core::invite::Method) -> (String, String) {
    use firstlight_core::invite;
    let h24 = b
        .settings
        .notify_for_engine(crate::notifier::system_uses_24h())
        .time_24h;
    let tz = crate::local_tz();
    match method {
        invite::Method::Request => invite::message(&b.event, tz, h24, true, &b.mailbox.address),
        invite::Method::Cancel => invite::cancel_message(&b.event, tz, h24, &b.mailbox.address),
    }
}

#[tauri::command]
pub async fn invite_preview(
    state: State<'_, AppState>,
    id: String,
    mailbox_id: String,
    kind: String,
) -> CmdResult<InvitePreview> {
    let basis = invite_basis(&state, &id, &mailbox_id)?;
    let (subject, body) = invite_text(&basis, invite_method(&kind)?);
    Ok(InvitePreview { subject, body })
}

/// At most this many messages an hour, so a bug or a stuck button can never become a flood.
const SEND_LIMIT_PER_HOUR: usize = 10;
static SENT: std::sync::Mutex<Vec<std::time::Instant>> = std::sync::Mutex::new(Vec::new());

pub(crate) fn within_send_limit() -> bool {
    let Ok(mut sent) = SENT.lock() else {
        return false;
    };
    let hour = std::time::Duration::from_secs(3600);
    sent.retain(|t| t.elapsed() < hour);
    if sent.len() >= SEND_LIMIT_PER_HOUR {
        return false;
    }
    sent.push(std::time::Instant::now());
    true
}

/// Sends a real calendar invitation (or a cancellation) from the chosen mailbox: the people get Accept and Decline
/// in their own calendar app and answers come back to that mailbox. Only when the user turned sending on, only for
/// an event they created, and only for the exact text `invite_preview` showed. Returns nothing about the message.
#[tauri::command]
pub async fn send_invite_mail(
    state: State<'_, AppState>,
    id: String,
    recipients: String,
    mailbox_id: String,
    kind: String,
) -> CmdResult<()> {
    use firstlight_core::invite;
    use firstlight_core::mail::smtp;
    let to = invite::parse_list(&recipients).map_err(user_input_error)?;
    if to.is_empty() {
        return Err("Add at least one email address.".into());
    }
    if to.len() > smtp::MAX_RECIPIENTS {
        return Err(format!(
            "You can send to at most {} people at once.",
            smtp::MAX_RECIPIENTS
        ));
    }
    let method = invite_method(&kind)?;
    let basis = invite_basis(&state, &id, &mailbox_id)?;
    let server = smtp::server_for(basis.mailbox.preset)
        .ok_or("Sending isn't available for that mailbox.")?;
    let password =
        smtp::password_for(state.deps.secrets.as_ref(), &basis.mailbox.id).map_err(|_| {
            "Firstlight can't find that mailbox's password. Add the mailbox again.".to_string()
        })?;
    let now = Utc::now();
    let (subject, text) = invite_text(&basis, method);
    let ics = invite::ics_itip(
        &basis.event,
        &invite::IcsExtras {
            tz: crate::local_tz(),
            rrule: basis.rule.as_deref(),
            include_notes: true,
            now,
            removed: &[],
            alarm_minutes: None,
        },
        &invite::Schedule {
            method,
            organizer: &basis.mailbox.address,
            attendees: &to,
            sequence: invite::sequence_at(now),
        },
    )
    .map_err(user_input_error)?;
    if !within_send_limit() {
        return Err("That's a lot of messages for one hour. Try again later.".into());
    }
    let mailbox = basis.mailbox;
    tauri::async_runtime::spawn_blocking(move || {
        let transport = smtp::TlsTransport::default();
        smtp::send(
            &transport,
            &server,
            &mailbox.address,
            &password,
            &smtp::Outgoing {
                from: &mailbox.address,
                to: &to,
                subject: &subject,
                text: &text,
                ics: Some(&ics),
                method: method.name(),
                now,
            },
        )
    })
    .await
    .map_err(|_| "Sending was interrupted.".to_string())?
    .map_err(|e| match e {
        Error::Invalid(m) => m.to_string(),
        Error::AuthRevoked => "The mail server didn't accept the saved password. Add the mailbox again with a fresh app password.".to_string(),
        Error::Offline => "Couldn't reach the mail server. Nothing was sent.".to_string(),
        Error::Provider(m) => format!("The mail server didn't send it ({m}). Check the addresses and try again."),
        _ => "The message couldn't be sent.".to_string(),
    })
}

fn save_own_event(
    app: &AppHandle,
    state: &AppState,
    id: Option<&str>,
    event: firstlight_core::local::EventInput,
) -> CmdResult<Event> {
    let saved = {
        let mut store = lock_store(state)?;
        firstlight_core::local::save(&mut store, id, &event, crate::local_tz(), Utc::now())
            .map_err(user_input_error)?
    };
    let _ = app.emit("events-changed", ());
    Ok(saved)
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
        google: state.deps.oauth.google().is_some(),
        microsoft: state.deps.oauth.microsoft().is_some(),
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
        ProviderKind::Google => state.deps.oauth.google(),
        ProviderKind::Microsoft => state.deps.oauth.microsoft(),
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

        let parsed = firstlight_core::http::Url::parse(&url)
            .map_err(|_| "bad sign-in address".to_string())?;
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
    /// What the person calls it here: "Calendar app" or "Outlook".
    name: &'static str,
    /// Whether events can be added here (not for Outlook yet).
    writable: bool,
    /// Whether this computer's mail app can be offered as a mail source: Mail on a Mac, classic Outlook on Windows
    /// (only when `supported`, which for Outlook means it is installed).
    mail_app: bool,
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
        name: state.deps.system.name(),
        writable: state.deps.system.writable(),
        mail_app: cfg!(any(target_os = "macos", windows)),
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
        Access::Denied => return Err("Calendar access is turned off for Firstlight. Open System Settings, switch Firstlight on under Calendars, then try again.".into()),
        Access::Restricted => return Err("This Mac's settings don't allow apps to read calendars.".into()),
        Access::Unsupported if cfg!(windows) => return Err("Firstlight reads classic Outlook, and it isn't installed here. The new Outlook can't be read; connect that account with “Sign in with Microsoft” instead.".into()),
        Access::Unsupported => return Err("The Calendar app can't be used here. It needs macOS 14 or later.".into()),
    }
    let new = NewAccount {
        kind: ProviderKind::System,
        name: state.deps.system.name().into(),
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

/// Today and tomorrow, worked out by the briefing engine (pure; see firstlight-core).
#[tauri::command]
pub async fn get_briefing(
    state: State<'_, AppState>,
) -> CmdResult<firstlight_core::briefing::Briefing> {
    use firstlight_core::briefing::{build, day_bounds};
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

/// A private meeting note as the event sheet shows it.
#[derive(serde::Serialize)]
pub struct MeetingNote {
    text: String,
    /// When it was last changed (RFC 3339).
    updated_at: chrono::DateTime<Utc>,
}

/// The note this meeting shares (every time it happens), if one was written. Kept only on this computer.
#[tauri::command]
pub async fn get_meeting_note(
    state: State<'_, AppState>,
    id: String,
) -> CmdResult<Option<MeetingNote>> {
    let store = lock_store(&state)?;
    let Some(event) = store
        .get_event(&id)
        .map_err(internal("loading the event"))?
    else {
        return Ok(None);
    };
    Ok(store
        .get_meeting_note(&firstlight_core::notes::meeting_key(&event))
        .map_err(internal("loading the note"))?
        .map(|(text, updated_at)| MeetingNote { text, updated_at }))
}

/// Keeps (or, when empty, removes) the note for this meeting. It shows again next time the meeting comes up.
#[tauri::command]
pub async fn save_meeting_note(
    state: State<'_, AppState>,
    id: String,
    text: String,
) -> CmdResult<Option<MeetingNote>> {
    let text = firstlight_core::notes::clean_note(&text).map_err(user_input_error)?;
    let now = Utc::now();
    let store = lock_store(&state)?;
    let event = store
        .get_event(&id)
        .map_err(internal("loading the event"))?
        .ok_or("That event is no longer in your calendar.")?;
    store
        .set_meeting_note(
            &firstlight_core::notes::meeting_key(&event),
            text.as_deref(),
            now,
        )
        .map_err(internal("saving the note"))?;
    Ok(text.map(|text| MeetingNote {
        text,
        updated_at: now,
    }))
}

/// Free stretches in working hours, from now, long enough for `minutes` (weekends skipped).
#[tauri::command]
pub async fn find_free_times(
    state: State<'_, AppState>,
    minutes: i64,
    days: i64,
) -> CmdResult<Vec<firstlight_core::briefing::slots::Slot>> {
    use firstlight_core::briefing::{
        day_bounds,
        slots::{MAX_DAYS, find_free_slots},
    };
    let now = Utc::now();
    let tz = crate::local_tz();
    let days = days.clamp(1, MAX_DAYS);
    let store = lock_store(&state)?;
    let settings = store.load_settings().briefing;
    let today = now.with_timezone(&tz).date_naive();
    let (from, _) = day_bounds(today, tz);
    let (_, to) = day_bounds(today + Duration::days(days), tz);
    let events = dedupe(
        store
            .events_between(from, to)
            .map_err(internal("loading events"))?,
    );
    Ok(find_free_slots(
        &events, today, days, minutes, now, tz, &settings,
    ))
}

/// Which of two overlapping meetings is easier to move, why, and where it could go instead. Suggests only: nothing
/// is changed. `move_id` lets the person choose the other one.
#[tauri::command]
pub async fn get_conflict_help(
    state: State<'_, AppState>,
    a: String,
    b: String,
    move_id: Option<String>,
) -> CmdResult<Option<firstlight_core::conflict::Help>> {
    let (now, tz) = (Utc::now(), crate::local_tz());
    let store = lock_store(&state)?;
    let settings = store.load_settings().briefing;
    let events = dedupe(
        store
            .events_between(now - Duration::days(1), now + Duration::days(21))
            .map_err(internal("loading events"))?,
    );
    let (Some(ea), Some(eb)) = (
        events.iter().find(|e| e.id == a),
        events.iter().find(|e| e.id == b),
    ) else {
        return Ok(None);
    };
    // Only events Firstlight created can be moved by Firstlight: ones kept here, and ones it saved into the
    // Calendar app or a CalDAV account (the same lookup the edit form uses).
    let mine = |e: &Event| matches!(firstlight_core::local::resolve(&store, &e.id), Ok(Some(_)));
    Ok(firstlight_core::conflict::help(
        ea,
        eb,
        &events,
        &mine,
        move_id.as_deref(),
        now,
        tz,
        &settings,
    ))
}

/// Opens a new message in the person's own mail app (a `mailto:` link) so they can read it, add a recipient and
/// send it themselves. Nothing is sent by Firstlight.
#[tauri::command]
pub async fn open_mail_draft(
    app: AppHandle,
    to: Option<String>,
    subject: String,
    body: String,
) -> CmdResult<()> {
    use firstlight_core::invite;
    use tauri_plugin_opener::OpenerExt;
    // One address, or several separated by commas (a recap to everyone at a meeting); each one is checked.
    let recipients: Vec<String> = match to.as_deref().map(str::trim).filter(|t| !t.is_empty()) {
        Some(t) => {
            invite::parse_list(t).map_err(|_| "That isn't a valid email address.".to_string())?
        }
        None => vec![],
    };
    let clip = |s: &str, max: usize| {
        s.chars()
            .filter(|c| !c.is_control() || *c == '\n')
            .take(max)
            .collect::<String>()
    };
    let (subject, body) = (clip(&subject, 200).replace('\n', " "), clip(&body, 1500));
    app.opener()
        .open_url(invite::mailto(&recipients, &subject, &body), None::<&str>)
        .map_err(|_| "Couldn't open your mail app.".to_string())
}

/// The next seven days (from tomorrow): how heavy each is, the busiest and lightest, early starts, the best block for focus.
#[tauri::command]
pub async fn get_week_ahead(
    state: State<'_, AppState>,
) -> CmdResult<firstlight_core::weekahead::WeekAhead> {
    let (now, tz) = (Utc::now(), crate::local_tz());
    let store = lock_store(&state)?;
    let settings = store.load_settings().briefing;
    let first = now.with_timezone(&tz).date_naive() + Duration::days(1);
    let (from, _) = firstlight_core::briefing::day_bounds(first, tz);
    let (_, to) = firstlight_core::briefing::day_bounds(first + Duration::days(6), tz);
    let events = dedupe(
        store
            .events_between(from, to)
            .map_err(internal("loading events"))?,
    );
    Ok(firstlight_core::weekahead::build(
        &events, first, 7, tz, &settings,
    ))
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

use firstlight_core::settings::AppSettings;

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
    s.ai.consented_host = stored.ai.consented_host.clone();
    s.ai.plan_consented = stored.ai.plan_consented;
    // Mailboxes change only through add/remove, which keep their keychain passwords in step.
    s.email.imap = stored.email.imap;
    // Which accounts may send, or keep tasks in step, is recorded only when that permission is granted.
    s.email.send_accounts = stored.email.send_accounts;
    s.email.autoreply_accounts = stored.email.autoreply_accounts;
    s.tasks = stored.tasks;
    s.sanitize();
    // Days off that are over are forgotten, so the list never fills up with old ones.
    s.day
        .drop_past_time_off(Utc::now().with_timezone(&crate::local_tz()).date_naive());
    // Try the shortcut first: if the system refuses it, nothing is saved and the user is told why.
    crate::apply_dock(app, s.general.show_in_dock);
    crate::apply_shortcut(
        app,
        s.general.global_shortcut.as_deref(),
        s.general.join_shortcut.as_deref(),
        s.general.capture_shortcut.as_deref(),
    )?;
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
    let mut path = dir.join("firstlight-settings.json");
    // Never overwrite something the user already has.
    for n in 2..100 {
        if !path.exists() {
            break;
        }
        path = dir.join(format!("firstlight-settings ({n}).json"));
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
        firstlight_core::erase::erase_everything(&store, state.deps.secrets.as_ref())
            .map_err(internal("erasing data"))?
    };
    log::info!(
        "erased {} accounts and {} credentials",
        report.accounts,
        report.credentials
    );
    let _ = app.autolaunch().disable();
    let _ = crate::apply_shortcut(&app, None, None, None);
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
        name: "Firstlight",
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

/// The text of a problem report (shown in full before it is saved).
fn problem_text(app: &AppHandle) -> String {
    use std::io::{Read, Seek, SeekFrom};
    use tauri::Manager;
    let log_dir = app.path().app_log_dir().ok();
    let crash = log_dir.as_deref().and_then(crate::crash_reports::latest);
    // The end of the log (the newest lines), at most 512 KB.
    let mut log = String::new();
    if let Some(mut f) = log_dir
        .as_deref()
        .and_then(|d| std::fs::File::open(d.join("Firstlight.log")).ok())
    {
        let len = f.metadata().map(|m| m.len()).unwrap_or(0);
        let _ = f.seek(SeekFrom::Start(len.saturating_sub(512 * 1024)));
        let mut bytes = Vec::new();
        let _ = f.read_to_end(&mut bytes);
        log = String::from_utf8_lossy(&bytes).into_owned();
    }
    let home = app.path().home_dir().ok().map(|h| h.display().to_string());
    firstlight_core::crash::problem_report(
        &app.package_info().version.to_string(),
        std::env::consts::OS,
        std::env::consts::ARCH,
        crash.as_deref(),
        &log,
        home.as_deref(),
        Utc::now(),
    )
}

/// Where problem reports go, if this build was given an address (`FIRSTLIGHT_SUPPORT_EMAIL` at build time).
pub(crate) fn support_email() -> Option<&'static str> {
    option_env!("FIRSTLIGHT_SUPPORT_EMAIL")
        .filter(|a| firstlight_core::invite::parse_address(a).is_some())
}

#[derive(serde::Serialize)]
pub struct ProblemReport {
    text: String,
    /// The address to email it to, when this build has one.
    support: Option<&'static str>,
}

#[tauri::command]
pub async fn get_problem_report(app: AppHandle) -> CmdResult<ProblemReport> {
    Ok(ProblemReport {
        text: problem_text(&app),
        support: support_email(),
    })
}

/// Saves the report (as the person edited it) to Downloads and shows it there. Nothing is sent.
#[tauri::command]
pub async fn save_problem_report(app: AppHandle, text: String) -> CmdResult<String> {
    use tauri::Manager;
    use tauri_plugin_opener::OpenerExt;
    let text: String = text.chars().take(200_000).collect();
    let dir = app
        .path()
        .download_dir()
        .map_err(|_| "Couldn't find your Downloads folder.".to_string())?;
    let path = unique_path(&dir, "firstlight-problem-report", "txt");
    std::fs::write(&path, text).map_err(|_| "Couldn't save the file.".to_string())?;
    let _ = app.opener().reveal_item_in_dir(&path);
    Ok(path.display().to_string())
}

#[tauri::command]
pub async fn clear_crash_reports(app: AppHandle) -> CmdResult<()> {
    use tauri::Manager;
    if let Ok(d) = app.path().app_log_dir() {
        crate::crash_reports::clear(&d);
    }
    Ok(())
}

#[cfg(test)]
mod accent_tests {
    use super::accent_name;

    #[test]
    fn macos_accent_numbers_map_to_palette_names() {
        assert_eq!(accent_name("0"), Some("red"));
        assert_eq!(accent_name("1"), Some("orange"));
        assert_eq!(accent_name("3"), Some("green"));
        assert_eq!(accent_name("5"), Some("purple"));
        assert_eq!(accent_name("6"), Some("pink"));
        for none in ["4", "2", "-1", "", "99", "red"] {
            assert_eq!(accent_name(none), None, "{none}");
        }
    }
}

/// Hides the quick capture box (Escape, or after something was added).
#[tauri::command]
pub async fn hide_capture(app: AppHandle) -> CmdResult<()> {
    crate::hide_capture(&app);
    Ok(())
}
