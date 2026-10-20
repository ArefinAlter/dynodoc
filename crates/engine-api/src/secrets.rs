//! Secret-bearing settings: `NAME`, or `NAME_FILE` naming a file that holds the value.
//!
//! Rules, in the order they apply:
//! - Only `NAME` set: the value is used exactly as before (no trimming, no checks here).
//! - Only `NAME_FILE` set: the file is read, trailing line breaks are removed, and
//!   the rest is the value. The file must be a regular file of at most
//!   [`MAX_SECRET_FILE_BYTES`] holding UTF-8 text. On Unix a file that group or others
//!   can access is refused; `DYNODOC_ALLOW_OPEN_SECRET_FILES=true` accepts it (the
//!   default modes of Docker and Kubernetes secret mounts need this).
//! - Both set: an error. A `NAME` that is empty counts as unset, so a template line
//!   such as `WEB_SERVICE_KEY=` cannot collide with `WEB_SERVICE_KEY_FILE`. An empty
//!   `NAME_FILE` is ignored the same way.
//!
//! Errors name the variable (and the file path) and never contain a secret value or
//! any part of one.
use std::{ffi::OsString, fmt, io::Read, path::Path};

/// Largest accepted secret file. Keys and connection URLs are far smaller; the bound
/// stops a wrong path (a log, a device) from being read into memory.
pub const MAX_SECRET_FILE_BYTES: u64 = 16 * 1024;

/// Set to `true` to accept a secret file that group or others can read (Unix only).
pub const ALLOW_OPEN_FILES: &str = "DYNODOC_ALLOW_OPEN_SECRET_FILES";

/// Environment access. The binary passes `std::env::var_os`; tests pass a map.
pub type Lookup<'a> = &'a dyn Fn(&str) -> Option<OsString>;

/// The text of a secret. `Debug` never shows it.
pub struct Secret(String);

impl Secret {
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret(***)")
    }
}

/// A readable reason for a rejected secret setting; carries no secret text.
#[derive(Debug, PartialEq, Eq)]
pub struct SecretError(String);

impl fmt::Display for SecretError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for SecretError {}

/// Read the secret `name`, or `None` when neither `name` nor `name_FILE` is set.
pub fn read(lookup: Lookup, name: &str) -> Result<Option<Secret>, SecretError> {
    let file_variable = format!("{name}_FILE");
    let plain = lookup(name);
    let Some(path) = lookup(&file_variable).filter(|path| !path.is_empty()) else {
        return plain
            .map(|value| {
                value
                    .into_string()
                    .map(Secret)
                    .map_err(|_| SecretError(format!("{name} is not valid UTF-8")))
            })
            .transpose();
    };
    if plain.is_some_and(|value| !value.is_empty()) {
        return Err(SecretError(format!(
            "{name} and {file_variable} are both set; set exactly one"
        )));
    }
    read_file(lookup, &file_variable, Path::new(&path)).map(Some)
}

fn read_file(lookup: Lookup, variable: &str, path: &Path) -> Result<Secret, SecretError> {
    let fail = |reason: String| SecretError(format!("{variable} ({}): {reason}", path.display()));
    let mut file = std::fs::File::open(path).map_err(|e| fail(format!("cannot open: {e}")))?;
    let metadata = file
        .metadata()
        .map_err(|e| fail(format!("cannot inspect: {e}")))?;
    if !metadata.is_file() {
        return Err(fail("is not a regular file".into()));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let open_allowed = lookup(ALLOW_OPEN_FILES).is_some_and(|v| v == "true");
        if !open_allowed && !owner_only(metadata.mode()) {
            return Err(fail(format!(
                "is accessible by group or others (mode {:04o}); run chmod 600 on it, or set {ALLOW_OPEN_FILES}=true where the platform fixes the mode",
                metadata.mode() & 0o777
            )));
        }
    }
    #[cfg(not(unix))]
    let _ = lookup;
    let too_large = || fail(format!("is larger than {MAX_SECRET_FILE_BYTES} bytes"));
    if metadata.len() > MAX_SECRET_FILE_BYTES {
        return Err(too_large());
    }
    let mut bytes = Vec::new();
    (&mut file)
        .take(MAX_SECRET_FILE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| fail(format!("cannot read: {e}")))?;
    if bytes.len() as u64 > MAX_SECRET_FILE_BYTES {
        return Err(too_large());
    }
    let mut text = String::from_utf8(bytes).map_err(|_| fail("is not valid UTF-8 text".into()))?;
    text.truncate(text.trim_end_matches(['\r', '\n']).len());
    if text.is_empty() {
        return Err(fail("is empty".into()));
    }
    Ok(Secret(text))
}

/// True when neither group nor others have any access bit.
#[cfg(any(unix, test))]
fn owner_only(mode: u32) -> bool {
    mode & 0o077 == 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    const VALUE: &str = "s3cr3t-value-that-must-not-leak-0123456789";

    fn lookup(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<OsString> {
        let map: HashMap<String, OsString> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), OsString::from(v)))
            .collect();
        move |name| map.get(name).cloned()
    }

    /// A private temporary file; removed on drop.
    struct TempFile(std::path::PathBuf);
    impl TempFile {
        fn new(content: &[u8]) -> Self {
            let path =
                std::env::temp_dir().join(format!("dynodoc-secret-{}", uuid::Uuid::new_v4()));
            std::fs::write(&path, content).unwrap();
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
            }
            Self(path)
        }
        fn path(&self) -> &str {
            self.0.to_str().unwrap()
        }
    }
    impl Drop for TempFile {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }

    fn text(result: Result<Option<Secret>, SecretError>) -> Option<String> {
        result.unwrap().map(|s| s.expose().to_string())
    }

    #[test]
    fn plain_variable_is_returned_byte_for_byte() {
        for value in [VALUE, " padded with spaces ", "ends with newline\n", ""] {
            let env = lookup(&[("KEY", value)]);
            assert_eq!(text(read(&env, "KEY")), Some(value.to_string()));
        }
    }

    #[test]
    fn nothing_set_is_none() {
        assert!(read(&lookup(&[]), "KEY").unwrap().is_none());
        // An empty NAME_FILE is ignored, as if unset.
        assert!(read(&lookup(&[("KEY_FILE", "")]), "KEY").unwrap().is_none());
    }

    #[test]
    fn file_variable_reads_the_file_and_trims_trailing_line_breaks() {
        for (content, expected) in [
            (VALUE.to_string(), VALUE.to_string()),
            (format!("{VALUE}\n"), VALUE.to_string()),
            (format!("{VALUE}\r\n"), VALUE.to_string()),
            (format!("{VALUE}\n\r\n\n"), VALUE.to_string()),
            (format!(" {VALUE} \n"), format!(" {VALUE} ")),
        ] {
            let file = TempFile::new(content.as_bytes());
            let env = lookup(&[("KEY_FILE", file.path())]);
            assert_eq!(text(read(&env, "KEY")), Some(expected));
        }
    }

    #[test]
    fn both_set_is_a_conflict_but_an_empty_plain_value_is_not() {
        let file = TempFile::new(VALUE.as_bytes());
        let env = lookup(&[("KEY", VALUE), ("KEY_FILE", file.path())]);
        let message = read(&env, "KEY").err().unwrap().to_string();
        assert!(
            message.contains("KEY and KEY_FILE are both set"),
            "{message}"
        );
        assert!(!message.contains(VALUE), "{message}");
        let env = lookup(&[("KEY", ""), ("KEY_FILE", file.path())]);
        assert_eq!(text(read(&env, "KEY")), Some(VALUE.to_string()));
    }

    #[test]
    fn missing_file_names_the_variable_and_path_only() {
        let path = std::env::temp_dir().join(format!("dynodoc-absent-{}", uuid::Uuid::new_v4()));
        let env = lookup(&[("KEY_FILE", path.to_str().unwrap())]);
        let message = read(&env, "KEY").err().unwrap().to_string();
        assert!(message.starts_with("KEY_FILE ("), "{message}");
        assert!(message.contains("cannot open"), "{message}");
    }

    #[test]
    fn directories_are_not_secret_files() {
        let dir = std::env::temp_dir();
        let env = lookup(&[("KEY_FILE", dir.to_str().unwrap())]);
        // Opening a directory fails on Windows and succeeds on Unix; both are refused.
        assert!(read(&env, "KEY").is_err());
    }

    #[test]
    fn oversized_and_empty_files_are_refused() {
        let big = TempFile::new(&vec![b'a'; MAX_SECRET_FILE_BYTES as usize + 1]);
        let env = lookup(&[("KEY_FILE", big.path())]);
        let message = read(&env, "KEY").err().unwrap().to_string();
        assert!(message.contains("larger than"), "{message}");
        let at_limit = TempFile::new(&vec![b'a'; MAX_SECRET_FILE_BYTES as usize]);
        let env = lookup(&[("KEY_FILE", at_limit.path())]);
        assert_eq!(
            text(read(&env, "KEY")).unwrap().len(),
            MAX_SECRET_FILE_BYTES as usize
        );
        for content in ["", "\n"] {
            let empty = TempFile::new(content.as_bytes());
            let env = lookup(&[("KEY_FILE", empty.path())]);
            assert!(read(&env, "KEY")
                .err()
                .unwrap()
                .to_string()
                .contains("is empty"));
        }
    }

    #[test]
    fn non_utf8_content_is_refused_without_echoing_it() {
        let file = TempFile::new(&[0xff, 0xfe, b'x', b'y']);
        let env = lookup(&[("KEY_FILE", file.path())]);
        let message = read(&env, "KEY").err().unwrap().to_string();
        assert!(message.contains("not valid UTF-8"), "{message}");
    }

    #[test]
    fn errors_never_contain_the_value() {
        // Oversized content that starts with the value, and conflicting settings.
        let mut content = VALUE.as_bytes().to_vec();
        content.resize(MAX_SECRET_FILE_BYTES as usize + 10, b'a');
        let file = TempFile::new(&content);
        let ok = TempFile::new(VALUE.as_bytes());
        for env in [
            lookup(&[("KEY_FILE", file.path())]),
            lookup(&[("KEY", VALUE), ("KEY_FILE", ok.path())]),
        ] {
            let message = read(&env, "KEY").err().unwrap().to_string();
            assert!(!message.contains(VALUE), "{message}");
            assert!(!message.contains(&VALUE[..8]), "{message}");
        }
        assert_eq!(format!("{:?}", Secret(VALUE.into())), "Secret(***)");
    }

    #[test]
    fn mode_check_accepts_owner_only_modes() {
        for mode in [0o400, 0o600, 0o100600] {
            assert!(owner_only(mode), "{mode:o}");
        }
        for mode in [0o440, 0o640, 0o644, 0o604, 0o666, 0o444] {
            assert!(!owner_only(mode), "{mode:o}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn group_or_other_access_is_refused_unless_allowed() {
        use std::os::unix::fs::PermissionsExt;
        let file = TempFile::new(VALUE.as_bytes());
        for mode in [0o644, 0o640, 0o604, 0o660] {
            std::fs::set_permissions(&file.0, std::fs::Permissions::from_mode(mode)).unwrap();
            let env = lookup(&[("KEY_FILE", file.path())]);
            let message = read(&env, "KEY").err().unwrap().to_string();
            assert!(
                message.contains("accessible by group or others"),
                "{mode:o}: {message}"
            );
            assert!(message.contains("KEY_FILE"), "{message}");
            assert!(!message.contains(VALUE), "{message}");
            let env = lookup(&[("KEY_FILE", file.path()), (ALLOW_OPEN_FILES, "true")]);
            assert_eq!(text(read(&env, "KEY")), Some(VALUE.to_string()));
        }
        std::fs::set_permissions(&file.0, std::fs::Permissions::from_mode(0o400)).unwrap();
        let env = lookup(&[("KEY_FILE", file.path())]);
        assert_eq!(text(read(&env, "KEY")), Some(VALUE.to_string()));
    }
}
