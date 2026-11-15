//! Updates. Packages built for release include Tauri's updater, which downloads a new version and refuses it
//! unless its signature verifies against the public key built into the app. Ordinary development builds have no
//! updater, and every function here then reports "not configured".
//!
//! The address is baked in at build time (`FIRSTLIGHT_UPDATE_BASE`), never read from settings or the environment
//! at run time, so nothing a user or a file can change redirects where updates come from.

use serde::Serialize;

// Most variants are only constructed in packages built with the updater.
#[cfg_attr(not(feature = "updater"), allow(dead_code))]
#[derive(Serialize, Clone)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum UpdateStatus {
    /// This build cannot update itself (a development build, or no update address was configured).
    NotConfigured,
    UpToDate,
    Available {
        version: String,
    },
    Failed,
}

/// The update address built into this package, if any.
pub fn configured_base() -> Option<&'static str> {
    option_env!("FIRSTLIGHT_UPDATE_BASE").filter(|b| !b.is_empty())
}

#[cfg(feature = "updater")]
mod real {
    use super::*;
    use firstlight_core::settings::UpdateChannel;
    use firstlight_core::updates::manifest_url;
    use tauri::AppHandle;
    use tauri_plugin_updater::UpdaterExt;

    fn updater(app: &AppHandle, channel: UpdateChannel) -> Option<tauri_plugin_updater::Updater> {
        let url = manifest_url(configured_base()?, channel)?.parse().ok()?;
        app.updater_builder()
            .endpoints(vec![url])
            .ok()?
            .build()
            .ok()
    }

    pub async fn check(app: &AppHandle, channel: UpdateChannel) -> UpdateStatus {
        let Some(u) = updater(app, channel) else {
            return UpdateStatus::NotConfigured;
        };
        match u.check().await {
            Ok(Some(update)) => UpdateStatus::Available {
                version: update.version,
            },
            Ok(None) => UpdateStatus::UpToDate,
            Err(e) => {
                log::warn!("update check failed: {e}");
                UpdateStatus::Failed
            }
        }
    }

    /// Downloads, verifies the signature, installs and restarts.
    pub async fn install(app: &AppHandle, channel: UpdateChannel) -> Result<(), String> {
        let u = updater(app, channel).ok_or("Updates aren't set up in this build.")?;
        let update = u
            .check()
            .await
            .map_err(|_| "Couldn't check for updates.".to_string())?
            .ok_or("You're already up to date.")?;
        update
            .download_and_install(|_, _| {}, || {})
            .await
            .map_err(|_| "The update couldn't be installed. Nothing was changed.".to_string())?;
        app.restart()
    }
}

#[cfg(not(feature = "updater"))]
mod real {
    use super::*;
    use firstlight_core::settings::UpdateChannel;
    use tauri::AppHandle;

    pub async fn check(_: &AppHandle, _: UpdateChannel) -> UpdateStatus {
        UpdateStatus::NotConfigured
    }

    pub async fn install(_: &AppHandle, _: UpdateChannel) -> Result<(), String> {
        Err("Updates aren't set up in this build.".into())
    }
}

pub use real::{check, install};
