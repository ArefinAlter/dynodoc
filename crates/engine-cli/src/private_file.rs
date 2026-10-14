//! Private local files for key material and credentials: exclusive creation,
//! owner-only access, fsync, and atomic replacement. A crash or full disk never
//! leaves a half-written credential under the target name, and a failed
//! replacement leaves the previous file intact.
//!
//! Owner-only means mode `0600` on Unix (set at creation) and, on Windows, a DACL
//! with inheritance removed and one entry for the current user, applied with the
//! system `icacls` right after creation and verified. If it cannot be applied the
//! file is removed and creation fails, unless `--allow-insecure-key-file` was given.
//! Loading a key on Unix refuses a file that group or others can access under the
//! same opt-out. These files hold unencrypted keys or tokens; encrypted key files
//! (`dynodoc-key-v1`, see keyfile.rs) are written through the same functions.
use anyhow::{anyhow, Context};
use std::{
    fs::{File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};
use uuid::Uuid;

static ALLOW_INSECURE: AtomicBool = AtomicBool::new(false);

/// Record the process-wide `--allow-insecure-key-file` choice. Off by default.
pub fn allow_insecure(allow: bool) {
    ALLOW_INSECURE.store(allow, Ordering::Relaxed);
}
fn insecure_allowed() -> bool {
    ALLOW_INSECURE.load(Ordering::Relaxed)
}

fn options() -> OpenOptions {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options
}

/// Create a new file that must not exist, readable and writable by the current
/// user only: mode 0600 on Unix, a one-entry DACL on Windows. If the restriction
/// cannot be applied or verified the new file is removed and an error returned
/// (fail closed) unless `--allow-insecure-key-file` was given, which only warns.
pub fn create_new(path: &Path) -> std::io::Result<File> {
    let file = options().open(path)?;
    if let Err(reason) = restrict(path) {
        if insecure_allowed() {
            eprintln!("warning: {reason}; continuing because --allow-insecure-key-file was given");
        } else {
            drop(file);
            let _ = std::fs::remove_file(path);
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                format!("{reason}; the file was not kept (pass --allow-insecure-key-file to accept the folder's permissions)"),
            ));
        }
    }
    Ok(file)
}

/// Unix creates the file with mode 0600 already (see [`options`]).
#[cfg(not(windows))]
fn restrict(_path: &Path) -> Result<(), String> {
    Ok(())
}

/// `icacls` arguments that drop inherited entries and leave one entry for
/// `principal` with full control. Built separately so it is testable everywhere.
#[cfg(any(windows, test))]
fn icacls_apply_args(path: &Path, principal: &str) -> Vec<std::ffi::OsString> {
    vec![
        path.as_os_str().to_owned(),
        "/inheritance:r".into(),
        "/grant:r".into(),
        format!("{principal}:F").into(),
    ]
}

/// The principals named in `icacls <path>` output. The first line starts with the
/// path as given; later entries are indented; a blank line ends the list. `None`
/// when the output has an unexpected shape (the caller then fails closed).
#[cfg(any(windows, test))]
fn acl_principals(output: &str, path: &Path) -> Option<Vec<String>> {
    let path = path.to_string_lossy();
    let mut principals = Vec::new();
    for (index, line) in output.lines().enumerate() {
        if line.trim().is_empty() {
            break;
        }
        let entry = if index == 0 {
            let head = line.get(..path.len())?;
            if !head.eq_ignore_ascii_case(&path) {
                return None;
            }
            &line[path.len()..]
        } else {
            line
        };
        let entry = entry.trim();
        let (principal, rights) = entry.split_once(":(")?;
        if principal.is_empty() || rights.is_empty() {
            return None;
        }
        principals.push(principal.to_owned());
    }
    (!principals.is_empty()).then_some(principals)
}

/// Exactly one entry and it is `username`'s (icacls prints `DOMAIN\name`).
#[cfg(any(windows, test))]
fn only_user(principals: &[String], username: &str) -> bool {
    matches!(principals, [one] if one.rsplit('\\').next().is_some_and(|name| name.eq_ignore_ascii_case(username)))
}

#[cfg(windows)]
fn restrict(path: &Path) -> Result<(), String> {
    use std::process::Command;
    let path =
        std::path::absolute(path).map_err(|e| format!("cannot resolve {}: {e}", path.display()))?;
    let username = std::env::var("USERNAME")
        .ok()
        .filter(|u| !u.is_empty())
        .ok_or("cannot determine the current Windows user (USERNAME is not set)")?;
    let principal = match std::env::var("USERDOMAIN").ok().filter(|d| !d.is_empty()) {
        Some(domain) => format!("{domain}\\{username}"),
        None => username.clone(),
    };
    // The system copy, not whatever a PATH or working directory lookup finds.
    let root = std::env::var_os("SystemRoot")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Windows"));
    let icacls = root.join("System32").join("icacls.exe");
    let fail = |what: &str, detail: String| {
        format!(
            "cannot restrict {} to the current user ({what}: {detail})",
            path.display()
        )
    };
    let applied = Command::new(&icacls)
        .args(icacls_apply_args(&path, &principal))
        .output()
        .map_err(|e| fail("icacls did not start", e.to_string()))?;
    if !applied.status.success() {
        let text = format!(
            "{}\n{}",
            String::from_utf8_lossy(&applied.stdout),
            String::from_utf8_lossy(&applied.stderr)
        );
        let first = text.lines().find(|l| !l.trim().is_empty());
        return Err(fail(
            "icacls failed",
            first.unwrap_or("no message").trim().to_owned(),
        ));
    }
    let shown = Command::new(&icacls)
        .arg(&path)
        .output()
        .map_err(|e| fail("icacls did not start for verification", e.to_string()))?;
    let text = String::from_utf8_lossy(&shown.stdout).into_owned();
    match acl_principals(&text, &path) {
        Some(found) if only_user(&found, &username) => Ok(()),
        Some(found) => Err(fail(
            "verification",
            format!("access list is {}", found.join(", ")),
        )),
        None => Err(fail(
            "verification",
            "unrecognised icacls output".to_owned(),
        )),
    }
}

/// Refuse a key file another local account can open (Unix: any group or other
/// permission bit). Called on the open handle so the check and the read are about
/// the same file. `--allow-insecure-key-file` accepts it. Windows has no portable
/// check here: only files this program created carry the restricted DACL.
pub fn check_key_file(file: &File, path: &Path) -> anyhow::Result<()> {
    check_key_file_with(file, path, insecure_allowed())
}

#[cfg(any(unix, test))]
fn mode_is_private(mode: u32) -> bool {
    mode & 0o077 == 0
}

fn check_key_file_with(file: &File, path: &Path, allow: bool) -> anyhow::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = file.metadata()?.permissions().mode() & 0o7777;
        if !mode_is_private(mode) && !allow {
            anyhow::bail!(
                "{} is accessible by other accounts (mode {mode:04o}); run `chmod 600` on it, or pass --allow-insecure-key-file",
                path.display()
            );
        }
    }
    #[cfg(not(unix))]
    {
        let _ = (file, path, allow);
    }
    Ok(())
}

/// Write a complete new file exclusively. A partial file is removed on failure.
pub fn write_new(path: &Path, bytes: &[u8]) -> anyhow::Result<()> {
    let mut file = create_new(path).map_err(|error| {
        let note = if error.kind() == std::io::ErrorKind::AlreadyExists {
            " (an existing file is never overwritten)"
        } else {
            ""
        };
        anyhow!("cannot create {}{note}: {error}", path.display())
    })?;
    let written = file.write_all(bytes).and_then(|()| file.sync_all());
    drop(file);
    if let Err(error) = written {
        let _ = std::fs::remove_file(path);
        return Err(anyhow!("cannot write {}: {error}", path.display()));
    }
    Ok(())
}

/// A same-directory temporary file prepared *before* a secret is rotated, so
/// permissions or disk problems are discovered while the old secret still works.
pub struct Replacement {
    target: PathBuf,
    temp: PathBuf,
    file: Option<File>,
}

impl Replacement {
    pub fn prepare(target: &Path) -> anyhow::Result<Self> {
        let name = target
            .file_name()
            .ok_or_else(|| anyhow!("credential path must name a file"))?
            .to_string_lossy();
        let temp = target.with_file_name(format!(".{name}.{}.tmp", Uuid::new_v4().simple()));
        let file = create_new(&temp)
            .with_context(|| format!("cannot prepare a replacement for {}", target.display()))?;
        Ok(Self {
            target: target.to_owned(),
            temp,
            file: Some(file),
        })
    }

    /// Keep a narrower owner-only mode of the file being replaced (for example
    /// 0400) instead of the 0600 the temporary file was created with. Never widens
    /// it. Windows keeps the one-entry access list the temporary file already has.
    pub fn keep_mode_of(&self, original: &Path) {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Ok(meta) = std::fs::metadata(original) {
                let mode = meta.permissions().mode() & 0o700;
                if mode != 0o600 && mode & 0o400 != 0 {
                    let _ =
                        std::fs::set_permissions(&self.temp, std::fs::Permissions::from_mode(mode));
                }
            }
        }
        #[cfg(not(unix))]
        {
            let _ = original;
        }
    }

    /// Write, fsync and rename over the target. The target is either the old
    /// complete content or the new complete content.
    pub fn commit(mut self, bytes: &[u8]) -> anyhow::Result<()> {
        let mut file = self.file.take().expect("prepared file");
        file.write_all(bytes)
            .and_then(|()| file.sync_all())
            .with_context(|| format!("cannot write {}", self.temp.display()))?;
        drop(file);
        std::fs::rename(&self.temp, &self.target)
            .with_context(|| format!("cannot replace {}", self.target.display()))?;
        #[cfg(unix)]
        if let Some(parent) = self.target.parent().filter(|p| !p.as_os_str().is_empty()) {
            if let Ok(directory) = File::open(parent) {
                let _ = directory.sync_all();
            }
        }
        Ok(())
    }
}

impl Drop for Replacement {
    fn drop(&mut self) {
        // Reached when not committed, or when rename failed after commit() consumed
        // the handle. A successfully renamed temp file no longer exists.
        drop(self.file.take());
        let _ = std::fs::remove_file(&self.temp);
    }
}

/// Test helper: the file is accessible by its owner only (Unix mode bits; on
/// Windows the real `icacls` listing has exactly the current user).
#[cfg(test)]
pub(crate) fn assert_owner_only(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(path).unwrap().permissions().mode();
        assert!(mode_is_private(mode & 0o7777), "{mode:o}");
    }
    #[cfg(windows)]
    {
        let root = std::env::var_os("SystemRoot").unwrap();
        let exe = PathBuf::from(root).join("System32").join("icacls.exe");
        let out = std::process::Command::new(exe).arg(path).output().unwrap();
        let text = String::from_utf8_lossy(&out.stdout).into_owned();
        let found = acl_principals(&text, path).unwrap();
        let username = std::env::var("USERNAME").unwrap();
        assert!(only_user(&found, &username), "{found:?}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("dynodoc-private-{}", Uuid::new_v4()));
        std::fs::create_dir(&dir).unwrap();
        dir
    }

    #[test]
    fn exclusive_creation_never_overwrites() {
        let dir = dir();
        let path = dir.join("key.der");
        write_new(&path, b"first").unwrap();
        assert!(write_new(&path, b"second").is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"first");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn icacls_arguments_drop_inheritance_and_replace_with_one_user() {
        let args = icacls_apply_args(Path::new(r"C:\keys\signing key.der"), r"PC\maya");
        let args: Vec<_> = args
            .iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            args,
            [
                r"C:\keys\signing key.der",
                "/inheritance:r",
                "/grant:r",
                r"PC\maya:F"
            ]
        );
    }

    #[test]
    fn acl_output_is_parsed_and_only_the_single_user_passes() {
        let path = Path::new(r"C:\Users\Maya Chen\k.der");
        let one = "C:\\Users\\Maya Chen\\k.der PC\\maya:(F)\n\nSuccessfully processed 1 files; Failed processing 0 files\n";
        let found = acl_principals(one, path).unwrap();
        assert_eq!(found, [r"PC\maya"]);
        assert!(only_user(&found, "Maya"));
        assert!(!only_user(&found, "sam"));
        // A different case of the path, and inherited entries beside the user.
        let many = "c:\\users\\maya chen\\k.der NT AUTHORITY\\SYSTEM:(I)(F)\n                              BUILTIN\\Users:(I)(RX)\n                              PC\\maya:(I)(F)\n\nSuccessfully processed 1 files; Failed processing 0 files\n";
        let found = acl_principals(many, path).unwrap();
        assert_eq!(found.len(), 3);
        assert!(!only_user(&found, "maya"));
        // Unrecognised shapes fail closed.
        assert!(acl_principals("", path).is_none());
        assert!(acl_principals("D:\\other\\k.der PC\\maya:(F)\n", path).is_none());
        assert!(acl_principals("C:\\Users\\Maya Chen\\k.der no entries here\n", path).is_none());
        assert!(!only_user(&[], "maya"));
    }

    #[test]
    fn group_or_other_permission_bits_are_not_private() {
        assert!(mode_is_private(0o600));
        assert!(mode_is_private(0o400));
        assert!(!mode_is_private(0o640));
        assert!(!mode_is_private(0o604));
        assert!(!mode_is_private(0o644));
        assert!(!mode_is_private(0o777));
    }

    #[cfg(unix)]
    #[test]
    fn a_unix_key_file_readable_by_group_or_others_is_refused_unless_allowed() {
        use std::os::unix::fs::PermissionsExt;
        let dir = dir();
        let path = dir.join("signing-key.der");
        std::fs::write(&path, b"key").unwrap();
        for (mode, accepted) in [
            (0o600, true),
            (0o400, true),
            (0o640, false),
            (0o604, false),
            (0o644, false),
        ] {
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(mode)).unwrap();
            let file = File::open(&path).unwrap();
            assert_eq!(
                check_key_file_with(&file, &path, false).is_ok(),
                accepted,
                "{mode:o}"
            );
            // The explicit opt-out accepts every mode.
            assert!(check_key_file_with(&file, &path, true).is_ok(), "{mode:o}");
        }
        let error = {
            let file = File::open(&path).unwrap();
            check_key_file_with(&file, &path, false)
                .unwrap_err()
                .to_string()
        };
        assert!(error.contains("--allow-insecure-key-file"), "{error}");
        // A file this module created is private and passes.
        let created = dir.join("created.der");
        write_new(&created, b"key").unwrap();
        assert!(check_key_file_with(&File::open(&created).unwrap(), &created, false).is_ok());
        // The atomic replacement keeps owner-only mode.
        Replacement::prepare(&created)
            .unwrap()
            .commit(b"new")
            .unwrap();
        assert_eq!(
            std::fs::metadata(&created).unwrap().permissions().mode() & 0o777,
            0o600
        );
        // A narrower original mode is kept, a wider one never is.
        for (original, expected) in [(0o400, 0o400), (0o600, 0o600), (0o640, 0o600)] {
            let target = dir.join(format!("mode-{original:o}"));
            std::fs::write(&target, b"old").unwrap();
            std::fs::set_permissions(&target, std::fs::Permissions::from_mode(original)).unwrap();
            let replacement = Replacement::prepare(&target).unwrap();
            replacement.keep_mode_of(&target);
            replacement.commit(b"new").unwrap();
            assert_eq!(
                std::fs::metadata(&target).unwrap().permissions().mode() & 0o777,
                expected,
                "{original:o}"
            );
            assert_eq!(std::fs::read(&target).unwrap(), b"new");
        }
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// Runs the real system `icacls` on this host: a created file and an atomically
    /// replaced file each end with exactly one access entry, the current user's.
    #[cfg(windows)]
    #[test]
    fn windows_files_carry_a_one_entry_acl_for_the_current_user() {
        let dir = dir();
        let username = std::env::var("USERNAME").unwrap();
        let listed = |path: &Path| {
            let root = std::env::var_os("SystemRoot").unwrap();
            let exe = PathBuf::from(root).join("System32").join("icacls.exe");
            let out = std::process::Command::new(exe).arg(path).output().unwrap();
            String::from_utf8_lossy(&out.stdout).into_owned()
        };
        let key = dir.join("signing-key.der");
        write_new(&key, b"secret").unwrap();
        let found = acl_principals(&listed(&key), &key).unwrap();
        assert!(only_user(&found, &username), "{found:?}");
        assert_eq!(std::fs::read(&key).unwrap(), b"secret");

        // The replacement temp file is restricted before the secret is written, and
        // the rename keeps that access list on the target.
        let token = dir.join("refresh.txt");
        std::fs::write(&token, b"old").unwrap();
        Replacement::prepare(&token)
            .unwrap()
            .commit(b"new")
            .unwrap();
        let found = acl_principals(&listed(&token), &token).unwrap();
        assert!(only_user(&found, &username), "{found:?}");
        assert_eq!(std::fs::read(&token).unwrap(), b"new");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn replacement_is_atomic_and_cleans_up() {
        let dir = dir();
        let path = dir.join("refresh.txt");
        std::fs::write(&path, b"old").unwrap();
        // Dropped without commit: old content retained, no temp file left.
        drop(Replacement::prepare(&path).unwrap());
        assert_eq!(std::fs::read(&path).unwrap(), b"old");
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
        Replacement::prepare(&path).unwrap().commit(b"new").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"new");
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
