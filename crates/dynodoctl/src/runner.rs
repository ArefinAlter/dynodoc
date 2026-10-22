//! The one place that starts other programs. Everything else builds a [`Cmd`] and hands it to a
//! [`Runner`], so tests run the whole tool against [`FakeRunner`] without Docker.

use std::cell::RefCell;
use std::fs::File;
use std::io::{self, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};

/// A program to run. Plain data, so a plan can be built, shown (`--dry-run`) and tested.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Cmd {
    pub program: String,
    pub args: Vec<String>,
    pub cwd: Option<PathBuf>,
    /// Extra environment variables (never a secret: secrets are read inside the containers).
    pub env: Vec<(String, String)>,
    /// Remove the variables the bundle's `.env` is meant to control from the inherited
    /// environment (`DYNODOC_*`, `COMPOSE_*`, `ORIGIN`, ...), so a stray shell variable cannot
    /// override the installation's settings. Variables in `env` are applied afterwards.
    pub scrub_env: bool,
    /// Feed this file to the program's standard input.
    pub stdin_file: Option<PathBuf>,
    /// Feed these bytes to the program's standard input.
    pub stdin_bytes: Option<Vec<u8>>,
    /// Write the program's standard output to this file instead of capturing it.
    pub stdout_file: Option<PathBuf>,
    /// Start the program in its own process group so the terminal's Ctrl-C does not reach it:
    /// a destructive step then finishes (or fails by itself) instead of being cut off half way,
    /// and dynodoctl unwinds at the next step boundary (see `signals`).
    pub shield: bool,
}

impl Cmd {
    pub fn new(program: &str, args: &[&str]) -> Self {
        Self {
            program: program.to_string(),
            args: args.iter().map(|a| a.to_string()).collect(),
            ..Self::default()
        }
    }

    /// The command as one line, for matching in tests and for display in plans.
    pub fn line(&self) -> String {
        let mut parts = vec![self.program.clone()];
        parts.extend(self.args.iter().cloned());
        parts.join(" ")
    }
}

/// What a finished program left behind.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Output {
    pub status: i32,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

impl Output {
    pub fn ok(&self) -> bool {
        self.status == 0
    }
    pub fn stdout_text(&self) -> String {
        String::from_utf8_lossy(&self.stdout).into_owned()
    }
    pub fn stderr_text(&self) -> String {
        String::from_utf8_lossy(&self.stderr).into_owned()
    }
    /// Standard output and error together, trimmed (for messages).
    pub fn text(&self) -> String {
        format!("{}{}", self.stdout_text(), self.stderr_text())
            .trim()
            .to_string()
    }
}

pub trait Runner {
    /// Run to completion. `Err` only when the program could not be started at all.
    fn run(&self, cmd: &Cmd) -> io::Result<Output>;
}

/// Variables the bundle's `.env` is meant to control.
pub fn is_bundle_variable(name: &str) -> bool {
    let upper = name.to_ascii_uppercase();
    ["DYNODOC_", "COMPOSE_", "SMTP_"]
        .iter()
        .any(|p| upper.starts_with(p))
        || matches!(
            upper.as_str(),
            "ORIGIN"
                | "DEV_AUTH"
                | "ADMIN_EMAILS"
                | "RUST_LOG"
                | "SESSION_SECRET"
                | "WEB_SERVICE_KEY"
                | "AUTH_EMAIL_LINK"
        )
}

pub struct SystemRunner;

impl Runner for SystemRunner {
    fn run(&self, cmd: &Cmd) -> io::Result<Output> {
        let mut command = Command::new(&cmd.program);
        command.args(&cmd.args);
        if let Some(cwd) = &cmd.cwd {
            command.current_dir(cwd);
        }
        if cmd.scrub_env {
            for (name, _) in std::env::vars() {
                if is_bundle_variable(&name) {
                    command.env_remove(name);
                }
            }
        }
        for (name, value) in &cmd.env {
            command.env(name, value);
        }
        command.stderr(Stdio::piped());
        if cmd.shield {
            #[cfg(unix)]
            {
                use std::os::unix::process::CommandExt;
                command.process_group(0);
            }
            #[cfg(windows)]
            {
                use std::os::windows::process::CommandExt;
                command.creation_flags(0x0000_0200); // CREATE_NEW_PROCESS_GROUP
            }
        }
        match &cmd.stdout_file {
            Some(path) => {
                command.stdout(Stdio::from(File::create(path)?));
            }
            None => {
                command.stdout(Stdio::piped());
            }
        }
        let mut pending_stdin: Option<Vec<u8>> = None;
        if let Some(path) = &cmd.stdin_file {
            command.stdin(Stdio::from(File::open(path)?));
        } else if let Some(bytes) = &cmd.stdin_bytes {
            command.stdin(Stdio::piped());
            pending_stdin = Some(bytes.clone());
        } else {
            command.stdin(Stdio::null());
        }
        let mut child = command.spawn()?;
        let writer = pending_stdin.and_then(|bytes| {
            child.stdin.take().map(|mut stdin| {
                std::thread::spawn(move || {
                    let _ = stdin.write_all(&bytes);
                })
            })
        });
        let output = child.wait_with_output()?;
        if let Some(handle) = writer {
            let _ = handle.join();
        }
        Ok(Output {
            status: output.status.code().unwrap_or(-1),
            stdout: output.stdout,
            stderr: output.stderr,
        })
    }
}

type Effect = Box<dyn Fn(&Cmd)>;

struct Rule {
    needle: String,
    outputs: Vec<Output>,
    used: usize,
    file_bytes: Option<Vec<u8>>,
}

/// A scripted [`Runner`] for tests: records every command and answers by substring match of
/// the command line (first matching rule wins; later calls repeat the last scripted answer).
pub struct FakeRunner {
    calls: RefCell<Vec<Cmd>>,
    rules: RefCell<Vec<Rule>>,
    effects: RefCell<Vec<(String, Effect)>>,
    default: Output,
}

impl Default for FakeRunner {
    fn default() -> Self {
        Self::new()
    }
}

impl FakeRunner {
    /// Unmatched commands succeed with no output.
    pub fn new() -> Self {
        Self {
            calls: RefCell::new(Vec::new()),
            rules: RefCell::new(Vec::new()),
            effects: RefCell::new(Vec::new()),
            default: Output::default(),
        }
    }

    /// Run `effect` (for example, create the files a real generator would) whenever a command
    /// line contains `needle`. Effects run in addition to the answering rule.
    pub fn on_effect(&self, needle: &str, effect: impl Fn(&Cmd) + 'static) -> &Self {
        self.effects
            .borrow_mut()
            .push((needle.to_string(), Box::new(effect)));
        self
    }

    pub fn on(&self, needle: &str, status: i32, stdout: &str) -> &Self {
        self.on_seq(needle, &[(status, stdout)])
    }

    /// Like [`on`](Self::on) but checked before every rule registered so far, so a test can
    /// override the canned answers of a shared runner.
    pub fn first(&self, needle: &str, status: i32, stdout: &str) -> &Self {
        self.rules.borrow_mut().insert(
            0,
            Rule {
                needle: needle.to_string(),
                outputs: vec![Output {
                    status,
                    stdout: stdout.as_bytes().to_vec(),
                    stderr: Vec::new(),
                }],
                used: 0,
                file_bytes: None,
            },
        );
        self
    }

    pub fn on_seq(&self, needle: &str, answers: &[(i32, &str)]) -> &Self {
        self.rules.borrow_mut().push(Rule {
            needle: needle.to_string(),
            outputs: answers
                .iter()
                .map(|(status, text)| Output {
                    status: *status,
                    stdout: text.as_bytes().to_vec(),
                    stderr: Vec::new(),
                })
                .collect(),
            used: 0,
            file_bytes: None,
        });
        self
    }

    /// Answer a command that writes to `stdout_file` with these bytes.
    pub fn on_file(&self, needle: &str, bytes: &[u8]) -> &Self {
        self.rules.borrow_mut().push(Rule {
            needle: needle.to_string(),
            outputs: vec![Output::default()],
            used: 0,
            file_bytes: Some(bytes.to_vec()),
        });
        self
    }

    pub fn calls(&self) -> Vec<Cmd> {
        self.calls.borrow().clone()
    }

    pub fn lines(&self) -> Vec<String> {
        self.calls.borrow().iter().map(Cmd::line).collect()
    }

    /// Did any command line contain `needle`?
    pub fn ran(&self, needle: &str) -> bool {
        self.calls
            .borrow()
            .iter()
            .any(|c| c.line().contains(needle))
    }

    pub fn count(&self, needle: &str) -> usize {
        self.calls
            .borrow()
            .iter()
            .filter(|c| c.line().contains(needle))
            .count()
    }

    /// Index of the first command containing `needle`, for ordering assertions.
    pub fn position(&self, needle: &str) -> Option<usize> {
        self.calls
            .borrow()
            .iter()
            .position(|c| c.line().contains(needle))
    }
}

impl Runner for FakeRunner {
    fn run(&self, cmd: &Cmd) -> io::Result<Output> {
        self.calls.borrow_mut().push(cmd.clone());
        let line = cmd.line();
        for (needle, effect) in self.effects.borrow().iter() {
            if line.contains(needle.as_str()) {
                effect(cmd);
            }
        }
        let mut rules = self.rules.borrow_mut();
        for rule in rules.iter_mut() {
            if line.contains(&rule.needle) {
                let index = rule.used.min(rule.outputs.len() - 1);
                rule.used += 1;
                if let (Some(bytes), Some(path)) = (&rule.file_bytes, &cmd.stdout_file) {
                    std::fs::write(path, bytes)?;
                }
                return Ok(rule.outputs[index].clone());
            }
        }
        if let Some(path) = &cmd.stdout_file {
            std::fs::write(path, b"")?;
        }
        Ok(self.default.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fake_runner_matches_in_order_and_records() {
        let fake = FakeRunner::new();
        fake.on("compose ps", 0, "one").on("compose", 3, "other");
        let a = fake.run(&Cmd::new("docker", &["compose", "ps"])).unwrap();
        let b = fake.run(&Cmd::new("docker", &["compose", "up"])).unwrap();
        assert_eq!((a.status, a.stdout_text()), (0, "one".to_string()));
        assert_eq!((b.status, b.stdout_text()), (3, "other".to_string()));
        assert_eq!(fake.lines(), vec!["docker compose ps", "docker compose up"]);
        assert_eq!(fake.position("compose up"), Some(1));
    }

    #[test]
    fn fake_runner_sequences_repeat_the_last_answer() {
        let fake = FakeRunner::new();
        fake.on_seq("x", &[(1, "first"), (0, "second")]);
        let answers: Vec<i32> = (0..3)
            .map(|_| fake.run(&Cmd::new("x", &[])).unwrap().status)
            .collect();
        assert_eq!(answers, vec![1, 0, 0]);
    }

    #[test]
    fn bundle_variables_are_scrubbed_but_docker_ones_are_not() {
        assert!(is_bundle_variable("DYNODOC_TLS_MODE"));
        assert!(is_bundle_variable("COMPOSE_PROFILES"));
        assert!(is_bundle_variable("ORIGIN"));
        assert!(is_bundle_variable("smtp_host"));
        assert!(!is_bundle_variable("DOCKER_HOST"));
        assert!(!is_bundle_variable("PATH"));
    }

    #[test]
    fn system_runner_reports_a_missing_program_as_an_error() {
        assert!(SystemRunner
            .run(&Cmd::new("dynodoctl-no-such-program-xyz", &[]))
            .is_err());
    }
}
