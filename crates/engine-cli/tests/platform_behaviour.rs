//! Behaviour that depends on the operating system's file system and text
//! conventions, through the real `engine-cli` binary: paths with spaces, Bengali
//! letters and `..` segments, forward slashes, the "never overwrite" rule on case
//! sensitive and case insensitive file systems, and passphrase files saved with
//! Windows line endings. Runs on every platform (no database, no key store); the
//! platform workflow runs it on Linux aarch64, macOS and Windows. See
//! `docs/PLATFORM-SUPPORT.md`.
use std::{
    path::{Path, PathBuf},
    process::{Command, Output},
};

const PASSPHRASE: &str = "synthetic platform test passphrase";

fn scratch() -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("dynodoc-platform-{}-{nanos}", std::process::id()));
    std::fs::create_dir(&dir).unwrap();
    dir
}

fn run(dir: Option<&Path>, args: &[&str]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_engine-cli"));
    command
        .args(args)
        .stdin(std::process::Stdio::null())
        .env_remove("DYNODOC_KEY_PASSPHRASE_FILE")
        .env("RUST_LOG", "off");
    if let Some(dir) = dir {
        command.current_dir(dir);
    }
    command.output().unwrap()
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

fn s(path: &Path) -> &str {
    path.to_str().unwrap()
}

/// A secret file only its owner may read: the loaders insist on it on Unix.
fn secret_file(path: &Path, bytes: &[u8]) {
    std::fs::write(path, bytes).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).unwrap();
    }
}

/// Create an encrypted key at `key` and return its public key.
fn keygen(key: &Path, pass: &Path) -> String {
    let created = run(
        None,
        &[
            "device",
            "keygen",
            "--encrypt",
            "--out",
            s(key),
            "--passphrase-file",
            s(pass),
        ],
    );
    assert!(created.status.success(), "{}", text(&created.stderr));
    let public = text(&created.stdout).trim().to_owned();
    assert_eq!(public.len(), 64, "{public}");
    public
}

fn public_key(dir: Option<&Path>, key: &str, pass: &str) -> Output {
    run(
        dir,
        &["device", "public-key", key, "--passphrase-file", pass],
    )
}

#[test]
fn key_paths_with_spaces_bengali_letters_and_dot_dot_segments_work() {
    let root = scratch();
    let nested = root.join("ফোল্ডার with spaces");
    std::fs::create_dir(&nested).unwrap();
    let key = nested.join("কী ফাইল.json");
    let pass = nested.join("pass file");
    secret_file(&pass, format!("{PASSPHRASE}\n").as_bytes());
    let public = keygen(&key, &pass);

    // The container is written byte for byte: no text-mode newline translation.
    assert!(!std::fs::read(&key).unwrap().contains(&b'\r'));

    // The same file through a `..` detour, with forward slashes, and relatively.
    let detour = nested
        .join("..")
        .join("ফোল্ডার with spaces")
        .join("কী ফাইল.json");
    let slashes = s(&key).replace('\\', "/");
    let relative = public_key(Some(&nested), "./কী ফাইল.json", "pass file");
    for output in [
        public_key(None, s(&detour), s(&pass)),
        public_key(None, &slashes, s(&pass)),
        relative,
    ] {
        assert!(output.status.success(), "{}", text(&output.stderr));
        assert_eq!(text(&output.stdout).trim(), public);
    }
    let _ = std::fs::remove_dir_all(&root);
}

/// Does this directory treat `Probe` and `pROBE` as one name (APFS and NTFS by
/// default) or two (Linux file systems)?
fn case_insensitive(dir: &Path) -> bool {
    let probe = dir.join("Case-Probe.tmp");
    std::fs::write(&probe, b"x").unwrap();
    let same = dir.join("cASE-pROBE.tmp").exists();
    std::fs::remove_file(&probe).unwrap();
    same
}

#[test]
fn an_existing_key_is_never_overwritten_not_even_under_another_letter_case() {
    let root = scratch();
    let pass = root.join("pass");
    secret_file(&pass, format!("{PASSPHRASE}\n").as_bytes());
    let key = root.join("Signing-Key.json");
    let variant = root.join("signing-key.JSON");
    let first = keygen(&key, &pass);
    let before = std::fs::read(&key).unwrap();

    let second = run(
        None,
        &[
            "device",
            "keygen",
            "--encrypt",
            "--out",
            s(&variant),
            "--passphrase-file",
            s(&pass),
        ],
    );
    if case_insensitive(&root) {
        // One name: the second key must be refused and the first left untouched.
        assert!(
            !second.status.success(),
            "overwrote a key through a different letter case"
        );
        assert_eq!(std::fs::read(&key).unwrap(), before);
    } else {
        // Two names: a second, different key appears and the first is untouched.
        assert!(second.status.success(), "{}", text(&second.stderr));
        assert_ne!(text(&second.stdout).trim(), first);
        assert_eq!(std::fs::read(&key).unwrap(), before);
    }
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_passphrase_file_saved_with_windows_line_endings_opens_the_same_key() {
    let root = scratch();
    let lf = root.join("pass-lf");
    let crlf = root.join("pass-crlf");
    let doubled = root.join("pass-crlf-crlf");
    secret_file(&lf, format!("{PASSPHRASE}\n").as_bytes());
    secret_file(&crlf, format!("{PASSPHRASE}\r\n").as_bytes());
    secret_file(&doubled, format!("{PASSPHRASE}\r\n\r\n").as_bytes());
    let key = root.join("key.json");
    let public = keygen(&key, &lf);

    // Notepad-style CRLF is one line ending and is removed.
    let ok = public_key(None, s(&key), s(&crlf));
    assert!(ok.status.success(), "{}", text(&ok.stderr));
    assert_eq!(text(&ok.stdout).trim(), public);
    // Only ONE line ending is removed: a second one is part of the passphrase.
    let bad = public_key(None, s(&key), s(&doubled));
    assert!(!bad.status.success());
    assert!(!text(&bad.stderr).contains(PASSPHRASE));
    let _ = std::fs::remove_dir_all(&root);
}
