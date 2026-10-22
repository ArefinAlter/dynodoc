//! One mutating dynodoctl at a time per bundle directory. `state/.lock` is created exclusively and
//! holds the owner's process id; a second run fails at once with who holds it. A lock whose process
//! no longer exists (a crash, a power cut) is stale and is replaced.

use crate::ctx::Layout;
use crate::error::{CliError, Result};
use crate::util::{create_private_dir, iso};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;

pub struct Lock {
    path: PathBuf,
    pid: u32,
}

/// Is a process with this id running? (Used only to recognise a stale lock.)
pub fn process_alive(pid: u32) -> bool {
    if pid == std::process::id() {
        return true;
    }
    #[cfg(unix)]
    {
        if pid == 0 || pid > i32::MAX as u32 {
            return false;
        }
        // SAFETY: signal 0 only checks that the process exists and may be signalled.
        let rc = unsafe { libc::kill(pid as libc::pid_t, 0) };
        rc == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
    }
    #[cfg(windows)]
    {
        use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, ERROR_ACCESS_DENIED};
        use windows_sys::Win32::System::Threading::{
            OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
        };
        // SAFETY: OpenProcess has no preconditions; a returned handle is closed at once.
        unsafe {
            let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if handle.is_null() {
                return GetLastError() == ERROR_ACCESS_DENIED;
            }
            CloseHandle(handle);
            true
        }
    }
    #[cfg(not(any(unix, windows)))]
    {
        true
    }
}

fn read_pid(path: &PathBuf) -> Option<(u32, String)> {
    let text = std::fs::read_to_string(path).ok()?;
    let pid = text
        .lines()
        .find_map(|l| l.strip_prefix("pid=")?.trim().parse().ok())?;
    let started = text
        .lines()
        .find_map(|l| l.strip_prefix("started="))
        .unwrap_or("")
        .to_string();
    Some((pid, started))
}

impl Lock {
    pub fn acquire(
        layout: &Layout,
        command: &str,
        now: chrono::DateTime<chrono::Utc>,
    ) -> Result<Lock> {
        create_private_dir(&layout.state_dir())?;
        let path = layout.state_dir().join(".lock");
        let pid = std::process::id();
        for attempt in 0..2 {
            match OpenOptions::new().write(true).create_new(true).open(&path) {
                Ok(mut file) => {
                    let _ = write!(file, "pid={pid}\nstarted={}\ncommand={command}\n", iso(now));
                    return Ok(Lock { path, pid });
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                    match read_pid(&path) {
                        Some((other, started)) if process_alive(other) => {
                            return Err(CliError::refused(
                                format!("another dynodoctl is already running in {} (process {other}, started {started})", layout.dir.display()),
                                "wait for it to finish; two runs at once could damage the installation. If you are sure it is gone, delete state/.lock",
                            ));
                        }
                        _ if attempt == 0 => {
                            // A lock left by a process that no longer exists (or an unreadable one).
                            let _ = std::fs::remove_file(&path);
                        }
                        _ => {}
                    }
                }
                Err(e) => return Err(e.into()),
            }
        }
        Err(CliError::refused(
            "could not take the run lock (state/.lock)",
            "check that no other dynodoctl is running here, then delete state/.lock",
        ))
    }
}

impl Drop for Lock {
    fn drop(&mut self) {
        // Remove the file only if it is still ours.
        if read_pid(&self.path).map(|(p, _)| p) == Some(self.pid) {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testkit::fixed_now;

    #[test]
    fn a_second_run_in_the_same_directory_fails_at_once_and_says_who_holds_the_lock() {
        let dir = tempfile::tempdir().unwrap();
        let layout = Layout::new(dir.path());
        let first = Lock::acquire(&layout, "backup", fixed_now()).unwrap();
        let err = Lock::acquire(&layout, "restore", fixed_now())
            .err()
            .expect("refused");
        assert_eq!(err.code(), crate::error::exit::REFUSED);
        assert!(
            err.what.contains("already running")
                && err.what.contains(&std::process::id().to_string()),
            "{err}"
        );
        drop(first);
        assert!(
            !dir.path().join("state").join(".lock").exists(),
            "released on drop"
        );
        Lock::acquire(&layout, "restore", fixed_now()).expect("free again");
    }

    #[test]
    fn a_lock_left_by_a_process_that_is_gone_is_replaced() {
        let dir = tempfile::tempdir().unwrap();
        let layout = Layout::new(dir.path());
        std::fs::create_dir_all(layout.state_dir()).unwrap();
        // A process id that cannot exist.
        std::fs::write(
            layout.state_dir().join(".lock"),
            "pid=4294967294\nstarted=2020-01-01T00:00:00Z\n",
        )
        .unwrap();
        let held = Lock::acquire(&layout, "backup", fixed_now()).expect("stale lock replaced");
        let text = std::fs::read_to_string(layout.state_dir().join(".lock")).unwrap();
        assert!(
            text.contains(&format!("pid={}", std::process::id())),
            "{text}"
        );
        drop(held);
    }

    #[test]
    fn an_unreadable_lock_file_is_treated_as_stale_not_as_a_live_owner() {
        let dir = tempfile::tempdir().unwrap();
        let layout = Layout::new(dir.path());
        std::fs::create_dir_all(layout.state_dir()).unwrap();
        std::fs::write(layout.state_dir().join(".lock"), "garbage").unwrap();
        assert!(Lock::acquire(&layout, "backup", fixed_now()).is_ok());
    }

    #[test]
    fn this_process_is_alive_and_an_impossible_one_is_not() {
        assert!(process_alive(std::process::id()));
        assert!(!process_alive(4_294_967_294));
    }

    #[test]
    fn a_lock_taken_over_by_another_owner_is_not_removed_by_the_first() {
        let dir = tempfile::tempdir().unwrap();
        let layout = Layout::new(dir.path());
        let held = Lock::acquire(&layout, "backup", fixed_now()).unwrap();
        std::fs::write(layout.state_dir().join(".lock"), "pid=1\nstarted=x\n").unwrap();
        drop(held);
        assert!(
            layout.state_dir().join(".lock").exists(),
            "someone else's lock stays"
        );
    }
}
