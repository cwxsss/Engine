use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use tokio::fs;
use tokio::io::AsyncWriteExt;
use tokio::sync::Mutex;
use uc_application::deps::{RePairingStateError, RePairingStateStorePort};
use uc_observability_contract::diagnostics::connectivity::{observe_local_result, LocalWorkStep};

use crate::security::{AdmissionKeyError, AdmissionKeyManager};

const PURPOSE: &[u8] = b"re-pairing-state-v1";
const FORMAT_VERSION: u16 = 1;

#[derive(serde::Serialize, serde::Deserialize)]
struct PersistedRePairingStateV1 {
    format_version: u16,
    required: bool,
}

pub struct EncryptedRePairingStateStore {
    path: PathBuf,
    keys: Arc<AdmissionKeyManager>,
    write_lock: Mutex<()>,
}

impl EncryptedRePairingStateStore {
    pub fn new(path: PathBuf, keys: Arc<AdmissionKeyManager>) -> Self {
        Self {
            path,
            keys,
            write_lock: Mutex::new(()),
        }
    }

    async fn quarantine_corrupt_state(&self) {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let quarantine_path = self.path.with_extension(format!("corrupt.{}", timestamp));
        tracing::warn!(
            path = ?self.path,
            quarantine_path = ?quarantine_path,
            "Re-pairing state file is corrupt or unreadable; quarantining"
        );
        if let Err(error) = fs::rename(&self.path, &quarantine_path).await {
            tracing::warn!(
                path = ?self.path,
                quarantine_path = ?quarantine_path,
                %error,
                "Failed to rename corrupt re-pairing state, falling back to removing it"
            );
            let _ = fs::remove_file(&self.path).await;
        }
    }
}

#[async_trait]
impl RePairingStateStorePort for EncryptedRePairingStateStore {
    async fn is_required(&self) -> Result<bool, RePairingStateError> {
        let ciphertext = match fs::read(&self.path).await {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(_) => return Err(RePairingStateError::Unavailable),
        };
        let plaintext = match self.keys.open_profile_payload(PURPOSE, &ciphertext) {
            Ok(bytes) => bytes,
            Err(AdmissionKeyError::Corrupt | AdmissionKeyError::OpenFailed) => {
                self.quarantine_corrupt_state().await;
                return Ok(false);
            }
            Err(AdmissionKeyError::SecureStorage) => return Err(RePairingStateError::Unavailable),
        };
        let state: PersistedRePairingStateV1 = match postcard::from_bytes(&plaintext) {
            Ok(s) => s,
            Err(_) => {
                self.quarantine_corrupt_state().await;
                return Ok(false);
            }
        };
        if state.format_version != FORMAT_VERSION {
            self.quarantine_corrupt_state().await;
            return Ok(false);
        }
        Ok(state.required)
    }

    async fn set_required(&self, required: bool) -> Result<(), RePairingStateError> {
        observe_local_result(LocalWorkStep::RePairingStateCommit, async {
            let _guard = self.write_lock.lock().await;
            let state = PersistedRePairingStateV1 {
                format_version: FORMAT_VERSION,
                required,
            };
            let plaintext =
                postcard::to_stdvec(&state).map_err(|_| RePairingStateError::Inconsistent)?;
            let ciphertext = self
                .keys
                .seal_profile_payload(PURPOSE, &plaintext)
                .map_err(map_key_error)?;
            let parent = self.path.parent().ok_or(RePairingStateError::Unavailable)?;
            fs::create_dir_all(parent)
                .await
                .map_err(|_| RePairingStateError::Unavailable)?;
            let mut file = fs::File::create(&self.path)
                .await
                .map_err(|_| RePairingStateError::Unavailable)?;
            file.write_all(&ciphertext)
                .await
                .map_err(|_| RePairingStateError::Unavailable)?;
            file.sync_all()
                .await
                .map_err(|_| RePairingStateError::Unavailable)
        })
        .await
    }
}

fn map_key_error(error: AdmissionKeyError) -> RePairingStateError {
    match error {
        AdmissionKeyError::SecureStorage => RePairingStateError::Unavailable,
        AdmissionKeyError::Corrupt | AdmissionKeyError::OpenFailed => {
            RePairingStateError::Inconsistent
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Mutex as StdMutex;

    use uc_core::ports::{SecureStorageError, SecureStoragePort};

    use super::*;

    #[derive(Default)]
    struct MemorySecureStorage(StdMutex<HashMap<String, Vec<u8>>>);

    impl SecureStoragePort for MemorySecureStorage {
        fn get(&self, key: &str) -> Result<Option<Vec<u8>>, SecureStorageError> {
            Ok(self.0.lock().unwrap().get(key).cloned())
        }

        fn set(&self, key: &str, value: &[u8]) -> Result<(), SecureStorageError> {
            self.0
                .lock()
                .unwrap()
                .insert(key.to_owned(), value.to_vec());
            Ok(())
        }

        fn delete(&self, key: &str) -> Result<(), SecureStorageError> {
            self.0.lock().unwrap().remove(key);
            Ok(())
        }
    }

    fn store(path: PathBuf) -> EncryptedRePairingStateStore {
        EncryptedRePairingStateStore::new(
            path,
            Arc::new(AdmissionKeyManager::new(
                Arc::new(MemorySecureStorage::default()),
                [0x51; 16],
            )),
        )
    }

    #[tokio::test]
    async fn missing_state_defaults_to_not_required() {
        let directory = tempfile::tempdir().unwrap();
        let store = store(directory.path().join("re-pairing"));

        assert!(!store.is_required().await.unwrap());
    }

    #[tokio::test]
    async fn required_and_resolved_states_survive_reads_without_plaintext_persistence() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("re-pairing");
        let store = store(path.clone());

        store.set_required(true).await.unwrap();
        assert!(store.is_required().await.unwrap());
        assert_ne!(fs::read(&path).await.unwrap(), vec![1]);

        store.set_required(false).await.unwrap();
        assert!(!store.is_required().await.unwrap());
    }

    #[tokio::test]
    async fn corrupt_or_undecryptable_state_is_quarantined_and_returns_false() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("re-pairing");
        tokio::fs::write(&path, b"corrupted bytes").await.unwrap();

        let store = store(path.clone());
        assert!(!store.is_required().await.unwrap());
        assert!(!path.exists());

        let has_quarantined = std::fs::read_dir(directory.path())
            .unwrap()
            .any(|entry| {
                let name = entry.unwrap().file_name().to_string_lossy().to_string();
                name.starts_with("re-pairing") && name.contains("corrupt")
            });
        assert!(has_quarantined);
    }
}
