//! Owner-only access on Windows. Unix modes (0600 files, 0700 directories) are set where files are
//! created; Windows has no such mode, so a file or directory gets a DACL with inheritance removed
//! and one entry for the current user, applied with the system `icacls` and then read back and
//! verified. If it cannot be applied or verified the caller fails closed unless
//! `--allow-insecure-files` was given.
//!
//! This is a faithful copy of the approach of `crates/engine-cli/src/private_file.rs` (that crate
//! is separate from this one): the elevated-token SYSTEM and Administrators entries are removed by
//! well-known SID, and a user name with letters outside the console code page is compared by shape.
//! The argument building and the output parsing are pure functions, tested on every platform; only
//! `restrict` starts a process, and only on Windows.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

static ALLOW_INSECURE: AtomicBool = AtomicBool::new(false);

/// Record the process-wide `--allow-insecure-files` choice. Off by default.
pub fn allow_insecure(allow: bool) {
    ALLOW_INSECURE.store(allow, Ordering::Relaxed);
}

pub fn insecure_allowed() -> bool {
    ALLOW_INSECURE.load(Ordering::Relaxed)
}

/// `icacls` arguments that drop inherited entries and leave one entry for `principal` with full
/// control (for a directory, inherited by what is created inside it).
#[cfg(any(windows, test))]
pub fn icacls_apply_args(path: &Path, principal: &str, directory: bool) -> Vec<std::ffi::OsString> {
    let grant = if directory {
        format!("{principal}:(OI)(CI)F")
    } else {
        format!("{principal}:F")
    };
    vec![
        path.as_os_str().to_owned(),
        "/inheritance:r".into(),
        "/remove:g".into(),
        "*S-1-5-18".into(),
        "*S-1-5-32-544".into(),
        "/grant:r".into(),
        grant.into(),
    ]
}

/// The principals named in `icacls <path>` output. The first line starts with the path as given;
/// later entries are indented; a blank line ends the list. `None` for an unexpected shape.
#[cfg(any(windows, test))]
pub fn acl_principals(output: &str, path: &Path) -> Option<Vec<String>> {
    let path = path.to_string_lossy();
    let mut principals = Vec::new();
    for (index, line) in output.lines().enumerate() {
        if line.trim().is_empty() {
            break;
        }
        let entry = if index == 0 && path.is_ascii() {
            let head = line.get(..path.len())?;
            if !head.eq_ignore_ascii_case(&path) {
                return None;
            }
            &line[path.len()..]
        } else {
            // A non-ASCII folder name is printed in the console code page, so the printed path
            // cannot be compared with ours; the user name is still the text after its last backslash.
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
pub fn only_user(principals: &[String], username: &str) -> bool {
    matches!(principals, [one] if one.rsplit('\\').next().is_some_and(|name| same_account_name(name, username)))
}

/// An ASCII name must match exactly (ignoring case); a name with letters outside the console code
/// page is compared by its ASCII letters and the positions of the others.
#[cfg(any(windows, test))]
pub fn same_account_name(printed: &str, username: &str) -> bool {
    if printed.eq_ignore_ascii_case(username) {
        return true;
    }
    let shape = |text: &str| -> Vec<char> {
        text.chars()
            .map(|c| {
                if c.is_ascii() && c != '?' {
                    c.to_ascii_lowercase()
                } else {
                    '?'
                }
            })
            .collect()
    };
    !username.is_ascii() && shape(printed) == shape(username)
}

/// Restrict `path` to the current user. A no-op on platforms with Unix modes (they are set at
/// creation). Fails closed: the caller removes what it just created and reports the reason.
#[cfg(not(windows))]
pub fn restrict(_path: &Path, _directory: bool) -> Result<(), String> {
    Ok(())
}

#[cfg(windows)]
pub fn restrict(path: &Path, directory: bool) -> Result<(), String> {
    use std::path::PathBuf;
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
        .args(icacls_apply_args(&path, &principal, directory))
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;
    use std::path::PathBuf;

    #[test]
    fn icacls_arguments_drop_inheritance_remove_system_entries_and_grant_one_user() {
        let args = icacls_apply_args(Path::new(r"C:\x\f"), r"HOST\ann", false);
        let text: Vec<String> = args
            .iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            text,
            vec![
                r"C:\x\f",
                "/inheritance:r",
                "/remove:g",
                "*S-1-5-18",
                "*S-1-5-32-544",
                "/grant:r",
                r"HOST\ann:F"
            ]
        );
        let dir: Vec<OsString> = icacls_apply_args(Path::new(r"C:\x\d"), r"HOST\ann", true);
        assert_eq!(
            dir.last().unwrap().to_string_lossy(),
            r"HOST\ann:(OI)(CI)F",
            "a directory's grant is inherited by what is created in it"
        );
    }

    #[test]
    fn acl_output_is_parsed_and_only_the_single_user_passes() {
        let path = PathBuf::from(r"C:\data\secret.txt");
        let one = "C:\\data\\secret.txt HOST\\ann:(F)\n\nSuccessfully processed 1 files; Failed processing 0 files\n";
        let found = acl_principals(one, &path).unwrap();
        assert_eq!(found, vec![r"HOST\ann"]);
        assert!(only_user(&found, "ann") && !only_user(&found, "bob"));
        let two =
            "C:\\data\\secret.txt HOST\\ann:(F)\n                    NT AUTHORITY\\SYSTEM:(F)\n\n";
        let found = acl_principals(two, &path).unwrap();
        assert_eq!(found.len(), 2);
        assert!(
            !only_user(&found, "ann"),
            "an extra entry fails verification"
        );
        assert!(acl_principals("garbage", &path).is_none());
        assert!(
            acl_principals("C:\\other\\file HOST\\ann:(F)\n", &path).is_none(),
            "a different path is not this file's list"
        );
    }

    #[test]
    fn a_user_name_outside_the_console_code_page_is_compared_by_shape() {
        assert!(same_account_name("ann", "ANN"));
        assert!(
            same_account_name("a?b", "aঅb"),
            "letters outside the code page arrive as ?"
        );
        assert!(
            !same_account_name("a?c", "aঅb"),
            "the ASCII letters still have to match"
        );
        assert!(
            !same_account_name("a?", "ab"),
            "an ASCII name must match exactly"
        );
        assert!(!same_account_name("bob", "ann"));
    }

    #[cfg(windows)]
    #[test]
    fn a_real_file_and_a_real_directory_end_up_with_one_entry() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("f.txt");
        std::fs::write(&file, "x").unwrap();
        restrict(&file, false).expect("file restricted and verified");
        let sub = dir.path().join("d");
        std::fs::create_dir(&sub).unwrap();
        restrict(&sub, true).expect("directory restricted and verified");
    }
}
