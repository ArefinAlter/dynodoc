//! The secrets the app keeps for the person: the local database's passwords and the engine's
//! keys, sign-in sessions per server, and the person's provider keys (the last ones only through
//! `providers`, never through this module's file fallback).
//!
//! They live in the operating system's credential store (Windows Credential Manager, the macOS
//! Keychain, the Linux Secret Service). Where there is none (a headless Linux session), the
//! infrastructure secrets fall back to a file readable by the person alone; provider keys do not
//! fall back to a file at all (see `agent::keys`: they need a passphrase).
//!
//! A secret is never written to a log, an event, an error, a URL or a process argument. Values
//! are `Zeroizing` strings and there is no `Serialize` or `Debug` on them.

use std::path::{Path, PathBuf};
use zeroize::Zeroizing;

pub const SERVICE: &str = "online.dynodoc.client";

/// The credential store's service name for a data folder. The ordinary per-user folder uses the
/// plain name; a portable or test folder gets its own (a short hash of its path), so that running
/// the app from a scratch folder can never read or overwrite the secrets of a real installation.
pub fn service_for(data_dir: &std::path::Path, ordinary: bool) -> String {
    if ordinary {
        return SERVICE.to_string();
    }
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(data_dir.to_string_lossy().to_lowercase().as_bytes());
    format!("{SERVICE}.{}", hex::encode(&digest[..4]))
}
/// A Windows credential blob holds up to 2,560 bytes; stay well under it.
const CHUNK: usize = 1800;
const MAX_CHUNKS: usize = 8;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SecretError {
    /// The credential store is not there (and no file fallback was allowed or possible).
    Unavailable,
    /// A read or write failed.
    Failed,
}

impl std::fmt::Display for SecretError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Unavailable => "The operating system's credential store is not available.",
            Self::Failed => "The operating system's credential store did not answer.",
        })
    }
}

impl std::error::Error for SecretError {}

pub type Secret = Zeroizing<String>;

/// A place to keep named secrets.
pub trait Store: Send + Sync {
    fn get(&self, name: &str) -> Result<Option<Secret>, SecretError>;
    fn set(&self, name: &str, value: &str) -> Result<(), SecretError>;
    fn delete(&self, name: &str) -> Result<(), SecretError>;
    /// What the person is told about where secrets are kept.
    fn describe(&self) -> &'static str;
}

fn entry(service: &str, name: &str) -> Result<keyring::Entry, SecretError> {
    keyring::Entry::new(service, name).map_err(|e| match e {
        keyring::Error::NoDefaultStore | keyring::Error::Invalid(..) => SecretError::Unavailable,
        _ => SecretError::Failed,
    })
}

/// The operating system's credential store. A value longer than a chunk is split over several
/// entries (`name`, `name.1`, ...), so a session cookie of a couple of kilobytes still fits.
pub struct OsStore {
    service: String,
}

impl OsStore {
    pub fn new(service: &str) -> Self {
        Self {
            service: service.to_string(),
        }
    }

    /// Whether the credential store answers at all (a probe write and delete).
    pub fn available(service: &str) -> bool {
        match entry(service, "probe") {
            Ok(e) => {
                let ok = e.set_secret(b"1").is_ok();
                let _ = e.delete_credential();
                ok
            }
            Err(_) => false,
        }
    }
}

fn chunk_name(name: &str, i: usize) -> String {
    if i == 0 {
        name.to_string()
    } else {
        format!("{name}.{i}")
    }
}

/// Splits on character boundaries, so every chunk is valid UTF-8.
pub fn chunks_of(value: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    while start < value.len() {
        let mut end = (start + CHUNK).min(value.len());
        while !value.is_char_boundary(end) {
            end -= 1;
        }
        out.push(&value[start..end]);
        start = end;
    }
    if out.is_empty() {
        out.push("");
    }
    out
}

impl Store for OsStore {
    fn get(&self, name: &str) -> Result<Option<Secret>, SecretError> {
        let mut value = Zeroizing::new(String::new());
        for i in 0..MAX_CHUNKS {
            let e = entry(&self.service, &chunk_name(name, i))?;
            match e.get_secret() {
                Ok(bytes) => {
                    let bytes = Zeroizing::new(bytes);
                    let text = std::str::from_utf8(&bytes).map_err(|_| SecretError::Failed)?;
                    // The first chunk says how many follow: a full chunk means "look for more".
                    value.push_str(text);
                    if text.len() < CHUNK {
                        return Ok(Some(value));
                    }
                }
                Err(keyring::Error::NoEntry) => {
                    return Ok(if i == 0 { None } else { Some(value) });
                }
                Err(keyring::Error::NoDefaultStore | keyring::Error::Invalid(..)) => {
                    return Err(SecretError::Unavailable)
                }
                Err(_) => return Err(SecretError::Failed),
            }
        }
        Ok(Some(value))
    }

    fn set(&self, name: &str, value: &str) -> Result<(), SecretError> {
        let parts = chunks_of(value);
        if parts.len() > MAX_CHUNKS {
            return Err(SecretError::Failed);
        }
        // A value that is an exact multiple of the chunk size needs an empty last chunk so that
        // `get` knows it has the end.
        let mut parts = parts;
        if value.len() >= CHUNK && value.len() % CHUNK == 0 {
            parts.push("");
        }
        for (i, part) in parts.iter().enumerate() {
            entry(&self.service, &chunk_name(name, i))?
                .set_secret(part.as_bytes())
                .map_err(|e| match e {
                    keyring::Error::NoDefaultStore | keyring::Error::Invalid(..) => {
                        SecretError::Unavailable
                    }
                    _ => SecretError::Failed,
                })?;
        }
        // Remove chunks a longer earlier value left behind.
        for i in parts.len()..MAX_CHUNKS {
            match entry(&self.service, &chunk_name(name, i))?.delete_credential() {
                Ok(()) | Err(keyring::Error::NoEntry) => {}
                Err(_) => break,
            }
        }
        Ok(())
    }

    fn delete(&self, name: &str) -> Result<(), SecretError> {
        for i in 0..MAX_CHUNKS {
            match entry(&self.service, &chunk_name(name, i))?.delete_credential() {
                Ok(()) => {}
                Err(keyring::Error::NoEntry) => {
                    if i == 0 {
                        return Ok(());
                    }
                }
                Err(_) => return Err(SecretError::Failed),
            }
        }
        Ok(())
    }

    fn describe(&self) -> &'static str {
        if cfg!(windows) {
            "Windows Credential Manager"
        } else if cfg!(target_os = "macos") {
            "the macOS Keychain"
        } else {
            "the system's Secret Service"
        }
    }
}

/// Infrastructure secrets in a file only the person can read (headless Linux, or a locked-down
/// machine with no credential store). Not used for provider keys.
pub struct FileStore {
    dir: PathBuf,
}

impl FileStore {
    pub fn new(dir: &Path) -> Self {
        Self {
            dir: dir.to_path_buf(),
        }
    }

    fn path(&self, name: &str) -> Option<PathBuf> {
        let safe = !name.is_empty()
            && name.len() <= 120
            && name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-_.:".contains(&b));
        safe.then(|| self.dir.join(format!("{}.secret", name.replace(':', "_"))))
    }
}

#[cfg(unix)]
fn private_file(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let temporary = path.with_extension("tmp");
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&temporary)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    std::fs::rename(&temporary, path)
}

#[cfg(not(unix))]
fn private_file(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    // The per-user data folder of Windows is already readable by the person and SYSTEM only.
    let temporary = path.with_extension("tmp");
    std::fs::write(&temporary, bytes)?;
    std::fs::rename(&temporary, path)
}

impl Store for FileStore {
    fn get(&self, name: &str) -> Result<Option<Secret>, SecretError> {
        let path = self.path(name).ok_or(SecretError::Failed)?;
        match std::fs::read(&path) {
            Ok(bytes) => String::from_utf8(bytes)
                .map(|s| Some(Zeroizing::new(s)))
                .map_err(|_| SecretError::Failed),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(_) => Err(SecretError::Failed),
        }
    }

    fn set(&self, name: &str, value: &str) -> Result<(), SecretError> {
        let path = self.path(name).ok_or(SecretError::Failed)?;
        std::fs::create_dir_all(&self.dir).map_err(|_| SecretError::Failed)?;
        private_file(&path, value.as_bytes()).map_err(|_| SecretError::Failed)
    }

    fn delete(&self, name: &str) -> Result<(), SecretError> {
        let path = self.path(name).ok_or(SecretError::Failed)?;
        match std::fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(_) => Err(SecretError::Failed),
        }
    }

    fn describe(&self) -> &'static str {
        "a file only you can read in the app's data folder (no credential store was found)"
    }
}

/// The credential store when there is one, the private file when there is not.
pub struct Preferred {
    os: Option<OsStore>,
    file: FileStore,
}

impl Preferred {
    pub fn open(fallback_dir: &Path, service: &str) -> Self {
        Self {
            os: OsStore::available(service).then(|| OsStore::new(service)),
            file: FileStore::new(fallback_dir),
        }
    }

    pub fn uses_credential_store(&self) -> bool {
        self.os.is_some()
    }
}

impl Store for Preferred {
    fn get(&self, name: &str) -> Result<Option<Secret>, SecretError> {
        match &self.os {
            Some(os) => os.get(name),
            None => self.file.get(name),
        }
    }
    fn set(&self, name: &str, value: &str) -> Result<(), SecretError> {
        match &self.os {
            Some(os) => os.set(name, value),
            None => self.file.set(name, value),
        }
    }
    fn delete(&self, name: &str) -> Result<(), SecretError> {
        match &self.os {
            Some(os) => os.delete(name),
            None => self.file.delete(name),
        }
    }
    fn describe(&self) -> &'static str {
        match &self.os {
            Some(os) => os.describe(),
            None => self.file.describe(),
        }
    }
}

/// `n` random bytes from the operating system as lower-case hex.
pub fn random_hex(n: usize) -> Result<Secret, SecretError> {
    let mut bytes = Zeroizing::new(vec![0u8; n]);
    getrandom::fill(&mut bytes).map_err(|_| SecretError::Failed)?;
    Ok(Zeroizing::new(hex::encode(&*bytes)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn long_values_split_on_character_boundaries_and_rejoin() {
        let value = "é".repeat(2500); // 5,000 bytes, every character two bytes
        let parts = chunks_of(&value);
        assert!(parts.len() >= 3);
        assert!(parts.iter().all(|p| p.len() <= CHUNK));
        assert_eq!(parts.concat(), value);
        assert_eq!(chunks_of(""), vec![""]);
    }

    #[test]
    fn the_file_store_round_trips_and_refuses_odd_names() {
        let dir = std::env::temp_dir().join(format!("dynodoc-secrets-{}", std::process::id()));
        let store = FileStore::new(&dir);
        assert_eq!(store.get("a").unwrap().map(|s| s.to_string()), None);
        store.set("local-secrets", "CANARY-value").unwrap();
        assert_eq!(
            store.get("local-secrets").unwrap().unwrap().as_str(),
            "CANARY-value"
        );
        store.delete("local-secrets").unwrap();
        assert!(store.get("local-secrets").unwrap().is_none());
        assert!(store.set("../escape", "x").is_err());
        assert!(store.set("a b", "x").is_err());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn random_hex_is_the_requested_length_and_differs() {
        let a = random_hex(32).unwrap();
        let b = random_hex(32).unwrap();
        assert_eq!(a.len(), 64);
        assert_ne!(*a, *b);
    }
}
