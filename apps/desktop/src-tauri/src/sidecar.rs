//! The bundled `engine-cli`, run for the launcher with a FIXED set of arguments.
//!
//! The page never supplies an argument. Each operation below has exactly one argument
//! list, built here from the key file path the app itself chose; `run` refuses any list
//! `allowed` does not recognise, so a bug elsewhere cannot widen what the program is asked
//! to do. Key creation, storage and encryption stay in `engine-cli` (see
//! docs/KEY-FILE-FORMAT.md); nothing here reads or writes a key.

use std::ffi::OsString;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// The sidecar's file name next to the app executable (Tauri strips the target triple).
pub const PROGRAM: &str = if cfg!(windows) {
    "engine-cli.exe"
} else {
    "engine-cli"
};
pub const KEY_FILE_NAME: &str = "device-key.json";

/// `--version` and `device public-key` answer at once.
pub const QUICK_TIMEOUT: Duration = Duration::from_secs(20);
/// Creating a key may wait for the operating system's key store (a Keychain or Secret
/// Service prompt is answered by a person).
pub const KEY_STORE_TIMEOUT: Duration = Duration::from_secs(90);
/// Every answer is one line; 16 KiB is far more than needed.
pub const MAX_OUTPUT: usize = 16 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operation {
    Version,
    CreateKey,
    PublicKey,
}

impl Operation {
    pub fn timeout(self) -> Duration {
        match self {
            Self::CreateKey => KEY_STORE_TIMEOUT,
            _ => QUICK_TIMEOUT,
        }
    }
}

/// The only path the app ever passes to the program: `<app local data dir>/device-key.json`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyPath(PathBuf);

impl KeyPath {
    pub fn in_dir(dir: &Path) -> Result<Self, SidecarError> {
        if !dir.is_absolute() || dir.components().any(|c| c.as_os_str() == "..") {
            return Err(SidecarError::BadKeyPath);
        }
        Ok(Self(dir.join(KEY_FILE_NAME)))
    }

    pub fn as_path(&self) -> &Path {
        &self.0
    }
}

/// The complete argument list for an operation.
pub fn arguments(op: Operation, key: &KeyPath) -> Vec<OsString> {
    let key = key.0.as_os_str().to_owned();
    match op {
        Operation::Version => vec!["--version".into()],
        Operation::CreateKey => vec![
            "device".into(),
            "keygen".into(),
            "--out".into(),
            key,
            "--encrypt".into(),
            "--keystore".into(),
        ],
        Operation::PublicKey => vec!["device".into(), "public-key".into(), key],
    }
}

/// True only for an argument list `arguments` would build for this key.
pub fn allowed(args: &[OsString], key: &KeyPath) -> bool {
    [
        Operation::Version,
        Operation::CreateKey,
        Operation::PublicKey,
    ]
    .into_iter()
    .any(|op| arguments(op, key) == args)
}

#[derive(Debug, PartialEq, Eq)]
pub enum SidecarError {
    BadKeyPath,
    NotAllowed,
    Missing(PathBuf),
    Start(String),
    TimedOut(Duration),
    TooMuchOutput,
    Failed { code: Option<i32>, message: String },
    Unexpected(&'static str),
}

impl std::fmt::Display for SidecarError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BadKeyPath => f.write_str("The key location is not valid."),
            Self::NotAllowed => f.write_str("That tool operation is not allowed."),
            Self::Missing(_) => f.write_str(
                "The bundled tools (engine-cli) are missing from this installation. When building from source, run scripts/prepare-sidecar.mjs first.",
            ),
            Self::Start(e) => write!(f, "The bundled tools could not be started: {e}"),
            Self::TimedOut(d) => write!(
                f,
                "The tools did not answer within {} seconds and were stopped.",
                d.as_secs()
            ),
            Self::TooMuchOutput => f.write_str("The tools printed more than expected and were stopped."),
            Self::Failed { message, .. } if message.is_empty() => {
                f.write_str("The tools reported an error without a message.")
            }
            Self::Failed { message, .. } => write!(f, "The tools reported: {message}"),
            Self::Unexpected(what) => write!(f, "The tools answered in an unexpected way ({what})."),
        }
    }
}

#[derive(Debug)]
pub struct Output {
    pub stdout: String,
    pub stderr: String,
}

/// Where the sidecar sits: in the same folder as the running executable.
pub fn program_path() -> Result<PathBuf, SidecarError> {
    let exe = std::env::current_exe().map_err(|e| SidecarError::Start(e.to_string()))?;
    let dir = exe
        .parent()
        .ok_or_else(|| SidecarError::Start("no program folder".into()))?;
    Ok(dir.join(PROGRAM))
}

/// Run one allowed operation. Blocking; call from a worker thread.
pub fn run(op: Operation, key: &KeyPath) -> Result<Output, SidecarError> {
    run_program(&program_path()?, op, key)
}

/// `run` with the program's location given (tests use the copy in `binaries/`).
fn run_program(program: &Path, op: Operation, key: &KeyPath) -> Result<Output, SidecarError> {
    if !program.is_file() {
        return Err(SidecarError::Missing(program.to_path_buf()));
    }
    let args = arguments(op, key);
    run_checked(program, &args, key, op.timeout(), MAX_OUTPUT)
}

fn run_checked(
    program: &Path,
    args: &[OsString],
    key: &KeyPath,
    timeout: Duration,
    max_output: usize,
) -> Result<Output, SidecarError> {
    if !allowed(args, key) {
        return Err(SidecarError::NotAllowed);
    }
    run_bounded(program, args, &[], timeout, max_output)
}

/// Run `program` with a time limit and an output limit per stream. Standard input is
/// closed, so nothing can wait for a typed answer. Logging is switched off so the only
/// thing on standard output is the answer. `extra_env` is for tests; production passes none.
fn run_bounded(
    program: &Path,
    args: &[OsString],
    extra_env: &[(&str, &str)],
    timeout: Duration,
    max_output: usize,
) -> Result<Output, SidecarError> {
    let mut command = Command::new(program);
    command
        .args(args)
        .envs(extra_env.iter().copied())
        .env("RUST_LOG", "off")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let mut child = command
        .spawn()
        .map_err(|e| SidecarError::Start(e.kind().to_string()))?;
    let over = Arc::new(AtomicBool::new(false));
    let out = capped(child.stdout.take(), max_output, over.clone());
    let err = capped(child.stderr.take(), max_output, over.clone());

    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {}
            Err(e) => {
                let _ = child.kill();
                return Err(SidecarError::Start(e.to_string()));
            }
        }
        if over.load(Ordering::SeqCst) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(SidecarError::TooMuchOutput);
        }
        if started.elapsed() >= timeout {
            let _ = child.kill();
            let _ = child.wait();
            return Err(SidecarError::TimedOut(timeout));
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    let stdout = out.join().unwrap_or_default();
    let stderr = err.join().unwrap_or_default();
    if over.load(Ordering::SeqCst) {
        return Err(SidecarError::TooMuchOutput);
    }
    let output = Output {
        stdout: String::from_utf8_lossy(&stdout).into_owned(),
        stderr: String::from_utf8_lossy(&stderr).into_owned(),
    };
    if status.success() {
        Ok(output)
    } else {
        Err(SidecarError::Failed {
            code: status.code(),
            message: tidy_message(&output.stderr),
        })
    }
}

/// Read at most `cap` bytes; one more byte than that sets `over`.
fn capped<R: Read + Send + 'static>(
    stream: Option<R>,
    cap: usize,
    over: Arc<AtomicBool>,
) -> std::thread::JoinHandle<Vec<u8>> {
    std::thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(stream) = stream {
            let _ = stream.take(cap as u64 + 1).read_to_end(&mut buf);
        }
        if buf.len() > cap {
            over.store(true, Ordering::SeqCst);
            buf.truncate(cap);
        }
        buf
    })
}

/// A message safe to show: no control characters, at most 400 characters, first lines only.
pub fn tidy_message(stderr: &str) -> String {
    let joined = stderr
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .take(3)
        .collect::<Vec<_>>()
        .join(" ");
    let clean: String = joined.chars().filter(|c| !c.is_control()).collect();
    if clean.chars().count() > 400 {
        let cut: String = clean.chars().take(400).collect();
        format!("{cut}...")
    } else {
        clean
    }
}

/// `engine-cli --version` prints `engine-cli <semver>`.
pub fn parse_version(stdout: &str) -> Result<String, SidecarError> {
    let line = stdout.trim();
    let ok = line.len() <= 80
        && !line.contains('\n')
        && line.strip_prefix("engine-cli ").is_some_and(|v| {
            !v.is_empty()
                && v.chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '+'))
        });
    if ok {
        Ok(line.to_string())
    } else {
        Err(SidecarError::Unexpected("version"))
    }
}

/// `device keygen` and `device public-key` print the Ed25519 public key: 64 lowercase
/// hexadecimal characters on one line.
pub fn parse_public_key(stdout: &str) -> Result<String, SidecarError> {
    let line = stdout.trim();
    if line.len() == 64 && line.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')) {
        Ok(line.to_string())
    } else {
        Err(SidecarError::Unexpected("public key"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir() -> PathBuf {
        if cfg!(windows) {
            PathBuf::from(r"C:\Users\someone\AppData\Local\online.dynodoc.client")
        } else {
            PathBuf::from("/home/someone/.local/share/online.dynodoc.client")
        }
    }
    fn key() -> KeyPath {
        KeyPath::in_dir(&dir()).unwrap()
    }
    fn os(items: &[&str]) -> Vec<OsString> {
        items.iter().map(OsString::from).collect()
    }

    #[test]
    fn each_operation_has_exactly_its_fixed_arguments() {
        let k = key();
        let path = k.as_path().as_os_str().to_owned();
        assert_eq!(arguments(Operation::Version, &k), os(&["--version"]));
        assert_eq!(
            arguments(Operation::CreateKey, &k),
            vec![
                OsString::from("device"),
                "keygen".into(),
                "--out".into(),
                path.clone(),
                "--encrypt".into(),
                "--keystore".into()
            ]
        );
        assert_eq!(
            arguments(Operation::PublicKey, &k),
            vec![OsString::from("device"), "public-key".into(), path]
        );
        assert!(k.as_path().ends_with(KEY_FILE_NAME));
        assert!(k.as_path().starts_with(dir()));
    }

    #[test]
    fn the_allowlist_accepts_only_the_built_lists() {
        let k = key();
        for op in [
            Operation::Version,
            Operation::CreateKey,
            Operation::PublicKey,
        ] {
            assert!(allowed(&arguments(op, &k), &k));
        }
    }

    #[test]
    fn everything_else_is_refused() {
        let k = key();
        let path = k.as_path().to_string_lossy().into_owned();
        let other = if cfg!(windows) {
            r"C:\Windows\win.ini"
        } else {
            "/etc/passwd"
        };
        let with_path = |items: &[&str]| -> Vec<OsString> {
            items
                .iter()
                .map(|s| OsString::from(s.replace("KEY", &path)))
                .collect()
        };
        let hostile: Vec<Vec<OsString>> = vec![
            os(&[]),
            os(&["--help"]),
            os(&["--version", "extra"]),
            os(&["-V"]),
            os(&["mcp"]),
            os(&["sync"]),
            os(&["migrate"]),
            os(&["verify"]),
            os(&["device"]),
            os(&["device", "prove"]),
            os(&["device", "unprotect"]),
            os(&[
                "device",
                "keygen",
                "--out",
                other,
                "--encrypt",
                "--keystore",
            ]),
            os(&["device", "public-key", other]),
            os(&["device", "public-key", "--passphrase-file", other]),
            with_path(&["device", "keygen", "--out", "KEY"]),
            with_path(&["device", "keygen", "--out", "KEY", "--encrypt"]),
            with_path(&[
                "device",
                "keygen",
                "--out",
                "KEY",
                "--keystore",
                "--encrypt",
            ]),
            with_path(&[
                "device",
                "keygen",
                "--out",
                "KEY",
                "--encrypt",
                "--keystore",
                "--allow-insecure-key-file",
            ]),
            with_path(&[
                "--allow-insecure-key-file",
                "device",
                "keygen",
                "--out",
                "KEY",
                "--encrypt",
                "--keystore",
            ]),
            with_path(&["--passphrase-file", other, "device", "public-key", "KEY"]),
            with_path(&["device", "public-key", "KEY", "KEY"]),
            with_path(&["device", "public-key", "KEY", "--passphrase-file", other]),
            with_path(&["device", "public-key", "--", "KEY"]),
            with_path(&["DEVICE", "public-key", "KEY"]),
        ];
        for args in hostile {
            assert!(!allowed(&args, &k), "{args:?}");
        }
        // A list built for another key is not valid for this one.
        let elsewhere = KeyPath::in_dir(&if cfg!(windows) {
            PathBuf::from(r"C:\other")
        } else {
            PathBuf::from("/other")
        })
        .unwrap();
        assert!(!allowed(&arguments(Operation::PublicKey, &elsewhere), &k));
    }

    #[test]
    fn the_key_location_must_be_an_absolute_folder_without_dots() {
        assert_eq!(
            KeyPath::in_dir(Path::new("relative/dir")),
            Err(SidecarError::BadKeyPath)
        );
        assert_eq!(
            KeyPath::in_dir(Path::new("")),
            Err(SidecarError::BadKeyPath)
        );
        assert_eq!(
            KeyPath::in_dir(Path::new("-rf")),
            Err(SidecarError::BadKeyPath)
        );
        let traversal = dir().join("..").join("x");
        assert_eq!(KeyPath::in_dir(&traversal), Err(SidecarError::BadKeyPath));
    }

    #[test]
    fn run_checked_refuses_a_list_that_is_not_allowed_without_starting_anything() {
        let k = key();
        let result = run_checked(
            Path::new("this-program-does-not-exist"),
            &os(&["mcp"]),
            &k,
            Duration::from_secs(1),
            100,
        );
        assert_eq!(result.unwrap_err(), SidecarError::NotAllowed);
    }

    #[test]
    fn the_version_line_is_checked() {
        assert_eq!(
            parse_version("engine-cli 0.1.0\n").unwrap(),
            "engine-cli 0.1.0"
        );
        assert_eq!(
            parse_version("engine-cli 1.2.3-rc.1+build5").unwrap(),
            "engine-cli 1.2.3-rc.1+build5"
        );
        for bad in [
            "",
            "0.1.0",
            "other 0.1.0",
            "engine-cli ",
            "engine-cli 0.1.0\nsecond line",
            "engine-cli 0.1.0; rm -rf /",
            "engine-cli <script>",
            &format!("engine-cli {}", "9".repeat(200)),
        ] {
            assert!(parse_version(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn the_public_key_line_is_checked() {
        let good = "ea4a6c63e29c520abef5507b132ec5f9954776aebebe7b92421eea691446d22c";
        assert_eq!(parse_public_key(good).unwrap(), good);
        assert_eq!(parse_public_key(&format!("{good}\r\n")).unwrap(), good);
        for bad in [
            "",
            "ea4a6c63",
            &good.to_uppercase(),
            &format!("{good}0"),
            &format!("{good}\n{good}"),
            &format!("INFO started\n{good}"),
            "zz4a6c63e29c520abef5507b132ec5f9954776aebebe7b92421eea691446d22c",
            "-----BEGIN PRIVATE KEY-----",
        ] {
            assert!(parse_public_key(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn error_messages_are_tidy_and_bounded() {
        assert_eq!(tidy_message(""), "");
        assert_eq!(
            tidy_message("Error: no secret-tool\n\ncaused by x\n"),
            "Error: no secret-tool caused by x"
        );
        assert_eq!(tidy_message("a\u{1b}[31mred\u{7}"), "a[31mred");
        assert_eq!(tidy_message("1\n2\n3\n4\n5"), "1 2 3");
        let long = tidy_message(&"x".repeat(5000));
        assert_eq!(long.chars().count(), 403);
        assert!(long.ends_with("..."));
    }

    // The helper tests below start this very test program as a child, so they work on
    // every platform without shell syntax. Run directly they do nothing.
    #[test]
    fn helper_child() {
        match std::env::var("DYNODOC_SIDECAR_TEST_HELPER").as_deref() {
            Ok("say") => println!("hello from the child"),
            Ok("flood") => {
                let block = "x".repeat(1024);
                for _ in 0..4096 {
                    println!("{block}");
                }
            }
            Ok("flood_err") => {
                let block = "y".repeat(1024);
                for _ in 0..4096 {
                    eprintln!("{block}");
                }
            }
            Ok("sleep") => std::thread::sleep(Duration::from_secs(60)),
            Ok("fail") => {
                eprintln!("bad thing\u{7} happened");
                std::process::exit(3);
            }
            Ok("stdin") => {
                let mut s = String::new();
                let n = std::io::stdin().read_line(&mut s).unwrap_or(99);
                println!("stdin read {n}");
            }
            Ok("env") => println!("RUST_LOG={}", std::env::var("RUST_LOG").unwrap_or_default()),
            _ => {}
        }
    }

    fn run_helper(mode: &str, timeout: Duration, cap: usize) -> Result<Output, SidecarError> {
        let exe = std::env::current_exe().unwrap();
        // The mode reaches only the child's environment, never this process's.
        run_bounded(
            &exe,
            &os(&[
                "--exact",
                "sidecar::tests::helper_child",
                "--nocapture",
                "--test-threads=1",
            ]),
            &[("DYNODOC_SIDECAR_TEST_HELPER", mode)],
            timeout,
            cap,
        )
    }

    #[test]
    fn bounded_runner_limits_time_output_and_input() {
        let ok = run_helper("say", Duration::from_secs(60), MAX_OUTPUT).unwrap();
        assert!(ok.stdout.contains("hello from the child"), "{}", ok.stdout);

        assert_eq!(
            run_helper("flood", Duration::from_secs(60), 2048).unwrap_err(),
            SidecarError::TooMuchOutput
        );
        assert_eq!(
            run_helper("flood_err", Duration::from_secs(60), 2048).unwrap_err(),
            SidecarError::TooMuchOutput
        );

        let started = Instant::now();
        let slow = run_helper("sleep", Duration::from_millis(600), MAX_OUTPUT).unwrap_err();
        assert_eq!(slow, SidecarError::TimedOut(Duration::from_millis(600)));
        assert!(started.elapsed() < Duration::from_secs(20));

        match run_helper("fail", Duration::from_secs(60), MAX_OUTPUT).unwrap_err() {
            SidecarError::Failed { code, message } => {
                assert_eq!(code, Some(3));
                assert!(message.contains("bad thing"), "{message}");
                assert!(!message.chars().any(|c| c.is_control()));
            }
            other => panic!("{other:?}"),
        }

        // Standard input is closed: a read returns 0 bytes at once instead of waiting.
        let stdin = run_helper("stdin", Duration::from_secs(60), MAX_OUTPUT).unwrap();
        assert!(stdin.stdout.contains("stdin read 0"), "{}", stdin.stdout);

        // Logging is switched off for the child.
        let env = run_helper("env", Duration::from_secs(60), MAX_OUTPUT).unwrap();
        assert!(env.stdout.contains("RUST_LOG=off"), "{}", env.stdout);

        assert!(matches!(
            run_bounded(
                Path::new("this-program-does-not-exist"),
                &[],
                &[],
                Duration::from_secs(1),
                10
            ),
            Err(SidecarError::Start(_))
        ));
    }

    /// The copy of engine-cli that tauri-build requires in `binaries/` (it refuses to
    /// compile without it), so these tests run the real program through the real code path.
    fn bundled_program() -> PathBuf {
        let suffix = if cfg!(windows) { ".exe" } else { "" };
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("binaries")
            .join(format!("engine-cli-{}{suffix}", env!("DYNODOC_TARGET")))
    }

    #[test]
    fn the_real_engine_cli_reports_its_version() {
        let out = run_program(&bundled_program(), Operation::Version, &key())
            .expect("engine-cli --version");
        let version = parse_version(&out.stdout).expect("a version line");
        assert!(version.starts_with("engine-cli "), "{version}");
    }

    // Key creation uses the operating system's key store, so it runs only where that is
    // Windows data protection (no prompt; the secret lives inside the file). On macOS and
    // Linux it would touch the user's Keychain or Secret Service.
    #[cfg(windows)]
    #[test]
    fn the_real_engine_cli_creates_and_reads_back_a_device_key() {
        let program = bundled_program();
        let dir = std::env::temp_dir().join(format!(
            "dynodoc-desktop-key-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let key = KeyPath::in_dir(&dir).unwrap();

        let created = run_program(&program, Operation::CreateKey, &key).expect("keygen");
        let public = parse_public_key(&created.stdout).expect("public key on stdout");
        assert!(key.as_path().is_file());
        let file = std::fs::read_to_string(key.as_path()).unwrap();
        assert!(
            file.contains("dynodoc-key-v1") && file.contains("windows-dpapi"),
            "{file}"
        );

        let again = run_program(&program, Operation::PublicKey, &key).expect("public-key");
        assert_eq!(parse_public_key(&again.stdout).unwrap(), public);

        // Never overwrites: a second creation fails and leaves the file as it was.
        let before = std::fs::read(key.as_path()).unwrap();
        match run_program(&program, Operation::CreateKey, &key) {
            Err(SidecarError::Failed { message, .. }) => {
                assert!(message.contains("overwritten"), "{message}")
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(std::fs::read(key.as_path()).unwrap(), before);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
