//! Secret buffers and passphrase sources for encrypted key files.
//!
//! A passphrase is never a command-line argument (it would show in the process
//! list and shell history). Sources, in order: `--passphrase-file`, the
//! `DYNODOC_KEY_PASSPHRASE_FILE` environment variable (a path, not the passphrase),
//! then an interactive prompt with echo off when standard input is a terminal.
//! Without any source a command fails with an explicit message.
//!
//! Zeroisation is best effort: a [`Secret`] overwrites its buffer when dropped,
//! but the operating system, the terminal and `ring` may keep their own copies
//! (for example `Ed25519KeyPair` has no wipe), and a swap file or core dump can
//! capture memory while a key is loaded.
use anyhow::{anyhow, ensure, Context};
use std::{
    io::{BufRead, IsTerminal, Read, Write},
    path::{Path, PathBuf},
    sync::Mutex,
};

/// Passphrase files and prompts are limited so a wrong file cannot be slurped.
pub const MAX_PASSPHRASE_BYTES: usize = 1024;
/// A new passphrase must be at least this many bytes (not characters).
pub const MIN_NEW_PASSPHRASE_BYTES: usize = 12;
pub const ENV_FILE: &str = "DYNODOC_KEY_PASSPHRASE_FILE";

/// Bytes that are overwritten with zeros when dropped. Deliberately not `Clone`
/// and with a `Debug` that prints nothing.
pub struct Secret(Vec<u8>);

impl Secret {
    pub fn new(bytes: Vec<u8>) -> Self {
        Self(bytes)
    }
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    /// Mutable access for in-place decryption.
    pub fn as_mut_vec(&mut self) -> &mut Vec<u8> {
        &mut self.0
    }
}

impl std::fmt::Debug for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Secret(<redacted>)")
    }
}

impl Drop for Secret {
    fn drop(&mut self) {
        wipe(&mut self.0);
    }
}

/// Overwrite with zeros using volatile writes, so the compiler cannot remove the
/// stores as dead. Covers the whole capacity, not only the live length.
pub fn wipe(buffer: &mut Vec<u8>) {
    let capacity = buffer.capacity();
    buffer.resize(capacity, 0);
    wipe_slice(buffer);
    buffer.clear();
}

pub fn wipe_slice(buffer: &mut [u8]) {
    for byte in buffer.iter_mut() {
        // SAFETY: `byte` is a valid, aligned, exclusive reference.
        unsafe { std::ptr::write_volatile(byte, 0) };
    }
    std::sync::atomic::compiler_fence(std::sync::atomic::Ordering::SeqCst);
}

static FILE_OVERRIDE: Mutex<Option<PathBuf>> = Mutex::new(None);

/// Record the process-wide `--passphrase-file` choice.
pub fn set_file(path: Option<PathBuf>) {
    *FILE_OVERRIDE.lock().unwrap_or_else(|e| e.into_inner()) = path;
}

/// Where a passphrase may come from for this call.
#[derive(Clone, Debug, Default)]
pub struct Sources {
    pub file: Option<PathBuf>,
    pub env_file: Option<PathBuf>,
    pub interactive: bool,
}

impl Sources {
    /// The real process: `--passphrase-file`, the environment, a terminal.
    pub fn current() -> Self {
        Self {
            file: FILE_OVERRIDE
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .clone(),
            env_file: std::env::var_os(ENV_FILE)
                .filter(|v| !v.is_empty())
                .map(PathBuf::from),
            interactive: std::io::stdin().is_terminal(),
        }
    }
}

/// Remove exactly one trailing line ending, nothing else. Leading and inner
/// whitespace belong to the passphrase.
fn strip_line_ending(mut bytes: Vec<u8>) -> Vec<u8> {
    if bytes.last() == Some(&b'\n') {
        bytes.pop();
        if bytes.last() == Some(&b'\r') {
            bytes.pop();
        }
    }
    bytes
}

fn from_file(path: &Path) -> anyhow::Result<Secret> {
    let file = std::fs::File::open(path)
        .with_context(|| format!("cannot open the passphrase file {}", path.display()))?;
    // Same rule as for key files: another local account must not read it.
    crate::private_file::check_key_file(&file, path)?;
    let mut bytes = Vec::with_capacity(MAX_PASSPHRASE_BYTES + 3);
    file.take(MAX_PASSPHRASE_BYTES as u64 + 3)
        .read_to_end(&mut bytes)?;
    let secret = Secret::new(strip_line_ending(bytes));
    ensure!(
        !secret.is_empty(),
        "the passphrase file {} is empty",
        path.display()
    );
    ensure!(
        secret.as_bytes().len() <= MAX_PASSPHRASE_BYTES,
        "the passphrase file {} is larger than {MAX_PASSPHRASE_BYTES} bytes",
        path.display()
    );
    Ok(secret)
}

/// Read one hidden line from the terminal.
fn prompt_line(label: &str) -> anyhow::Result<Secret> {
    let mut err = std::io::stderr();
    write!(err, "{label}")?;
    err.flush()?;
    let result = read_hidden_line();
    // The user's Enter was not echoed; finish the prompt line.
    let _ = writeln!(err);
    let mut line = result.context("cannot read the passphrase from the terminal")?;
    let bytes = strip_line_ending(std::mem::take(&mut line).into_bytes());
    ensure!(!bytes.is_empty(), "an empty passphrase was entered");
    Ok(Secret::new(bytes))
}

fn read_line_bounded() -> std::io::Result<String> {
    let mut line = String::with_capacity(MAX_PASSPHRASE_BYTES + 3);
    std::io::stdin()
        .lock()
        .take(MAX_PASSPHRASE_BYTES as u64 + 3)
        .read_line(&mut line)?;
    Ok(line)
}

#[cfg(windows)]
fn read_hidden_line() -> std::io::Result<String> {
    use windows_sys::Win32::System::Console::{
        GetConsoleMode, GetStdHandle, SetConsoleMode, ENABLE_ECHO_INPUT, STD_INPUT_HANDLE,
    };
    // SAFETY: plain Win32 console calls on this process's own input handle.
    unsafe {
        let handle = GetStdHandle(STD_INPUT_HANDLE);
        let mut mode = 0u32;
        if GetConsoleMode(handle, &mut mode) == 0 {
            return Err(std::io::Error::last_os_error());
        }
        if SetConsoleMode(handle, mode & !ENABLE_ECHO_INPUT) == 0 {
            return Err(std::io::Error::last_os_error());
        }
        let line = read_line_bounded();
        SetConsoleMode(handle, mode);
        line
    }
}

/// Unix: `stty` from a fixed system path (never found through `PATH`), on the
/// inherited terminal. Echo is restored from the saved settings afterwards. A
/// signal that kills the process mid-prompt leaves echo off; `stty sane` fixes it.
#[cfg(unix)]
fn read_hidden_line() -> std::io::Result<String> {
    use std::process::{Command, Stdio};
    let stty = ["/bin/stty", "/usr/bin/stty"]
        .into_iter()
        .find(|p| Path::new(p).is_file())
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "stty was not found"))?;
    let saved = Command::new(stty)
        .arg("-g")
        .stdin(Stdio::inherit())
        .output()?;
    if !saved.status.success() {
        return Err(std::io::Error::other("cannot read the terminal settings"));
    }
    let saved = String::from_utf8_lossy(&saved.stdout).trim().to_owned();
    let off = Command::new(stty)
        .arg("-echo")
        .stdin(Stdio::inherit())
        .status()?;
    if !off.success() {
        return Err(std::io::Error::other("cannot turn terminal echo off"));
    }
    let line = read_line_bounded();
    let _ = Command::new(stty)
        .arg(saved)
        .stdin(Stdio::inherit())
        .status();
    line
}

#[cfg(not(any(windows, unix)))]
fn read_hidden_line() -> std::io::Result<String> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "no hidden prompt on this platform",
    ))
}

const NO_SOURCE: &str = "no passphrase source: pass --passphrase-file FILE, set DYNODOC_KEY_PASSPHRASE_FILE to a file path, or run in a terminal to be prompted (a passphrase is never accepted as an argument)";

fn existing(sources: &Sources) -> anyhow::Result<Option<Secret>> {
    match sources.file.as_ref().or(sources.env_file.as_ref()) {
        Some(path) => from_file(path).map(Some),
        None => Ok(None),
    }
}

/// The passphrase of an existing encrypted key file.
pub fn unlock(sources: &Sources, label: &str) -> anyhow::Result<Secret> {
    if let Some(secret) = existing(sources)? {
        return Ok(secret);
    }
    ensure!(sources.interactive, NO_SOURCE);
    prompt_line(&format!("Passphrase for {label}: "))
}

/// A passphrase for a new or re-encrypted key. A prompt asks twice; a file is
/// taken as is. At least [`MIN_NEW_PASSPHRASE_BYTES`] bytes either way.
pub fn create(sources: &Sources, label: &str) -> anyhow::Result<Secret> {
    let secret = match existing(sources)? {
        Some(secret) => secret,
        None => {
            ensure!(sources.interactive, NO_SOURCE);
            let first = prompt_line(&format!("New passphrase for {label}: "))?;
            let again = prompt_line("Repeat the passphrase: ")?;
            ensure!(
                first.as_bytes() == again.as_bytes(),
                "the two passphrases differ; nothing was written"
            );
            first
        }
    };
    check_new(&secret)?;
    Ok(secret)
}

fn check_new(secret: &Secret) -> anyhow::Result<()> {
    if secret.as_bytes().len() < MIN_NEW_PASSPHRASE_BYTES {
        return Err(anyhow!(
            "a new passphrase must be at least {MIN_NEW_PASSPHRASE_BYTES} bytes; use several random words"
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("dynodoc-pass-{}", Uuid::new_v4()));
        std::fs::create_dir(&dir).unwrap();
        dir
    }

    #[test]
    fn secret_buffers_are_wiped_and_never_printed() {
        let mut bytes = Vec::with_capacity(64);
        bytes.extend_from_slice(b"hunter2 hunter2");
        wipe(&mut bytes);
        assert!(bytes.is_empty());
        let mut array = [7u8; 16];
        wipe_slice(&mut array);
        assert_eq!(array, [0u8; 16]);
        let secret = Secret::new(b"correct horse battery".to_vec());
        assert_eq!(format!("{secret:?}"), "Secret(<redacted>)");
    }

    #[test]
    fn one_trailing_line_ending_is_removed_and_nothing_else() {
        assert_eq!(strip_line_ending(b"abc\n".to_vec()), b"abc");
        assert_eq!(strip_line_ending(b"abc\r\n".to_vec()), b"abc");
        assert_eq!(strip_line_ending(b"abc\n\n".to_vec()), b"abc\n");
        assert_eq!(strip_line_ending(b" a b \n".to_vec()), b" a b ");
        assert_eq!(strip_line_ending(b"abc".to_vec()), b"abc");
    }

    #[test]
    fn sources_are_taken_in_order_and_a_missing_source_is_an_error() {
        let dir = dir();
        let first = dir.join("first");
        let second = dir.join("second");
        crate::private_file::write_private_test(&first, "first passphrase value\n").unwrap();
        crate::private_file::write_private_test(&second, "second passphrase value").unwrap();
        let both = Sources {
            file: Some(first.clone()),
            env_file: Some(second.clone()),
            interactive: false,
        };
        assert_eq!(
            unlock(&both, "k").unwrap().as_bytes(),
            b"first passphrase value"
        );
        let env_only = Sources {
            file: None,
            env_file: Some(second),
            interactive: false,
        };
        assert_eq!(
            unlock(&env_only, "k").unwrap().as_bytes(),
            b"second passphrase value"
        );
        let none = Sources::default();
        let error = unlock(&none, "k").unwrap_err().to_string();
        assert!(error.contains("--passphrase-file"), "{error}");
        assert!(error.contains(ENV_FILE), "{error}");
        assert!(create(&none, "k").is_err());
        // A named but missing file is an error, never a silent fall through to a prompt.
        let missing = Sources {
            file: Some(dir.join("absent")),
            env_file: Some(first),
            interactive: true,
        };
        assert!(unlock(&missing, "k").is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn passphrase_files_are_bounded_and_new_passphrases_have_a_minimum() {
        let dir = dir();
        let empty = dir.join("empty");
        crate::private_file::write_private_test(&empty, "\n").unwrap();
        let huge = dir.join("huge");
        crate::private_file::write_private_test(&huge, vec![b'a'; MAX_PASSPHRASE_BYTES + 1])
            .unwrap();
        let short = dir.join("short");
        crate::private_file::write_private_test(&short, "too short").unwrap();
        let exact = dir.join("exact");
        crate::private_file::write_private_test(&exact, vec![b'b'; MAX_PASSPHRASE_BYTES]).unwrap();
        let with = |p: &Path| Sources {
            file: Some(p.to_owned()),
            ..Default::default()
        };
        assert!(unlock(&with(&empty), "k").is_err());
        assert!(unlock(&with(&huge), "k").is_err());
        assert!(unlock(&with(&exact), "k").is_ok());
        // An existing key may have a short passphrase; a new one may not.
        assert!(unlock(&with(&short), "k").is_ok());
        let error = create(&with(&short), "k").unwrap_err().to_string();
        assert!(error.contains("at least"), "{error}");
        std::fs::remove_dir_all(dir).unwrap();
    }
}
