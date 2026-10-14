//! Ed25519 PKCS#8 signing keys for the local signing commands: unencrypted DER, or
//! a `dynodoc-key-v1` encrypted container (docs/KEY-FILE-FORMAT.md). Every command
//! that reads a key file goes through [`load`], so encrypted files work wherever a
//! plain key does. A decrypted key is cached in memory for this process only.
use crate::{
    keyfile, keystore,
    passphrase::{Secret, Sources},
};
use ring::signature::Ed25519KeyPair;
use std::{
    io::Read,
    path::{Path, PathBuf},
    sync::Mutex,
};

fn parse(bytes: &[u8]) -> anyhow::Result<Ed25519KeyPair> {
    anyhow::ensure!(bytes.len() <= 4096, "signing key file too large");
    // OpenSSL exports v1 without a redundant public key. Ring
    // derives that key from the seed; v2 still checks both components match.
    Ed25519KeyPair::from_pkcs8_maybe_unchecked(bytes).map_err(|_| {
        anyhow::anyhow!(
            "invalid Ed25519 PKCS#8 key file (unencrypted DER or a dynodoc-key-v1 container)"
        )
    })
}

/// Read a key file's bytes. On Unix a file that group or others can access is
/// refused (see `private_file::check_key_file`); `--allow-insecure-key-file`
/// accepts it.
pub(crate) fn read_key_file(path: &Path) -> anyhow::Result<Vec<u8>> {
    let file = std::fs::File::open(path)?;
    crate::private_file::check_key_file(&file, path)?;
    let mut bytes = Vec::with_capacity(4097);
    file.take(keyfile::MAX_FILE_BYTES as u64 + 1)
        .read_to_end(&mut bytes)?;
    anyhow::ensure!(
        bytes.len() <= keyfile::MAX_FILE_BYTES,
        "signing key file too large"
    );
    Ok(bytes)
}

/// Decrypt a parsed container: its OS keystore when it names one, otherwise a
/// passphrase from the configured sources.
pub(crate) fn unlock(
    container: &keyfile::Container,
    path: &Path,
    sources: &Sources,
) -> anyhow::Result<Secret> {
    let secret = match container.keystore() {
        Some(binding) => {
            let provider = keystore::Provider::parse(&binding.provider)
                .ok_or_else(|| anyhow::anyhow!("unknown keystore provider"))?;
            keystore::load(
                provider,
                container.public_key(),
                container.keystore_blob().as_deref(),
            )?
        }
        None => crate::passphrase::unlock(sources, &path.display().to_string())?,
    };
    container.open(secret.as_bytes())
}

/// Decrypted keys (PKCS#8) for this process, keyed by canonical path and the
/// digest of the exact file content. Never written anywhere.
static CACHE: Mutex<Vec<(PathBuf, Vec<u8>, Secret)>> = Mutex::new(Vec::new());

/// Load a plain or encrypted key file.
pub(crate) fn load(path: &Path) -> anyhow::Result<Ed25519KeyPair> {
    load_with(path, &Sources::current())
}

pub(crate) fn load_with(path: &Path, sources: &Sources) -> anyhow::Result<Ed25519KeyPair> {
    let bytes = read_key_file(path)?;
    if !keyfile::is_container(&bytes) {
        return parse(&bytes);
    }
    let canonical = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_owned());
    let digest = ring::digest::digest(&ring::digest::SHA256, &bytes)
        .as_ref()
        .to_vec();
    {
        let cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(entry) = cache
            .iter()
            .find(|entry| entry.0 == canonical && entry.1 == digest)
        {
            return parse(entry.2.as_bytes());
        }
    }
    let container = keyfile::Container::parse(&bytes)?;
    let pkcs8 = unlock(&container, path, sources)?;
    let pair = parse(pkcs8.as_bytes())?;
    CACHE
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .push((canonical, digest, pkcs8));
    Ok(pair)
}

#[cfg(test)]
mod tests {
    use super::*;
    use engine_core::repository::attestation::SignedReceipt;
    #[test]
    fn standard_v1_and_v2_keys_work_without_weakening_component_checks() {
        // Public synthetic seed, matching the independent OpenSSL signature fixture.
        let mut v1 = vec![
            0x30, 0x2e, 0x02, 0x01, 0x00, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x04, 0x22,
            0x04, 0x20,
        ];
        v1.extend([7; 32]);
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../engine-core/tests/fixtures/ref-receipt-signatures-v1.json"
        ))
        .unwrap();
        let expected: SignedReceipt =
            serde_json::from_value(fixture["first"]["signed"].clone()).unwrap();
        assert_eq!(
            SignedReceipt::sign(expected.receipt.clone(), &parse(&v1).unwrap()).unwrap(),
            expected
        );
        assert!(parse(&v1[..v1.len() - 1]).is_err());
        v1[11] = 0x71; // Ed448 is not an Ed25519 key.
        assert!(parse(&v1).is_err());
        assert!(parse(&vec![0; 4097]).is_err());
        let v2 = Ed25519KeyPair::generate_pkcs8(&ring::rand::SystemRandom::new()).unwrap();
        assert!(parse(v2.as_ref()).is_ok());
        let mut mismatch = v2.as_ref().to_vec();
        *mismatch.last_mut().unwrap() ^= 1;
        assert!(
            parse(&mismatch).is_err(),
            "v2 public/private consistency must remain enforced"
        );
    }
}
