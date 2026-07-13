use crate::error::AppError;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

const SERVICE: &str = "com.arcris.dbclient";

// Entries are cached because the mock credential backend (used in tests) keeps
// its state inside the Entry instance; the real Keychain doesn't care either way.
static ENTRIES: OnceLock<Mutex<HashMap<String, Arc<keyring::Entry>>>> = OnceLock::new();

pub struct CredentialStore;

impl CredentialStore {
    fn entry(conn_id: &str) -> Result<Arc<keyring::Entry>, AppError> {
        let cache = ENTRIES.get_or_init(|| Mutex::new(HashMap::new()));
        let mut map = cache.lock().unwrap();
        if let Some(e) = map.get(conn_id) {
            return Ok(e.clone());
        }
        let e = Arc::new(
            keyring::Entry::new(SERVICE, conn_id).map_err(|e| AppError::internal(e.to_string()))?,
        );
        map.insert(conn_id.to_string(), e.clone());
        Ok(e)
    }

    pub fn set(conn_id: &str, secret: &str) -> Result<(), AppError> {
        Self::entry(conn_id)?
            .set_password(secret)
            .map_err(|e| AppError::internal(e.to_string()))
    }

    pub fn get(conn_id: &str) -> Result<Option<String>, AppError> {
        match Self::entry(conn_id)?.get_password() {
            Ok(p) => Ok(Some(p)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(AppError::internal(e.to_string())),
        }
    }

    pub fn delete(conn_id: &str) -> Result<(), AppError> {
        match Self::entry(conn_id)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(AppError::internal(e.to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_get_delete_roundtrip() {
        keyring::set_default_credential_builder(keyring::mock::default_credential_builder());
        CredentialStore::set("conn-1", "s3cret").unwrap();
        assert_eq!(CredentialStore::get("conn-1").unwrap(), Some("s3cret".into()));
        CredentialStore::delete("conn-1").unwrap();
        assert_eq!(CredentialStore::get("conn-1").unwrap(), None);
    }
}
