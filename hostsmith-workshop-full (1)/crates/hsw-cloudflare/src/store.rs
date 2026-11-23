//! Token storage (TL-10). The production store is the OS credential store
//! (Windows Credential Manager via keyring, Windows builds only). An
//! in-memory store exists for tests and non-Windows development builds and
//! is documented as NOT persistent - the interface surfaces this.

use serde::Serialize;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use thiserror::Error;

#[derive(Debug, Error, Serialize)]
#[error("credential store error: {message}")]
pub struct StoreError {
    pub message: String,
}

pub trait TokenStore: Send + Sync {
    fn save(&self, key: &str, secret: &str) -> Result<(), StoreError>;
    fn load(&self, key: &str) -> Result<Option<String>, StoreError>;
    fn delete(&self, key: &str) -> Result<(), StoreError>;
}

/// Non-persistent store for tests and non-Windows development builds. It is
/// never used on production Windows builds (see [`default_store`]).
#[derive(Default)]
pub struct MemoryStore {
    entries: Mutex<HashMap<String, String>>,
}

impl TokenStore for MemoryStore {
    fn save(&self, key: &str, secret: &str) -> Result<(), StoreError> {
        self.entries
            .lock()
            .expect("memory store mutex")
            .insert(key.to_string(), secret.to_string());
        Ok(())
    }

    fn load(&self, key: &str) -> Result<Option<String>, StoreError> {
        Ok(self.entries.lock().expect("memory store mutex").get(key).cloned())
    }

    fn delete(&self, key: &str) -> Result<(), StoreError> {
        self.entries.lock().expect("memory store mutex").remove(key);
        Ok(())
    }
}

/// Windows Credential Manager store. Compiled only on Windows targets so
/// Linux builds never link the credential backend.
#[cfg(windows)]
pub struct WindowsCredentialStore;

#[cfg(windows)]
impl TokenStore for WindowsCredentialStore {
    fn entry(key: &str) -> Result<keyring::Entry, StoreError> {
        keyring::Entry::new("HostSmith Workshop", key)
            .map_err(|err| StoreError { message: err.to_string() })
    }

    fn save(&self, key: &str, secret: &str) -> Result<(), StoreError> {
        let entry = Self::entry(key)?;
        entry
            .set_password(secret)
            .map_err(|err| StoreError { message: err.to_string() })
    }

    fn load(&self, key: &str) -> Result<Option<String>, StoreError> {
        let entry = Self::entry(key)?;
        match entry.get_password() {
            Ok(secret) => Ok(Some(secret)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(err) => Err(StoreError { message: err.to_string() }),
        }
    }

    fn delete(&self, key: &str) -> Result<(), StoreError> {
        let entry = Self::entry(key)?;
        match entry.delete_credential() {
            Ok(()) => Ok(()),
            Err(keyring::Error::NoEntry) => Ok(()),
            Err(err) => Err(StoreError { message: err.to_string() }),
        }
    }
}

/// The store the application uses: OS credential store on Windows, the
/// non-persistent memory store elsewhere (with a documented warning).
pub fn default_store() -> Arc<dyn TokenStore> {
    #[cfg(windows)]
    {
        Arc::new(WindowsCredentialStore)
    }
    #[cfg(not(windows))]
    {
        Arc::new(MemoryStore::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_store_roundtrip_and_delete() {
        let store = MemoryStore::default();
        assert_eq!(store.load("token").unwrap(), None);
        store.save("token", "secret-value").unwrap();
        assert_eq!(store.load("token").unwrap().as_deref(), Some("secret-value"));
        store.delete("token").unwrap();
        assert_eq!(store.load("token").unwrap(), None);
    }

    #[test]
    fn keys_are_independent() {
        let store = MemoryStore::default();
        store.save("a", "1").unwrap();
        store.save("b", "2").unwrap();
        assert_eq!(store.load("a").unwrap().as_deref(), Some("1"));
        assert_eq!(store.load("b").unwrap().as_deref(), Some("2"));
    }
}
