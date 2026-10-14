//! The real `engine-cli` binary with encrypted key files: key creation, protection
//! and conversion, transparent loading by `device`, `repository commit` and
//! `sync`, the passphrase sources, and that no secret appears in arguments or
//! output. Everything is synthetic and local; the sync case talks only to a
//! throwaway loopback socket that records what the client sends.
use std::{
    io::{Read, Write},
    net::TcpListener,
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::{Arc, Mutex},
    time::Duration,
};

const PASSPHRASE: &str = "synthetic test passphrase for the key file tests";
const PROJECT: &str = "33333333-3333-4333-8333-333333333333";
const DOCUMENT: &str = "44444444-4444-4444-8444-444444444444";

fn exe() -> &'static str {
    env!("CARGO_BIN_EXE_engine-cli")
}

fn scratch() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("dynodoc-key-files-{}", unique()));
    std::fs::create_dir(&dir).unwrap();
    dir
}

fn unique() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!(
        "{}-{nanos}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    )
}

fn run(args: &[&str], envs: &[(&str, &Path)]) -> Output {
    let mut command = Command::new(exe());
    command
        .args(args)
        .stdin(std::process::Stdio::null())
        .env_remove("DYNODOC_KEY_PASSPHRASE_FILE")
        .env("RUST_LOG", "off");
    for (name, value) in envs {
        command.env(name, value);
    }
    command.output().unwrap()
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

fn passphrase_file(dir: &Path, name: &str, value: &str) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, format!("{value}\n")).unwrap();
    // The loaders refuse a secret file that other accounts can read.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    }
    path
}

fn s(path: &Path) -> &str {
    path.to_str().unwrap()
}

/// stdout and stderr of a finished process, for leak checks.
fn all_output(output: &Output) -> String {
    format!("{}\n{}", text(&output.stdout), text(&output.stderr))
}

#[test]
fn encrypted_keys_are_created_loaded_converted_and_never_leak_the_passphrase() {
    let dir = scratch();
    let pass = passphrase_file(&dir, "pass", PASSPHRASE);
    let key = dir.join("signing-key.json");

    // Create. The public key is printed; the file is a container, not DER.
    let created = run(
        &[
            "device",
            "keygen",
            "--encrypt",
            "--out",
            s(&key),
            "--passphrase-file",
            s(&pass),
        ],
        &[],
    );
    assert!(created.status.success(), "{}", all_output(&created));
    let public = text(&created.stdout).trim().to_owned();
    assert_eq!(public.len(), 64, "{public}");
    let file = std::fs::read_to_string(&key).unwrap();
    assert!(file.contains("\"format\": \"dynodoc-key-v1\""));
    assert!(file.contains(&public));
    assert!(!all_output(&created).contains(PASSPHRASE));

    // It is never overwritten.
    let again = run(
        &[
            "device",
            "keygen",
            "--encrypt",
            "--out",
            s(&key),
            "--passphrase-file",
            s(&pass),
        ],
        &[],
    );
    assert!(!again.status.success());
    assert_eq!(std::fs::read_to_string(&key).unwrap(), file);

    // Load through each passphrase source; --passphrase-file wins over the env.
    let by_flag = run(
        &[
            "device",
            "public-key",
            s(&key),
            "--passphrase-file",
            s(&pass),
        ],
        &[],
    );
    assert!(by_flag.status.success(), "{}", all_output(&by_flag));
    assert_eq!(text(&by_flag.stdout).trim(), public);
    let by_env = run(
        &["device", "public-key", s(&key)],
        &[("DYNODOC_KEY_PASSPHRASE_FILE", &pass)],
    );
    assert!(by_env.status.success(), "{}", all_output(&by_env));
    assert_eq!(text(&by_env.stdout).trim(), public);
    let wrong_file = passphrase_file(&dir, "wrong", "definitely not the right passphrase");
    let flag_over_env = run(
        &[
            "device",
            "public-key",
            s(&key),
            "--passphrase-file",
            s(&pass),
        ],
        &[("DYNODOC_KEY_PASSPHRASE_FILE", &wrong_file)],
    );
    assert!(flag_over_env.status.success());

    // Failures: wrong passphrase, none at all (stdin is not a terminal).
    let wrong = run(
        &[
            "device",
            "public-key",
            s(&key),
            "--passphrase-file",
            s(&wrong_file),
        ],
        &[],
    );
    assert!(!wrong.status.success());
    let message = all_output(&wrong);
    assert!(
        message.contains("wrong passphrase, or the file was changed"),
        "{message}"
    );
    assert!(!message.contains("definitely not the right passphrase"));
    let none = run(&["device", "public-key", s(&key)], &[]);
    assert!(!none.status.success());
    assert!(all_output(&none).contains("passphrase source"));

    // A passphrase can never be given as an argument.
    let argument = run(
        &[
            "device",
            "keygen",
            "--encrypt",
            "--out",
            s(&dir.join("x.json")),
            "--passphrase",
            "an-argument-passphrase-value",
        ],
        &[],
    );
    assert!(!argument.status.success());
    assert!(!dir.join("x.json").exists());
    assert!(!all_output(&argument).contains("an-argument-passphrase-value"));

    // A too-short new passphrase is refused and nothing is created.
    let short = passphrase_file(&dir, "short", "short");
    let refused = run(
        &[
            "device",
            "keygen",
            "--encrypt",
            "--out",
            s(&dir.join("short.json")),
            "--passphrase-file",
            s(&short),
        ],
        &[],
    );
    assert!(!refused.status.success());
    assert!(!dir.join("short.json").exists());

    // Convert: unprotect needs the confirmation, then protect restores encryption
    // with the same public key.
    let plain_copy = dir.join("plain.der");
    let unconfirmed = run(
        &[
            "device",
            "unprotect",
            s(&key),
            "--out",
            s(&plain_copy),
            "--passphrase-file",
            s(&pass),
        ],
        &[],
    );
    assert!(!unconfirmed.status.success());
    assert!(!plain_copy.exists());
    let unprotected = run(
        &[
            "device",
            "unprotect",
            s(&key),
            "--out",
            s(&plain_copy),
            "--i-understand",
            "--passphrase-file",
            s(&pass),
        ],
        &[],
    );
    assert!(unprotected.status.success(), "{}", all_output(&unprotected));
    assert_eq!(text(&unprotected.stdout).trim(), public);
    assert_eq!(std::fs::read(&plain_copy).unwrap().len(), 83);
    let plain_public = run(&["device", "public-key", s(&plain_copy)], &[]);
    assert_eq!(text(&plain_public.stdout).trim(), public);
    let reprotected = dir.join("again.json");
    let protect = run(
        &[
            "device",
            "protect",
            s(&plain_copy),
            "--out",
            s(&reprotected),
            "--passphrase-file",
            s(&pass),
        ],
        &[],
    );
    assert!(protect.status.success(), "{}", all_output(&protect));
    assert_eq!(text(&protect.stdout).trim(), public);
    let again_public = run(
        &[
            "device",
            "public-key",
            s(&reprotected),
            "--passphrase-file",
            s(&pass),
        ],
        &[],
    );
    assert_eq!(text(&again_public.stdout).trim(), public);
    // In-place protection replaces the plain file atomically.
    let in_place = run(
        &[
            "device",
            "protect",
            s(&plain_copy),
            "--passphrase-file",
            s(&pass),
        ],
        &[],
    );
    assert!(in_place.status.success(), "{}", all_output(&in_place));
    assert!(std::fs::read_to_string(&plain_copy)
        .unwrap()
        .contains("dynodoc-key-v1"));
    assert_eq!(
        std::fs::read_dir(&dir)
            .unwrap()
            .filter(|e| e
                .as_ref()
                .unwrap()
                .file_name()
                .to_string_lossy()
                .ends_with(".tmp"))
            .count(),
        0
    );
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn an_encrypted_key_signs_a_commit_in_the_local_repository_flow() {
    let dir = scratch();
    let pass = passphrase_file(&dir, "pass", PASSPHRASE);
    let key = dir.join("author.json");
    let created = run(
        &[
            "device",
            "keygen",
            "--encrypt",
            "--out",
            s(&key),
            "--passphrase-file",
            s(&pass),
        ],
        &[],
    );
    assert!(created.status.success(), "{}", all_output(&created));
    let public = text(&created.stdout).trim().to_owned();

    let database = dir.join("project.sqlite");
    let repo = |action: &[&str], extra: &[&str]| {
        let mut args = vec!["repository", s(&database), PROJECT];
        args.extend(action);
        args.extend(extra);
        run(&args, &[("DYNODOC_KEY_PASSPHRASE_FILE", &pass)])
    };
    let state = dir.join("state.json");
    std::fs::write(
        &state,
        r#"{"nodes":{"00000000000000000000000001":{"id":"00000000000000000000000001","parent_id":null,"type":"paragraph","pos":"p000000","var_name":null,"deleted":false,"current_fields":{"label":"Encrypted key flow"}}},"comments":[],"suggestions":{},"removed_choices":[]}"#,
    )
    .unwrap();
    let stored = repo(&["store-state", DOCUMENT, s(&state)], &[]);
    assert!(stored.status.success(), "{}", all_output(&stored));
    let stored: serde_json::Value = serde_json::from_slice(&stored.stdout).unwrap();
    let manifest = dir.join("commit.json");
    std::fs::write(
        &manifest,
        serde_json::json!({
            "version": 1, "project": PROJECT, "parents": [],
            "files": {"Report.docx": {"document": DOCUMENT, "state": stored["state"]}},
            "author_key": "", "authored_at_ms": 1_700_000_000_000i64, "message": "Signed with an encrypted key"
        })
        .to_string(),
    )
    .unwrap();

    // Without a passphrase source the command fails before storing anything.
    let refused = run(
        &[
            "repository",
            s(&database),
            PROJECT,
            "commit",
            s(&manifest),
            s(&key),
        ],
        &[],
    );
    assert!(!refused.status.success());
    assert!(all_output(&refused).contains("passphrase source"));
    let usage = repo(&["storage"], &[]);
    let usage: serde_json::Value = serde_json::from_slice(&usage.stdout).unwrap();
    assert_eq!(usage["commits_used"], 0);

    let output = repo(&["commit", s(&manifest), s(&key)], &[]);
    assert!(output.status.success(), "{}", all_output(&output));
    assert!(!all_output(&output).contains(PASSPHRASE));
    let committed: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let address = committed["commit"].as_str().unwrap();
    let shown = repo(&["show", address], &[]);
    let shown: serde_json::Value = serde_json::from_slice(&shown.stdout).unwrap();
    assert_eq!(shown["commit"]["author_key"], public);
    std::fs::remove_dir_all(dir).unwrap();
}

/// A loopback socket that records every request line and answers 500, so the
/// client's first network actions can be observed without a server.
fn recorder() -> (String, Arc<Mutex<Vec<String>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/", listener.local_addr().unwrap());
    let seen = Arc::new(Mutex::new(Vec::new()));
    let log = seen.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { return };
            stream
                .set_read_timeout(Some(Duration::from_millis(500)))
                .ok();
            let mut buffer = vec![0u8; 16 * 1024];
            let n = stream.read(&mut buffer).unwrap_or(0);
            log.lock()
                .unwrap()
                .push(String::from_utf8_lossy(&buffer[..n]).into_owned());
            let _ = stream.write_all(
                b"HTTP/1.1 500 Internal Server Error\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            );
        }
    });
    (url, seen)
}

#[test]
fn sync_loads_an_encrypted_device_key_and_signs_in_with_its_public_key() {
    let dir = scratch();
    let pass = passphrase_file(&dir, "pass", PASSPHRASE);
    let key = dir.join("device.json");
    let created = run(
        &[
            "device",
            "keygen",
            "--encrypt",
            "--out",
            s(&key),
            "--passphrase-file",
            s(&pass),
        ],
        &[],
    );
    assert!(created.status.success(), "{}", all_output(&created));
    let public = text(&created.stdout).trim().to_owned();
    let (url, seen) = recorder();
    let remote = dir.join("remote.json");
    std::fs::write(
        &remote,
        serde_json::json!({
            "version": 1, "url": url,
            "authority": "55555555-5555-4555-8555-555555555555",
            "public_key": "aa".repeat(32)
        })
        .to_string(),
    )
    .unwrap();
    let repository = dir.join("repo.sqlite");
    let journal = dir.join("pins.sqlite");
    let sync = |passphrase: Option<&Path>| {
        let mut args = vec![
            "sync",
            s(&repository),
            PROJECT,
            "--remote",
            s(&remote),
            "--journal",
            s(&journal),
            "--branch",
            "main",
            "--device-key",
            s(&key),
            "--allow-loopback-http",
        ];
        if let Some(path) = passphrase {
            args.extend(["--passphrase-file", s(path)]);
        }
        args.push("fetch");
        run(&args, &[])
    };

    // No passphrase source: fails before any network traffic.
    let none = sync(None);
    assert!(!none.status.success());
    assert!(all_output(&none).contains("passphrase source"));
    assert!(seen.lock().unwrap().is_empty());
    // A wrong passphrase: the generic error, still no traffic.
    let wrong = passphrase_file(&dir, "wrong", "another passphrase that is wrong");
    let denied = sync(Some(&wrong));
    assert!(!denied.status.success());
    assert!(all_output(&denied).contains("wrong passphrase, or the file was changed"));
    assert!(seen.lock().unwrap().is_empty());

    // The right one: the client reaches the network and asks for a device-session
    // challenge for exactly the key inside the encrypted file.
    let outcome = sync(Some(&pass));
    assert!(!outcome.status.success(), "the recorder answers 500");
    let requests = seen.lock().unwrap().join("\n");
    assert!(
        requests.contains("device-sessions/challenge"),
        "first requests were: {requests}"
    );
    assert!(requests.contains(&public), "{requests}");
    assert!(!requests.contains(PASSPHRASE));
    assert!(!all_output(&outcome).contains(PASSPHRASE));
    std::fs::remove_dir_all(dir).unwrap();
}
