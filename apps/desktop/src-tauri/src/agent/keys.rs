//! The person's provider keys.
//!
//! They live in the operating system's credential store, one entry per provider entry
//! (`provider:<id>`), written and read only by this process. Where there is no credential store (a
//! headless Linux session), they go to one file, encrypted with a key made from a passphrase the
//! person types (Argon2id, then XChaCha20-Poly1305); the app asks for it once per run and keeps
//! the derived key in memory only. A key never goes to a log, an event, an error, a URL, a
//! process argument, the page or the preferences file.

use super::types::ApiKey;
use crate::local::secrets::{OsStore, Store};
use argon2::{Algorithm, Argon2, Params, Version};
use base64::Engine as _;
use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{Key, XChaCha20Poly1305, XNonce};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use zeroize::Zeroizing;

const VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyError {
    /// The id is not one this store accepts.
    BadId,
    /// The encrypted file needs its passphrase first.
    Locked,
    /// The passphrase did not open the file.
    WrongPassphrase,
    Failed,
}

impl std::fmt::Display for KeyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::BadId => "That provider entry has no valid name.",
            Self::Locked => "The keys are locked. Enter the passphrase first.",
            Self::WrongPassphrase => "That passphrase did not open the keys.",
            Self::Failed => "The key could not be stored or read.",
        })
    }
}

pub fn id_ok(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 40
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'-')
}

#[derive(Serialize, Deserialize)]
struct Sealed {
    v: u32,
    salt: String,
    nonce: String,
    data: String,
}

fn derive(passphrase: &str, salt: &[u8]) -> Option<Zeroizing<[u8; 32]>> {
    // 19 MiB, 2 passes: the OWASP-recommended floor for Argon2id, quick enough for a prompt.
    let params = Params::new(19 * 1024, 2, 1, Some(32)).ok()?;
    let argon = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let mut out = Zeroizing::new([0u8; 32]);
    argon
        .hash_password_into(passphrase.as_bytes(), salt, &mut *out)
        .ok()?;
    Some(out)
}

pub fn seal(map: &BTreeMap<String, String>, passphrase: &str) -> Option<String> {
    let mut salt = [0u8; 16];
    let mut nonce = [0u8; 24];
    getrandom::fill(&mut salt).ok()?;
    getrandom::fill(&mut nonce).ok()?;
    let key = derive(passphrase, &salt)?;
    let cipher = XChaCha20Poly1305::new(Key::from_slice(&*key));
    let plain = Zeroizing::new(serde_json::to_vec(map).ok()?);
    let data = cipher
        .encrypt(XNonce::from_slice(&nonce), plain.as_slice())
        .ok()?;
    let b64 = |b: &[u8]| base64::engine::general_purpose::STANDARD.encode(b);
    serde_json::to_string(&Sealed {
        v: VERSION,
        salt: b64(&salt),
        nonce: b64(&nonce),
        data: b64(&data),
    })
    .ok()
}

pub fn open(text: &str, passphrase: &str) -> Result<BTreeMap<String, String>, KeyError> {
    let sealed: Sealed = serde_json::from_str(text).map_err(|_| KeyError::Failed)?;
    if sealed.v != VERSION {
        return Err(KeyError::Failed);
    }
    let b64 = |s: &str| {
        base64::engine::general_purpose::STANDARD
            .decode(s)
            .map_err(|_| KeyError::Failed)
    };
    let (salt, nonce, data) = (b64(&sealed.salt)?, b64(&sealed.nonce)?, b64(&sealed.data)?);
    if nonce.len() != 24 {
        return Err(KeyError::Failed);
    }
    let key = derive(passphrase, &salt).ok_or(KeyError::Failed)?;
    let cipher = XChaCha20Poly1305::new(Key::from_slice(&*key));
    let plain = cipher
        .decrypt(XNonce::from_slice(&nonce), data.as_slice())
        .map_err(|_| KeyError::WrongPassphrase)?;
    let plain = Zeroizing::new(plain);
    serde_json::from_slice(&plain).map_err(|_| KeyError::Failed)
}

enum Mode {
    Os(OsStore),
    File {
        path: PathBuf,
        unlocked: Mutex<Option<(Zeroizing<String>, BTreeMap<String, Zeroizing<String>>)>>,
    },
}

pub struct KeyStore {
    mode: Mode,
}

impl KeyStore {
    /// `service` is the credential-store name of this installation (see `secrets::service_for`).
    pub fn open(service: &str, file: PathBuf) -> Self {
        let mode = if OsStore::available(service) {
            Mode::Os(OsStore::new(service))
        } else {
            Mode::File {
                path: file,
                unlocked: Mutex::new(None),
            }
        };
        Self { mode }
    }

    pub fn in_credential_store(&self) -> bool {
        matches!(self.mode, Mode::Os(_))
    }

    /// Where the keys are, in words.
    pub fn describe(&self) -> &'static str {
        match &self.mode {
            Mode::Os(os) => os.describe(),
            Mode::File { .. } => {
                "a file encrypted with your passphrase (no credential store was found)"
            }
        }
    }

    pub fn locked(&self) -> bool {
        match &self.mode {
            Mode::Os(_) => false,
            Mode::File { unlocked, .. } => unlocked.lock().map(|g| g.is_none()).unwrap_or(true),
        }
    }

    pub fn file_exists(&self) -> bool {
        matches!(&self.mode, Mode::File { path, .. } if path.is_file())
    }

    /// Opens the encrypted file with the passphrase (making it, empty, the first time).
    pub fn unlock(&self, passphrase: &str) -> Result<(), KeyError> {
        let Mode::File { path, unlocked } = &self.mode else {
            return Ok(());
        };
        if passphrase.chars().count() < 8 {
            return Err(KeyError::WrongPassphrase);
        }
        let map = match std::fs::read_to_string(path) {
            Ok(text) => open(&text, passphrase)?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => BTreeMap::new(),
            Err(_) => return Err(KeyError::Failed),
        };
        let map = map
            .into_iter()
            .map(|(k, v)| (k, Zeroizing::new(v)))
            .collect();
        *unlocked.lock().map_err(|_| KeyError::Failed)? =
            Some((Zeroizing::new(passphrase.to_string()), map));
        Ok(())
    }

    fn save(
        path: &Path,
        passphrase: &str,
        map: &BTreeMap<String, Zeroizing<String>>,
    ) -> Result<(), KeyError> {
        let plain: BTreeMap<String, String> = map
            .iter()
            .map(|(k, v)| (k.clone(), v.to_string()))
            .collect();
        let text = seal(&plain, passphrase).ok_or(KeyError::Failed)?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|_| KeyError::Failed)?;
        }
        let tmp = path.with_extension("tmp");
        std::fs::write(&tmp, text).map_err(|_| KeyError::Failed)?;
        std::fs::rename(&tmp, path).map_err(|_| KeyError::Failed)
    }

    pub fn get(&self, id: &str) -> Result<Option<ApiKey>, KeyError> {
        if !id_ok(id) {
            return Err(KeyError::BadId);
        }
        match &self.mode {
            Mode::Os(os) => Ok(os
                .get(&format!("provider:{id}"))
                .map_err(|_| KeyError::Failed)?
                .and_then(|s| ApiKey::new(&s))),
            Mode::File { unlocked, .. } => {
                let g = unlocked.lock().map_err(|_| KeyError::Failed)?;
                let (_, map) = g.as_ref().ok_or(KeyError::Locked)?;
                Ok(map.get(id).and_then(|s| ApiKey::new(s)))
            }
        }
    }

    pub fn set(&self, id: &str, key: &ApiKey) -> Result<(), KeyError> {
        if !id_ok(id) {
            return Err(KeyError::BadId);
        }
        match &self.mode {
            Mode::Os(os) => os
                .set(&format!("provider:{id}"), key.expose())
                .map_err(|_| KeyError::Failed),
            Mode::File { path, unlocked } => {
                let mut g = unlocked.lock().map_err(|_| KeyError::Failed)?;
                let (pass, map) = g.as_mut().ok_or(KeyError::Locked)?;
                map.insert(id.to_string(), Zeroizing::new(key.expose().to_string()));
                Self::save(path, pass, map)
            }
        }
    }

    pub fn delete(&self, id: &str) -> Result<(), KeyError> {
        if !id_ok(id) {
            return Err(KeyError::BadId);
        }
        match &self.mode {
            Mode::Os(os) => os
                .delete(&format!("provider:{id}"))
                .map_err(|_| KeyError::Failed),
            Mode::File { path, unlocked } => {
                let mut g = unlocked.lock().map_err(|_| KeyError::Failed)?;
                let (pass, map) = g.as_mut().ok_or(KeyError::Locked)?;
                map.remove(id);
                Self::save(path, pass, map)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CANARY: &str = "DYNODOC-CANARY-key-0123456789abcdef";

    #[test]
    fn a_sealed_file_opens_with_its_passphrase_only_and_hides_the_key() {
        let mut map = BTreeMap::new();
        map.insert("lp1".to_string(), CANARY.to_string());
        let text = seal(&map, "correct horse battery").unwrap();
        assert!(
            !text.contains("CANARY") && !text.contains("lp1"),
            "the file shows nothing of what it holds"
        );
        assert_eq!(
            open(&text, "correct horse battery")
                .unwrap()
                .get("lp1")
                .map(String::as_str),
            Some(CANARY)
        );
        assert_eq!(
            open(&text, "wrong horse battery").unwrap_err(),
            KeyError::WrongPassphrase
        );
        // Two seals of the same keys differ (fresh salt and nonce).
        assert_ne!(text, seal(&map, "correct horse battery").unwrap());
    }

    #[test]
    fn the_file_store_locks_unlocks_and_forgets() {
        let dir = std::env::temp_dir().join(format!("dynodoc-keys-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let store = KeyStore {
            mode: Mode::File {
                path: dir.join("provider-keys.enc"),
                unlocked: Mutex::new(None),
            },
        };
        assert!(store.locked());
        assert_eq!(store.get("lp1").err(), Some(KeyError::Locked));
        assert_eq!(
            store.unlock("short").unwrap_err(),
            KeyError::WrongPassphrase
        );
        store.unlock("a long enough passphrase").unwrap();
        let key = ApiKey::new(CANARY).unwrap();
        store.set("lp1", &key).unwrap();
        assert_eq!(store.get("lp1").unwrap().unwrap().expose(), CANARY);
        let on_disk = std::fs::read_to_string(dir.join("provider-keys.enc")).unwrap();
        assert!(!on_disk.contains("CANARY"));
        // A new run starts locked; the passphrase brings the key back.
        let again = KeyStore {
            mode: Mode::File {
                path: dir.join("provider-keys.enc"),
                unlocked: Mutex::new(None),
            },
        };
        assert!(again.locked());
        assert_eq!(
            again.unlock("another passphrase here").unwrap_err(),
            KeyError::WrongPassphrase
        );
        again.unlock("a long enough passphrase").unwrap();
        assert_eq!(again.get("lp1").unwrap().unwrap().hint(), "cdef");
        again.delete("lp1").unwrap();
        assert!(again.get("lp1").unwrap().is_none());
        assert_eq!(again.get("../x").err(), Some(KeyError::BadId));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
