//! Your own sign-in app ("app registration") for Google and Microsoft, set in Settings instead of built in.
//!
//! Normally the IDs are built into the app by whoever publishes it (`.env` / repository secrets), and nobody using it
//! needs to know they exist. These override them: for a build without IDs, or a company whose IT only allows its own
//! registered apps. The IDs identify an app and are not passwords; Google's desktop "client secret" is not
//! confidential either, but it still goes in the keychain like everything secret-shaped.
//!
//! Kept in the database's settings table under [`KV_KEY`], NOT in the settings document, so they are never exported
//! or imported: a settings file someone sends you can't swap in their sign-in app.

use crate::secrets::SecretStore;
use crate::store::Store;
use crate::{Error, Result};
use serde::{Deserialize, Serialize};

pub const KV_KEY: &str = "oauth_apps";
/// Keychain entry for the Google client secret of your own app.
pub const GOOGLE_SECRET_KEY: &str = "oauth-google-client-secret";

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct OwnApps {
    #[serde(default)]
    pub google_client_id: Option<String>,
    #[serde(default)]
    pub microsoft_client_id: Option<String>,
}

/// A Google OAuth client ID: `<digits>-<letters and digits>.apps.googleusercontent.com`.
pub fn valid_google_client_id(s: &str) -> bool {
    let Some(head) = s.strip_suffix(".apps.googleusercontent.com") else {
        return false;
    };
    !head.is_empty()
        && head.len() <= 150
        && head
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// Google's desktop client secret (`GOCSPX-...`): plain printable characters, no spaces.
pub fn valid_google_secret(s: &str) -> bool {
    (10..=200).contains(&s.len()) && s.bytes().all(|b| b.is_ascii_graphic())
}

/// A Microsoft application (client) ID: a GUID, `xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx`.
pub fn valid_microsoft_client_id(s: &str) -> bool {
    let parts: Vec<&str> = s.split('-').collect();
    parts.len() == 5
        && parts
            .iter()
            .zip([8, 4, 4, 4, 12])
            .all(|(p, n)| p.len() == n && p.bytes().all(|b| b.is_ascii_hexdigit()))
}

pub fn load(store: &Store) -> OwnApps {
    store
        .get_setting(KV_KEY)
        .ok()
        .flatten()
        .and_then(|v| serde_json::from_value::<OwnApps>(v).ok())
        .map(|a| OwnApps {
            // A stored value that no longer passes (edited by hand) is ignored, never used.
            google_client_id: a.google_client_id.filter(|i| valid_google_client_id(i)),
            microsoft_client_id: a
                .microsoft_client_id
                .filter(|i| valid_microsoft_client_id(i)),
        })
        .unwrap_or_default()
}

/// What to change. `None` leaves a field as it is; `Some("")` removes it.
#[derive(Debug, Default, Deserialize)]
pub struct Change {
    pub google_client_id: Option<String>,
    pub google_client_secret: Option<String>,
    pub microsoft_client_id: Option<String>,
}

/// Checks and saves a change; returns what is stored now. Nothing is saved if any field is wrong.
pub fn save(store: &Store, secrets: &dyn SecretStore, change: &Change) -> Result<OwnApps> {
    let tidy = |s: &Option<String>| s.as_ref().map(|v| v.trim().to_string());
    let (g, gs, m) = (
        tidy(&change.google_client_id),
        tidy(&change.google_client_secret),
        tidy(&change.microsoft_client_id),
    );
    if let Some(id) = g.as_deref().filter(|i| !i.is_empty())
        && !valid_google_client_id(id)
    {
        return Err(Error::Invalid(
            "That isn't a Google client ID. It ends in .apps.googleusercontent.com.",
        ));
    }
    if let Some(sec) = gs.as_deref().filter(|s| !s.is_empty())
        && !valid_google_secret(sec)
    {
        return Err(Error::Invalid(
            "That isn't a Google client secret. It usually starts with GOCSPX-.",
        ));
    }
    if let Some(id) = m.as_deref().filter(|i| !i.is_empty())
        && !valid_microsoft_client_id(id)
    {
        return Err(Error::Invalid(
            "That isn't a Microsoft application ID. It looks like 1a2b3c4d-1234-5678-9abc-1a2b3c4d5e6f.",
        ));
    }
    let mut apps = load(store);
    if let Some(id) = g {
        apps.google_client_id = (!id.is_empty()).then_some(id);
    }
    if let Some(id) = m {
        apps.microsoft_client_id = (!id.is_empty()).then_some(id.to_ascii_lowercase());
    }
    match gs.as_deref() {
        Some("") => secrets.delete(GOOGLE_SECRET_KEY)?,
        Some(sec) => secrets.set(GOOGLE_SECRET_KEY, sec)?,
        None => {}
    }
    // Without your own Google ID, a secret for it means nothing.
    if apps.google_client_id.is_none() {
        secrets.delete(GOOGLE_SECRET_KEY)?;
    }
    store.set_setting(
        KV_KEY,
        &serde_json::to_value(&apps).map_err(|_| Error::Invalid("couldn't save"))?,
    )?;
    Ok(apps)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::secrets::{MemorySecretStore, load_or_create_db_key};

    fn store() -> Store {
        let key = load_or_create_db_key(&MemorySecretStore::default()).unwrap();
        Store::open_in_memory(&key).unwrap()
    }

    const G: &str = "123456789012-abcdefghijklmnopqrstuvwxyz012345.apps.googleusercontent.com";
    const M: &str = "1A2B3C4D-1234-5678-9ABC-1A2B3C4D5E6F";

    #[test]
    fn ids_are_recognised_and_typos_refused() {
        assert!(valid_google_client_id(G));
        assert!(!valid_google_client_id(
            "123.apps.googleusercontent.com.evil"
        ));
        assert!(!valid_google_client_id(
            "Hello World.apps.googleusercontent.com"
        ));
        assert!(!valid_google_client_id(".apps.googleusercontent.com"));
        assert!(valid_microsoft_client_id(M));
        assert!(!valid_microsoft_client_id("1a2b3c4d-1234-5678-9abc"));
        assert!(!valid_microsoft_client_id(
            "zzzzzzzz-1234-5678-9abc-1a2b3c4d5e6f"
        ));
        assert!(valid_google_secret("GOCSPX-abcdefghijklmnop"));
        assert!(!valid_google_secret("has a space in it"));
    }

    #[test]
    fn saving_checks_everything_first_and_keeps_the_secret_in_the_keychain() {
        let (s, secrets) = (store(), MemorySecretStore::default());
        assert_eq!(load(&s), OwnApps::default());
        // One bad field: nothing is saved.
        let bad = Change {
            google_client_id: Some(G.into()),
            microsoft_client_id: Some("nope".into()),
            ..Default::default()
        };
        assert!(save(&s, &secrets, &bad).is_err());
        assert_eq!(load(&s), OwnApps::default());

        let apps = save(
            &s,
            &secrets,
            &Change {
                google_client_id: Some(format!("  {G} ")),
                google_client_secret: Some("GOCSPX-abcdefghijklmnop".into()),
                microsoft_client_id: Some(M.into()),
            },
        )
        .unwrap();
        assert_eq!(apps.google_client_id.as_deref(), Some(G));
        assert_eq!(
            apps.microsoft_client_id.as_deref(),
            Some(M.to_ascii_lowercase().as_str())
        );
        assert_eq!(load(&s), apps);
        assert_eq!(
            secrets.get(GOOGLE_SECRET_KEY).unwrap().as_deref(),
            Some("GOCSPX-abcdefghijklmnop")
        );
        // The secret is not in the database.
        assert!(
            !serde_json::to_string(&s.get_setting(KV_KEY).unwrap())
                .unwrap()
                .contains("GOCSPX")
        );

        // Leaving a field out keeps it; an empty one removes it (and Google's secret with its ID).
        let apps = save(
            &s,
            &secrets,
            &Change {
                google_client_id: Some(String::new()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(apps.google_client_id, None);
        assert!(apps.microsoft_client_id.is_some());
        assert_eq!(secrets.get(GOOGLE_SECRET_KEY).unwrap(), None);
    }

    #[test]
    fn they_are_never_part_of_a_settings_export() {
        let (s, secrets) = (store(), MemorySecretStore::default());
        save(
            &s,
            &secrets,
            &Change {
                microsoft_client_id: Some(M.into()),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(
            !s.load_settings()
                .export()
                .to_ascii_lowercase()
                .contains(&M.to_ascii_lowercase())
        );
    }
}
