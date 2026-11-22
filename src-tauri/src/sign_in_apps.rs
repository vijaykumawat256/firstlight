//! Settings, Accounts: your own Google / Microsoft sign-in app instead of the one built in
//! (see `firstlight_core::oauth::apps`).

use crate::AppState;
use crate::commands::{CmdResult, lock_store};
use crate::providers::Deps;
use firstlight_core::Error;
use firstlight_core::oauth::apps::{self, Change, GOOGLE_SECRET_KEY, OwnApps};
use firstlight_core::store::Store;
use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

/// Puts the stored apps into use (at start and after a change).
pub(crate) fn apply(store: &Store, deps: &Deps) {
    let own = apps::load(store);
    let secret = deps.secrets.get(GOOGLE_SECRET_KEY).ok().flatten();
    deps.oauth.set_own(
        own.google_client_id.map(|id| (id, secret)),
        own.microsoft_client_id,
    );
}

#[derive(Serialize)]
pub struct SignInApps {
    google_client_id: Option<String>,
    google_has_secret: bool,
    microsoft_client_id: Option<String>,
    /// Whether this package has IDs of its own, so leaving a field empty still works.
    built_in_google: bool,
    built_in_microsoft: bool,
}

fn view(own: OwnApps, deps: &Deps) -> SignInApps {
    let (built_in_google, built_in_microsoft) = deps.oauth.built_in();
    SignInApps {
        google_has_secret: own.google_client_id.is_some()
            && deps.secrets.get(GOOGLE_SECRET_KEY).ok().flatten().is_some(),
        google_client_id: own.google_client_id,
        microsoft_client_id: own.microsoft_client_id,
        built_in_google,
        built_in_microsoft,
    }
}

#[tauri::command]
pub async fn get_sign_in_apps(state: State<'_, AppState>) -> CmdResult<SignInApps> {
    let own = apps::load(&*lock_store(&state)?);
    Ok(view(own, &state.deps))
}

/// Saves your own apps (a field left out stays; an empty one is removed) and uses them from now on. Accounts already
/// connected through another app will be asked to sign in again when their sign-in next renews.
#[tauri::command]
pub async fn set_sign_in_apps(
    app: AppHandle,
    state: State<'_, AppState>,
    change: Change,
) -> CmdResult<SignInApps> {
    let own = {
        let store = lock_store(&state)?;
        let own =
            apps::save(&store, state.deps.secrets.as_ref(), &change).map_err(|e| match e {
                Error::Invalid(m) => m.to_string(),
                _ => "Couldn't save the sign-in app.".to_string(),
            })?;
        apply(&store, &state.deps);
        own
    };
    let _ = app.emit("settings-changed", ());
    Ok(view(own, &state.deps))
}
