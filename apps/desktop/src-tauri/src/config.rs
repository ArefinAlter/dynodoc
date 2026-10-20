//! The one setting the app stores: which server to open. A small JSON file in the app's
//! config directory, re-validated every time it is read, written atomically.

use crate::server::{parse_server_address, ServerOrigin};
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

pub const FILE_NAME: &str = "config.json";
/// The file holds one short address; anything bigger is not ours.
pub const MAX_FILE_BYTES: u64 = 4096;

#[derive(Serialize, Deserialize)]
struct Stored {
    version: u32,
    server: String,
}

pub fn path_in(dir: &Path) -> PathBuf {
    dir.join(FILE_NAME)
}

/// The saved server, or None when there is no file, it is too big or damaged, or it names an
/// address the validator refuses (a hand-edited file cannot widen what the window may open).
pub fn load(dir: &Path) -> Option<ServerOrigin> {
    let file = std::fs::File::open(path_in(dir)).ok()?;
    let mut bytes = Vec::new();
    file.take(MAX_FILE_BYTES + 1).read_to_end(&mut bytes).ok()?;
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return None;
    }
    let stored: Stored = serde_json::from_slice(&bytes).ok()?;
    if stored.version != 1 {
        return None;
    }
    parse_server_address(&stored.server).ok()
}

/// Write the file next to its final name, then rename over it, so a crash leaves either
/// the old content or the new, never half of one.
pub fn save(dir: &Path, server: &ServerOrigin) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let stored = Stored {
        version: 1,
        server: server.as_text(),
    };
    let json = serde_json::to_vec_pretty(&stored).map_err(std::io::Error::other)?;
    let temporary = dir.join(format!("{FILE_NAME}.tmp"));
    {
        let mut file = std::fs::File::create(&temporary)?;
        file.write_all(&json)?;
        file.write_all(b"\n")?;
        file.sync_all()?;
    }
    std::fs::rename(&temporary, path_in(dir))
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

    #[test]
    fn a_saved_server_comes_back_and_can_be_replaced() {
        let dir = scratch("roundtrip");
        assert!(load(&dir).is_none());
        let a = parse_server_address("https://dynodoc.example.org:8443").unwrap();
        save(&dir, &a).unwrap();
        assert_eq!(load(&dir), Some(a));
        let b = parse_server_address("https://app.dynodoc.online").unwrap();
        save(&dir, &b).unwrap();
        assert_eq!(load(&dir), Some(b));
        assert!(!dir.join("config.json.tmp").exists());
        let text = std::fs::read_to_string(path_in(&dir)).unwrap();
        assert!(text.contains("\"https://app.dynodoc.online\""), "{text}");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn saving_creates_the_folder() {
        let dir = scratch("create").join("nested").join("config");
        let a = parse_server_address("https://dynodoc.example.org").unwrap();
        save(&dir, &a).unwrap();
        assert_eq!(load(&dir), Some(a));
        std::fs::remove_dir_all(dir.parent().unwrap().parent().unwrap()).unwrap();
    }

    #[test]
    fn hostile_or_damaged_files_are_ignored() {
        let dir = scratch("hostile");
        let write = |text: &str| std::fs::write(path_in(&dir), text).unwrap();
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
            assert!(load(&dir).is_none(), "{bad}");
        }
        let huge = format!(
            r#"{{"version":1,"server":"https://dynodoc.example.org","pad":"{}"}}"#,
            "a".repeat(MAX_FILE_BYTES as usize)
        );
        write(&huge);
        assert!(load(&dir).is_none());
        // A directory where the file should be is not a crash either.
        std::fs::remove_file(path_in(&dir)).unwrap();
        std::fs::create_dir(path_in(&dir)).unwrap();
        assert!(load(&dir).is_none());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn unknown_extra_fields_are_tolerated() {
        let dir = scratch("extra");
        std::fs::write(
            path_in(&dir),
            r#"{"version":1,"server":"https://dynodoc.example.org","later":true}"#,
        )
        .unwrap();
        assert_eq!(
            load(&dir).map(|s| s.as_text()),
            Some("https://dynodoc.example.org".to_string())
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
