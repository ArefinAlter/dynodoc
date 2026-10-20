//! The real operating-system key store, end to end through the real `engine-cli`
//! binary: `device keygen --encrypt --keystore`, opening the key from the store
//! (macOS Keychain through `/usr/bin/security`, Linux Secret Service through
//! `secret-tool`, Windows DPAPI), and removing the stored secret again.
//!
//! Ignored by default because it writes to the signed-in user's key store. The
//! platform workflow (`.github/workflows/platforms.yml`) prepares a throwaway one
//! first (a temporary keychain on macOS, a private D-Bus session with a fresh
//! gnome-keyring on Linux) and runs it with `--ignored`:
//!
//! ```text
//! cargo test -p engine-cli --test native_keystore -- --ignored --nocapture
//! ```
//!
//! Every child process has a hard timeout, because a headless key store that wants
//! to show a prompt would otherwise block forever.
use std::{
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
    time::{Duration, Instant},
};

const TIMEOUT: Duration = Duration::from_secs(60);

fn scratch() -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "dynodoc-native-keystore-{}-{nanos}",
        std::process::id()
    ));
    std::fs::create_dir(&dir).unwrap();
    dir
}

/// Run the CLI with no passphrase source and no terminal, killing it at TIMEOUT.
fn run(args: &[&str]) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_engine-cli"))
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env_remove("DYNODOC_KEY_PASSPHRASE_FILE")
        .env("RUST_LOG", "off")
        .spawn()
        .unwrap();
    let started = Instant::now();
    loop {
        if child.try_wait().unwrap().is_some() {
            return child.wait_with_output().unwrap();
        }
        if started.elapsed() > TIMEOUT {
            child.kill().unwrap();
            let output = child.wait_with_output().unwrap();
            panic!(
                "engine-cli {args:?} did not finish in {TIMEOUT:?} (a key store prompt?)\n{}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

fn s(path: &Path) -> &str {
    path.to_str().unwrap()
}

fn provider() -> &'static str {
    if cfg!(windows) {
        "windows-dpapi"
    } else if cfg!(target_os = "macos") {
        "macos-keychain"
    } else {
        "linux-secret-service"
    }
}

#[test]
#[ignore = "writes to the user's OS key store; run by the platform workflow against a throwaway one"]
fn the_native_key_store_holds_the_secret_and_gives_it_back() {
    let dir = scratch();
    let key = dir.join("signing-key.json");
    let copy = dir.join("signing-key-copy.json");
    let plain = dir.join("signing-key-plain.der");

    // Create: no passphrase exists anywhere; the secret goes to the OS store.
    let created = run(&[
        "device",
        "keygen",
        "--encrypt",
        "--keystore",
        "--out",
        s(&key),
    ]);
    assert!(
        created.status.success(),
        "keygen --keystore failed: {}",
        text(&created.stderr)
    );
    let public = text(&created.stdout).trim().to_owned();
    assert_eq!(public.len(), 64, "{public}");
    let file = std::fs::read_to_string(&key).unwrap();
    assert!(file.contains("\"format\": \"dynodoc-key-v1\""), "{file}");
    assert!(file.contains(provider()), "{file}");
    // Windows keeps the DPAPI blob in the file; the other providers keep nothing.
    assert_eq!(file.contains("\"blob\""), cfg!(windows), "{file}");
    std::fs::copy(&key, &copy).unwrap();

    // Open: the secret comes back from the store and decrypts the key.
    let opened = run(&["device", "public-key", s(&key)]);
    assert!(
        opened.status.success(),
        "reading the key back failed: {}",
        text(&opened.stderr)
    );
    assert_eq!(text(&opened.stdout).trim(), public);

    // Remove: unprotecting forgets the stored secret (a no-op for DPAPI, whose
    // blob lives in the file and so keeps opening for this account).
    let plain_out = run(&[
        "device",
        "unprotect",
        s(&key),
        "--out",
        s(&plain),
        "--i-understand",
    ]);
    assert!(
        plain_out.status.success(),
        "unprotect failed: {}",
        text(&plain_out.stderr)
    );
    assert!(
        !text(&plain_out.stderr).contains("warning"),
        "the store entry was not removed: {}",
        text(&plain_out.stderr)
    );
    let after = run(&["device", "public-key", s(&copy)]);
    if cfg!(windows) {
        assert!(after.status.success(), "{}", text(&after.stderr));
        assert_eq!(text(&after.stdout).trim(), public);
    } else {
        assert!(
            !after.status.success(),
            "the OS store still gave the secret back after unprotect"
        );
        assert_eq!(text(&after.stdout).trim(), "");
    }
    let _ = std::fs::remove_dir_all(&dir);
}
