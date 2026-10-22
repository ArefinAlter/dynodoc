//! Interrupts. Ctrl-C, SIGTERM (and on Windows Ctrl-Break and a closing console) must not kill a
//! destructive sequence half way: that is how a database ends up renamed away with nothing in its
//! place. The handler only sets a flag. Long sequences check it at step boundaries and unwind to a
//! consistent state (see `restore.rs`); the destructive child processes are started in their own
//! process group (`Cmd.shield`), so the terminal's Ctrl-C does not reach them either.
//!
//! The flag is process-wide for the real binary; tests use `Ctx.interrupt` instead, so they do not
//! disturb each other.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

static SIGNALLED: AtomicBool = AtomicBool::new(false);
static COUNT: AtomicUsize = AtomicUsize::new(0);
/// Only the real binary switches this on (`install`); unit tests never do, so the process-wide
/// flag cannot leak from one test into another.
static HONOUR: AtomicBool = AtomicBool::new(false);

/// Has an interrupt arrived since `install` was called?
pub fn signalled() -> bool {
    HONOUR.load(Ordering::SeqCst) && SIGNALLED.load(Ordering::SeqCst)
}

/// Install the handlers and start honouring them. Called once by the binary.
pub fn install() {
    install_handlers();
    HONOUR.store(true, Ordering::SeqCst);
}

/// How many interrupts arrived (a second one does not stop recovery either; it is only counted).
pub fn count() -> usize {
    COUNT.load(Ordering::SeqCst)
}

fn note() {
    COUNT.fetch_add(1, Ordering::SeqCst);
    SIGNALLED.store(true, Ordering::SeqCst);
}

#[cfg(unix)]
extern "C" fn on_signal(_: libc::c_int) {
    note();
}

#[cfg(unix)]
fn install_handlers() {
    // SAFETY: the handler only touches two atomics, which is async-signal-safe.
    unsafe {
        libc::signal(libc::SIGINT, on_signal as libc::sighandler_t);
        libc::signal(libc::SIGTERM, on_signal as libc::sighandler_t);
        libc::signal(libc::SIGHUP, on_signal as libc::sighandler_t);
    }
}

#[cfg(windows)]
unsafe extern "system" fn on_ctrl(kind: u32) -> i32 {
    // CTRL_C_EVENT 0, CTRL_BREAK_EVENT 1, CTRL_CLOSE_EVENT 2: handled here, the process stays alive
    // until the sequence is consistent.
    if kind <= 2 {
        note();
        1
    } else {
        0
    }
}

#[cfg(windows)]
fn install_handlers() {
    use windows_sys::Win32::System::Console::SetConsoleCtrlHandler;
    // SAFETY: registering a handler that only touches atomics.
    unsafe {
        SetConsoleCtrlHandler(Some(on_ctrl), 1);
    }
}

#[cfg(not(any(unix, windows)))]
fn install_handlers() {}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn a_real_sigint_sets_the_flag_instead_of_killing_the_process() {
        // Only the raw flag is read: the handlers are installed, but `HONOUR` stays off, so no
        // other test can see this interrupt.
        install_handlers();
        SIGNALLED.store(false, Ordering::SeqCst);
        // SAFETY: raising a signal that the installed handler turns into a flag.
        unsafe {
            libc::raise(libc::SIGINT);
        }
        assert!(
            SIGNALLED.load(Ordering::SeqCst),
            "the handler ran and the test is still alive"
        );
        assert!(
            !signalled(),
            "an interrupt is not honoured until the binary asks for it"
        );
        SIGNALLED.store(false, Ordering::SeqCst);
    }
}
