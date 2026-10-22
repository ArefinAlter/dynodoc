//! Errors and exit codes. Every message says what happened and what to do next.

use std::fmt;

/// Process exit codes, documented in docs/SELF-HOSTED-OPERATIONS.md.
pub mod exit {
    /// The command did what it was asked and every check passed.
    pub const OK: i32 = 0;
    /// The command ran and a check or step failed (unhealthy service, failed verification, a
    /// failed restore). Something may have been changed; the message says what.
    pub const FAILED: i32 = 1;
    /// The arguments were wrong (same code clap uses).
    pub const USAGE: i32 = 2;
    /// The command refused before changing anything: a precondition was not met or a required
    /// confirmation was missing.
    pub const REFUSED: i32 = 3;
    /// The command finished and found only warnings (`status`, `verify-delivery`).
    pub const WARNINGS: i32 = 4;
}

/// How a failed command ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Failed,
    Refused,
    Usage,
}

impl Kind {
    pub fn code(self) -> i32 {
        match self {
            Kind::Failed => exit::FAILED,
            Kind::Refused => exit::REFUSED,
            Kind::Usage => exit::USAGE,
        }
    }
}

/// An error with a plain-words explanation and a next step.
#[derive(Debug, Clone)]
pub struct CliError {
    pub kind: Kind,
    pub what: String,
    pub todo: String,
}

pub type Result<T> = std::result::Result<T, CliError>;

impl CliError {
    pub fn failed(what: impl Into<String>, todo: impl Into<String>) -> Self {
        Self {
            kind: Kind::Failed,
            what: what.into(),
            todo: todo.into(),
        }
    }
    pub fn refused(what: impl Into<String>, todo: impl Into<String>) -> Self {
        Self {
            kind: Kind::Refused,
            what: what.into(),
            todo: todo.into(),
        }
    }
    pub fn usage(what: impl Into<String>, todo: impl Into<String>) -> Self {
        Self {
            kind: Kind::Usage,
            what: what.into(),
            todo: todo.into(),
        }
    }
    pub fn code(&self) -> i32 {
        self.kind.code()
    }
}

impl fmt::Display for CliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.what)?;
        if !self.todo.is_empty() {
            write!(f, "\n  What to do: {}", self.todo)?;
        }
        Ok(())
    }
}

impl std::error::Error for CliError {}

impl From<std::io::Error> for CliError {
    fn from(error: std::io::Error) -> Self {
        CliError::failed(
            format!("file system error: {error}"),
            "check the path, the permissions and the free disk space, then run the command again",
        )
    }
}

impl From<serde_json::Error> for CliError {
    fn from(error: serde_json::Error) -> Self {
        CliError::failed(
            format!("could not read or write JSON: {error}"),
            "the file may be damaged; do not edit it by hand",
        )
    }
}
