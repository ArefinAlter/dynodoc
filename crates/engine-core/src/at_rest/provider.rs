//! Key providers: wrap and unwrap a data key (DEK) under a key-encryption key (KEK) that
//! the customer controls, bound to where the DEK belongs.
//!
//! The trait follows docs/END-TO-END-ENCRYPTION.md section 7.5 and is synchronous. A
//! provider that talks to a network service (Vault Transit, a cloud KMS) will need an
//! asynchronous form or a blocking adapter; none is built. Two providers exist here:
//! [`FileKeyProvider`], which reads KEK versions from owner-only local files and is
//! documented as the weakest option, and [`TestKeyProvider`] for tests.
//!
//! Wrapped form produced by both (a provider-specific, opaque value to its caller):
//!
//! ```text
//! version u8 (0x01) | nonce[12] | AES-256-GCM(KEK, nonce, aad, DEK)[32] | tag[16]   = 61 bytes
//! aad = LP("dynodoc.at-rest.v1/dek-wrap") || LP(installation) || LP(project, empty for the
//!       installation scope) || LP(purpose) || LP(version) || LP(kek_version u32 BE)
//! ```
//!
//! The KEK version is stored beside the wrapped bytes (as `data_key.kek_version`) and is
//! authenticated, so a wrapped DEK cannot be replayed under another installation,
//! project, purpose or KEK version. A random 96-bit nonce is safe here: a KEK version
//! wraps one DEK per project plus rewraps, far below the 2^32 messages of the bound.
use super::{
    envelope::lp,
    secret::{wipe, Wiped},
    Dek, Kek,
};
use ring::{
    aead::{Aad, LessSafeKey, Nonce, UnboundKey, AES_256_GCM},
    rand::{SecureRandom, SystemRandom},
};
use std::{
    collections::BTreeMap,
    fs::File,
    io::Read,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};
use uuid::Uuid;

/// Shortest and longest wrapped value a `data_key.wrapped` column accepts.
pub const MIN_WRAPPED_BYTES: usize = 16;
pub const MAX_WRAPPED_BYTES: usize = 4096;
/// A KEK file is 64 hex digits and at most a line ending; anything larger is refused unread.
pub const MAX_KEK_FILE_BYTES: usize = 128;
/// Most KEK versions one [`FileKeyProvider`] loads.
pub const MAX_KEK_VERSIONS: usize = 256;

const WRAP_VERSION: u8 = 1;
const WRAP_LABEL: &[u8] = b"dynodoc.at-rest.v1/dek-wrap";
const NONCE_LEN: usize = 12;
const DEK_LEN: usize = 32;
const TAG_LEN: usize = 16;
const WRAPPED_LEN: usize = 1 + NONCE_LEN + DEK_LEN + TAG_LEN;

/// Why a provider call failed. The four kinds call for different responses: retry
/// later, fix a permission, fix a configuration, or restore from backup. No message
/// contains key bytes.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ProviderError {
    /// The provider could not be reached or read right now; a retry may succeed.
    #[error("key provider unavailable: {0}")]
    Unavailable(String),
    /// The provider refused: a permission, a policy or a disabled key.
    #[error("key provider denied the request: {0}")]
    Denied(String),
    /// The named KEK version (or key) does not exist in the provider.
    #[error("key not found: {0}")]
    NotFound(String),
    /// Stored or supplied material is malformed, or failed authentication (damaged
    /// wrapped key, or wrapped for a different context).
    #[error("key material is corrupt or does not match its context: {0}")]
    Corrupt(String),
}

/// What a DEK's KEK is used for; matches `kms_key.purpose` in the proposed schema.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Purpose {
    Data,
    Backup,
}

impl Purpose {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Data => "data",
            Self::Backup => "backup",
        }
    }
}

/// The encryption context a wrapped DEK is bound to: the installation, the project (none
/// for the installation-scope DEK) and the purpose.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WrapContext {
    pub installation: Uuid,
    pub project: Option<Uuid>,
    pub purpose: Purpose,
}

impl WrapContext {
    pub fn new(installation: Uuid, project: Option<Uuid>, purpose: Purpose) -> Self {
        Self {
            installation,
            project,
            purpose,
        }
    }

    /// Canonical bytes of the context, length-framed: the value a provider binds as AAD,
    /// as a KMS encryption context (hashed or encoded) or as a Vault `context`. A
    /// missing project is an empty field, distinct from the nil UUID.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(8 * 4 + WRAP_LABEL.len() + 16 + 16 + 6);
        lp(&mut out, WRAP_LABEL);
        lp(&mut out, self.installation.as_bytes());
        lp(
            &mut out,
            self.project.as_ref().map_or(&[][..], |p| p.as_bytes()),
        );
        lp(&mut out, self.purpose.as_str().as_bytes());
        out
    }
}

/// A wrapped DEK as stored: the opaque provider bytes and the KEK version that made them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Wrapped {
    kek_version: u32,
    bytes: Vec<u8>,
}

impl Wrapped {
    /// `bytes` must fit the `data_key.wrapped` bound of [`MIN_WRAPPED_BYTES`] to
    /// [`MAX_WRAPPED_BYTES`].
    pub fn new(kek_version: u32, bytes: Vec<u8>) -> Result<Self, ProviderError> {
        if !(MIN_WRAPPED_BYTES..=MAX_WRAPPED_BYTES).contains(&bytes.len()) {
            return Err(ProviderError::Corrupt(format!(
                "a wrapped key is {MIN_WRAPPED_BYTES} to {MAX_WRAPPED_BYTES} bytes, got {}",
                bytes.len()
            )));
        }
        Ok(Self { kek_version, bytes })
    }

    pub fn kek_version(&self) -> u32 {
        self.kek_version
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }
}

/// What a provider says about itself, for audit records.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProviderInfo {
    /// Provider type, as in the proposed `kms_key.provider` (`"file"`).
    pub provider: &'static str,
    /// Where the key lives (a path here); never secret material.
    pub key_ref: String,
    /// The KEK version new wraps use.
    pub kek_version: u32,
}

/// Wraps and unwraps DEKs. Implementations must bind `ctx` so a wrapped DEK fails to
/// unwrap under any other context, and must never put key material in an error.
pub trait KeyProvider: Send + Sync {
    /// Wrap `dek` under the newest KEK version.
    fn wrap(&self, dek: &Dek, ctx: &WrapContext) -> Result<Wrapped, ProviderError>;

    /// Unwrap with the KEK version recorded in `wrapped` (older versions still work).
    fn unwrap(&self, wrapped: &Wrapped, ctx: &WrapContext) -> Result<Dek, ProviderError>;

    fn describe(&self) -> ProviderInfo;

    /// KEK rotation for one DEK: unwrap, then wrap under the newest version. No row data
    /// is touched. Providers with a native rewrap can override it.
    fn rewrap(&self, wrapped: &Wrapped, ctx: &WrapContext) -> Result<Wrapped, ProviderError> {
        let dek = self.unwrap(wrapped, ctx)?;
        self.wrap(&dek, ctx)
    }
}

fn wrap_aad(ctx: &WrapContext, kek_version: u32) -> Vec<u8> {
    let mut out = ctx.to_bytes();
    lp(&mut out, &[WRAP_VERSION]);
    lp(&mut out, &kek_version.to_be_bytes());
    out
}

fn cipher(kek: &Kek) -> Result<LessSafeKey, ProviderError> {
    UnboundKey::new(&AES_256_GCM, kek.as_bytes())
        .map(LessSafeKey::new)
        .map_err(|_| ProviderError::Corrupt("KEK is not a valid AES-256 key".into()))
}

fn wrap_with_nonce(
    kek: &Kek,
    kek_version: u32,
    ctx: &WrapContext,
    dek: &Dek,
    nonce: [u8; NONCE_LEN],
) -> Result<Wrapped, ProviderError> {
    let mut sealed = Wiped::<DEK_LEN>(*dek.as_bytes());
    let tag = cipher(kek)?
        .seal_in_place_separate_tag(
            Nonce::assume_unique_for_key(nonce),
            Aad::from(wrap_aad(ctx, kek_version)),
            &mut sealed.0,
        )
        .map_err(|_| ProviderError::Corrupt("wrapping failed".into()))?;
    let mut out = Vec::with_capacity(WRAPPED_LEN);
    out.push(WRAP_VERSION);
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&sealed.0);
    out.extend_from_slice(tag.as_ref());
    Wrapped::new(kek_version, out)
}

fn unwrap_with(kek: &Kek, wrapped: &Wrapped, ctx: &WrapContext) -> Result<Dek, ProviderError> {
    let bytes = wrapped.as_bytes();
    if bytes.len() != WRAPPED_LEN {
        return Err(ProviderError::Corrupt(format!(
            "a wrapped key from this provider is {WRAPPED_LEN} bytes, got {}",
            bytes.len()
        )));
    }
    if bytes[0] != WRAP_VERSION {
        return Err(ProviderError::Corrupt(format!(
            "unsupported wrapped-key version {}",
            bytes[0]
        )));
    }
    let nonce: [u8; NONCE_LEN] = bytes[1..1 + NONCE_LEN]
        .try_into()
        .map_err(|_| ProviderError::Corrupt("wrapped key".into()))?;
    let mut buffer = Wiped::<{ DEK_LEN + TAG_LEN }>::zero();
    buffer.0.copy_from_slice(&bytes[1 + NONCE_LEN..]);
    let opened = cipher(kek)?
        .open_in_place(
            Nonce::assume_unique_for_key(nonce),
            Aad::from(wrap_aad(ctx, wrapped.kek_version())),
            &mut buffer.0,
        )
        .map_err(|_| {
            ProviderError::Corrupt(
                "the wrapped key failed authentication: it was changed, or it belongs to \
                 another installation, project, purpose or KEK version"
                    .into(),
            )
        })?;
    Dek::from_slice(opened).map_err(|_| ProviderError::Corrupt("wrapped key length".into()))
}

/// KEK versions by number; the highest is the one new wraps use.
struct Keys(BTreeMap<u32, Kek>);

impl Keys {
    fn newest(&self) -> Result<(u32, &Kek), ProviderError> {
        self.0
            .iter()
            .next_back()
            .map(|(version, kek)| (*version, kek))
            .ok_or_else(|| ProviderError::NotFound("no KEK version is loaded".into()))
    }

    fn unwrap(&self, wrapped: &Wrapped, ctx: &WrapContext) -> Result<Dek, ProviderError> {
        let kek = self.0.get(&wrapped.kek_version()).ok_or_else(|| {
            ProviderError::NotFound(format!(
                "KEK version {} is not loaded",
                wrapped.kek_version()
            ))
        })?;
        unwrap_with(kek, wrapped, ctx)
    }
}

fn random_nonce() -> Result<[u8; NONCE_LEN], ProviderError> {
    let mut nonce = [0u8; NONCE_LEN];
    SystemRandom::new()
        .fill(&mut nonce)
        .map_err(|_| ProviderError::Unavailable("system random generator failed".into()))?;
    Ok(nonce)
}

/// KEK versions read from local files. The weakest provider: the KEK sits on the same
/// host as the service, so it protects against stolen disks and backups that do not carry
/// the key file, not against anyone with access to the running host. Intended for
/// evaluation and small sites (docs/END-TO-END-ENCRYPTION.md, section 7.5).
///
/// A KEK file holds exactly 64 lowercase hexadecimal digits (the 32-byte key), then at
/// most one line ending, and is at most [`MAX_KEK_FILE_BYTES`]. On Unix a file readable
/// by its group or others is refused; on other systems the access list is not checked.
/// Files are read once, when the provider is built; rotate by adding a file with a
/// higher version and building the provider again. Key bytes are never printed.
pub struct FileKeyProvider {
    keys: Keys,
    key_ref: String,
}

impl std::fmt::Debug for FileKeyProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FileKeyProvider")
            .field("key_ref", &self.key_ref)
            .field("kek_versions", &self.keys.0.keys().collect::<Vec<_>>())
            .finish()
    }
}

impl FileKeyProvider {
    /// Load the files named `kek-<version>.key` (version a canonical decimal `u32`) from
    /// `dir`. Other names are ignored; a malformed `kek-*.key` name is an error.
    pub fn from_dir(dir: &Path) -> Result<Self, ProviderError> {
        let entries = std::fs::read_dir(dir).map_err(|e| io_error(dir, &e))?;
        let mut files = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|e| io_error(dir, &e))?;
            let name = entry.file_name();
            let Some(name) = name.to_str() else { continue };
            let Some(middle) = name
                .strip_prefix("kek-")
                .and_then(|rest| rest.strip_suffix(".key"))
            else {
                continue;
            };
            let version = canonical_version(middle).ok_or_else(|| {
                ProviderError::Corrupt(format!(
                    "KEK file name {name:?} must be kek-<version>.key with a canonical decimal version"
                ))
            })?;
            files.push((version, entry.path()));
            if files.len() > MAX_KEK_VERSIONS {
                return Err(ProviderError::Corrupt(format!(
                    "more than {MAX_KEK_VERSIONS} KEK files in {}",
                    dir.display()
                )));
            }
        }
        let mut provider = Self::from_files(files)?;
        provider.key_ref = dir.display().to_string();
        Ok(provider)
    }

    /// Load the given `(version, path)` files. At least one, at most
    /// [`MAX_KEK_VERSIONS`], no repeated version.
    pub fn from_files(
        files: impl IntoIterator<Item = (u32, PathBuf)>,
    ) -> Result<Self, ProviderError> {
        let mut keys = BTreeMap::new();
        let mut paths = Vec::new();
        for (version, path) in files {
            if keys.len() >= MAX_KEK_VERSIONS {
                return Err(ProviderError::Corrupt(format!(
                    "more than {MAX_KEK_VERSIONS} KEK files"
                )));
            }
            let kek = read_kek_file(&path)?;
            if keys.insert(version, kek).is_some() {
                return Err(ProviderError::Corrupt(format!(
                    "KEK version {version} is given twice"
                )));
            }
            paths.push(path.display().to_string());
        }
        if keys.is_empty() {
            return Err(ProviderError::NotFound("no KEK file was given".into()));
        }
        Ok(Self {
            keys: Keys(keys),
            key_ref: paths.join(";"),
        })
    }

    /// The loaded KEK versions, ascending.
    pub fn kek_versions(&self) -> Vec<u32> {
        self.keys.0.keys().copied().collect()
    }
}

impl KeyProvider for FileKeyProvider {
    fn wrap(&self, dek: &Dek, ctx: &WrapContext) -> Result<Wrapped, ProviderError> {
        let (version, kek) = self.keys.newest()?;
        wrap_with_nonce(kek, version, ctx, dek, random_nonce()?)
    }

    fn unwrap(&self, wrapped: &Wrapped, ctx: &WrapContext) -> Result<Dek, ProviderError> {
        self.keys.unwrap(wrapped, ctx)
    }

    fn describe(&self) -> ProviderInfo {
        ProviderInfo {
            provider: "file",
            key_ref: self.key_ref.clone(),
            kek_version: self.keys.newest().map_or(0, |(version, _)| version),
        }
    }
}

/// `1`, `20`, `4294967295`; not `01`, `+1`, an empty string or a value over `u32::MAX`.
fn canonical_version(text: &str) -> Option<u32> {
    let canonical = !text.is_empty()
        && text.bytes().all(|b| b.is_ascii_digit())
        && (text == "0" || !text.starts_with('0'));
    canonical.then(|| text.parse().ok()).flatten()
}

fn io_error(path: &Path, error: &std::io::Error) -> ProviderError {
    use std::io::ErrorKind::{NotFound, PermissionDenied};
    let shown = path.display();
    match error.kind() {
        NotFound => ProviderError::NotFound(format!("{shown} does not exist")),
        PermissionDenied => {
            ProviderError::Denied(format!("the operating system denied reading {shown}"))
        }
        kind => ProviderError::Unavailable(format!("cannot read {shown}: {kind}")),
    }
}

#[cfg(unix)]
fn check_access(meta: &std::fs::Metadata, path: &Path) -> Result<(), ProviderError> {
    use std::os::unix::fs::PermissionsExt;
    let mode = meta.permissions().mode() & 0o777;
    if mode & 0o077 != 0 {
        return Err(ProviderError::Denied(format!(
            "KEK file {} is accessible to its group or others (mode {mode:03o}); \
             it must be owner-only, for example 0600",
            path.display()
        )));
    }
    Ok(())
}

#[cfg(not(unix))]
fn check_access(_meta: &std::fs::Metadata, _path: &Path) -> Result<(), ProviderError> {
    Ok(())
}

fn read_kek_file(path: &Path) -> Result<Kek, ProviderError> {
    let shown = path.display();
    // Look before opening so a FIFO or device is never read; then check the open handle.
    let before = std::fs::metadata(path).map_err(|e| io_error(path, &e))?;
    if !before.is_file() {
        return Err(ProviderError::Corrupt(format!(
            "{shown} is not a regular file"
        )));
    }
    let file = File::open(path).map_err(|e| io_error(path, &e))?;
    let meta = file.metadata().map_err(|e| io_error(path, &e))?;
    if !meta.is_file() {
        return Err(ProviderError::Corrupt(format!(
            "{shown} is not a regular file"
        )));
    }
    check_access(&meta, path)?;
    if meta.len() > MAX_KEK_FILE_BYTES as u64 {
        return Err(ProviderError::Corrupt(format!(
            "KEK file {shown} is larger than {MAX_KEK_FILE_BYTES} bytes"
        )));
    }
    let mut raw = Vec::with_capacity(MAX_KEK_FILE_BYTES + 1);
    file.take(MAX_KEK_FILE_BYTES as u64 + 1)
        .read_to_end(&mut raw)
        .map_err(|e| io_error(path, &e))?;
    let parsed = parse_kek_text(&raw, &shown.to_string());
    wipe(&mut raw);
    parsed
}

fn parse_kek_text(raw: &[u8], shown: &str) -> Result<Kek, ProviderError> {
    let corrupt = || {
        ProviderError::Corrupt(format!(
            "KEK file {shown} must hold exactly 64 lowercase hexadecimal digits \
             and at most one line ending"
        ))
    };
    let text = raw
        .strip_suffix(b"\r\n")
        .or_else(|| raw.strip_suffix(b"\n"))
        .unwrap_or(raw);
    if text.len() != 2 * Kek::LEN {
        return Err(corrupt());
    }
    let nibble = |b: u8| match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        _ => None,
    };
    let mut bytes = Wiped::<{ Kek::LEN }>::zero();
    for (out, pair) in bytes.0.iter_mut().zip(text.chunks_exact(2)) {
        match (nibble(pair[0]), nibble(pair[1])) {
            (Some(high), Some(low)) => *out = (high << 4) | low,
            _ => return Err(corrupt()),
        }
    }
    Ok(Kek::from_bytes(bytes.0))
}

/// A provider for tests: KEKs are given in memory and every nonce comes from a counter,
/// so the same sequence of calls gives the same wrapped bytes. **Never use it outside
/// tests**: the counter restarts with the process, which repeats nonces under one KEK.
pub struct TestKeyProvider {
    keys: Keys,
    counter: AtomicU64,
}

impl std::fmt::Debug for TestKeyProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TestKeyProvider")
            .field("kek_versions", &self.keys.0.keys().collect::<Vec<_>>())
            .finish()
    }
}

impl TestKeyProvider {
    pub fn new(keks: impl IntoIterator<Item = (u32, [u8; 32])>) -> Self {
        Self {
            keys: Keys(
                keks.into_iter()
                    .map(|(version, bytes)| (version, Kek::from_bytes(bytes)))
                    .collect(),
            ),
            counter: AtomicU64::new(0),
        }
    }

    /// Wrap under a chosen KEK version with a chosen nonce: the deterministic form behind
    /// the published test vectors.
    pub fn wrap_at(
        &self,
        kek_version: u32,
        dek: &Dek,
        ctx: &WrapContext,
        nonce: [u8; NONCE_LEN],
    ) -> Result<Wrapped, ProviderError> {
        let kek = self.keys.0.get(&kek_version).ok_or_else(|| {
            ProviderError::NotFound(format!("KEK version {kek_version} is not loaded"))
        })?;
        wrap_with_nonce(kek, kek_version, ctx, dek, nonce)
    }
}

impl KeyProvider for TestKeyProvider {
    fn wrap(&self, dek: &Dek, ctx: &WrapContext) -> Result<Wrapped, ProviderError> {
        let (version, _) = self.keys.newest()?;
        let n = self.counter.fetch_add(1, Ordering::SeqCst);
        let mut nonce = [0u8; NONCE_LEN];
        nonce[4..].copy_from_slice(&n.to_be_bytes());
        self.wrap_at(version, dek, ctx, nonce)
    }

    fn unwrap(&self, wrapped: &Wrapped, ctx: &WrapContext) -> Result<Dek, ProviderError> {
        self.keys.unwrap(wrapped, ctx)
    }

    fn describe(&self) -> ProviderInfo {
        ProviderInfo {
            provider: "test",
            key_ref: "memory".into(),
            kek_version: self.keys.newest().map_or(0, |(version, _)| version),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;

    fn installation() -> Uuid {
        Uuid::from_u128(0x1111_2222_3333_4444_5555_6666_7777_8888)
    }

    fn project() -> Uuid {
        Uuid::from_u128(0xaaaa_bbbb_cccc_dddd_eeee_ffff_0000_1111)
    }

    fn ctx() -> WrapContext {
        WrapContext::new(installation(), Some(project()), Purpose::Data)
    }

    fn two_versions() -> TestKeyProvider {
        TestKeyProvider::new([(1, [0x01; 32]), (2, [0x02; 32])])
    }

    /// A scratch directory under the system temp directory, removed on drop.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            static N: AtomicUsize = AtomicUsize::new(0);
            let path = std::env::temp_dir().join(format!(
                "dynodoc-at-rest-{}-{}",
                std::process::id(),
                N.fetch_add(1, Ordering::SeqCst)
            ));
            std::fs::create_dir_all(&path).unwrap();
            Self(path)
        }

        fn write(&self, name: &str, contents: &[u8], mode: u32) -> PathBuf {
            let path = self.0.join(name);
            // A file written earlier may have been made read-only; replace it, not write through it.
            let _ = std::fs::remove_file(&path);
            std::fs::write(&path, contents).unwrap();
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&path, std::fs::Permissions::from_mode(mode)).unwrap();
            }
            #[cfg(not(unix))]
            let _ = mode;
            path
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn hex_of(byte: u8) -> String {
        format!("{byte:02x}").repeat(32)
    }

    #[test]
    fn wrap_round_trips_and_has_the_documented_layout() {
        let provider = two_versions();
        let dek = Dek::from_bytes([0x42; 32]);
        let wrapped = provider.wrap(&dek, &ctx()).unwrap();
        assert_eq!(wrapped.kek_version(), 2);
        assert_eq!(wrapped.as_bytes().len(), WRAPPED_LEN);
        assert_eq!(wrapped.as_bytes()[0], WRAP_VERSION);
        assert_ne!(&wrapped.as_bytes()[13..45], dek.as_bytes());
        assert_eq!(
            provider.unwrap(&wrapped, &ctx()).unwrap().as_bytes(),
            dek.as_bytes()
        );
    }

    #[test]
    fn a_wrapped_dek_is_bound_to_its_context() {
        let provider = two_versions();
        let dek = Dek::from_bytes([0x42; 32]);
        let wrapped = provider.wrap(&dek, &ctx()).unwrap();
        let other_installation =
            WrapContext::new(Uuid::from_u128(1), Some(project()), Purpose::Data);
        let other_project =
            WrapContext::new(installation(), Some(Uuid::from_u128(2)), Purpose::Data);
        let no_project = WrapContext::new(installation(), None, Purpose::Data);
        let nil_project = WrapContext::new(installation(), Some(Uuid::nil()), Purpose::Data);
        let other_purpose = WrapContext::new(installation(), Some(project()), Purpose::Backup);
        for wrong in [
            other_installation,
            other_project,
            no_project,
            nil_project,
            other_purpose,
        ] {
            assert!(
                matches!(
                    provider.unwrap(&wrapped, &wrong),
                    Err(ProviderError::Corrupt(_))
                ),
                "{wrong:?}"
            );
        }
        // None and the nil UUID are different scopes.
        let installation_scope = provider.wrap(&dek, &no_project).unwrap();
        assert!(provider.unwrap(&installation_scope, &no_project).is_ok());
        assert!(provider.unwrap(&installation_scope, &nil_project).is_err());
    }

    #[test]
    fn the_recorded_kek_version_is_authenticated() {
        let provider = two_versions();
        let dek = Dek::from_bytes([0x42; 32]);
        let wrapped = provider.wrap(&dek, &ctx()).unwrap();
        // Claiming the other version fails even though that KEK exists.
        let relabelled = Wrapped::new(1, wrapped.as_bytes().to_vec()).unwrap();
        assert!(matches!(
            provider.unwrap(&relabelled, &ctx()),
            Err(ProviderError::Corrupt(_))
        ));
        let missing = Wrapped::new(9, wrapped.as_bytes().to_vec()).unwrap();
        assert!(matches!(
            provider.unwrap(&missing, &ctx()),
            Err(ProviderError::NotFound(_))
        ));
    }

    #[test]
    fn every_byte_of_a_wrapped_key_is_authenticated() {
        let provider = two_versions();
        let wrapped = provider.wrap(&Dek::from_bytes([0x42; 32]), &ctx()).unwrap();
        for i in 0..wrapped.as_bytes().len() {
            let mut bytes = wrapped.as_bytes().to_vec();
            bytes[i] ^= 0x01;
            let damaged = Wrapped::new(wrapped.kek_version(), bytes).unwrap();
            assert!(
                matches!(
                    provider.unwrap(&damaged, &ctx()),
                    Err(ProviderError::Corrupt(_))
                ),
                "byte {i}"
            );
        }
    }

    #[test]
    fn wrapped_length_and_version_are_checked() {
        let provider = two_versions();
        let wrapped = provider.wrap(&Dek::from_bytes([0x42; 32]), &ctx()).unwrap();
        for len in [
            MIN_WRAPPED_BYTES,
            WRAPPED_LEN - 1,
            WRAPPED_LEN + 1,
            MAX_WRAPPED_BYTES,
        ] {
            let mut bytes = wrapped.as_bytes().to_vec();
            bytes.resize(len, 0);
            let odd = Wrapped::new(2, bytes).unwrap();
            assert!(
                matches!(
                    provider.unwrap(&odd, &ctx()),
                    Err(ProviderError::Corrupt(_))
                ),
                "len {len}"
            );
        }
        for len in [0, MIN_WRAPPED_BYTES - 1, MAX_WRAPPED_BYTES + 1] {
            assert!(
                matches!(
                    Wrapped::new(2, vec![0; len]),
                    Err(ProviderError::Corrupt(_))
                ),
                "len {len}"
            );
        }
        let mut bytes = wrapped.as_bytes().to_vec();
        bytes[0] = 2;
        let future = Wrapped::new(2, bytes).unwrap();
        assert!(
            matches!(provider.unwrap(&future, &ctx()), Err(ProviderError::Corrupt(m)) if m.contains("version"))
        );
    }

    #[test]
    fn rotation_new_wraps_use_the_newest_and_old_ones_still_unwrap() {
        let dek = Dek::from_bytes([0x42; 32]);
        let v1 = TestKeyProvider::new([(1, [0x01; 32])]);
        let old = v1.wrap(&dek, &ctx()).unwrap();
        assert_eq!(old.kek_version(), 1);
        assert_eq!(v1.describe().kek_version, 1);

        let v2 = two_versions();
        assert_eq!(v2.describe().kek_version, 2);
        assert_eq!(v2.unwrap(&old, &ctx()).unwrap().as_bytes(), dek.as_bytes());
        assert_eq!(v2.wrap(&dek, &ctx()).unwrap().kek_version(), 2);

        let rewrapped = v2.rewrap(&old, &ctx()).unwrap();
        assert_eq!(rewrapped.kek_version(), 2);
        assert_ne!(rewrapped.as_bytes(), old.as_bytes());
        assert_eq!(
            v2.unwrap(&rewrapped, &ctx()).unwrap().as_bytes(),
            dek.as_bytes()
        );
        // The old KEK alone can no longer open the rewrapped value.
        assert!(matches!(
            v1.unwrap(&rewrapped, &ctx()),
            Err(ProviderError::NotFound(_))
        ));
    }

    #[test]
    fn an_empty_provider_has_no_key_to_wrap_with() {
        let empty = TestKeyProvider::new([]);
        assert!(matches!(
            empty.wrap(&Dek::from_bytes([1; 32]), &ctx()),
            Err(ProviderError::NotFound(_))
        ));
    }

    #[test]
    fn the_test_provider_is_deterministic_and_never_repeats_a_nonce() {
        let dek = Dek::from_bytes([0x42; 32]);
        let a = two_versions();
        let b = two_versions();
        let a1 = a.wrap(&dek, &ctx()).unwrap();
        let a2 = a.wrap(&dek, &ctx()).unwrap();
        assert_eq!(a1, b.wrap(&dek, &ctx()).unwrap());
        assert_eq!(a2, b.wrap(&dek, &ctx()).unwrap());
        assert_ne!(a1, a2);
        assert_ne!(a1.as_bytes()[1..13], a2.as_bytes()[1..13]);
    }

    #[test]
    fn the_file_provider_picks_random_nonces() {
        let dir = TempDir::new();
        dir.write("kek-1.key", hex_of(1).as_bytes(), 0o600);
        let provider = FileKeyProvider::from_dir(&dir.0).unwrap();
        let dek = Dek::from_bytes([0x42; 32]);
        let a = provider.wrap(&dek, &ctx()).unwrap();
        let b = provider.wrap(&dek, &ctx()).unwrap();
        assert_ne!(a.as_bytes()[1..13], b.as_bytes()[1..13]);
        assert_eq!(
            provider.unwrap(&a, &ctx()).unwrap().as_bytes(),
            dek.as_bytes()
        );
    }

    #[test]
    fn context_bytes_are_framed_and_distinguish_no_project_from_nil() {
        let a = WrapContext::new(installation(), None, Purpose::Data).to_bytes();
        let b = WrapContext::new(installation(), Some(Uuid::nil()), Purpose::Data).to_bytes();
        assert_ne!(a, b);
        assert_eq!(b.len(), a.len() + 16);
        assert!(a.starts_with(&(WRAP_LABEL.len() as u64).to_be_bytes()));
    }

    #[test]
    fn file_provider_reads_versions_and_rotates_by_adding_a_file() {
        let dir = TempDir::new();
        dir.write("kek-1.key", format!("{}\n", hex_of(0x01)).as_bytes(), 0o600);
        dir.write("README.txt", b"not a key", 0o644);
        let first = FileKeyProvider::from_dir(&dir.0).unwrap();
        assert_eq!(first.kek_versions(), vec![1]);
        let dek = Dek::from_bytes([0x42; 32]);
        let old = first.wrap(&dek, &ctx()).unwrap();
        assert_eq!(old.kek_version(), 1);

        dir.write(
            "kek-2.key",
            format!("{}\r\n", hex_of(0x02)).as_bytes(),
            0o400,
        );
        let second = FileKeyProvider::from_dir(&dir.0).unwrap();
        assert_eq!(second.kek_versions(), vec![1, 2]);
        assert_eq!(second.describe().provider, "file");
        assert_eq!(second.describe().kek_version, 2);
        assert_eq!(second.wrap(&dek, &ctx()).unwrap().kek_version(), 2);
        assert_eq!(
            second.unwrap(&old, &ctx()).unwrap().as_bytes(),
            dek.as_bytes()
        );
        // The two providers agree with an in-memory one holding the same keys.
        let mirror = TestKeyProvider::new([(1, [0x01; 32]), (2, [0x02; 32])]);
        assert_eq!(
            mirror.unwrap(&old, &ctx()).unwrap().as_bytes(),
            dek.as_bytes()
        );
    }

    #[test]
    fn file_provider_refuses_bad_files_without_printing_the_key() {
        let dir = TempDir::new();
        let secret = hex_of(0xAB);
        let cases: Vec<(&str, Vec<u8>)> = vec![
            ("short", secret.as_bytes()[..63].to_vec()),
            ("long", format!("{secret}00").into_bytes()),
            ("upper", secret.to_uppercase().into_bytes()),
            ("nonhex", format!("{}zz", &secret[..62]).into_bytes()),
            ("two-newlines", format!("{secret}\n\n").into_bytes()),
            ("empty", Vec::new()),
            ("spaces", format!(" {secret}").into_bytes()),
            ("huge", vec![b'a'; MAX_KEK_FILE_BYTES + 1]),
        ];
        for (name, contents) in cases {
            let path = dir.write(&format!("{name}.key"), &contents, 0o600);
            let error = FileKeyProvider::from_files([(1, path)]).unwrap_err();
            assert!(
                matches!(error, ProviderError::Corrupt(_)),
                "{name}: {error}"
            );
            assert!(
                !error.to_string().contains("abab"),
                "{name} leaked key text"
            );
        }
        // The same text with exactly one trailing newline is accepted.
        let ok = dir.write("ok.key", format!("{secret}\n").as_bytes(), 0o600);
        assert!(FileKeyProvider::from_files([(1, ok)]).is_ok());
    }

    #[test]
    fn file_provider_distinguishes_missing_and_unreadable() {
        let dir = TempDir::new();
        let missing = dir.0.join("nope.key");
        assert!(matches!(
            FileKeyProvider::from_files([(1, missing)]).unwrap_err(),
            ProviderError::NotFound(_)
        ));
        assert!(matches!(
            FileKeyProvider::from_dir(&dir.0.join("no-dir")).unwrap_err(),
            ProviderError::NotFound(_)
        ));
        assert!(matches!(
            FileKeyProvider::from_dir(&dir.0).unwrap_err(),
            ProviderError::NotFound(_)
        ));
        // A directory is not a key file.
        let sub = dir.0.join("sub");
        std::fs::create_dir(&sub).unwrap();
        assert!(matches!(
            FileKeyProvider::from_files([(1, sub)]).unwrap_err(),
            ProviderError::Corrupt(_)
        ));
        assert!(matches!(
            FileKeyProvider::from_files(Vec::new()).unwrap_err(),
            ProviderError::NotFound(_)
        ));
    }

    #[test]
    fn file_provider_rejects_duplicate_versions_and_bad_names() {
        let dir = TempDir::new();
        let a = dir.write("a.key", hex_of(1).as_bytes(), 0o600);
        let b = dir.write("b.key", hex_of(2).as_bytes(), 0o600);
        assert!(matches!(
            FileKeyProvider::from_files([(1, a), (1, b)]).unwrap_err(),
            ProviderError::Corrupt(_)
        ));
        for name in [
            "kek-01.key",
            "kek-.key",
            "kek-+1.key",
            "kek-4294967296.key",
            "kek-x.key",
        ] {
            let dir = TempDir::new();
            dir.write(name, hex_of(1).as_bytes(), 0o600);
            assert!(
                matches!(
                    FileKeyProvider::from_dir(&dir.0).unwrap_err(),
                    ProviderError::Corrupt(_)
                ),
                "{name}"
            );
        }
        let dir = TempDir::new();
        dir.write("kek-0.key", hex_of(1).as_bytes(), 0o600);
        dir.write("kek-4294967295.key", hex_of(2).as_bytes(), 0o600);
        assert_eq!(
            FileKeyProvider::from_dir(&dir.0).unwrap().kek_versions(),
            vec![0, u32::MAX]
        );
    }

    #[test]
    fn canonical_versions() {
        assert_eq!(canonical_version("0"), Some(0));
        assert_eq!(canonical_version("7"), Some(7));
        assert_eq!(canonical_version("4294967295"), Some(u32::MAX));
        for bad in ["", "00", "01", "+1", "-1", "4294967296", "1 ", "٣"] {
            assert_eq!(canonical_version(bad), None, "{bad:?}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn file_provider_refuses_files_open_to_group_or_others() {
        let dir = TempDir::new();
        for mode in [0o640, 0o604, 0o644, 0o660, 0o666, 0o777, 0o601] {
            let path = dir.write("loose.key", hex_of(1).as_bytes(), mode);
            let error = FileKeyProvider::from_files([(1, path)]).unwrap_err();
            assert!(
                matches!(error, ProviderError::Denied(_)),
                "{mode:o}: {error}"
            );
        }
        for mode in [0o600, 0o400, 0o700] {
            let path = dir.write("tight.key", hex_of(1).as_bytes(), mode);
            assert!(FileKeyProvider::from_files([(1, path)]).is_ok(), "{mode:o}");
        }
    }

    #[test]
    fn debug_output_names_versions_not_keys() {
        let dir = TempDir::new();
        dir.write("kek-3.key", hex_of(0xCD).as_bytes(), 0o600);
        let shown = format!("{:?}", FileKeyProvider::from_dir(&dir.0).unwrap());
        assert!(shown.contains("kek_versions"));
        assert!(!shown.to_lowercase().contains("cdcd"));
        assert!(!shown.contains("205"));
    }
}
