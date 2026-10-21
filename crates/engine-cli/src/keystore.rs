//! Optional operating-system keystore binding for an encrypted key file.
//!
//! With `--keystore` the file is encrypted under a random 256-bit wrapping secret
//! that the CLI generates, and no passphrase exists. The secret is held by the
//! operating system, never written beside the key in the clear:
//!
//! - Windows: DPAPI (`CryptProtectData`, current user, UI forbidden). The
//!   protected blob is stored inside the key file's `keystore` object, so a copy of
//!   the file on another account or machine cannot open it.
//! - macOS: the login keychain through the system `security` tool (generic
//!   password, service `dynodoc-key-v1`, account = the public key). The secret
//!   goes to `security -i` on standard input, not on its command line.
//! - Linux: Secret Service through `secret-tool` when it is installed; otherwise
//!   the option is refused with a pointer to the passphrase mode.
//!
//! This binds the key to an OS account. It does not defend against code running as
//! that same account (DPAPI and an unlocked keychain answer to it), and it is not a
//! hardware token. All three providers are run for real by `tests/native_keystore.rs`
//! (a temporary keychain on macOS, a private gnome-keyring on Linux, DPAPI on
//! Windows; see `docs/PLATFORM-SUPPORT.md`); the unit tests below cover command
//! construction and the container logic on every platform.
use crate::passphrase::{wipe_slice, Secret};
use anyhow::{anyhow, bail, ensure, Context};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use ring::rand::{SecureRandom, SystemRandom};

pub const SERVICE: &str = "dynodoc-key-v1";
/// Decoded DPAPI blob bound, well above the real size (a few hundred bytes).
pub const MAX_BLOB_BYTES: usize = 2048;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Provider {
    WindowsDpapi,
    MacosKeychain,
    LinuxSecretService,
    /// In-process stand-in so the container binding is tested on every platform.
    #[cfg(test)]
    Memory,
}

impl Provider {
    pub fn name(self) -> &'static str {
        match self {
            Self::WindowsDpapi => "windows-dpapi",
            Self::MacosKeychain => "macos-keychain",
            Self::LinuxSecretService => "linux-secret-service",
            #[cfg(test)]
            Self::Memory => "test-memory",
        }
    }
    pub fn parse(name: &str) -> Option<Self> {
        Some(match name {
            "windows-dpapi" => Self::WindowsDpapi,
            "macos-keychain" => Self::MacosKeychain,
            "linux-secret-service" => Self::LinuxSecretService,
            #[cfg(test)]
            "test-memory" => Self::Memory,
            _ => return None,
        })
    }
    /// DPAPI returns an opaque blob that the key file must carry; the others keep
    /// the secret in the external store and the file only names the provider.
    pub fn blob_in_file(self) -> bool {
        matches!(self, Self::WindowsDpapi)
    }
    /// The provider of the platform this binary runs on.
    pub fn native() -> anyhow::Result<Self> {
        if cfg!(windows) {
            Ok(Self::WindowsDpapi)
        } else if cfg!(target_os = "macos") {
            Ok(Self::MacosKeychain)
        } else if cfg!(target_os = "linux") {
            Ok(Self::LinuxSecretService)
        } else {
            bail!("no operating-system keystore is supported on this platform; use an encrypted file with a passphrase (omit --keystore)")
        }
    }
}

/// A fresh wrapping secret: 32 random bytes as 64 lowercase hex characters.
pub fn random_secret() -> anyhow::Result<Secret> {
    let mut raw = [0u8; 32];
    SystemRandom::new()
        .fill(&mut raw)
        .map_err(|_| anyhow!("system random generator failed"))?;
    let hex: Vec<u8> = raw
        .iter()
        .flat_map(|b| format!("{b:02x}").into_bytes())
        .collect();
    wipe_slice(&mut raw);
    Ok(Secret::new(hex))
}

fn valid_secret(bytes: &[u8]) -> bool {
    bytes.len() == 64 && bytes.iter().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// One external command, with the secret (if any) only on standard input.
#[derive(Debug, PartialEq, Eq)]
pub struct Command {
    pub program: String,
    pub args: Vec<String>,
    pub stdin: Option<Vec<u8>>,
}

pub fn macos_store(account: &str, secret: &[u8]) -> Command {
    // `security -i` reads its commands from standard input, so the secret never
    // appears in the process arguments. Account and secret are hex: no quoting.
    let mut line = format!(
        "add-generic-password -a \"{account}\" -s \"{SERVICE}\" -l \"Dynodoc signing key {}\" -U -w \"",
        &account[..account.len().min(8)]
    )
    .into_bytes();
    line.extend_from_slice(secret);
    line.extend_from_slice(b"\"\n");
    Command {
        program: "/usr/bin/security".into(),
        args: vec!["-i".into()],
        stdin: Some(line),
    }
}
pub fn macos_load(account: &str) -> Command {
    Command {
        program: "/usr/bin/security".into(),
        args: ["find-generic-password", "-a", account, "-s", SERVICE, "-w"]
            .map(String::from)
            .into(),
        stdin: None,
    }
}
pub fn macos_forget(account: &str) -> Command {
    Command {
        program: "/usr/bin/security".into(),
        args: ["delete-generic-password", "-a", account, "-s", SERVICE]
            .map(String::from)
            .into(),
        stdin: None,
    }
}
pub fn secret_tool_store(account: &str, secret: &[u8]) -> Command {
    Command {
        program: "secret-tool".into(),
        args: vec![
            "store".into(),
            format!(
                "--label=Dynodoc signing key {}",
                &account[..account.len().min(8)]
            ),
            "service".into(),
            SERVICE.into(),
            "account".into(),
            account.into(),
        ],
        stdin: Some(secret.to_vec()),
    }
}
pub fn secret_tool_load(account: &str) -> Command {
    Command {
        program: "secret-tool".into(),
        args: ["lookup", "service", SERVICE, "account", account]
            .map(String::from)
            .into(),
        stdin: None,
    }
}
pub fn secret_tool_forget(account: &str) -> Command {
    Command {
        program: "secret-tool".into(),
        args: ["clear", "service", SERVICE, "account", account]
            .map(String::from)
            .into(),
        stdin: None,
    }
}

/// Run a command; its output is returned only as a [`Secret`] so it is wiped.
/// Errors carry the exit status and never the command's input or output.
fn run(command: &Command, what: &str) -> anyhow::Result<Secret> {
    use std::io::Write;
    use std::process::{Command as Process, Stdio};
    let mut child = Process::new(&command.program)
        .args(&command.args)
        .stdin(if command.stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                anyhow!("{} is not installed; the OS keystore is unavailable here. Use an encrypted file with a passphrase instead (omit --keystore)", command.program)
            } else {
                anyhow!("cannot start {}: {}", command.program, error.kind())
            }
        })?;
    if let (Some(input), Some(mut pipe)) = (&command.stdin, child.stdin.take()) {
        pipe.write_all(input)
            .with_context(|| format!("cannot send data to {}", command.program))?;
    }
    let output = child.wait_with_output()?;
    let out = Secret::new(output.stdout);
    ensure!(
        output.status.success(),
        "the OS keystore refused to {what} (exit status {:?})",
        output.status.code()
    );
    Ok(out)
}

fn trim_line(secret: Secret) -> Secret {
    let mut bytes = secret.as_bytes().to_vec();
    while matches!(bytes.last(), Some(b'\n' | b'\r')) {
        bytes.pop();
    }
    Secret::new(bytes)
}

#[cfg(test)]
static MEMORY: std::sync::Mutex<Vec<(String, Vec<u8>)>> = std::sync::Mutex::new(Vec::new());

/// Store `secret` for the key `account` (its public key, hex). Returns the opaque
/// blob that the key file must carry, for providers that need one.
pub fn store(
    provider: Provider,
    account: &str,
    secret: &Secret,
) -> anyhow::Result<Option<Vec<u8>>> {
    ensure!(valid_secret(secret.as_bytes()), "invalid wrapping secret");
    match provider {
        Provider::WindowsDpapi => {
            let blob = dpapi_protect(secret.as_bytes(), &entropy(account))?;
            ensure!(
                blob.len() <= MAX_BLOB_BYTES,
                "DPAPI blob is unexpectedly large"
            );
            Ok(Some(blob))
        }
        Provider::MacosKeychain => {
            run(
                &macos_store(account, secret.as_bytes()),
                "store the key secret",
            )?;
            Ok(None)
        }
        Provider::LinuxSecretService => {
            run(
                &secret_tool_store(account, secret.as_bytes()),
                "store the key secret",
            )?;
            Ok(None)
        }
        #[cfg(test)]
        Provider::Memory => {
            let mut store = MEMORY.lock().unwrap_or_else(|e| e.into_inner());
            store.retain(|(a, _)| a != account);
            store.push((account.to_owned(), secret.as_bytes().to_vec()));
            Ok(None)
        }
    }
}

/// Retrieve the wrapping secret for `account`.
pub fn load(provider: Provider, account: &str, blob: Option<&[u8]>) -> anyhow::Result<Secret> {
    let secret = match provider {
        Provider::WindowsDpapi => {
            let blob = blob.ok_or_else(|| anyhow!("the key file has no DPAPI blob"))?;
            dpapi_unprotect(blob, &entropy(account)).context(
                "this key file is bound to Windows DPAPI and can only be opened by the Windows account that created it",
            )?
        }
        Provider::MacosKeychain => trim_line(run(&macos_load(account), "read the key secret")?),
        Provider::LinuxSecretService => {
            trim_line(run(&secret_tool_load(account), "read the key secret")?)
        }
        #[cfg(test)]
        Provider::Memory => {
            let store = MEMORY.lock().unwrap_or_else(|e| e.into_inner());
            let found = store
                .iter()
                .find(|(a, _)| a == account)
                .ok_or_else(|| anyhow!("the OS keystore has no secret for this key"))?;
            Secret::new(found.1.clone())
        }
    };
    ensure!(
        valid_secret(secret.as_bytes()),
        "the OS keystore returned an unexpected value for this key"
    );
    Ok(secret)
}

/// Remove the stored secret (best effort; DPAPI keeps nothing outside the file).
pub fn forget(provider: Provider, account: &str) -> anyhow::Result<()> {
    match provider {
        Provider::WindowsDpapi => Ok(()),
        Provider::MacosKeychain => run(&macos_forget(account), "remove the key secret").map(drop),
        Provider::LinuxSecretService => {
            run(&secret_tool_forget(account), "remove the key secret").map(drop)
        }
        #[cfg(test)]
        Provider::Memory => {
            MEMORY
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .retain(|(a, _)| a != account);
            Ok(())
        }
    }
}

/// DPAPI optional entropy: ties a blob to one key so blobs cannot be swapped
/// between key files of the same account.
fn entropy(account: &str) -> Vec<u8> {
    let mut bytes = SERVICE.as_bytes().to_vec();
    bytes.push(0);
    bytes.extend_from_slice(account.as_bytes());
    bytes
}

pub fn encode_blob(blob: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(blob)
}

#[cfg(windows)]
fn dpapi_protect(data: &[u8], entropy: &[u8]) -> anyhow::Result<Vec<u8>> {
    use windows_sys::Win32::{
        Foundation::LocalFree,
        Security::Cryptography::{CryptProtectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB},
    };
    let input = CRYPT_INTEGER_BLOB {
        cbData: u32::try_from(data.len())?,
        pbData: data.as_ptr().cast_mut(),
    };
    let extra = CRYPT_INTEGER_BLOB {
        cbData: u32::try_from(entropy.len())?,
        pbData: entropy.as_ptr().cast_mut(),
    };
    let mut output = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: std::ptr::null_mut(),
    };
    // SAFETY: the blobs point at live slices for the call; the output buffer is
    // allocated by the system and released with LocalFree below.
    unsafe {
        let ok = CryptProtectData(
            &input,
            std::ptr::null(),
            &extra,
            std::ptr::null(),
            std::ptr::null(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        );
        if ok == 0 {
            bail!(
                "Windows DPAPI could not protect the key secret: {}",
                std::io::Error::last_os_error()
            );
        }
        let blob = std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec();
        LocalFree(output.pbData.cast());
        Ok(blob)
    }
}

#[cfg(windows)]
fn dpapi_unprotect(blob: &[u8], entropy: &[u8]) -> anyhow::Result<Secret> {
    use windows_sys::Win32::{
        Foundation::LocalFree,
        Security::Cryptography::{
            CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
        },
    };
    let input = CRYPT_INTEGER_BLOB {
        cbData: u32::try_from(blob.len())?,
        pbData: blob.as_ptr().cast_mut(),
    };
    let extra = CRYPT_INTEGER_BLOB {
        cbData: u32::try_from(entropy.len())?,
        pbData: entropy.as_ptr().cast_mut(),
    };
    let mut output = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: std::ptr::null_mut(),
    };
    // SAFETY: as for protect; the plaintext buffer is wiped before it is freed.
    unsafe {
        let ok = CryptUnprotectData(
            &input,
            std::ptr::null_mut(),
            &extra,
            std::ptr::null(),
            std::ptr::null(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        );
        if ok == 0 {
            // Deliberately without the OS error text: it is not useful and the
            // call may be fed attacker-chosen blobs.
            bail!("Windows DPAPI refused to unprotect the key secret");
        }
        let plain = std::slice::from_raw_parts_mut(output.pbData, output.cbData as usize);
        let secret = Secret::new(plain.to_vec());
        wipe_slice(plain);
        LocalFree(output.pbData.cast());
        Ok(secret)
    }
}

#[cfg(not(windows))]
fn dpapi_protect(_data: &[u8], _entropy: &[u8]) -> anyhow::Result<Vec<u8>> {
    bail!("Windows DPAPI is not available on this platform")
}

#[cfg(not(windows))]
fn dpapi_unprotect(_blob: &[u8], _entropy: &[u8]) -> anyhow::Result<Secret> {
    bail!("this key file is bound to Windows DPAPI and can only be opened on Windows by the account that created it")
}

#[cfg(test)]
mod tests {
    use super::*;

    const ACCOUNT: &str = "3f1c0a77e2b94d65a8c1d0e9f7b6a5c4d3e2f1a0b9c8d7e6f5a4b3c2d1e0f9a8";

    #[test]
    fn provider_names_round_trip_and_unknown_names_are_rejected() {
        for provider in [
            Provider::WindowsDpapi,
            Provider::MacosKeychain,
            Provider::LinuxSecretService,
        ] {
            assert_eq!(Provider::parse(provider.name()), Some(provider));
        }
        assert_eq!(Provider::parse("gnome-keyring"), None);
        assert!(Provider::WindowsDpapi.blob_in_file());
        assert!(!Provider::MacosKeychain.blob_in_file());
    }

    #[test]
    fn wrapping_secrets_are_random_hex() {
        let a = random_secret().unwrap();
        let b = random_secret().unwrap();
        assert!(valid_secret(a.as_bytes()));
        assert_ne!(a.as_bytes(), b.as_bytes());
    }

    #[test]
    fn external_commands_never_carry_the_secret_in_their_arguments() {
        let secret = random_secret().unwrap();
        let secret_text = String::from_utf8(secret.as_bytes().to_vec()).unwrap();
        for command in [
            macos_store(ACCOUNT, secret.as_bytes()),
            macos_load(ACCOUNT),
            macos_forget(ACCOUNT),
            secret_tool_store(ACCOUNT, secret.as_bytes()),
            secret_tool_load(ACCOUNT),
            secret_tool_forget(ACCOUNT),
        ] {
            assert!(
                command.args.iter().all(|arg| !arg.contains(&secret_text)),
                "{command:?}"
            );
            assert!(!command.program.contains(&secret_text));
        }
        // The macOS secret is a command line of `security -i` on stdin.
        let store = macos_store(ACCOUNT, secret.as_bytes());
        assert_eq!(store.args, ["-i"]);
        let line = String::from_utf8(store.stdin.unwrap()).unwrap();
        assert!(line.starts_with("add-generic-password -a \""));
        assert!(line.contains(&format!("-w \"{secret_text}\"")));
        assert!(line.ends_with('\n'));
        let tool = secret_tool_store(ACCOUNT, secret.as_bytes());
        assert_eq!(tool.stdin.as_deref(), Some(secret.as_bytes()));
        assert_eq!(
            secret_tool_load(ACCOUNT).args,
            ["lookup", "service", SERVICE, "account", ACCOUNT]
        );
    }

    #[test]
    fn an_absent_keystore_tool_is_a_clear_error() {
        let missing = Command {
            program: "dynodoc-no-such-keystore-tool".into(),
            args: vec![],
            stdin: None,
        };
        let error = run(&missing, "read").unwrap_err().to_string();
        assert!(error.contains("not installed"), "{error}");
        assert!(error.contains("passphrase"), "{error}");
    }

    #[test]
    fn memory_provider_stores_loads_and_forgets() {
        let account = "ab".repeat(32);
        let secret = random_secret().unwrap();
        assert_eq!(store(Provider::Memory, &account, &secret).unwrap(), None);
        assert_eq!(
            load(Provider::Memory, &account, None).unwrap().as_bytes(),
            secret.as_bytes()
        );
        forget(Provider::Memory, &account).unwrap();
        assert!(load(Provider::Memory, &account, None).is_err());
    }

    #[cfg(windows)]
    #[test]
    fn dpapi_protects_for_this_account_and_binds_the_entropy() {
        let secret = random_secret().unwrap();
        let blob = store(Provider::WindowsDpapi, ACCOUNT, &secret)
            .unwrap()
            .unwrap();
        assert!(blob.len() > 64 && blob.len() <= MAX_BLOB_BYTES);
        // The secret is not in the blob in the clear.
        assert!(!blob
            .windows(secret.as_bytes().len())
            .any(|w| w == secret.as_bytes()));
        let back = load(Provider::WindowsDpapi, ACCOUNT, Some(&blob)).unwrap();
        assert_eq!(back.as_bytes(), secret.as_bytes());
        // A blob cannot be moved to another key (different entropy), nor damaged.
        let other = "cd".repeat(32);
        assert!(load(Provider::WindowsDpapi, &other, Some(&blob)).is_err());
        let mut damaged = blob.clone();
        let last = damaged.len() - 1;
        damaged[last] ^= 1;
        assert!(load(Provider::WindowsDpapi, ACCOUNT, Some(&damaged)).is_err());
        assert!(load(Provider::WindowsDpapi, ACCOUNT, Some(&[])).is_err());
        assert!(load(Provider::WindowsDpapi, ACCOUNT, None).is_err());
    }
}
