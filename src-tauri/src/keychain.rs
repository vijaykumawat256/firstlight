//! OS keychain implementation of [`SecretStore`]
//! (Keychain on macOS, Credential Manager on Windows).

use daybrief_core::secrets::SecretStore;
use daybrief_core::{Error, Result};
use keyring::Entry;

pub struct KeychainStore {
    service: String,
}

impl KeychainStore {
    pub fn new(service: String) -> Self {
        Self { service }
    }

    fn entry(&self, key: &str) -> Result<Entry> {
        Entry::new(&self.service, key).map_err(map_err)
    }
}

// keyring errors describe the failure class, never the secret.
fn map_err(e: keyring::Error) -> Error {
    Error::Secret(e.to_string())
}

impl SecretStore for KeychainStore {
    fn get(&self, key: &str) -> Result<Option<String>> {
        match self.entry(key)?.get_password() {
            Ok(v) => Ok(Some(v)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(map_err(e)),
        }
    }

    fn set(&self, key: &str, value: &str) -> Result<()> {
        self.entry(key)?.set_password(value).map_err(map_err)
    }

    fn delete(&self, key: &str) -> Result<()> {
        match self.entry(key)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(map_err(e)),
        }
    }
}
