use super::*;
use crate::ctx::ScriptedConfirm;
use crate::runner::FakeRunner;
use crate::testkit::{self, assert_no_seeded_secret};

fn setup() -> (tempfile::TempDir, FakeRunner) {
    let dir = tempfile::tempdir().unwrap();
    testkit::installed(dir.path(), testkit::ENV_TEXT);
    (dir, testkit::healthy_runner())
}

fn assignments(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| s.to_string()).collect()
}

fn opts(items: &[&str]) -> SetOptions {
    SetOptions { assignments: assignments(items), apply: false, mail_to: None, send_test_mail: false }
}

#[test]
fn names_resolve_in_either_spelling() {
    assert_eq!(find("smtp-host").unwrap().key, "SMTP_HOST");
    assert_eq!(find("SMTP_HOST").unwrap().key, "SMTP_HOST");
    assert_eq!(find("auth_email_link").unwrap().key, "AUTH_EMAIL_LINK");
    assert!(find("session_secret").is_none(), "secrets are not settings");
    assert!(find("SESSION_SECRET_FILE").is_none());
}

#[test]
fn invalid_values_are_usage_errors_that_say_what_is_allowed() {
    for bad in ["origin=http://x.example", "tls-mode=none", "bind=localhost", "https-port=99999", "smtp-security=ssl", "smtp-from=nobody", "telemetry=maybe", "auth-email-link=yes", "admin-emails=a@x.example,b", "dev-auth=1", "smtp-host=has space"] {
        let e = parse_assignments(&assignments(&[bad])).err().unwrap_or_else(|| panic!("{bad} was accepted"));
        assert_eq!(e.code(), crate::error::exit::USAGE, "{bad}");
        assert!(!e.todo.is_empty(), "{bad}");
    }
    assert!(parse_assignments(&assignments(&["nonsense"])).is_err());
    let e = parse_assignments(&assignments(&["session-secret=abc"])).err().unwrap();
    assert!(e.todo.contains("Secrets are never set here"));
}

#[test]
fn a_valid_change_is_saved_with_a_dated_private_copy_of_the_previous_file() {
    let (dir, runner) = setup();
    let confirm = ScriptedConfirm::new(&[]);
    let ctx = testkit::ctx(&runner, dir.path(), &confirm);
    let before = std::fs::read_to_string(dir.path().join(".env")).unwrap();
    let report = set(&ctx, &opts(&["brand=Example Docs", "telemetry=off"])).unwrap();
    assert!(!report.applied && report.ok);
    let after = std::fs::read_to_string(dir.path().join(".env")).unwrap();
    assert!(after.contains("DYNODOC_BRAND_NAME=Example Docs") && after.contains("DYNODOC_TELEMETRY=off"));
    let copy = dir.path().join("state").join("env-20261022T120000Z.bak");
    assert_eq!(std::fs::read_to_string(&copy).unwrap(), before, "the copy is the file as it was");
    #[cfg(unix)]
    assert_eq!(crate::util::unix_mode(&copy), Some(0o600));
    assert!(report.render().contains("saved, not applied yet"));
    assert!(runner.ran("run --rm --no-deps -T secrets-init") && runner.ran("run --rm --no-deps -T preflight"), "the web tier's own check ran: {:?}", runner.lines());
    assert!(!runner.ran("up -d"), "nothing restarts without --apply");
}

#[test]
fn the_check_runs_before_apply_and_apply_restarts_with_wait() {
    let (dir, runner) = setup();
    let confirm = ScriptedConfirm::new(&[]);
    let ctx = testkit::ctx(&runner, dir.path(), &confirm);
    let report = set(&ctx, &SetOptions { apply: true, ..opts(&["telemetry=off"]) }).unwrap();
    assert!(report.applied);
    assert!(runner.position("-T preflight").unwrap() < runner.position("up -d --wait --wait-timeout 300").unwrap());
}

#[test]
fn a_setting_the_web_server_rejects_is_taken_back() {
    let (dir, runner) = setup();
    runner.first("-T preflight", 1, "invalid configuration (1 problem):\n  - ORIGIN must use https\n");
    let confirm = ScriptedConfirm::new(&[]);
    let ctx = testkit::ctx(&runner, dir.path(), &confirm);
    let before = std::fs::read_to_string(dir.path().join(".env")).unwrap();
    let e = set(&ctx, &opts(&["telemetry=on"])).unwrap_err();
    assert_eq!(e.code(), crate::error::exit::REFUSED);
    assert!(e.what.contains("ORIGIN must use https") && e.todo.contains("previous .env is back"), "{e}");
    assert_eq!(std::fs::read_to_string(dir.path().join(".env")).unwrap(), before);
    assert!(runner.count("-T secrets-init") >= 2, "the staged copy is refreshed after the take-back");
    assert!(!runner.ran("up -d"));
}

#[test]
fn a_failed_mail_check_takes_the_settings_back_and_says_why() {
    let (dir, runner) = setup();
    runner.first("mailcheck --to", 1, "TLS certificate not trusted: set SMTP_CA_FILE to the certificate authority that signed the mail server certificate");
    let confirm = ScriptedConfirm::new(&[]);
    let ctx = testkit::ctx(&runner, dir.path(), &confirm);
    let before = std::fs::read_to_string(dir.path().join(".env")).unwrap();
    let e = set(&ctx, &SetOptions { mail_to: Some("admin@example.org".into()), ..opts(&["smtp-host=mail.example.org"]) }).unwrap_err();
    assert!(e.what.contains("SMTP_CA_FILE"), "{e}");
    assert_eq!(std::fs::read_to_string(dir.path().join(".env")).unwrap(), before);
}

#[test]
fn a_passing_mail_check_is_reported_and_a_skipped_one_is_flagged() {
    let (dir, runner) = setup();
    let confirm = ScriptedConfirm::new(&[]);
    let ctx = testkit::ctx(&runner, dir.path(), &confirm);
    let checked = set(&ctx, &SetOptions { mail_to: Some("admin@example.org".into()), ..opts(&["smtp-port=2525"]) }).unwrap();
    assert!(checked.mail.as_ref().unwrap().ok && checked.render().contains("mail check: passed"));
    assert!(runner.ran("--profile maintenance run --rm -T mailcheck --to admin@example.org"));
    let (dir2, runner2) = setup();
    let ctx2 = testkit::ctx(&runner2, dir2.path(), &confirm);
    let unchecked = set(&ctx2, &opts(&["smtp-port=2525"])).unwrap();
    assert!(unchecked.notes.iter().any(|n| n.contains("were not tested")));
    assert!(!runner2.ran("mailcheck"));
}

#[test]
fn an_apply_that_fails_puts_the_previous_settings_back_and_starts_them() {
    let (dir, runner) = setup();
    runner.on_seq("up -d --wait", &[(1, "dependency failed to start: container web is unhealthy"), (0, "")]);
    let confirm = ScriptedConfirm::new(&[]);
    let ctx = testkit::ctx(&runner, dir.path(), &confirm);
    let before = std::fs::read_to_string(dir.path().join(".env")).unwrap();
    let e = set(&ctx, &SetOptions { apply: true, ..opts(&["telemetry=on"]) }).unwrap_err();
    assert!(e.what.contains("previous settings were put back and the stack is running with them"), "{e}");
    assert_eq!(std::fs::read_to_string(dir.path().join(".env")).unwrap(), before);
    assert_eq!(runner.count("up -d --wait"), 2);
}

#[test]
fn setting_the_mail_server_turns_the_smtp_transport_on_and_needs_a_sender() {
    let (dir, runner) = setup();
    let confirm = ScriptedConfirm::new(&[]);
    let ctx = testkit::ctx(&runner, dir.path(), &confirm);
    let bare = std::fs::read_to_string(dir.path().join(".env")).unwrap().replace("DYNODOC_MAIL_TRANSPORT=smtp\n", "DYNODOC_MAIL_TRANSPORT=none\n").replace("SMTP_FROM=docs@example.org\n", "");
    std::fs::write(dir.path().join(".env"), bare).unwrap();
    let e = set(&ctx, &opts(&["smtp-host=mail.example.org"])).unwrap_err();
    assert_eq!(e.code(), crate::error::exit::USAGE);
    set(&ctx, &opts(&["smtp-host=mail.example.org", "smtp-from=docs@example.org"])).unwrap();
    let env = EnvFile::read(&dir.path().join(".env")).unwrap();
    assert_eq!(env.get("DYNODOC_MAIL_TRANSPORT"), Some("smtp"));
}

#[test]
fn changing_the_origin_follows_its_port_and_points_at_the_guide() {
    let (dir, runner) = setup();
    let confirm = ScriptedConfirm::new(&[]);
    let ctx = testkit::ctx(&runner, dir.path(), &confirm);
    let report = set(&ctx, &opts(&["origin=https://docs.example.org:8443"])).unwrap();
    let env = EnvFile::read(&dir.path().join(".env")).unwrap();
    assert_eq!(env.get("DYNODOC_HTTPS_PORT"), Some("8443"));
    assert!(report.notes.iter().any(|n| n.contains("9.9")));
}

#[test]
fn switching_to_acme_needs_the_authority_the_contact_and_the_terms() {
    let (dir, runner) = setup();
    let confirm = ScriptedConfirm::new(&[]);
    let ctx = testkit::ctx(&runner, dir.path(), &confirm);
    let e = set(&ctx, &opts(&["tls-mode=acme"])).unwrap_err();
    assert!(e.what.contains("DYNODOC_ACME_EMAIL"), "{e}");
    let e2 = set(&ctx, &opts(&["tls-mode=acme", "acme-email=a@x.example", "acme-ca=letsencrypt-staging"])).unwrap_err();
    assert!(e2.what.contains("terms"), "{e2}");
    set(&ctx, &opts(&["tls-mode=acme", "acme-email=a@x.example", "acme-ca=letsencrypt-staging", "acme-accept-tos=true"])).unwrap();
    let env = EnvFile::read(&dir.path().join(".env")).unwrap();
    assert_eq!(env.get("COMPOSE_PROFILES"), Some("acme"));
    set(&ctx, &opts(&["tls-mode=provided"])).unwrap();
    let env = EnvFile::read(&dir.path().join(".env")).unwrap();
    assert_eq!(env.get("COMPOSE_PROFILES"), None, "the acme profile is dropped when the mode changes");
}

#[test]
fn a_change_to_the_same_value_changes_nothing_and_keeps_no_copy() {
    let (dir, runner) = setup();
    let confirm = ScriptedConfirm::new(&[]);
    let ctx = testkit::ctx(&runner, dir.path(), &confirm);
    let report = set(&ctx, &opts(&["origin=https://docs.example.org"])).unwrap();
    assert!(report.changes.is_empty() && report.previous_copy.is_none());
    assert!(!dir.path().join("state").exists());
    assert!(!runner.ran("preflight"));
}

#[test]
fn show_lists_the_settings_hides_secret_looking_values_and_flags_invalid_ones() {
    let (dir, runner) = setup();
    let env = format!("{}SMTP_SECURITY=ssl\nDYNODOC_TELEMETRY=off\n", testkit::ENV_TEXT);
    std::fs::write(dir.path().join(".env"), env).unwrap();
    let confirm = ScriptedConfirm::new(&[]);
    let ctx = testkit::ctx(&runner, dir.path(), &confirm);
    let shown = show(&ctx).unwrap();
    let get = |n: &str| shown.iter().find(|s| s.name == n).unwrap().clone();
    assert_eq!(get("origin").value.as_deref(), Some("https://docs.example.org"));
    assert_eq!(get("telemetry").value.as_deref(), Some("off"));
    assert!(get("smtp-security").problem.is_some());
    assert_eq!(get("smtp-port").value, None);
    let text = render_show(&shown);
    assert!(text.contains("INVALID") && text.contains("Secrets are not settings"));
    assert_no_seeded_secret(&text, "configure show");
    assert!(!text.contains("SESSION_SECRET"), "secret settings are not even listed");
}

#[test]
fn check_runs_the_web_check_and_only_then_the_mail_check() {
    let (dir, runner) = setup();
    let confirm = ScriptedConfirm::new(&[]);
    let ctx = testkit::ctx(&runner, dir.path(), &confirm);
    let (problems, mail) = check(&ctx, Some("admin@example.org"), false).unwrap();
    assert!(problems.is_empty() && mail.unwrap().ok);
    runner.first("-T preflight", 1, "invalid configuration (2 problems):\n  - A\n  - B\n");
    let (problems, mail) = check(&ctx, Some("admin@example.org"), false).unwrap();
    assert_eq!(problems, vec!["A", "B"]);
    assert!(mail.is_none(), "no mail check against a configuration the server would refuse");
}

#[test]
fn nothing_to_change_is_a_usage_error() {
    let (dir, runner) = setup();
    let confirm = ScriptedConfirm::new(&[]);
    let ctx = testkit::ctx(&runner, dir.path(), &confirm);
    assert_eq!(set(&ctx, &opts(&[])).unwrap_err().code(), crate::error::exit::USAGE);
}
