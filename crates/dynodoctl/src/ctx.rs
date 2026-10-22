//! The installation directory layout and the context every command runs in.

use crate::envfile::EnvFile;
use crate::error::{CliError, Result};
use crate::runner::Runner;
use chrono::{DateTime, Utc};
use std::cell::RefCell;
use std::io::{BufRead, IsTerminal, Write};
use std::path::{Path, PathBuf};

pub const DEFAULT_PROJECT: &str = "dynodoc-selfhosted";
pub const OVERLAY_NAME: &str = "compose.release.generated.yml";

/// The bundle directory: the directory that holds `compose.yml`, `.env`, `secrets/`.
#[derive(Clone, Debug)]
pub struct Layout {
    pub dir: PathBuf,
}

impl Layout {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }
    pub fn compose_file(&self) -> PathBuf {
        self.dir.join("compose.yml")
    }
    pub fn overlay_file(&self) -> PathBuf {
        self.dir.join(OVERLAY_NAME)
    }
    pub fn env_file(&self) -> PathBuf {
        self.dir.join(".env")
    }
    pub fn secrets_dir(&self) -> PathBuf {
        self.dir.join("secrets")
    }
    pub fn tls_dir(&self) -> PathBuf {
        self.dir.join("tls")
    }
    pub fn config_dir(&self) -> PathBuf {
        self.dir.join("config")
    }
    pub fn backups_dir(&self) -> PathBuf {
        self.dir.join("backups")
    }
    /// Where dynodoctl keeps its own records (upgrade records, dated copies of `.env`).
    pub fn state_dir(&self) -> PathBuf {
        self.dir.join("state")
    }
    pub fn is_bundle(&self) -> bool {
        self.compose_file().is_file()
    }
    pub fn is_installed(&self) -> bool {
        self.env_file().is_file() && self.secrets_dir().is_dir()
    }
    pub fn read_env(&self) -> Result<EnvFile> {
        EnvFile::read(&self.env_file())
    }
    /// `compose.yml`, the release overlay if one is installed, then any extra files.
    pub fn compose_files(&self, extra: &[PathBuf]) -> Vec<PathBuf> {
        let mut files = vec![self.compose_file()];
        if self.overlay_file().is_file() {
            files.push(self.overlay_file());
        }
        files.extend(extra.iter().cloned());
        files
    }
}

/// Asks the operator to type a phrase before something dangerous.
pub trait Confirm {
    /// True only when the operator gave exactly `phrase`.
    fn confirm(&self, prompt: &str, phrase: &str) -> bool;
}

/// Phrases passed with `--confirm`, else a prompt on a terminal, else refusal.
pub struct Interactive {
    pub provided: Vec<String>,
}

impl Confirm for Interactive {
    fn confirm(&self, prompt: &str, phrase: &str) -> bool {
        if self.provided.iter().any(|p| p == phrase) {
            return true;
        }
        if !std::io::stdin().is_terminal() {
            return false;
        }
        println!("{prompt}");
        print!("Type exactly `{phrase}` to continue, anything else cancels: ");
        let _ = std::io::stdout().flush();
        let mut line = String::new();
        if std::io::stdin().lock().read_line(&mut line).is_err() {
            return false;
        }
        line.trim() == phrase
    }
}

/// Test double: answers from a list and remembers what it was asked.
pub struct ScriptedConfirm {
    answers: RefCell<Vec<bool>>,
    pub asked: RefCell<Vec<String>>,
}

impl ScriptedConfirm {
    pub fn new(answers: &[bool]) -> Self {
        Self {
            answers: RefCell::new(answers.iter().rev().cloned().collect()),
            asked: RefCell::new(Vec::new()),
        }
    }
}

impl Confirm for ScriptedConfirm {
    fn confirm(&self, _prompt: &str, phrase: &str) -> bool {
        self.asked.borrow_mut().push(phrase.to_string());
        self.answers.borrow_mut().pop().unwrap_or(false)
    }
}

pub struct Ctx<'a> {
    pub runner: &'a dyn Runner,
    pub layout: Layout,
    /// `--project`; the bundle's own name applies when absent.
    pub project: Option<String>,
    /// Extra `-f` files after compose.yml and the release overlay.
    pub extra_files: Vec<PathBuf>,
    pub confirm: &'a dyn Confirm,
    pub now: fn() -> DateTime<Utc>,
    /// Progress goes to standard error when standard output carries JSON.
    pub progress_to_stderr: bool,
}

impl<'a> Ctx<'a> {
    pub fn new(runner: &'a dyn Runner, layout: Layout, confirm: &'a dyn Confirm) -> Self {
        Self {
            runner,
            layout,
            project: None,
            extra_files: Vec::new(),
            confirm,
            now: Utc::now,
            progress_to_stderr: false,
        }
    }

    /// One line of progress.
    pub fn say(&self, text: &str) {
        if self.progress_to_stderr {
            eprintln!("{text}");
        } else {
            println!("{text}");
        }
    }

    pub fn env(&self) -> Result<EnvFile> {
        self.layout.read_env()
    }

    /// The Compose project name: `--project`, else `COMPOSE_PROJECT_NAME` in `.env`, else the bundle's.
    pub fn project_name(&self) -> String {
        if let Some(p) = &self.project {
            return p.clone();
        }
        if let Ok(env) = self.layout.read_env() {
            if let Some(name) = env.get("COMPOSE_PROJECT_NAME") {
                if !name.is_empty() {
                    return name.to_string();
                }
            }
        }
        DEFAULT_PROJECT.to_string()
    }

    pub fn require_bundle(&self) -> Result<()> {
        if !self.layout.is_bundle() {
            return Err(CliError::refused(
                format!("{} has no compose.yml, so it is not a Dynodoc bundle directory", self.layout.dir.display()),
                "run from the bundle directory, or pass --dir with the directory that holds compose.yml",
            ));
        }
        Ok(())
    }

    pub fn require_installed(&self) -> Result<()> {
        self.require_bundle()?;
        if !self.layout.is_installed() {
            return Err(CliError::refused(
                format!("{} has no .env and secrets/ yet", self.layout.dir.display()),
                "run `dynodoctl install` first",
            ));
        }
        Ok(())
    }
}

/// `path` relative to `base` for display, if it is inside it.
pub fn display_rel(base: &Path, path: &Path) -> String {
    path.strip_prefix(base)
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| path.display().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runner::FakeRunner;

    #[test]
    fn layout_paths_follow_the_bundle() {
        let layout = Layout::new("/srv/dynodoc");
        assert!(layout.secrets_dir().ends_with("secrets"));
        assert!(layout.backups_dir().ends_with("backups"));
        assert!(!layout.is_bundle());
    }

    #[test]
    fn compose_files_include_the_overlay_only_when_present() {
        let dir = tempfile::tempdir().unwrap();
        let layout = Layout::new(dir.path());
        assert_eq!(layout.compose_files(&[]).len(), 1);
        std::fs::write(layout.overlay_file(), "services: {}\n").unwrap();
        assert_eq!(layout.compose_files(&[PathBuf::from("extra.yml")]).len(), 3);
    }

    #[test]
    fn project_name_prefers_flag_then_env_then_default() {
        let dir = tempfile::tempdir().unwrap();
        let runner = FakeRunner::new();
        let confirm = ScriptedConfirm::new(&[]);
        let mut ctx = Ctx::new(&runner, Layout::new(dir.path()), &confirm);
        assert_eq!(ctx.project_name(), DEFAULT_PROJECT);
        std::fs::write(dir.path().join(".env"), "COMPOSE_PROJECT_NAME=mine\n").unwrap();
        assert_eq!(ctx.project_name(), "mine");
        ctx.project = Some("flag".into());
        assert_eq!(ctx.project_name(), "flag");
    }

    #[test]
    fn scripted_confirm_records_the_phrase_asked() {
        let confirm = ScriptedConfirm::new(&[false, true]);
        assert!(!confirm.confirm("p", "A"));
        assert!(confirm.confirm("p", "B"));
        assert_eq!(*confirm.asked.borrow(), vec!["A", "B"]);
    }

    #[test]
    fn provided_phrase_confirms_without_a_terminal() {
        let confirm = Interactive {
            provided: vec!["I HAVE NO BACKUP".into()],
        };
        assert!(confirm.confirm("p", "I HAVE NO BACKUP"));
        assert!(
            !Interactive { provided: vec![] }.confirm("p", "x") || std::io::stdin().is_terminal()
        );
    }
}
