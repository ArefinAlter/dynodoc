use super::*;
use crate::cli;
use crate::ctx::{Confirm, Interactive, ScriptedConfirm};
use crate::net::FakeNet;
use crate::runner::FakeRunner;
use crate::testkit::{self, assert_no_seeded_secret};

/// A token as engine-cli makes it, and the answer it prints with `--json`.
const TOKEN: &str = "5f0c7a39d1b24e86a3c9f01d77e2b84c0a6d93e15f78b2c4d0e91a3b6c85f742";

fn answer_with(link: &str, extra: serde_json::Value) -> String {
    let mut a = serde_json::json!({
        "ok": true,
        "link": link,
        "link_id": "2f3a1c0e-5b7d-4e19-8a60-9c1d4e7f0b32",
        "token_prefix": "e3b0c442",
        "email": "admin@example.org",
        "expires_at": "2026-10-22T12:15:00+00:00",
        "ttl_seconds": 900,
        "created_account": true,
        "administrator": true,
        "superseded": 0,
    });
    for (k, v) in extra.as_object().cloned().unwrap_or_default() {
        a[k] = v;
    }
    format!("{a}\n")
}

fn answer() -> String {
    answer_with(
        &format!("https://docs.example.org/login?token={TOKEN}"),
        serde_json::json!({}),
    )
}

fn needs(why: &str) -> String {
    format!("{{\"ok\":false,\"needs_confirmation\":\"{why}\"}}\n")
}

fn installation() -> (tempfile::TempDir, FakeRunner) {
    let dir = tempfile::tempdir().unwrap();
    testkit::installed(dir.path(), testkit::ENV_TEXT);
    (dir, testkit::healthy_runner())
}

fn run_cli(
    args: &[&str],
    dir: &std::path::Path,
    runner: &FakeRunner,
    confirm: &dyn Confirm,
) -> (i32, String, String) {
    let mut full: Vec<String> = vec![
        "dynodoctl".into(),
        "--dir".into(),
        dir.display().to_string(),
    ];
    full.extend(args.iter().map(|a| a.to_string()));
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let code = cli::run(
        full,
        runner,
        &|_| Box::new(FakeNet::default()),
        Some(confirm),
        &mut out,
        &mut err,
    );
    (
        code,
        String::from_utf8(out).unwrap(),
        String::from_utf8(err).unwrap(),
    )
}

fn issue_calls(runner: &FakeRunner) -> Vec<String> {
    runner
        .lines()
        .into_iter()
        .filter(|l| l.contains("auth issue-link"))
        .collect()
}

/// Every file under `dir`, read as bytes: does any of them hold `needle`?
fn some_file_holds(dir: &std::path::Path, needle: &str) -> Option<std::path::PathBuf> {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            if let Some(found) = some_file_holds(&path, needle) {
                return Some(found);
            }
        } else if String::from_utf8_lossy(&std::fs::read(&path).unwrap()).contains(needle) {
            return Some(path);
        }
    }
    None
}

#[test]
fn the_link_is_shown_once_after_the_address_is_typed_and_is_saved_nowhere() {
    let (dir, runner) = installation();
    runner.first("auth issue-link", 0, &answer());
    let confirm = ScriptedConfirm::new(&[true]);
    let (code, out, err) = run_cli(
        &[
            "admin-link",
            "--email",
            "  Admin@Example.org ",
            "--create",
            "--reason",
            "first administrator",
        ],
        dir.path(),
        &runner,
        &confirm,
    );
    assert_eq!(code, 0, "{err}");
    let link = format!("https://docs.example.org/login?token={TOKEN}");
    assert_eq!(out.matches(&link).count(), 1, "shown once: {out}");
    assert_eq!(out.matches(TOKEN).count(), 1);
    assert!(!err.contains(TOKEN), "not on standard error");
    assert!(
        out.contains("Works once, in any browser") && out.contains("administrator"),
        "{out}"
    );
    assert!(
        out.contains("auth revoke-link e3b0c442"),
        "how to end it: {out}"
    );
    // The person was asked for the normalised address, before anything was made.
    assert_eq!(*confirm.asked.borrow(), ["admin@example.org"]);
    // The container was asked with the address, the origin and the administrators from .env.
    let calls = issue_calls(&runner);
    assert_eq!(calls.len(), 1);
    let call = &calls[0];
    for part in [
        "database-admin",
        "sh dynodoc owner auth issue-link --json --email admin@example.org",
        "--origin https://docs.example.org",
        "--admin-emails admin@example.org",
        "--operator ",
        "--reason first administrator",
        "--create",
    ] {
        assert!(call.contains(part), "{part}: {call}");
    }
    assert!(!call.contains("--confirm-create"));
    // The token is in no command line, no file of the installation, and no seeded secret leaked.
    assert!(runner.lines().iter().all(|l| !l.contains(TOKEN)));
    assert_eq!(
        some_file_holds(dir.path(), TOKEN),
        None,
        "a file holds the link"
    );
    assert_no_seeded_secret(&out, "admin-link output");
    // The lock was released and left no record of the link.
    assert!(!dir.path().join("state").join(".lock").exists());
}

#[test]
fn nothing_is_made_unless_a_person_types_the_address() {
    let (dir, runner) = installation();
    runner.first("auth issue-link", 0, &answer());
    // "No" from the person.
    let (code, out, _) = run_cli(
        &["admin-link", "--email", "admin@example.org"],
        dir.path(),
        &runner,
        &ScriptedConfirm::new(&[false]),
    );
    assert_eq!(code, crate::error::exit::REFUSED);
    assert!(!out.contains(TOKEN));
    assert!(issue_calls(&runner).is_empty(), "no container ran");
    // A phrase passed in advance (--confirm), with input that is not a terminal: still no.
    let advance = Interactive::with_input(
        vec!["admin@example.org".into()],
        false,
        Box::new(std::io::Cursor::new(b"admin@example.org\n".to_vec())),
    );
    let (code, out, err) = run_cli(
        &[
            "--confirm",
            "admin@example.org",
            "admin-link",
            "--email",
            "admin@example.org",
        ],
        dir.path(),
        &runner,
        &advance,
    );
    assert_eq!(code, crate::error::exit::REFUSED, "{err}");
    assert!(err.contains("terminal") && !out.contains(TOKEN));
    assert!(issue_calls(&runner).is_empty(), "no container ran");
    // Output redirected to a file: the link would be written there, so no.
    let redirected = Interactive::with_input(
        vec![],
        true,
        Box::new(std::io::Cursor::new(b"admin@example.org\n".to_vec())),
    )
    .with_redirected_output();
    let (code, _, _) = run_cli(
        &["admin-link", "--email", "admin@example.org"],
        dir.path(),
        &runner,
        &redirected,
    );
    assert_eq!(code, crate::error::exit::REFUSED);
    assert!(issue_calls(&runner).is_empty());
}

#[test]
fn there_is_no_json_mode_because_a_link_in_machine_output_ends_up_in_a_log() {
    let (dir, runner) = installation();
    runner.first("auth issue-link", 0, &answer());
    let confirm = ScriptedConfirm::new(&[true]);
    let (code, out, err) = run_cli(
        &["--json", "admin-link", "--email", "admin@example.org"],
        dir.path(),
        &runner,
        &confirm,
    );
    assert_eq!(code, crate::error::exit::USAGE);
    assert!(!out.contains(TOKEN) && !err.contains(TOKEN));
    assert!(out.contains("\"ok\":false"), "the JSON error: {out}");
    assert!(issue_calls(&runner).is_empty());
    assert!(confirm.asked.borrow().is_empty(), "not even asked");
    // The report's machine form, were it ever used, has everything but the link.
    let report = parse(
        "admin@example.org",
        "https://docs.example.org",
        Some(serde_json::from_str(&answer()).unwrap()),
    )
    .unwrap();
    let json = report.public_json().to_string();
    assert!(!json.contains(TOKEN) && !json.contains("login?token"));
    assert!(json.contains("e3b0c442"), "the prefix is there: {json}");
    assert!(!format!("{report:?}").contains(TOKEN));
}

#[test]
fn it_refuses_before_changing_anything_when_it_cannot_work() {
    // Not installed.
    let dir = tempfile::tempdir().unwrap();
    testkit::bundle(dir.path());
    let runner = testkit::healthy_runner();
    runner.first("auth issue-link", 0, &answer());
    let (code, _, err) = run_cli(
        &["admin-link", "--email", "a@example.org"],
        dir.path(),
        &runner,
        &ScriptedConfirm::new(&[true]),
    );
    assert_eq!(code, crate::error::exit::REFUSED, "{err}");
    assert!(err.contains("dynodoctl install"));
    // Not even a bundle.
    let empty = tempfile::tempdir().unwrap();
    let (code, _, _) = run_cli(
        &["admin-link", "--email", "a@example.org"],
        empty.path(),
        &runner,
        &ScriptedConfirm::new(&[true]),
    );
    assert_eq!(code, crate::error::exit::REFUSED);
    // Installed, PostgreSQL stopped.
    let (dir, runner) = installation();
    runner.first("auth issue-link", 0, &answer());
    runner.first(
        "ps -a --format json",
        0,
        &testkit::ps_line("postgres", "exited", "", 1),
    );
    let (code, out, err) = run_cli(
        &["admin-link", "--email", "a@example.org"],
        dir.path(),
        &runner,
        &ScriptedConfirm::new(&[true]),
    );
    assert_eq!(code, crate::error::exit::REFUSED, "{err}");
    assert!(err.contains("PostgreSQL is not running") && !out.contains(TOKEN));
    assert!(issue_calls(&runner).is_empty());
    // No ORIGIN: the link cannot be written.
    let dir = tempfile::tempdir().unwrap();
    testkit::installed(
        dir.path(),
        &testkit::ENV_TEXT.replace("ORIGIN=https://docs.example.org\n", ""),
    );
    let (code, _, err) = run_cli(
        &["admin-link", "--email", "a@example.org"],
        dir.path(),
        &runner,
        &ScriptedConfirm::new(&[true]),
    );
    assert_eq!(code, crate::error::exit::REFUSED, "{err}");
    assert!(err.contains("ORIGIN"));
}

#[test]
fn arguments_are_checked_before_docker_is_used() {
    let (dir, runner) = installation();
    runner.first("auth issue-link", 0, &answer());
    for args in [
        vec!["admin-link", "--email", "not an address"],
        vec!["admin-link", "--email", "a@example.org,b@example.org"],
        vec!["admin-link", "--email", "a@example.org", "--ttl", "5d"],
        vec!["admin-link", "--email", "a@example.org", "--ttl", "m"],
        vec![
            "admin-link",
            "--email",
            "a@example.org",
            "--ttl",
            "9999999999m",
        ],
        vec!["admin-link", "--email", "a@example.org", "--role", "root"],
        vec![
            "admin-link",
            "--email",
            "a@example.org",
            "--reason",
            "two\nlines",
        ],
        vec!["admin-link"],
    ] {
        let (code, out, _) = run_cli(&args, dir.path(), &runner, &ScriptedConfirm::new(&[true]));
        assert_eq!(code, crate::error::exit::USAGE, "{args:?}");
        assert!(!out.contains(TOKEN));
    }
    assert!(issue_calls(&runner).is_empty());
    assert!(!runner.ran("ps -a"), "Docker was not used");
}

#[test]
fn an_account_the_first_administrator_rule_does_not_allow_needs_a_second_typed_phrase() {
    let (dir, runner) = installation();
    runner.first_seq(
        "auth issue-link",
        &[(3, &needs("administrator_exists")), (0, &answer())],
    );
    let confirm = ScriptedConfirm::new(&[true, true]);
    let (code, out, err) = run_cli(
        &[
            "admin-link",
            "--email",
            "new.person@example.org",
            "--create",
        ],
        dir.path(),
        &runner,
        &confirm,
    );
    assert_eq!(code, 0, "{err}");
    assert_eq!(out.matches(TOKEN).count(), 1);
    assert_eq!(
        *confirm.asked.borrow(),
        [
            "new.person@example.org",
            "CREATE ACCOUNT new.person@example.org"
        ]
    );
    let calls = issue_calls(&runner);
    assert_eq!(calls.len(), 2);
    assert!(!calls[0].contains("--confirm-create"));
    assert!(calls[1].contains("--confirm-create new.person@example.org"));

    // Declined at the second phrase: one container ran, nothing was made, no link shown.
    let (dir, runner) = installation();
    runner.first_seq(
        "auth issue-link",
        &[
            (3, &needs("not_a_configured_administrator")),
            (0, &answer()),
        ],
    );
    let (code, out, err) = run_cli(
        &[
            "admin-link",
            "--email",
            "new.person@example.org",
            "--create",
        ],
        dir.path(),
        &runner,
        &ScriptedConfirm::new(&[true, false]),
    );
    assert_eq!(code, crate::error::exit::REFUSED, "{err}");
    assert!(!out.contains(TOKEN));
    assert_eq!(issue_calls(&runner).len(), 1);

    // Without --create the second phrase is never offered: a stop is a stop.
    let (dir, runner) = installation();
    runner.first("auth issue-link", 3, &needs("administrator_exists"));
    let confirm = ScriptedConfirm::new(&[true, true]);
    let (code, out, _) = run_cli(
        &["admin-link", "--email", "new.person@example.org"],
        dir.path(),
        &runner,
        &confirm,
    );
    assert_eq!(code, crate::error::exit::REFUSED);
    assert!(!out.contains(TOKEN));
    assert_eq!(confirm.asked.borrow().len(), 1);
}

#[test]
fn a_failure_or_a_strange_answer_shows_no_link() {
    for (status, stdout) in [
        (1, String::new()),
        (1, answer()), // an answer with a link, but the status says it failed
        (0, "not json\n".to_string()),
        (
            0,
            answer_with(
                "https://evil.example/login?token=".to_string().as_str(),
                serde_json::json!({}),
            ),
        ),
        (
            0,
            answer_with(
                &format!("https://evil.example/login?token={TOKEN}"),
                serde_json::json!({}),
            ),
        ),
        (
            0,
            answer_with(
                &format!("https://docs.example.org/login?token={}", &TOKEN[..40]),
                serde_json::json!({}),
            ),
        ),
        (
            0,
            answer_with(
                &format!("https://docs.example.org/login?token={TOKEN}"),
                serde_json::json!({"ok": false}),
            ),
        ),
        (
            0,
            answer_with(
                &format!("https://docs.example.org/x?token={TOKEN}"),
                serde_json::json!({}),
            ),
        ),
    ] {
        let (dir, runner) = installation();
        runner.first("auth issue-link", status, &stdout);
        let (code, out, err) = run_cli(
            &["admin-link", "--email", "admin@example.org", "--create"],
            dir.path(),
            &runner,
            &ScriptedConfirm::new(&[true]),
        );
        assert_ne!(code, 0, "{status} {stdout}");
        let shown = format!("{out}{err}");
        assert!(
            !shown.contains(TOKEN),
            "{status}: the link was shown: {shown}"
        );
        assert!(!shown.contains("login?token"), "{shown}");
    }
}

#[test]
fn the_options_reach_the_container_exactly() {
    let (dir, runner) = installation();
    runner.first("auth issue-link", 0, &answer());
    let (code, _, err) = run_cli(
        &[
            "admin-link",
            "--email",
            "admin@example.org",
            "--ttl",
            "2h",
            "--role",
            "admin",
        ],
        dir.path(),
        &runner,
        &ScriptedConfirm::new(&[true]),
    );
    assert_eq!(code, 0, "{err}");
    let call = &issue_calls(&runner)[0];
    assert!(
        call.contains("--ttl 2h") && call.contains("--role admin"),
        "{call}"
    );
    assert!(
        !call.contains("--create") && !call.contains("--reason"),
        "{call}"
    );
}

#[test]
fn the_install_instructions_name_the_command_for_the_first_administrator() {
    let env = crate::envfile::EnvFile::parse(
        "ORIGIN=https://docs.example.org\nADMIN_EMAILS=first@example.org, second@example.org\n",
    );
    let origin = crate::origin::Origin::parse("https://docs.example.org").unwrap();
    let steps = crate::install::next_steps(&origin, "provided", &env, false).join("\n");
    assert!(
        steps.contains("dynodoctl admin-link --email first@example.org --create"),
        "{steps}"
    );
    assert!(steps.contains("Email sign-in is OFF"), "{steps}");
    let with_mail = crate::install::next_steps(&origin, "provided", &env, true).join("\n");
    assert!(with_mail.contains("dynodoctl admin-link --email first@example.org --create"));
    assert!(with_mail.contains("auth-email-link=off"), "{with_mail}");
    let none = crate::envfile::EnvFile::parse("ORIGIN=https://docs.example.org\n");
    let unset = crate::install::next_steps(&origin, "provided", &none, false).join("\n");
    assert!(
        unset.contains("admin-emails=you@example.org") && unset.contains("dynodoctl admin-link"),
        "{unset}"
    );
}
