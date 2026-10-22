use super::*;
use crate::ctx::ScriptedConfirm;
use crate::net::{FakeNet, HttpResponse};
use crate::runner::FakeRunner;
use crate::testkit;

fn good_net() -> FakeNet {
    let net = FakeNet::default();
    let facts = testkit::tls_facts(false, Ok(()));
    let headers: Vec<(&str, &str)> = vec![
        (
            "content-security-policy",
            "default-src 'self'; script-src 'self'; connect-src 'self' https://docs.example.org",
        ),
        ("strict-transport-security", "max-age=31536000"),
        ("x-content-type-options", "nosniff"),
        ("referrer-policy", "no-referrer"),
        ("x-frame-options", "SAMEORIGIN"),
        ("server", "nginx"),
        ("x-robots-tag", "noindex"),
    ];
    let ok = |r: HttpResponse| Ok((r, facts.clone()));
    net.on_https(
        "/login",
        ok(testkit::response(200, &headers, "<html>Docs</html>")),
    );
    net.on_https(
        "/healthz",
        ok(testkit::response(200, &[], "{\"status\":\"ok\"}")),
    );
    net.on_https(
        "@dynodoc-verify-delivery.invalid/",
        ok(testkit::response(421, &[], "")),
    );
    net.on_https("/api/engine/documents", ok(testkit::response(401, &[], "")));
    net.on_https(
        "/",
        ok(testkit::response(302, &[("location", "/login")], "")),
    );
    for p in [
        "/features",
        "/privacy",
        "/terms",
        "/pricing",
        "/sitemap.xml",
        "/llms.txt",
    ] {
        net.on_https(p, ok(testkit::response(404, &[], "")));
    }
    net.on_http(
        "/",
        Ok(testkit::response(
            301,
            &[("location", "https://docs.example.org/")],
            "",
        )),
    );
    net
}

fn run_cli(
    args: &[&str],
    dir: &std::path::Path,
    runner: &FakeRunner,
    answers: &[bool],
) -> (i32, String, String) {
    let mut full: Vec<String> = vec![
        "dynodoctl".into(),
        "--dir".into(),
        dir.display().to_string(),
    ];
    full.extend(args.iter().map(|a| a.to_string()));
    let confirm = ScriptedConfirm::new(answers);
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let code = run(
        full,
        runner,
        &|_| Box::new(good_net()),
        Some(&confirm),
        &mut out,
        &mut err,
    );
    (
        code,
        String::from_utf8(out).unwrap(),
        String::from_utf8(err).unwrap(),
    )
}

fn production_dir() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    testkit::installed(
        dir.path(),
        &testkit::ENV_TEXT.replace("DYNODOC_TLS_MODE=evaluation", "DYNODOC_TLS_MODE=provided"),
    );
    dir
}

#[test]
fn help_and_version_exit_zero_and_describe_the_exit_codes() {
    let (code, out, _) = run_cli(
        &["--help"],
        std::path::Path::new("."),
        &FakeRunner::new(),
        &[],
    );
    assert_eq!(code, 0);
    for command in [
        "install",
        "configure",
        "status",
        "backup",
        "restore-verify",
        "restore",
        "rehearse-lite",
        "upgrade",
        "rollback",
        "support-bundle",
        "verify-delivery",
    ] {
        assert!(out.contains(command), "{command}");
    }
    assert!(out.contains("Exit codes") && out.contains("3 refused before changing anything"));
    let mut v = Vec::new();
    let code = run(
        vec!["dynodoctl".into(), "--version".into()],
        &FakeRunner::new(),
        &|_| Box::new(FakeNet::default()),
        None,
        &mut v,
        &mut Vec::new(),
    );
    assert_eq!(code, 0);
    assert!(String::from_utf8(v).unwrap().starts_with("dynodoctl 0."));
}

#[test]
fn wrong_arguments_exit_2_and_print_the_parser_message_on_standard_error() {
    let dir = production_dir();
    for args in [
        &["frobnicate"][..],
        &["backup", "--keep", "many"][..],
        &["configure", "set"][..],
        &["restore-verify", "--no-such-flag"][..],
        &[][..],
    ] {
        let (code, out, err) = run_cli(args, dir.path(), &FakeRunner::new(), &[]);
        assert_eq!(code, crate::error::exit::USAGE, "{args:?}");
        assert!(out.is_empty() && !err.is_empty(), "{args:?}");
    }
}

#[test]
fn restore_without_into_live_is_a_usage_error_that_points_at_restore_verify() {
    let dir = production_dir();
    let runner = testkit::healthy_runner();
    let (code, _, err) = run_cli(&["restore"], dir.path(), &runner, &[]);
    assert_eq!(code, crate::error::exit::USAGE);
    assert!(
        err.contains("restore-verify") && err.contains("What to do"),
        "{err}"
    );
    assert!(runner.calls().is_empty());
}

#[test]
fn errors_print_what_happened_and_what_to_do_and_nothing_on_standard_output() {
    let dir = production_dir();
    let runner = testkit::healthy_runner();
    runner.first("ps -a --format json", 0, "");
    let (code, out, err) = run_cli(&["backup"], dir.path(), &runner, &[]);
    assert_eq!(code, crate::error::exit::REFUSED);
    assert!(out.is_empty());
    assert!(
        err.starts_with("ERROR: PostgreSQL is not running") && err.contains("\n  What to do: "),
        "{err}"
    );
}

#[test]
fn json_mode_prints_one_object_on_standard_output_and_progress_elsewhere() {
    let dir = production_dir();
    let runner = testkit::healthy_runner();
    let (code, out, _) = run_cli(
        &["--json", "backup", "--no-config"],
        dir.path(),
        &runner,
        &[],
    );
    assert_eq!(code, 0);
    let v: serde_json::Value =
        serde_json::from_str(&out).expect("standard output is exactly one JSON document");
    assert_eq!(v["ok"], true);
    assert_eq!(v["kind"], "routine");
    assert!(v["steps"].as_array().unwrap().len() >= 4);
}

#[test]
fn json_errors_carry_the_exit_code() {
    let dir = production_dir();
    let runner = testkit::healthy_runner();
    runner.first("ps -a --format json", 0, "");
    let (code, out, err) = run_cli(&["--json", "backup"], dir.path(), &runner, &[]);
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(
        (code, v["ok"].clone(), v["error"]["exit_code"].clone()),
        (3, serde_json::json!(false), serde_json::json!(3))
    );
    assert!(err.contains("ERROR:"));
}

#[test]
fn status_maps_ok_warnings_and_problems_to_0_4_and_1() {
    let dir = production_dir();
    let elsewhere = tempfile::tempdir().unwrap();
    testkit::make_backup(
        elsewhere.path(),
        "dynodoc-20990101T000000Z",
        "2099-01-01T00:00:00Z",
        "routine",
        None,
    );
    let runner = testkit::healthy_runner();
    // Real clock in the CLI: a backup dated in the future counts as age 0 (not older than two days).
    let (code, out, _) = run_cli(
        &[
            "status",
            "--no-volumes",
            "--backup-dir",
            &elsewhere.path().display().to_string(),
        ],
        dir.path(),
        &runner,
        &[],
    );
    assert_eq!(code, 0, "{out}");
    let (code, out, _) = run_cli(&["status", "--no-volumes"], dir.path(), &runner, &[]);
    assert_eq!(
        code,
        crate::error::exit::WARNINGS,
        "no backup yet is a warning: {out}"
    );
    runner.first(
        "ps -a --format json",
        0,
        &testkit::ps_line("postgres", "exited", "", 1),
    );
    let (code, _, _) = run_cli(&["status", "--no-volumes"], dir.path(), &runner, &[]);
    assert_eq!(code, crate::error::exit::FAILED);
}

#[test]
fn verify_delivery_exit_codes_follow_the_overall_result() {
    let dir = production_dir();
    let runner = testkit::healthy_runner();
    let (code, out, _) = run_cli(&["verify-delivery"], dir.path(), &runner, &[]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("Result: PASSED"));
    let (code, out, _) = run_cli(
        &["--json", "verify-delivery", "--tls-mode", "evaluation"],
        dir.path(),
        &runner,
        &[],
    );
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(v["overall"], "passed");
    assert_eq!(code, 0);
}

#[test]
fn a_typed_confirmation_can_be_given_in_advance_and_only_the_exact_phrase_counts() {
    let dir = production_dir();
    testkit::make_backup(
        &dir.path().join("backups"),
        "dynodoc-20261021T000000Z",
        "2026-10-21T00:00:00Z",
        "routine",
        Some(dir.path()),
    );
    // The real Interactive confirm is used when no test double is given: build it through the CLI.
    let runner = testkit::healthy_runner();
    let mut full: Vec<String> = vec![
        "dynodoctl".into(),
        "--dir".into(),
        dir.path().display().to_string(),
        "--confirm".into(),
        "replace live data".into(),
        "restore".into(),
        "--into-live".into(),
        "--no-safety-backup".into(),
    ];
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let code = run(
        full.clone(),
        &runner,
        &|_| Box::new(FakeNet::default()),
        None,
        &mut out,
        &mut err,
    );
    assert_eq!(
        code,
        crate::error::exit::REFUSED,
        "a wrong-case phrase does not count: {}",
        String::from_utf8_lossy(&err)
    );
    assert!(!runner.ran("alter database"));
    full[4] = crate::restore::LIVE_PHRASE.to_string();
    let code = run(
        full,
        &runner,
        &|_| Box::new(FakeNet::default()),
        None,
        &mut out,
        &mut err,
    );
    assert_eq!(code, 0, "{}", String::from_utf8_lossy(&err));
    assert!(runner.ran("alter database"));
}

#[test]
fn upgrade_without_the_backup_phrase_is_refused_with_exit_3() {
    let dir = production_dir();
    let runner = testkit::healthy_runner();
    let (code, _, err) = run_cli(
        &["upgrade", "--local", "--skip-backup"],
        dir.path(),
        &runner,
        &[false],
    );
    assert_eq!(code, crate::error::exit::REFUSED);
    assert!(err.contains("without a backup was not confirmed"), "{err}");
    assert!(!runner.ran("up -d"));
}

#[test]
fn upgrade_plan_prints_the_plan_and_changes_nothing() {
    let dir = production_dir();
    let runner = testkit::healthy_runner();
    let (code, out, _) = run_cli(&["upgrade", "--local", "--plan"], dir.path(), &runner, &[]);
    assert_eq!(code, 0);
    assert!(out.contains("Plan") && out.contains("take a pre-upgrade backup (mandatory)"));
    assert!(runner.calls().is_empty());
}

#[test]
fn support_bundle_list_prints_the_inventory_and_writes_nothing() {
    let dir = production_dir();
    let runner = testkit::healthy_runner();
    let (code, out, _) = run_cli(&["support-bundle", "--list"], dir.path(), &runner, &[]);
    assert_eq!(code, 0);
    assert!(
        out.contains("listing only")
            && out.contains("manifest.json")
            && out.contains("Never included")
    );
    assert!(!dir.path().join("support").exists());
}

#[test]
fn configure_set_and_show_round_trip_through_the_cli() {
    let dir = production_dir();
    let runner = testkit::healthy_runner();
    let (code, out, _) = run_cli(
        &["configure", "set", "telemetry=off", "brand=Example Docs"],
        dir.path(),
        &runner,
        &[],
    );
    assert_eq!(code, 0, "{out}");
    let (code, out, _) = run_cli(&["configure", "show"], dir.path(), &runner, &[]);
    assert_eq!(code, 0);
    assert!(out.contains("Example Docs") && out.contains("DYNODOC_TELEMETRY"));
    let (code, _, err) = run_cli(
        &["configure", "set", "telemetry=perhaps"],
        dir.path(),
        &runner,
        &[],
    );
    assert_eq!(code, crate::error::exit::USAGE);
    assert!(
        err.contains("on | off") || err.contains("use one of"),
        "{err}"
    );
}

#[test]
fn install_dry_run_with_a_failing_preflight_exits_3_and_changes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    testkit::bundle(dir.path());
    let runner = FakeRunner::new();
    runner.on(
        "version --format {{.Server.Version}}",
        1,
        "Cannot connect to the Docker daemon",
    );
    let (code, out, _) = run_cli(
        &[
            "install",
            "--dry-run",
            "--origin",
            "https://localhost:18443",
            "--tls",
            "evaluation",
        ],
        dir.path(),
        &runner,
        &[],
    );
    assert_eq!(code, crate::error::exit::REFUSED);
    assert!(
        out.contains("FAIL  Docker daemon is not reachable") && out.contains("Plan"),
        "{out}"
    );
    assert!(!dir.path().join(".env").exists());
}

#[test]
fn install_asks_for_the_certificate_mode_instead_of_picking_one() {
    let dir = tempfile::tempdir().unwrap();
    testkit::bundle(dir.path());
    let (code, _, err) = run_cli(
        &["install", "--origin", "https://localhost:18443"],
        dir.path(),
        &testkit::healthy_runner(),
        &[true],
    );
    assert_eq!(code, crate::error::exit::USAGE);
    assert!(err.contains("evaluation") && err.contains("provided") && err.contains("acme"));
}

#[test]
fn every_command_that_needs_an_installation_says_so_when_there_is_none() {
    let dir = tempfile::tempdir().unwrap();
    testkit::bundle(dir.path());
    let runner = testkit::healthy_runner();
    for args in [
        &["status"][..],
        &["backup"][..],
        &["restore-verify"][..],
        &["rehearse-lite"][..],
        &["upgrade", "--local"][..],
        &["configure", "show"][..],
        &["rollback"][..],
    ] {
        let (code, _, err) = run_cli(args, dir.path(), &runner, &[true]);
        assert_eq!(code, crate::error::exit::REFUSED, "{args:?}: {err}");
        assert!(err.contains("dynodoctl install"), "{args:?}: {err}");
    }
}

#[test]
fn a_ca_file_without_a_certificate_is_a_usage_error() {
    let dir = production_dir();
    let bad = dir.path().join("not-a-ca.pem");
    std::fs::write(&bad, "hello").unwrap();
    let (code, _, err) = run_cli(
        &["--ca-file", &bad.display().to_string(), "status"],
        dir.path(),
        &testkit::healthy_runner(),
        &[],
    );
    assert_eq!(code, crate::error::exit::USAGE);
    assert!(err.contains("contains no certificate"));
}

#[path = "cli_review.rs"]
mod review;
