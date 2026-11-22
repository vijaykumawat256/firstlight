//! Automatic backups (see `firstlight_core::backup`): the list for Settings, and putting one back.

use crate::AppState;
use crate::commands::{CmdResult, internal};
use firstlight_core::backup::{self, BackupInfo};
use std::path::{Path, PathBuf};
use tauri::{AppHandle, State};

/// The backups folder for the database at `db_path`.
pub(crate) fn dir(db_path: &Path) -> Option<PathBuf> {
    let data = db_path.parent()?;
    let file = db_path.file_name()?.to_str()?;
    Some(backup::dir_for(data, file))
}

#[tauri::command]
pub async fn list_backups(state: State<'_, AppState>) -> CmdResult<Vec<BackupInfo>> {
    Ok(dir(&state.db_path)
        .map(|d| backup::list(&d))
        .unwrap_or_default())
}

/// Puts `name` back: asks for it, then restarts, and the start swaps it in before opening the database (keeping a
/// copy of what is there now, so this can be undone the same way).
#[tauri::command]
pub async fn restore_backup(
    app: AppHandle,
    state: State<'_, AppState>,
    name: String,
) -> CmdResult<()> {
    let d = dir(&state.db_path).ok_or("The backups folder can't be found.")?;
    backup::request_restore(&d, &name).map_err(internal("asking for the restore"))?;
    log::info!("restore of {name} asked for; restarting");
    app.restart()
}
