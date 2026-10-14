//! Offline signing of an enrollment challenge, and key file creation and
//! protection. Private keys never leave this process.
use crate::{
    keyfile, keystore,
    keystore::Provider,
    passphrase::{Secret, Sources},
};
use clap::Subcommand;
use engine_core::repository::device::{Challenge, Proof, MAX_PROOF_BYTES};
use ring::{
    rand::SystemRandom,
    signature::{Ed25519KeyPair, KeyPair},
};
use std::{
    io::Read,
    path::{Path, PathBuf},
};
use uuid::Uuid;

#[derive(Subcommand)]
pub enum Action {
    /// Create a new Ed25519 signing key and print its public key. Never overwrites
    /// a file. The file is the long-lived credential for `sync --device-key`: it is
    /// created readable by you only (mode 0600; on Windows an access list with just
    /// your account, verified) and creation fails if that cannot be applied unless
    /// `--allow-insecure-key-file` is passed. Without `--encrypt` it is an
    /// unencrypted DER PKCS#8 file. Enroll it in your profile, then grant it the
    /// projects it may use.
    Keygen {
        #[arg(long)]
        out: PathBuf,
        /// Encrypt the key file (dynodoc-key-v1). The passphrase comes from
        /// --passphrase-file, DYNODOC_KEY_PASSPHRASE_FILE or a hidden prompt (asked
        /// twice); it is never accepted as an argument.
        #[arg(long)]
        encrypt: bool,
        /// With --encrypt: do not use a passphrase. A random secret is generated and
        /// held by the operating system (Windows DPAPI, macOS Keychain, Linux Secret
        /// Service through secret-tool). The file then opens only for that OS account.
        #[arg(long, requires = "encrypt")]
        keystore: bool,
    },
    /// Display the public key to enroll through your authenticated profile.
    PublicKey { key: PathBuf },
    /// Prove possession for the installation/account shown in your profile.
    Prove {
        challenge: PathBuf,
        #[arg(long)]
        authority: Uuid,
        #[arg(long)]
        identity: Uuid,
        #[arg(long)]
        key: PathBuf,
        #[arg(long)]
        out: PathBuf,
    },
    /// Encrypt an existing unencrypted key file. Replaces the file atomically with
    /// the same owner-only protection, or writes a new file with --out (never
    /// overwriting one). The key and its public key do not change, so an enrolled
    /// key stays enrolled. Old plaintext disk blocks are not erased: if the key may
    /// have been copied, generate and enroll a new one instead.
    Protect {
        key: PathBuf,
        /// Write the encrypted copy here and leave KEY untouched.
        #[arg(long)]
        out: Option<PathBuf>,
        /// Use an OS-held secret instead of a passphrase (see keygen --keystore).
        #[arg(long)]
        keystore: bool,
    },
    /// Write the unencrypted key back to disk (the opposite of protect). The
    /// private key will again be readable by anyone who can read the file.
    Unprotect {
        key: PathBuf,
        /// Write the plain key here and leave KEY untouched.
        #[arg(long)]
        out: Option<PathBuf>,
        /// Required: confirms that you want an unencrypted private key on disk.
        #[arg(long)]
        i_understand: bool,
    },
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Encrypt `pkcs8` for storage. `provider` None locks it with a new passphrase;
/// Some stores a random wrapping secret in that OS keystore. Returns the file
/// bytes and, when an external keystore entry was created, its provider so a
/// failed write can remove it again.
fn container(
    pkcs8: &[u8],
    provider: Option<Provider>,
    sources: &Sources,
    label: &str,
) -> anyhow::Result<(Vec<u8>, Option<Provider>)> {
    let public = keyfile::public_key_of(pkcs8)?;
    match provider {
        None => {
            let secret = crate::passphrase::create(sources, label)?;
            Ok((keyfile::seal(pkcs8, secret.as_bytes(), None)?, None))
        }
        Some(provider) => {
            let secret = keystore::random_secret()?;
            let blob = keystore::store(provider, &public, &secret)?;
            let binding = keyfile::Keystore {
                provider: provider.name().to_owned(),
                blob: blob.as_deref().map(keystore::encode_blob),
            };
            match keyfile::seal(pkcs8, secret.as_bytes(), Some(binding)) {
                Ok(bytes) => Ok((bytes, (!provider.blob_in_file()).then_some(provider))),
                Err(error) => {
                    let _ = keystore::forget(provider, &public);
                    Err(error)
                }
            }
        }
    }
}

/// Generate a key with the system CSPRNG, store it exclusively and owner-only
/// (see `private_file`) and return its lowercase hexadecimal public key. The
/// private key is never printed. `provider` is meaningful only with `encrypt`.
fn keygen(
    out: &Path,
    encrypt: bool,
    provider: Option<Provider>,
    sources: &Sources,
) -> anyhow::Result<String> {
    // Before any secret is stored anywhere (a keystore entry must not be orphaned).
    anyhow::ensure!(
        !out.exists(),
        "cannot create {} (an existing file is never overwritten)",
        out.display()
    );
    let pkcs8 = Ed25519KeyPair::generate_pkcs8(&SystemRandom::new())
        .map_err(|_| anyhow::anyhow!("system random generator failed"))?;
    let pkcs8 = Secret::new(pkcs8.as_ref().to_vec());
    // The written bytes must load through the same parser every signing command uses.
    let key = Ed25519KeyPair::from_pkcs8(pkcs8.as_bytes())
        .map_err(|_| anyhow::anyhow!("generated key failed validation"))?;
    let public = hex(key.public_key().as_ref());
    let (bytes, external) = if encrypt {
        container(
            pkcs8.as_bytes(),
            provider,
            sources,
            &out.display().to_string(),
        )?
    } else {
        (pkcs8.as_bytes().to_vec(), None)
    };
    if let Err(error) = crate::private_file::write_new(out, &bytes) {
        if let Some(provider) = external {
            let _ = keystore::forget(provider, &public);
        }
        return Err(error);
    }
    Ok(public)
}

/// Encrypt an existing plain key file (in place, or into a new `out`).
fn protect(
    path: &Path,
    out: Option<&Path>,
    provider: Option<Provider>,
    sources: &Sources,
) -> anyhow::Result<String> {
    let bytes = Secret::new(crate::signing::read_key_file(path)?);
    anyhow::ensure!(
        !keyfile::is_container(bytes.as_bytes()),
        "{} is already encrypted",
        path.display()
    );
    let public = keyfile::public_key_of(bytes.as_bytes())?;
    // Prepared before the secret is chosen: permission problems surface while the
    // original is still the only copy.
    let replacement = match out {
        Some(_) => None,
        None => {
            let replacement = crate::private_file::Replacement::prepare(path)?;
            replacement.keep_mode_of(path);
            Some(replacement)
        }
    };
    let (encrypted, external) = container(
        bytes.as_bytes(),
        provider,
        sources,
        &path.display().to_string(),
    )?;
    let written = match (out, replacement) {
        (Some(out), _) => crate::private_file::write_new(out, &encrypted),
        (None, Some(replacement)) => replacement.commit(&encrypted),
        (None, None) => unreachable!("in-place protection always has a replacement"),
    };
    if let Err(error) = written {
        if let Some(provider) = external {
            let _ = keystore::forget(provider, &public);
        }
        return Err(error);
    }
    Ok(public)
}

/// Decrypt an encrypted key file back to plain DER (in place, or into `out`).
fn unprotect(
    path: &Path,
    out: Option<&Path>,
    understood: bool,
    sources: &Sources,
) -> anyhow::Result<String> {
    anyhow::ensure!(
        understood,
        "this writes your private key to disk unencrypted; repeat with --i-understand to confirm"
    );
    let bytes = crate::signing::read_key_file(path)?;
    anyhow::ensure!(
        keyfile::is_container(&bytes),
        "{} is not an encrypted key file",
        path.display()
    );
    let parsed = keyfile::Container::parse(&bytes)?;
    let pkcs8 = crate::signing::unlock(&parsed, path, sources)?;
    let public = parsed.public_key().to_owned();
    let binding = parsed
        .keystore()
        .and_then(|k| Provider::parse(&k.provider))
        .filter(|p| !p.blob_in_file());
    match out {
        Some(out) => crate::private_file::write_new(out, pkcs8.as_bytes())?,
        None => {
            let replacement = crate::private_file::Replacement::prepare(path)?;
            replacement.keep_mode_of(path);
            replacement.commit(pkcs8.as_bytes())?;
        }
    }
    if let Some(provider) = binding {
        if let Err(error) = keystore::forget(provider, &public) {
            eprintln!("warning: the key file is now plain, but its OS keystore entry could not be removed: {error}");
        }
    }
    Ok(public)
}

pub fn run(action: Action) -> anyhow::Result<()> {
    match action {
        Action::Keygen {
            out,
            encrypt,
            keystore: os_keystore,
        } => {
            let provider = if os_keystore {
                Some(Provider::native()?)
            } else {
                None
            };
            println!("{}", keygen(&out, encrypt, provider, &Sources::current())?);
        }
        Action::PublicKey { key } => {
            let key = crate::signing::load(&key)?;
            println!("{}", hex(key.public_key().as_ref()));
        }
        Action::Prove {
            challenge,
            authority,
            identity,
            key,
            out,
        } => {
            let mut bytes = Vec::new();
            std::fs::File::open(challenge)?
                .take(MAX_PROOF_BYTES as u64 + 1)
                .read_to_end(&mut bytes)?;
            anyhow::ensure!(bytes.len() <= MAX_PROOF_BYTES, "challenge too large");
            let challenge: Challenge = serde_json::from_slice(&bytes)?;
            anyhow::ensure!(
                challenge.expires_at_ms as u128
                    > std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)?
                        .as_millis(),
                "challenge expired; request a fresh challenge"
            );
            let proof = Proof::sign(challenge, authority, identity, &crate::signing::load(&key)?)?;
            // Never overwrite a key or an unrelated existing file by mistake.
            let file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(out)?;
            serde_json::to_writer_pretty(file, &proof)?;
            println!("Enrollment proof saved. Submit it through the same signed-in profile.");
        }
        Action::Protect {
            key,
            out,
            keystore: os_keystore,
        } => {
            let provider = if os_keystore {
                Some(Provider::native()?)
            } else {
                None
            };
            let public = protect(&key, out.as_deref(), provider, &Sources::current())?;
            eprintln!(
                "The key file is now encrypted (dynodoc-key-v1). Its public key is unchanged:"
            );
            println!("{public}");
        }
        Action::Unprotect {
            key,
            out,
            i_understand,
        } => {
            let public = unprotect(&key, out.as_deref(), i_understand, &Sources::current())?;
            eprintln!("The key file is now unencrypted. Its public key is unchanged:");
            println!("{public}");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::OnceLock;

    pub(crate) const PASSPHRASE: &[u8] = b"synthetic test passphrase 0001";

    /// One shared passphrase file for every test that needs an encrypted key, so
    /// parallel tests agree on the process-wide `--passphrase-file` value.
    pub(crate) fn passphrase_file() -> &'static Path {
        static FILE: OnceLock<PathBuf> = OnceLock::new();
        FILE.get_or_init(|| {
            let path =
                std::env::temp_dir().join(format!("dynodoc-test-passphrase-{}", Uuid::new_v4()));
            std::fs::write(&path, [PASSPHRASE, b"\n"].concat()).unwrap();
            path
        })
    }

    pub(crate) fn sources() -> Sources {
        Sources {
            file: Some(passphrase_file().to_owned()),
            ..Default::default()
        }
    }

    fn dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("dynodoc-keygen-{}", Uuid::new_v4()));
        std::fs::create_dir(&dir).unwrap();
        dir
    }

    fn public_of(key: &Ed25519KeyPair) -> String {
        hex(key.public_key().as_ref())
    }

    #[test]
    fn keygen_creates_a_loadable_private_key_once() {
        let dir = dir();
        let path = dir.join("signing-key.der");
        let none = Sources::default();
        let public = keygen(&path, false, None, &none).unwrap();
        assert_eq!(public.len(), 64);
        let loaded = crate::signing::load(&path).unwrap();
        assert_eq!(public, public_of(&loaded));
        // A second run neither overwrites the key nor prints another one.
        let before = std::fs::read(&path).unwrap();
        assert!(keygen(&path, false, None, &none).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), before);
        assert_ne!(
            keygen(&dir.join("another.der"), false, None, &none).unwrap(),
            public
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn encrypted_keygen_round_trips_and_hides_the_private_key() {
        let dir = dir();
        let path = dir.join("encrypted-key.json");
        let public = keygen(&path, true, None, &sources()).unwrap();
        let file = std::fs::read(&path).unwrap();
        assert!(keyfile::is_container(&file));
        let text = String::from_utf8(file.clone()).unwrap();
        assert!(text.contains(&public), "the public key is readable");
        assert!(!text.contains(std::str::from_utf8(PASSPHRASE).unwrap()));
        crate::private_file::assert_owner_only(&path);
        // No source: a clear error, and nothing about the key. (Checked before the
        // first successful load, which would cache the key for this process.)
        let error = crate::signing::load_with(&path, &Sources::default())
            .err()
            .unwrap()
            .to_string();
        assert!(error.contains("passphrase source"), "{error}");
        // A wrong passphrase file is the generic error.
        let wrong = dir.join("wrong");
        std::fs::write(
            &wrong,
            "not the right passphrase at all
",
        )
        .unwrap();
        let error = crate::signing::load_with(
            &path,
            &Sources {
                file: Some(wrong),
                ..Default::default()
            },
        )
        .err()
        .unwrap()
        .to_string();
        assert!(
            error.contains("wrong passphrase, or the file was changed"),
            "{error}"
        );
        let loaded = crate::signing::load_with(&path, &sources()).unwrap();
        assert_eq!(public_of(&loaded), public);
        // An existing file is never overwritten and the file is unchanged.
        assert!(keygen(&path, true, None, &sources()).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), file);
        // A too-short new passphrase creates nothing.
        let short = dir.join("short");
        std::fs::write(&short, "short\n").unwrap();
        let target = dir.join("never.json");
        assert!(keygen(
            &target,
            true,
            None,
            &Sources {
                file: Some(short),
                ..Default::default()
            }
        )
        .is_err());
        assert!(!target.exists());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn protect_in_place_keeps_the_key_and_the_owner_only_access() {
        let dir = dir();
        let path = dir.join("signing-key.der");
        let public = keygen(&path, false, None, &Sources::default()).unwrap();
        let plain = std::fs::read(&path).unwrap();
        let before_public = public_of(&crate::signing::load(&path).unwrap());
        assert_eq!(before_public, public);

        assert_eq!(protect(&path, None, None, &sources()).unwrap(), public);
        let encrypted = std::fs::read(&path).unwrap();
        assert!(keyfile::is_container(&encrypted));
        assert!(!encrypted.windows(32).any(|w| w == &plain[16..48]));
        crate::private_file::assert_owner_only(&path);
        // No temporary files are left behind.
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
        // Same key, same public key, through the transparent loader.
        let loaded = crate::signing::load_with(&path, &sources()).unwrap();
        assert_eq!(public_of(&loaded), public);
        // A second protect refuses and leaves the file as it is.
        let error = protect(&path, None, None, &sources())
            .unwrap_err()
            .to_string();
        assert!(error.contains("already encrypted"), "{error}");
        assert_eq!(std::fs::read(&path).unwrap(), encrypted);

        // Unprotect needs the explicit confirmation and then restores the plain key.
        let error = unprotect(&path, None, false, &sources())
            .unwrap_err()
            .to_string();
        assert!(error.contains("--i-understand"), "{error}");
        assert_eq!(std::fs::read(&path).unwrap(), encrypted);
        assert_eq!(unprotect(&path, None, true, &sources()).unwrap(), public);
        assert_eq!(std::fs::read(&path).unwrap(), plain);
        crate::private_file::assert_owner_only(&path);
        assert!(unprotect(&path, None, true, &sources()).is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn protect_to_a_new_file_never_overwrites_and_failure_changes_nothing() {
        let dir = dir();
        let path = dir.join("signing-key.der");
        keygen(&path, false, None, &Sources::default()).unwrap();
        let plain = std::fs::read(&path).unwrap();
        let out = dir.join("protected.json");
        protect(&path, Some(&out), None, &sources()).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), plain, "original untouched");
        let first = std::fs::read(&out).unwrap();
        // The destination exists now: refused, both files unchanged.
        assert!(protect(&path, Some(&out), None, &sources()).is_err());
        assert_eq!(std::fs::read(&out).unwrap(), first);
        assert_eq!(std::fs::read(&path).unwrap(), plain);
        // No passphrase source: nothing is created, the key stays.
        let other = dir.join("other.json");
        assert!(protect(&path, Some(&other), None, &Sources::default()).is_err());
        assert!(!other.exists());
        assert!(protect(&path, None, None, &Sources::default()).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), plain);
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 2);
        // Unprotect into a new file leaves the encrypted source in place.
        let plain_copy = dir.join("plain-copy.der");
        unprotect(&out, Some(&plain_copy), true, &sources()).unwrap();
        assert_eq!(std::fs::read(&plain_copy).unwrap(), plain);
        assert_eq!(std::fs::read(&out).unwrap(), first);
        assert!(unprotect(&out, Some(&plain_copy), true, &sources()).is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn keystore_bound_keys_need_no_passphrase_and_forget_on_unprotect() {
        let dir = dir();
        let path = dir.join("keystore-key.json");
        // No passphrase source at all: the secret is held by the (test) keystore.
        let none = Sources::default();
        let public = keygen(&path, true, Some(Provider::Memory), &none).unwrap();
        let file = std::fs::read(&path).unwrap();
        let text = String::from_utf8(file).unwrap();
        assert!(text.contains("test-memory"));
        let loaded = crate::signing::load_with(&path, &none).unwrap();
        assert_eq!(public_of(&loaded), public);
        // Converting back removes the stored secret; the file can no longer open.
        assert_eq!(unprotect(&path, None, true, &none).unwrap(), public);
        assert_eq!(
            public_of(&crate::signing::load_with(&path, &none).unwrap()),
            public
        );
        assert!(keystore::load(Provider::Memory, &public, None).is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// Real DPAPI on Windows: the wrapping secret is inside the file, protected for
    /// this account, and the key loads with no passphrase and no external state.
    #[cfg(windows)]
    #[test]
    fn dpapi_bound_key_works_on_this_windows_account() {
        let dir = dir();
        let path = dir.join("dpapi-key.json");
        let none = Sources::default();
        let public = keygen(&path, true, Some(Provider::WindowsDpapi), &none).unwrap();
        let file = std::fs::read(&path).unwrap();
        let text = String::from_utf8(file).unwrap();
        assert!(text.contains("windows-dpapi"));
        assert!(text.contains("\"blob\""));
        crate::private_file::assert_owner_only(&path);
        assert_eq!(
            public_of(&crate::signing::load_with(&path, &none).unwrap()),
            public
        );
        // The file is plain JSON with an opaque blob; copying only it elsewhere on
        // the same account still works, and editing the blob does not.
        let mut value: serde_json::Value = serde_json::from_str(&text).unwrap();
        let blob = value["keystore"]["blob"].as_str().unwrap().to_owned();
        // DPAPI blobs start with the same bytes ("AQAAANCM..."); change one inside.
        let mut chars: Vec<char> = blob.chars().collect();
        chars[40] = if chars[40] == 'A' { 'B' } else { 'A' };
        value["keystore"]["blob"] = chars.into_iter().collect::<String>().into();
        let damaged = dir.join("damaged.json");
        std::fs::write(&damaged, serde_json::to_vec(&value).unwrap()).unwrap();
        assert!(crate::signing::load_with(&damaged, &none).is_err());
        // Existing plain key -> DPAPI-bound -> plain, in place.
        let plain = dir.join("plain.der");
        keygen(&plain, false, None, &none).unwrap();
        let before = std::fs::read(&plain).unwrap();
        protect(&plain, None, Some(Provider::WindowsDpapi), &none).unwrap();
        assert!(keyfile::is_container(&std::fs::read(&plain).unwrap()));
        unprotect(&plain, None, true, &none).unwrap();
        assert_eq!(std::fs::read(&plain).unwrap(), before);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn no_command_line_option_accepts_a_passphrase_value() {
        use clap::CommandFactory;
        #[derive(clap::Parser)]
        struct Wrapper {
            #[command(subcommand)]
            action: Action,
        }
        let command = Wrapper::command();
        for sub in command.get_subcommands() {
            for arg in sub.get_arguments() {
                let id = arg.get_id().as_str();
                assert!(
                    !id.contains("passphrase")
                        && !id.contains("password")
                        && !id.contains("secret"),
                    "{} {id}",
                    sub.get_name()
                );
            }
        }
        // And clap itself refuses an attempt to pass one.
        let attempt = <Wrapper as clap::Parser>::try_parse_from([
            "engine-cli",
            "keygen",
            "--out",
            "k",
            "--encrypt",
            "--passphrase",
            "hunter2-hunter2",
        ]);
        assert!(attempt.is_err());
    }
}
