//! Secret storage abstraction. The OS-keychain implementation lives in the
//! Tauri shell; this crate only defines the trait, an in-memory test double,
//! and the logic that decides what to store.

use crate::{Error, Result};
use std::collections::HashMap;
use std::fmt;
use std::sync::Mutex;

pub trait SecretStore: Send + Sync {
    fn get(&self, key: &str) -> Result<Option<String>>;
    fn set(&self, key: &str, value: &str) -> Result<()>;
    fn delete(&self, key: &str) -> Result<()>;
}

/// A secret value whose `Debug`/`Display` never reveal the contents.
#[derive(Clone)]
pub struct Redacted(String);

impl Redacted {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Redacted {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("[redacted]")
    }
}
impl fmt::Display for Redacted {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("[redacted]")
    }
}

/// In-memory store for tests and the browser/dev fallback. Never used in release builds of the app.
#[derive(Default)]
pub struct MemorySecretStore(Mutex<HashMap<String, String>>);

impl SecretStore for MemorySecretStore {
    fn get(&self, key: &str) -> Result<Option<String>> {
        Ok(self
            .0
            .lock()
            .map_err(|_| Error::Secret("poisoned".into()))?
            .get(key)
            .cloned())
    }
    fn set(&self, key: &str, value: &str) -> Result<()> {
        self.0
            .lock()
            .map_err(|_| Error::Secret("poisoned".into()))?
            .insert(key.into(), value.into());
        Ok(())
    }
    fn delete(&self, key: &str) -> Result<()> {
        self.0
            .lock()
            .map_err(|_| Error::Secret("poisoned".into()))?
            .remove(key);
        Ok(())
    }
}

pub const DB_KEY_NAME: &str = "database-key";

/// Returns the database key (64 hex chars = 32 random bytes), creating and
/// storing it in the secret store on first use.
pub fn load_or_create_db_key(store: &dyn SecretStore) -> Result<Redacted> {
    if let Some(existing) = store.get(DB_KEY_NAME)? {
        if existing.len() == 64 && existing.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Ok(Redacted::new(existing));
        }
        return Err(Error::Secret("stored database key is malformed".into()));
    }
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).map_err(|e| Error::Secret(format!("no randomness: {e}")))?;
    let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    store.set(DB_KEY_NAME, &hex)?;
    Ok(Redacted::new(hex))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_is_created_once_and_reused() {
        let s = MemorySecretStore::default();
        let a = load_or_create_db_key(&s).unwrap();
        let b = load_or_create_db_key(&s).unwrap();
        assert_eq!(a.expose(), b.expose());
        assert_eq!(a.expose().len(), 64);
    }

    #[test]
    fn keys_differ_between_stores() {
        let a = load_or_create_db_key(&MemorySecretStore::default()).unwrap();
        let b = load_or_create_db_key(&MemorySecretStore::default()).unwrap();
        assert_ne!(a.expose(), b.expose());
    }

    #[test]
    fn malformed_key_is_rejected_not_replaced() {
        let s = MemorySecretStore::default();
        s.set(DB_KEY_NAME, "short").unwrap();
        assert!(load_or_create_db_key(&s).is_err());
        assert_eq!(s.get(DB_KEY_NAME).unwrap().as_deref(), Some("short"));
    }

    #[test]
    fn redacted_never_prints() {
        let r = Redacted::new("hunter2");
        assert!(!format!("{r:?} {r}").contains("hunter2"));
    }
}
