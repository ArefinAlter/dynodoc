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

    /// Like [`confirm`](Self::confirm), but only a person at a terminal can give it: a phrase passed
    /// in advance with `--confirm` does not count, and neither does text piped in. For acts whose
    /// result is a credential shown on the screen (`admin-link`), where a script must not be able to
    /// say "yes" for the person. (Test doubles answer from their script, as with `confirm`.)
    fn confirm_interactive(&self, prompt: &str, phrase: &str) -> bool {
        self.confirm(prompt, phrase)
    }
}

/// Phrases passed with `--confirm`, else a prompt on a terminal, else refusal. Whether input is a
/// terminal, and where the answer is read from, are injected so the refusal without a terminal is
/// testable (a script must never be able to confirm by piping text in).
pub struct Interactive {
    pub provided: Vec<String>,
    terminal: bool,
    /// Standard output is a terminal too (for [`Confirm::confirm_interactive`]: the credential it
    /// leads to is printed there).
    stdout_terminal: bool,
    input: RefCell<Box<dyn BufRead>>,
}

impl Interactive {
    /// The real thing: standard input, and whether it is a terminal.
    pub fn stdio(provided: Vec<String>) -> Self {
        Self {
            provided,
            terminal: std::io::stdin().is_terminal(),
            stdout_terminal: std::io::stdout().is_terminal(),
            input: RefCell::new(Box::new(std::io::stdin().lock())),
        }
    }

    /// For tests: say whether the input is a terminal and what it would answer.
    pub fn with_input(provided: Vec<String>, terminal: bool, input: Box<dyn BufRead>) -> Self {
        Self {
            provided,
            terminal,
            stdout_terminal: terminal,
            input: RefCell::new(input),
        }
    }

    /// For tests: input is a terminal but standard output is not (`dynodoctl ... > file`).
    pub fn with_redirected_output(mut self) -> Self {
        self.stdout_terminal = false;
        self
    }

    /// Show the prompt, read one line, and say whether it is exactly `phrase`.
    fn ask(&self, prompt: &str, phrase: &str) -> bool {
        println!("{prompt}");
        print!("Type exactly `{phrase}` to continue, anything else cancels: ");
        let _ = std::io::stdout().flush();
        let mut line = String::new();
        if self.input.borrow_mut().read_line(&mut line).is_err() {
            return false;
        }
        line.trim() == phrase
    }
}

impl Confirm for Interactive {
    fn confirm(&self, prompt: &str, phrase: &str) -> bool {
        if self.provided.iter().any(|p| p == phrase) {
            return true;
        }
        if !self.terminal {
            return false;
        }
        self.ask(prompt, phrase)
    }

    fn confirm_interactive(&self, prompt: &str, phrase: &str) -> bool {
        // `provided` (--confirm) is deliberately not consulted.
        if !(self.terminal && self.stdout_terminal) {
            return false;
        }
        self.ask(prompt, phrase)
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
    /// Random text for names that must not collide (scratch databases). Fixed in tests.
    pub nonce: fn() -> String,
    /// Set by an interrupt (Ctrl-C, SIGTERM) or by a test: destructive sequences stop at the next
    /// step boundary and unwind. See `signals`.
    pub interrupt: std::sync::Arc<std::sync::atomic::AtomicBool>,
    /// Progress goes to standard error when standard output carries JSON.
    pub progress_to_stderr: bool,
}

fn random_nonce() -> String {
    crate::util::random_hex(4)
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
            nonce: random_nonce,
            interrupt: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            progress_to_stderr: false,
        }
    }

    /// A name for a database this run creates: `<prefix><UTC time>_<random>`, lowercase.
    pub fn scratch_name(&self, prefix: &str) -> String {
        format!(
            "{prefix}{}_{}",
            crate::util::stamp((self.now)()).to_ascii_lowercase(),
            (self.nonce)()
        )
    }

    /// Has the operator asked to stop (a signal), or has a test asked for it?
    pub fn interrupted(&self) -> bool {
        self.interrupt.load(std::sync::atomic::Ordering::SeqCst) || crate::signals::signalled()
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

    fn answers(text: &str) -> Box<dyn BufRead> {
        Box::new(std::io::Cursor::new(text.as_bytes().to_vec()))
    }

    #[test]
    fn a_phrase_given_in_advance_confirms_without_a_terminal() {
        let confirm = Interactive::with_input(vec!["I HAVE NO BACKUP".into()], false, answers(""));
        assert!(confirm.confirm("p", "I HAVE NO BACKUP"));
        assert!(
            !confirm.confirm("p", "SKIP REHEARSAL"),
            "only the phrase that was given"
        );
    }

    #[test]
    fn text_piped_into_a_non_terminal_never_confirms() {
        // M10: the right phrase is on the input, but the input is not a terminal: refused.
        let confirm = Interactive::with_input(vec![], false, answers("REPLACE LIVE DATA\n"));
        assert!(!confirm.confirm("p", "REPLACE LIVE DATA"));
    }

    #[test]
    fn an_interactive_confirmation_ignores_phrases_given_in_advance_and_needs_two_terminals() {
        // The phrase is on the command line and the input would say it: still no, for a
        // credential that is shown on the screen.
        let advance = Interactive::with_input(
            vec!["me@example.org".into()],
            false,
            answers(
                "me@example.org
",
            ),
        );
        assert!(advance.confirm("p", "me@example.org"));
        assert!(!advance.confirm_interactive("p", "me@example.org"));
        // A terminal for input, but the output redirected: no.
        let redirected = Interactive::with_input(
            vec![],
            true,
            answers(
                "me@example.org
",
            ),
        )
        .with_redirected_output();
        assert!(!redirected.confirm_interactive("p", "me@example.org"));
        // On a terminal, only the exact phrase.
        assert!(Interactive::with_input(
            vec![],
            true,
            answers(
                "me@example.org
"
            )
        )
        .confirm_interactive("p", "me@example.org"));
        assert!(!Interactive::with_input(
            vec![],
            true,
            answers(
                "ME@example.org
"
            )
        )
        .confirm_interactive("p", "me@example.org"));
        assert!(!Interactive::with_input(vec![], true, answers("")).confirm_interactive("p", "x"));
    }

    #[test]
    fn on_a_terminal_only_the_exact_phrase_confirms() {
        assert!(
            Interactive::with_input(vec![], true, answers("REPLACE LIVE DATA\n"))
                .confirm("p", "REPLACE LIVE DATA")
        );
        assert!(
            !Interactive::with_input(vec![], true, answers("replace live data\n"))
                .confirm("p", "REPLACE LIVE DATA")
        );
        assert!(!Interactive::with_input(vec![], true, answers("yes\n"))
            .confirm("p", "REPLACE LIVE DATA"));
        assert!(
            !Interactive::with_input(vec![], true, answers("")).confirm("p", "REPLACE LIVE DATA"),
            "end of input is a no"
        );
    }
}
