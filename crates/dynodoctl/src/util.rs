//! Small shared helpers: hashing, private files, time and size formatting, free disk space.

use crate::error::Result;
use chrono::{DateTime, Utc};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::Path;

/// SHA-256 of a byte slice as lowercase hex.
pub fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(ring::digest::digest(&ring::digest::SHA256, bytes))
}

/// SHA-256 and length of a file, read in blocks.
pub fn sha256_file(path: &Path) -> Result<(String, u64)> {
    let mut file = File::open(path)?;
    let mut context = ring::digest::Context::new(&ring::digest::SHA256);
    let mut buffer = vec![0u8; 1 << 20];
    let mut total = 0u64;
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        context.update(&buffer[..n]);
        total += n as u64;
    }
    Ok((hex::encode(context.finish()), total))
}

/// `20261022T101500Z`, used in file names.
pub fn stamp(now: DateTime<Utc>) -> String {
    now.format("%Y%m%dT%H%M%SZ").to_string()
}

/// RFC 3339 with seconds, UTC.
pub fn iso(now: DateTime<Utc>) -> String {
    now.format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

pub fn human_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

pub fn human_duration(millis: u128) -> String {
    if millis < 1000 {
        format!("{millis} ms")
    } else if millis < 120_000 {
        format!("{:.1} s", millis as f64 / 1000.0)
    } else {
        format!("{} min {} s", millis / 60_000, (millis % 60_000) / 1000)
    }
}

/// Create (or truncate) a file readable by its owner only, and write `bytes`.
pub fn write_private(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut options = OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    file.write_all(bytes)?;
    set_private(path)?;
    Ok(())
}

/// Restrict an existing file to its owner (no-op where there are no Unix modes).
pub fn set_private(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
    Ok(())
}

/// Create a directory readable by its owner only.
pub fn create_private_dir(path: &Path) -> Result<()> {
    fs::create_dir_all(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

/// The Unix mode of a file, or None where there is none.
pub fn unix_mode(path: &Path) -> Option<u32> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::metadata(path)
            .ok()
            .map(|m| m.permissions().mode() & 0o777)
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        None
    }
}

/// Free bytes on the file system holding `path`, if the platform tells us.
// `statvfs` field types differ by platform (u32 on macOS, u64 on 64-bit Linux): the casts are
// needed on some targets and redundant on others.
#[allow(clippy::unnecessary_cast)]
pub fn free_bytes(path: &Path) -> Option<u64> {
    #[cfg(unix)]
    {
        use std::ffi::CString;
        use std::os::unix::ffi::OsStrExt;
        let c = CString::new(path.as_os_str().as_bytes()).ok()?;
        // SAFETY: `c` is a valid NUL-terminated path and `stat` is a plain output buffer.
        unsafe {
            let mut stat: libc::statvfs = std::mem::zeroed();
            if libc::statvfs(c.as_ptr(), &mut stat) == 0 {
                return Some((stat.f_bavail as u64).saturating_mul(stat.f_frsize as u64));
            }
        }
        None
    }
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
        let mut wide: Vec<u16> = path.as_os_str().encode_wide().collect();
        wide.push(0);
        let mut free = 0u64;
        let mut total = 0u64;
        let mut total_free = 0u64;
        // SAFETY: `wide` is NUL-terminated and the three outputs are valid for writes.
        let ok =
            unsafe { GetDiskFreeSpaceExW(wide.as_ptr(), &mut free, &mut total, &mut total_free) };
        if ok != 0 {
            Some(free)
        } else {
            None
        }
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = path;
        None
    }
}

/// Quote one argument for display in a copy-paste shell line (display only, never executed).
pub fn shell_quote(arg: &str) -> String {
    if !arg.is_empty()
        && arg
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_./:=@%+,".contains(c))
    {
        arg.to_string()
    } else {
        format!("'{}'", arg.replace('\'', "'\\''"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_of_known_input() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn sizes_and_durations_read_naturally() {
        assert_eq!(human_bytes(512), "512 B");
        assert_eq!(human_bytes(1536), "1.5 KiB");
        assert_eq!(human_duration(250), "250 ms");
        assert_eq!(human_duration(2500), "2.5 s");
        assert_eq!(human_duration(185_000), "3 min 5 s");
    }

    #[test]
    fn private_files_hold_their_content_and_hash() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sub").join("f");
        write_private(&path, b"abc").unwrap();
        let (hash, len) = sha256_file(&path).unwrap();
        assert_eq!(len, 3);
        assert_eq!(hash, sha256_hex(b"abc"));
        #[cfg(unix)]
        assert_eq!(unix_mode(&path), Some(0o600));
    }

    #[test]
    fn quoting_is_display_safe() {
        assert_eq!(shell_quote("a-b"), "a-b");
        assert_eq!(shell_quote("a b"), "'a b'");
        assert_eq!(shell_quote("it's"), "'it'\\''s'");
    }

    #[test]
    fn free_space_is_reported_for_the_temp_directory() {
        let dir = tempfile::tempdir().unwrap();
        assert!(free_bytes(dir.path()).map(|b| b > 0).unwrap_or(true));
    }
}
