//! The one setting the app stores: which server to open. A small JSON file in the app's
//! config directory, re-validated every time it is read, written atomically, and SEALED:
//! the SHA-256 of exactly the bytes the app wrote is kept in a second file in the local data
//! folder. If the config no longer matches its seal (another program or user edited it, or
//! the seal is gone) the address is not opened silently: the launcher shows it with a plain
//! notice and the person confirms it.
//!
//! The seal detects edits by anything that does not also rewrite the seal. It is not a
//! defence against malware running as the same user, which can rewrite both.

use crate::server::{parse_server_address, ServerOrigin};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

pub const FILE_NAME: &str = "config.json";
pub const SEAL_NAME: &str = "config.seal";
/// The file holds one short address; anything bigger is not ours.
pub const MAX_FILE_BYTES: u64 = 4096;

#[derive(Serialize, Deserialize)]
struct Stored {
    version: u32,
    server: String,
}

/// What the config folder held at start-up.
#[derive(Debug, PartialEq, Eq)]
pub enum Loaded {
    /// No usable saved address.
    Nothing,
    /// A valid address that matches its seal: opened as before.
    Confirmed(ServerOrigin),
    /// A valid address that does NOT match its seal (or has none): shown, not opened.
    Unconfirmed(ServerOrigin),
}

pub fn path_in(dir: &Path) -> PathBuf {
    dir.join(FILE_NAME)
}

pub fn seal_in(data_dir: &Path) -> PathBuf {
    data_dir.join(SEAL_NAME)
}

/// Lower-case hexadecimal SHA-256.
pub fn seal_of(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn read_bounded(path: &Path, max: u64) -> Option<Vec<u8>> {
    let file = std::fs::File::open(path).ok()?;
    let mut bytes = Vec::new();
    file.take(max + 1).read_to_end(&mut bytes).ok()?;
    (bytes.len() as u64 <= max).then_some(bytes)
}

/// The saved server, checked against its seal. A missing, too big, damaged or refused file
/// is `Nothing` (a hand-edited file cannot widen what the window may open).
pub fn load(config_dir: &Path, data_dir: &Path) -> Loaded {
    let Some(bytes) = read_bounded(&path_in(config_dir), MAX_FILE_BYTES) else {
        return Loaded::Nothing;
    };
    let Ok(stored) = serde_json::from_slice::<Stored>(&bytes) else {
        return Loaded::Nothing;
    };
    if stored.version != 1 {
        return Loaded::Nothing;
    }
    let Ok(server) = parse_server_address(&stored.server) else {
        return Loaded::Nothing;
    };
    let sealed = read_bounded(&seal_in(data_dir), 256)
        .and_then(|s| String::from_utf8(s).ok())
        .is_some_and(|s| s.trim() == seal_of(&bytes));
    if sealed {
        Loaded::Confirmed(server)
    } else {
        Loaded::Unconfirmed(server)
    }
}

fn write_atomically(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(dir)?;
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".tmp");
    let temporary = dir.join(name);
    {
        let mut file = std::fs::File::create(&temporary)?;
        file.write_all(bytes)?;
        file.sync_all()?;
    }
    std::fs::rename(&temporary, path)
}

/// Write the config next to its final name, then rename over it, then the seal the same
/// way, so a crash leaves either the old content or the new, never half of one. A crash
/// between the two leaves a config that does not match its seal, which is shown, not opened.
pub fn save(config_dir: &Path, data_dir: &Path, server: &ServerOrigin) -> std::io::Result<()> {
    let stored = Stored {
        version: 1,
        server: server.as_text(),
    };
    let mut json = serde_json::to_vec_pretty(&stored).map_err(std::io::Error::other)?;
    json.push(b'\n');
    write_atomically(&path_in(config_dir), &json)?;
    write_atomically(
        &seal_in(data_dir),
        format!("{}\n", seal_of(&json)).as_bytes(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "dynodoc-desktop-test-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn pair(name: &str) -> (PathBuf, PathBuf, PathBuf) {
        let root = scratch(name);
        let config = root.join("config");
        let data = root.join("data");
        std::fs::create_dir_all(&config).unwrap();
        std::fs::create_dir_all(&data).unwrap();
        (root, config, data)
    }

    fn server(text: &str) -> ServerOrigin {
        parse_server_address(text).unwrap()
    }

    #[test]
    fn a_saved_server_comes_back_confirmed_and_can_be_replaced() {
        let (root, config, data) = pair("roundtrip");
        assert_eq!(load(&config, &data), Loaded::Nothing);
        let a = server("https://dynodoc.example.org:8443");
        save(&config, &data, &a).unwrap();
        assert_eq!(load(&config, &data), Loaded::Confirmed(a));
        let b = server("https://app.dynodoc.online");
        save(&config, &data, &b).unwrap();
        assert_eq!(load(&config, &data), Loaded::Confirmed(b));
        assert!(!config.join("config.json.tmp").exists());
        assert!(!data.join("config.seal.tmp").exists());
        let text = std::fs::read_to_string(path_in(&config)).unwrap();
        assert!(text.contains("\"https://app.dynodoc.online\""), "{text}");
        assert_eq!(
            std::fs::read_to_string(seal_in(&data)).unwrap().trim(),
            seal_of(text.as_bytes())
        );
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn saving_creates_the_folders() {
        let root = scratch("create");
        let config = root.join("a").join("config");
        let data = root.join("b").join("data");
        let a = server("https://dynodoc.example.org");
        save(&config, &data, &a).unwrap();
        assert_eq!(load(&config, &data), Loaded::Confirmed(a));
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn a_config_edited_outside_the_app_is_shown_not_opened() {
        let (root, config, data) = pair("tamper");
        save(&config, &data, &server("https://dynodoc.example.org")).unwrap();
        // Another program points the app somewhere else, keeping the file valid.
        std::fs::write(
            path_in(&config),
            "{\n  \"version\": 1,\n  \"server\": \"https://evil.example\"\n}\n",
        )
        .unwrap();
        assert_eq!(
            load(&config, &data),
            Loaded::Unconfirmed(server("https://evil.example"))
        );
        // Even a harmless change of spacing breaks the seal: it is the bytes that are sealed.
        save(&config, &data, &server("https://dynodoc.example.org")).unwrap();
        let text = std::fs::read_to_string(path_in(&config)).unwrap();
        std::fs::write(path_in(&config), text.replace("  ", "\t")).unwrap();
        assert!(matches!(load(&config, &data), Loaded::Unconfirmed(_)));
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn a_missing_wrong_or_damaged_seal_means_unconfirmed() {
        let (root, config, data) = pair("seal");
        let a = server("https://dynodoc.example.org");
        save(&config, &data, &a).unwrap();
        for seal in ["", "not a hash", "0000", &"0".repeat(64), &"a".repeat(1000)] {
            std::fs::write(seal_in(&data), seal).unwrap();
            assert_eq!(
                load(&config, &data),
                Loaded::Unconfirmed(a.clone()),
                "{seal:.20}"
            );
        }
        std::fs::remove_file(seal_in(&data)).unwrap();
        assert_eq!(load(&config, &data), Loaded::Unconfirmed(a));
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn the_seal_is_a_sha256() {
        assert_eq!(
            seal_of(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            seal_of(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn hostile_or_damaged_files_are_ignored() {
        let (root, config, data) = pair("hostile");
        let write = |text: &str| std::fs::write(path_in(&config), text).unwrap();
        for bad in [
            "",
            "not json",
            "[]",
            r#"{"version":1}"#,
            r#"{"version":2,"server":"https://dynodoc.example.org"}"#,
            r#"{"version":1,"server":"javascript:alert(1)"}"#,
            r#"{"version":1,"server":"file:///etc/passwd"}"#,
            r#"{"version":1,"server":"http://evil.example"}"#,
            r#"{"version":1,"server":"https://app.dynodoc.online@evil.example"}"#,
            r#"{"version":1,"server":"https://dynodoc.example.org/path"}"#,
            r#"{"version":1,"server":""}"#,
            r#"{"version":1,"server":7}"#,
        ] {
            write(bad);
            // Even with a seal that matches the bytes, an address the validator refuses is nothing.
            std::fs::write(seal_in(&data), seal_of(bad.as_bytes())).unwrap();
            assert_eq!(load(&config, &data), Loaded::Nothing, "{bad}");
        }
        let huge = format!(
            r#"{{"version":1,"server":"https://dynodoc.example.org","pad":"{}"}}"#,
            "a".repeat(MAX_FILE_BYTES as usize)
        );
        write(&huge);
        std::fs::write(seal_in(&data), seal_of(huge.as_bytes())).unwrap();
        assert_eq!(load(&config, &data), Loaded::Nothing);
        // A directory where the file should be is not a crash either.
        std::fs::remove_file(path_in(&config)).unwrap();
        std::fs::create_dir(path_in(&config)).unwrap();
        assert_eq!(load(&config, &data), Loaded::Nothing);
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn unknown_extra_fields_are_tolerated_once_sealed() {
        let (root, config, data) = pair("extra");
        let text = r#"{"version":1,"server":"https://dynodoc.example.org","later":true}"#;
        std::fs::write(path_in(&config), text).unwrap();
        assert!(matches!(load(&config, &data), Loaded::Unconfirmed(_)));
        std::fs::write(seal_in(&data), seal_of(text.as_bytes())).unwrap();
        assert_eq!(
            load(&config, &data),
            Loaded::Confirmed(server("https://dynodoc.example.org"))
        );
        std::fs::remove_dir_all(&root).unwrap();
    }
}
