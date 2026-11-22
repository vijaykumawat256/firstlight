//! Sharing between your own computers through a synced folder: the commands and the scheduler hook. The files and
//! the rules are in `firstlight_core::folder_sync` and `firstlight_logic::foldersync`.

use crate::AppState;
use crate::commands::{CmdResult, internal, lock_store};
use crate::providers::Deps;
use chrono::Utc;
use firstlight_core::Error;
use firstlight_core::folder_sync::{self, FolderFiles, KEY_SECRET, Report, SyncFiles, cloud};
use firstlight_core::model::{Account, ProviderKind};
use firstlight_core::oauth::TokenManager;
use firstlight_core::settings::AppSettings;
use firstlight_core::store::Store;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use tauri::{AppHandle, Emitter, State};

/// This computer's name as the person knows it ("Vijay's MacBook Air"), shown on the other computers.
pub(crate) fn computer_name() -> String {
    static NAME: OnceLock<String> = OnceLock::new();
    NAME.get_or_init(|| {
        #[cfg(target_os = "macos")]
        {
            if let Ok(out) = std::process::Command::new("/usr/sbin/scutil")
                .args(["--get", "ComputerName"])
                .output()
                && out.status.success()
            {
                let n = String::from_utf8_lossy(&out.stdout).trim().to_string();
                if !n.is_empty() {
                    return n;
                }
            }
        }
        std::env::var("COMPUTERNAME")
            .ok()
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| "This computer".into())
    })
    .clone()
}

pub(crate) fn key_of(deps: &Deps) -> Option<[u8; 32]> {
    let code = deps.secrets.get(KEY_SECRET).ok().flatten()?;
    folder_sync::parse_code(&code).ok()
}

/// Where the files are kept, from the settings: Google Drive / OneDrive (an account) or a synced folder.
fn place(store: &Store, deps: &Deps) -> Result<Option<Box<dyn SyncFiles>>, String> {
    let s = store.load_settings();
    if let Some(id) = s.tasks.cloud_sync {
        let account = store
            .get_account(&id)
            .map_err(internal("loading the account"))?
            .ok_or("The account used for sharing was removed.")?;
        return cloud_files(deps, &account).map(Some);
    }
    Ok(s.tasks
        .folder_sync
        .map(|f| Box::new(FolderFiles::new(Path::new(&f))) as Box<dyn SyncFiles>))
}

fn cloud_files(deps: &Deps, account: &Account) -> Result<Box<dyn SyncFiles>, String> {
    let cfg = match account.provider {
        ProviderKind::Google => deps.oauth.google(),
        ProviderKind::Microsoft => deps.oauth.microsoft(),
        _ => None,
    }
    .ok_or("Sign-in for this account isn't set up in this copy of Firstlight.")?;
    let tokens = TokenManager {
        http: deps.http.clone(),
        secrets: deps.secrets.clone(),
        cfg,
    };
    Ok(match account.provider {
        ProviderKind::Google => Box::new(cloud::GoogleDriveFiles::new(
            deps.http.clone(),
            tokens,
            &account.id,
            cloud::GOOGLE_API,
        )),
        _ => Box::new(cloud::OneDriveFiles::new(
            deps.http.clone(),
            tokens,
            &account.id,
            cloud::GRAPH_API,
        )),
    })
}

/// What a failure means for the person.
fn explain(e: Error) -> String {
    match e {
        Error::AuthRevoked => "The account used for sharing needs you to sign in again (Accounts, Reconnect).".into(),
        Error::PermissionNeeded => "Firstlight isn't allowed to use that account's app folder any more. Turn sharing on again to allow it.".into(),
        Error::Offline => "Couldn't reach the cloud storage. Check your connection and try again.".into(),
        Error::Invalid(m) => m.to_string(),
        _ => "Sharing didn't work this time. Try again in a moment.".into(),
    }
}

/// One round, if sharing is on (called after each calendar sync). Returns whether anything changed here.
pub(crate) fn run(store: &mut Store, deps: &Deps) -> bool {
    let Ok(Some(files)) = place(store, deps) else {
        return false;
    };
    let Some(key) = key_of(deps) else {
        return false;
    };
    match folder_sync::sync(store, files.as_ref(), &key, &computer_name(), Utc::now()) {
        Ok(r) => {
            if r.applied > 0 {
                log::info!(
                    "sharing: {} changes from {} other computers",
                    r.applied,
                    r.peers.len()
                );
            }
            r.applied > 0
        }
        Err(e) => {
            log::warn!("sharing failed: {e}");
            false
        }
    }
}

#[derive(Serialize)]
pub struct FolderChoice {
    label: String,
    path: String,
}

/// Synced folders found on this computer (iCloud Drive, OneDrive, Dropbox, Google Drive), to pick from.
#[tauri::command]
pub async fn folder_sync_choices() -> CmdResult<Vec<FolderChoice>> {
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from);
    let mut found: Vec<(String, PathBuf)> = Vec::new();
    if let Some(h) = &home {
        found.push((
            "iCloud Drive".into(),
            h.join("Library/Mobile Documents/com~apple~CloudDocs"),
        ));
        found.push(("iCloud Drive".into(), h.join("iCloudDrive")));
        found.push(("Dropbox".into(), h.join("Dropbox")));
        found.push(("OneDrive".into(), h.join("OneDrive")));
        // macOS puts the newer cloud folders here (OneDrive-Company, GoogleDrive-name, Dropbox).
        if let Ok(entries) = std::fs::read_dir(h.join("Library/CloudStorage")) {
            for e in entries.flatten() {
                let name = e.file_name().to_string_lossy().to_string();
                let label = if name.starts_with("OneDrive") {
                    "OneDrive"
                } else if name.starts_with("GoogleDrive") {
                    "Google Drive"
                } else if name.starts_with("Dropbox") {
                    "Dropbox"
                } else {
                    continue;
                };
                let p = e.path();
                let my_drive = p.join("My Drive");
                found.push((label.into(), if my_drive.is_dir() { my_drive } else { p }));
            }
        }
    }
    if let Some(od) = std::env::var_os("OneDrive") {
        found.push(("OneDrive".into(), PathBuf::from(od)));
    }
    if cfg!(windows) {
        found.push(("Google Drive".into(), PathBuf::from("G:\\My Drive")));
    }
    let mut seen = std::collections::HashSet::new();
    Ok(found
        .into_iter()
        .filter(|(_, p)| p.is_dir() && seen.insert(p.clone()))
        .map(|(label, p)| FolderChoice {
            label,
            path: p.display().to_string(),
        })
        .collect())
}

fn check_folder(folder: &str) -> CmdResult<PathBuf> {
    let p = PathBuf::from(folder.trim());
    if folder.trim().is_empty() || !p.is_absolute() || p.parent().is_none() {
        return Err("Choose a folder (its full path).".into());
    }
    if !p.is_dir() {
        return Err("That folder doesn't exist on this computer.".into());
    }
    Ok(p)
}

#[derive(Serialize)]
pub(crate) struct FolderSyncOn {
    settings: AppSettings,
    /// The sync code to type on your other computers.
    code: String,
    report: Report,
}

impl FolderSyncOn {
    pub(crate) fn new(settings: AppSettings, code: String, report: Report) -> Self {
        Self {
            settings,
            code,
            report,
        }
    }
}

/// Starts sharing through `folder`. On the first computer `code` is empty and a new code is made; on the others it is
/// the code shown on the first one (needed when the folder already has files this computer cannot open).
#[tauri::command]
pub async fn enable_folder_sync(
    app: AppHandle,
    state: State<'_, AppState>,
    folder: String,
    code: Option<String>,
) -> CmdResult<FolderSyncOn> {
    let path = check_folder(&folder)?;
    let typed = code.map(|c| c.trim().to_string()).filter(|c| !c.is_empty());
    let (code, key) = match typed {
        Some(c) => {
            let key = folder_sync::parse_code(&c).map_err(|e| e.to_string())?;
            (folder_sync::code_of(&key), key)
        }
        None => match key_of(&state.deps) {
            Some(k) => (folder_sync::code_of(&k), k),
            None => folder_sync::new_code().map_err(|e| e.to_string())?,
        },
    };
    {
        let store = lock_store(&state)?;
        let me = folder_sync::device_id(&store).map_err(internal("naming this computer"))?;
        if folder_sync::needs_code(&FolderFiles::new(&path), Some(&key), &me) {
            return Err("That folder already has Firstlight data from another computer. Type the sync code shown on that computer.".into());
        }
    }
    state
        .deps
        .secrets
        .set(KEY_SECRET, &code)
        .map_err(internal("saving the sync code"))?;
    let (settings, report) = {
        let mut store = lock_store(&state)?;
        let mut s = store.load_settings();
        if s.tasks.folder_sync.as_deref() != Some(&path.display().to_string()) {
            folder_sync::forget_place(&store)
                .map_err(internal("switching where files are kept"))?;
        }
        s.tasks.folder_sync = Some(path.display().to_string());
        s.tasks.cloud_sync = None;
        store
            .save_settings(&s)
            .map_err(internal("saving settings"))?;
        let report = folder_sync::sync(
            &mut store,
            &FolderFiles::new(&path),
            &key,
            &computer_name(),
            Utc::now(),
        )
        .map_err(|e| e.to_string())?;
        (s, report)
    };
    let _ = app.emit("settings-changed", ());
    let _ = app.emit("tasks-changed", ());
    let _ = app.emit("events-changed", ());
    Ok(FolderSyncOn {
        settings,
        code,
        report,
    })
}

/// Stops sharing on this computer. The files stay in the folder (other computers keep their copies); the code stays in
/// the keychain so turning it on again needs no new code.
#[tauri::command]
pub async fn disable_folder_sync(
    app: AppHandle,
    state: State<'_, AppState>,
) -> CmdResult<AppSettings> {
    let store = lock_store(&state)?;
    let mut s = store.load_settings();
    s.tasks.folder_sync = None;
    store
        .save_settings(&s)
        .map_err(internal("saving settings"))?;
    drop(store);
    let _ = app.emit("settings-changed", ());
    Ok(s)
}

/// The sync code, to type on another computer.
#[tauri::command]
pub async fn folder_sync_code(state: State<'_, AppState>) -> CmdResult<Option<String>> {
    Ok(key_of(&state.deps).map(|k| folder_sync::code_of(&k)))
}

/// Shares now (the "Sync now" button). Says which other computers were seen.
#[tauri::command]
pub async fn sync_folder_now(app: AppHandle, state: State<'_, AppState>) -> CmdResult<Report> {
    let key = key_of(&state.deps)
        .ok_or("The sync code is missing on this computer. Turn sharing on again with the code.")?;
    let deps = state.deps.clone();
    let (db, db_key) = (state.db_path.clone(), state.key.clone());
    // Its own connection, so a slow upload never holds up the rest of the app.
    let report = tauri::async_runtime::spawn_blocking(move || -> Result<Report, String> {
        let mut store = Store::open(&db, &db_key).map_err(internal("opening the database"))?;
        let files = place(&store, &deps)?.ok_or("Sharing between computers is off.")?;
        folder_sync::sync(
            &mut store,
            files.as_ref(),
            &key,
            &computer_name(),
            Utc::now(),
        )
        .map_err(explain)
    })
    .await
    .map_err(|_| "sharing failed".to_string())??;
    if report.applied > 0 {
        let _ = app.emit("tasks-changed", ());
        let _ = app.emit("events-changed", ());
        let _ = app.emit("settings-changed", ());
    }
    Ok(report)
}

/// Starts sharing through the cloud storage of a connected Google or Microsoft account (Google Drive's hidden app
/// folder or OneDrive's app folder), for computers with no synced folder. Asks the account for that one permission
/// first (in the browser). `code` as for `enable_folder_sync`.
#[tauri::command]
pub async fn enable_cloud_sync(
    app: AppHandle,
    state: State<'_, AppState>,
    account_id: String,
    code: Option<String>,
) -> CmdResult<FolderSyncOn> {
    let account = lock_store(&state)?
        .get_account(&account_id)
        .map_err(internal("loading the account"))?
        .ok_or("That account no longer exists.")?;
    let scope = match account.provider {
        ProviderKind::Google => cloud::GOOGLE_SCOPE,
        ProviderKind::Microsoft => cloud::MICROSOFT_SCOPE,
        _ => return Err("Only a Google or Microsoft account can keep the files.".into()),
    };
    let typed = code.map(|c| c.trim().to_string()).filter(|c| !c.is_empty());
    let (code, key) = match typed {
        Some(c) => {
            let key = folder_sync::parse_code(&c).map_err(|e| e.to_string())?;
            (folder_sync::code_of(&key), key)
        }
        None => match key_of(&state.deps) {
            Some(k) => (folder_sync::code_of(&k), k),
            None => folder_sync::new_code().map_err(|e| e.to_string())?,
        },
    };
    crate::mail_ai::upgrade_grant(&app, &state, &account, scope).await?;
    let files = cloud_files(&state.deps, &account)?;
    let me =
        folder_sync::device_id(&*lock_store(&state)?).map_err(internal("naming this computer"))?;
    let (files, needs) = tauri::async_runtime::spawn_blocking(move || {
        let needs = folder_sync::needs_code(files.as_ref(), Some(&key), &me);
        (files, needs)
    })
    .await
    .map_err(|_| "sharing failed".to_string())?;
    if needs {
        return Err("That account's storage already has Firstlight data from another computer. Type the sync code shown on that computer.".into());
    }
    state
        .deps
        .secrets
        .set(KEY_SECRET, &code)
        .map_err(internal("saving the sync code"))?;
    let settings = {
        let store = lock_store(&state)?;
        let mut s = store.load_settings();
        if s.tasks.cloud_sync.as_deref() != Some(account.id.as_str()) {
            folder_sync::forget_place(&store)
                .map_err(internal("switching where files are kept"))?;
        }
        s.tasks.cloud_sync = Some(account.id.clone());
        s.tasks.folder_sync = None;
        store
            .save_settings(&s)
            .map_err(internal("saving settings"))?;
        s
    };
    let (db, db_key) = (state.db_path.clone(), state.key.clone());
    let report = tauri::async_runtime::spawn_blocking(move || -> Result<Report, String> {
        let mut store = Store::open(&db, &db_key).map_err(internal("opening the database"))?;
        folder_sync::sync(
            &mut store,
            files.as_ref(),
            &key,
            &computer_name(),
            Utc::now(),
        )
        .map_err(explain)
    })
    .await
    .map_err(|_| "sharing failed".to_string())??;
    let _ = app.emit("settings-changed", ());
    let _ = app.emit("tasks-changed", ());
    let _ = app.emit("events-changed", ());
    Ok(FolderSyncOn {
        settings,
        code,
        report,
    })
}

/// Stops sharing through cloud storage on this computer. The files stay in that storage for the other devices.
#[tauri::command]
pub async fn disable_cloud_sync(
    app: AppHandle,
    state: State<'_, AppState>,
) -> CmdResult<AppSettings> {
    let store = lock_store(&state)?;
    let mut s = store.load_settings();
    s.tasks.cloud_sync = None;
    store
        .save_settings(&s)
        .map_err(internal("saving settings"))?;
    drop(store);
    let _ = app.emit("settings-changed", ());
    Ok(s)
}
