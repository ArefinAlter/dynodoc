//! Private local files for key material and credentials: exclusive creation,
//! owner-only mode where the platform supports it, fsync, and atomic replacement.
//! A crash or full disk never leaves a half-written credential under the target
//! name, and a failed replacement leaves the previous file intact.
use anyhow::{anyhow, Context};
use std::{
    fs::{File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};
use uuid::Uuid;

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

/// Create a new file that must not exist, mode 0600 on Unix. On Windows the file
/// inherits its folder's ACL: keep credentials in a private directory.
pub fn create_new(path: &Path) -> std::io::Result<File> {
    options().open(path)
}

/// Write a complete new file exclusively. A partial file is removed on failure.
pub fn write_new(path: &Path, bytes: &[u8]) -> anyhow::Result<()> {
    let mut file = create_new(path).with_context(|| {
        format!(
            "cannot create {}; an existing file is never overwritten",
            path.display()
        )
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
