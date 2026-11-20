//! Sharing between your own computers through a synced folder: the commands and the scheduler hook. The files and
//! the rules are in `firstlight_core::folder_sync` and `firstlight_logic::foldersync`.

use crate::AppState;
use crate::commands::{CmdResult, internal, lock_store};
use crate::providers::Deps;
use chrono::Utc;
use firstlight_core::folder_sync::{self, KEY_SECRET, Report};
use firstlight_core::settings::AppSettings;
use firstlight_core::store::Store;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use tauri::{AppHandle, Emitter, State};

/// This computer's name as the person knows it ("Vijay's MacBook Air"), shown on the other computers.
fn computer_name() -> String {
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

fn key_of(deps: &Deps) -> Option<[u8; 32]> {
    let code = deps.secrets.get(KEY_SECRET).ok().flatten()?;
    folder_sync::parse_code(&code).ok()
}

/// One round, if sharing is on (called after each calendar sync). Returns whether anything changed here.
pub(crate) fn run(store: &mut Store, deps: &Deps) -> bool {
    let Some(folder) = store.load_settings().tasks.folder_sync else {
        return false;
    };
    let Some(key) = key_of(deps) else {
        return false;
    };
    match folder_sync::sync(
        store,
        Path::new(&folder),
        &key,
        &computer_name(),
        Utc::now(),
    ) {
        Ok(r) => {
            if r.applied > 0 {
                log::info!(
                    "folder sync: {} changes from {} other computers",
                    r.applied,
                    r.peers.len()
                );
            }
            r.applied > 0
        }
        Err(_) => {
            log::warn!("folder sync failed");
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
pub struct FolderSyncOn {
    settings: AppSettings,
    /// The sync code to type on your other computers.
    code: String,
    report: Report,
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
        if folder_sync::needs_code(&path, Some(&key), &me) {
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
        s.tasks.folder_sync = Some(path.display().to_string());
        store
            .save_settings(&s)
            .map_err(internal("saving settings"))?;
        let report = folder_sync::sync(&mut store, &path, &key, &computer_name(), Utc::now())
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
    let folder = lock_store(&state)?
        .load_settings()
        .tasks
        .folder_sync
        .ok_or("Sharing between computers is off.")?;
    let key = key_of(&state.deps)
        .ok_or("The sync code is missing on this computer. Turn sharing on again with the code.")?;
    let report = {
        let mut store = lock_store(&state)?;
        folder_sync::sync(
            &mut store,
            Path::new(&folder),
            &key,
            &computer_name(),
            Utc::now(),
        )
        .map_err(|e| e.to_string())?
    };
    if report.applied > 0 {
        let _ = app.emit("tasks-changed", ());
        let _ = app.emit("events-changed", ());
        let _ = app.emit("settings-changed", ());
    }
    Ok(report)
}
