//! Secrets on Android: the [`SecretStore`] behind the database key, sign-ins, app passwords and the sync code. The
//! values are sealed by the Kotlin side with a key in the Android Keystore (`SecretsPlugin.kt` in
//! `gen/android/app/src/main/java/...`). The `keyring` crate has no Android backend: it would silently keep secrets
//! in memory, so the database key would be lost at every restart.

use firstlight_core::secrets::SecretStore;
use firstlight_core::{Error, Result};
use serde::{Deserialize, Serialize};
use tauri::plugin::{Builder, PluginHandle, TauriPlugin};
use tauri::{Manager, Wry};

const PLUGIN_PACKAGE: &str = "io.github.vijaykumawat256.firstlight";

/// The handle the setup step reads to build the store.
pub(crate) struct Handle(pub PluginHandle<Wry>);

pub(crate) fn init() -> TauriPlugin<Wry> {
    Builder::<Wry, ()>::new("fl-secrets")
        .setup(|app, api| {
            let handle = api.register_android_plugin(PLUGIN_PACKAGE, "SecretsPlugin")?;
            app.manage(Handle(handle));
            Ok(())
        })
        .build()
}

pub(crate) struct AndroidSecretStore {
    handle: PluginHandle<Wry>,
    /// Kept apart per profile, as the keychain service is on a computer.
    service: String,
}

impl AndroidSecretStore {
    pub(crate) fn new(handle: PluginHandle<Wry>, service: String) -> Self {
        Self { handle, service }
    }

    fn name(&self, key: &str) -> String {
        format!("{}/{key}", self.service)
    }
}

#[derive(Serialize)]
struct Args<'a> {
    key: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    value: Option<&'a str>,
}

#[derive(Deserialize, Default)]
struct Got {
    #[serde(default)]
    value: Option<String>,
}

#[derive(Deserialize)]
struct Nothing {}

fn failed(e: tauri::plugin::mobile::PluginInvokeError) -> Error {
    // The Kotlin side reports the class of failure only, never a value.
    Error::Secret(e.to_string())
}

impl SecretStore for AndroidSecretStore {
    fn get(&self, key: &str) -> Result<Option<String>> {
        let name = self.name(key);
        let got: Got = self
            .handle
            .run_mobile_plugin(
                "get",
                Args {
                    key: &name,
                    value: None,
                },
            )
            .map_err(failed)?;
        Ok(got.value)
    }

    fn set(&self, key: &str, value: &str) -> Result<()> {
        let name = self.name(key);
        let _: Option<Nothing> = self
            .handle
            .run_mobile_plugin(
                "set",
                Args {
                    key: &name,
                    value: Some(value),
                },
            )
            .map_err(failed)?;
        Ok(())
    }

    fn delete(&self, key: &str) -> Result<()> {
        let name = self.name(key);
        let _: Option<Nothing> = self
            .handle
            .run_mobile_plugin(
                "delete",
                Args {
                    key: &name,
                    value: None,
                },
            )
            .map_err(failed)?;
        Ok(())
    }
}
