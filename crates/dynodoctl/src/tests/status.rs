use super::*;
use crate::ctx::ScriptedConfirm;
use crate::net::FakeNet;
use crate::runner::FakeRunner;
use crate::testkit::{self, assert_no_seeded_secret};
use chrono::{DateTime, TimeZone, Utc};

fn net(evaluation: bool, trust: std::result::Result<(), String>) -> FakeNet {
    let n = FakeNet::default();
    n.on_https(
        "/healthz",
        Ok((
            testkit::response(200, &[], "{\"status\":\"ok\"}"),
            testkit::tls_facts(evaluation, trust),
        )),
    );
    n
}

fn healthy_installation(env: &str) -> (tempfile::TempDir, FakeRunner) {
    let dir = tempfile::tempdir().unwrap();
    testkit::installed(dir.path(), env);
    let runner = testkit::healthy_runner();
    runner.on("system df -v", 0, "{\"Volumes\":[{\"Name\":\"dynodoc-selfhosted_database\",\"Size\":\"137.6MB\"},{\"Name\":\"other_database\",\"Size\":\"1GB\"},{\"Name\":\"dynodoc-selfhosted_tls-state\",\"Size\":\"16.38kB\"}]}");
    (dir, runner)
}

fn collect_with(
    runner: &FakeRunner,
    dir: &std::path::Path,
    net: &FakeNet,
    now: fn() -> DateTime<Utc>,
    opts: &Options,
) -> StatusReport {
    let confirm = ScriptedConfirm::new(&[]);
    let mut ctx = testkit::ctx(runner, dir, &confirm);
    ctx.now = now;
    collect(&ctx, net, opts).unwrap()
}

fn production_env() -> String {
    testkit::ENV_TEXT.replace("DYNODOC_TLS_MODE=evaluation", "DYNODOC_TLS_MODE=provided")
}

#[test]
fn docker_sizes_are_parsed_in_decimal_units() {
    assert_eq!(parse_docker_size("137.6MB"), Some(137_600_000));
    assert_eq!(parse_docker_size("16.38kB"), Some(16_380));
    assert_eq!(parse_docker_size("0B"), Some(0));
    assert_eq!(parse_docker_size("1.5GB"), Some(1_500_000_000));
    assert_eq!(parse_docker_size("n/a"), None);
}

#[test]
fn a_healthy_production_installation_with_a_fresh_backup_is_ok() {
    let (dir, runner) = healthy_installation(&production_env());
    let elsewhere = tempfile::tempdir().unwrap();
    testkit::make_backup(
        elsewhere.path(),
        "dynodoc-20261022T060000Z",
        "2026-10-22T06:00:00Z",
        "routine",
        None,
    );
    let opts = Options {
        backup_dir: Some(elsewhere.path().to_path_buf()),
        ..Options::default()
    };
    let r = collect_with(
        &runner,
        dir.path(),
        &net(false, Ok(())),
        testkit::fixed_now,
        &opts,
    );
    assert_eq!(r.overall, "ok", "{:?} {:?}", r.problems, r.warnings);
    assert_eq!(r.exit_code(), crate::error::exit::OK);
    assert_eq!(r.tls.chain_trusted, Some(true));
    assert!(r.tls.days_left.unwrap() > 7000);
}

#[test]
fn backups_on_the_same_disk_as_the_installation_always_warn() {
    let (dir, runner) = healthy_installation(&production_env());
    testkit::make_backup(
        &dir.path().join("backups"),
        "dynodoc-20261022T060000Z",
        "2026-10-22T06:00:00Z",
        "routine",
        None,
    );
    let r = collect_with(
        &runner,
        dir.path(),
        &net(false, Ok(())),
        testkit::fixed_now,
        &Options::default(),
    );
    assert_eq!(r.overall, "warnings");
    assert!(r.warnings.iter().any(|w| w.contains("same disk")));
}

#[test]
fn the_report_has_services_versions_certificate_schema_volumes_and_backups() {
    let (dir, runner) = healthy_installation(&production_env());
    testkit::make_backup(
        &dir.path().join("backups"),
        "dynodoc-20261022T060000Z",
        "2026-10-22T06:00:00Z",
        "routine",
        None,
    );
    let r = collect_with(
        &runner,
        dir.path(),
        &net(false, Ok(())),
        testkit::fixed_now,
        &Options::default(),
    );
    assert_eq!(r.schema_version, SCHEMA_VERSION);
    assert!(r.services.iter().any(|s| s.service == "proxy" && s.up()));
    assert_eq!(r.versions.bundle_version, "dev");
    assert!(r.versions.images.contains_key("api"));
    assert_eq!(r.tls.mode, "provided");
    assert_eq!(r.tls.subject, "O=Example Ltd., CN=docs.example.org");
    assert_eq!(r.tls.protocol, "TLSv1_3");
    assert_eq!(
        (
            r.database.latest_migration,
            r.database.events,
            r.database.documents,
            r.database.size_bytes
        ),
        (52, 36, 2, 123_456)
    );
    assert_eq!(
        r.volumes
            .iter()
            .map(|v| v.name.as_str())
            .collect::<Vec<_>>(),
        vec![
            "dynodoc-selfhosted_database",
            "dynodoc-selfhosted_tls-state"
        ],
        "only this project's volumes"
    );
    assert_eq!(r.backup.count, 1);
    assert_eq!(r.backup.newest_age_days, Some(0));
    let text = r.render();
    for needle in [
        "Services",
        "Certificate",
        "Database",
        "Volumes",
        "Backups",
        "schema level 52",
    ] {
        assert!(text.contains(needle), "{needle}\n{text}");
    }
}

#[test]
fn json_has_the_documented_top_level_keys() {
    let (dir, runner) = healthy_installation(&production_env());
    let r = collect_with(
        &runner,
        dir.path(),
        &net(false, Ok(())),
        testkit::fixed_now,
        &Options::default(),
    );
    let v = serde_json::to_value(&r).unwrap();
    for key in [
        "schema_version",
        "generated_utc",
        "directory",
        "project",
        "overall",
        "services",
        "versions",
        "tls",
        "database",
        "volumes",
        "backup",
        "warnings",
        "problems",
    ] {
        assert!(v.get(key).is_some(), "missing {key}");
    }
    assert_eq!(v["schema_version"], 1);
    assert_eq!(v["tls"]["chain_trusted"], true);
    assert_eq!(v["database"]["latest_migration"], 52);
}

#[test]
fn evaluation_tls_dev_auth_open_binds_and_missing_backups_are_warnings() {
    let env = format!("{}DEV_AUTH=true\nDYNODOC_BIND=0.0.0.0\n", testkit::ENV_TEXT);
    let (dir, runner) = healthy_installation(&env);
    let r = collect_with(
        &runner,
        dir.path(),
        &net(true, Err("self-signed".into())),
        testkit::fixed_now,
        &Options::default(),
    );
    assert_eq!(r.overall, "warnings");
    assert_eq!(r.exit_code(), crate::error::exit::WARNINGS);
    let all = r.warnings.join("\n");
    for needle in [
        "TLS mode `evaluation`",
        "DEV_AUTH=true",
        "DYNODOC_BIND is 0.0.0.0",
        "no backup yet",
    ] {
        assert!(all.contains(needle), "{needle}\n{all}");
    }
    assert!(r.tls.evaluation_certificate && r.tls.chain_trusted == Some(false));
}

#[test]
fn an_old_backup_warns_after_the_configured_number_of_days() {
    let (dir, runner) = healthy_installation(&production_env());
    testkit::make_backup(
        &dir.path().join("backups"),
        "dynodoc-20261015T000000Z",
        "2026-10-15T00:00:00Z",
        "routine",
        None,
    );
    let r = collect_with(
        &runner,
        dir.path(),
        &net(false, Ok(())),
        testkit::fixed_now,
        &Options::default(),
    );
    assert!(
        r.warnings
            .iter()
            .any(|w| w.contains("7 days old") && w.contains("warning after 2 days")),
        "{:?}",
        r.warnings
    );
    let lenient = collect_with(
        &runner,
        dir.path(),
        &net(false, Ok(())),
        testkit::fixed_now,
        &Options {
            max_backup_age_days: 14,
            ..Options::default()
        },
    );
    assert!(!lenient.warnings.iter().any(|w| w.contains("days old")));
}

fn in_late_september_2046() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2046, 9, 25, 12, 0, 0).unwrap()
}
fn in_early_october_2046() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2046, 10, 12, 12, 0, 0).unwrap()
}
fn after_expiry_2046() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2046, 10, 20, 12, 0, 0).unwrap()
}

#[test]
fn certificate_expiry_warns_inside_30_days_and_is_a_problem_inside_7_or_after() {
    let (dir, runner) = healthy_installation(&production_env());
    testkit::make_backup(
        &dir.path().join("backups"),
        "dynodoc-20460925T000000Z",
        "2046-09-25T06:00:00Z",
        "routine",
        None,
    );
    let soon = collect_with(
        &runner,
        dir.path(),
        &net(false, Ok(())),
        in_late_september_2046,
        &Options::default(),
    );
    assert!(
        soon.problems.is_empty()
            && soon
                .warnings
                .iter()
                .any(|w| w.contains("expires in 22 days")),
        "{:?} {:?}",
        soon.problems,
        soon.warnings
    );
    let very_soon = collect_with(
        &runner,
        dir.path(),
        &net(false, Ok(())),
        in_early_october_2046,
        &Options::default(),
    );
    assert!(
        very_soon
            .problems
            .iter()
            .any(|p| p.contains("expires in 5 days")),
        "{:?}",
        very_soon.problems
    );
    let expired = collect_with(
        &runner,
        dir.path(),
        &net(false, Ok(())),
        after_expiry_2046,
        &Options::default(),
    );
    assert!(
        expired
            .problems
            .iter()
            .any(|p| p.contains("expired") && p.contains("days ago")),
        "{:?}",
        expired.problems
    );
    assert_eq!(expired.exit_code(), crate::error::exit::FAILED);
}

#[test]
fn a_service_that_is_down_or_a_job_that_failed_is_a_problem() {
    let (dir, runner) = healthy_installation(&production_env());
    let mut ps = String::new();
    for (s, st, h, e) in [
        ("postgres", "running", "healthy", 0),
        ("api", "running", "unhealthy", 0),
        ("web", "exited", "", 137),
        ("init", "exited", "", 1),
    ] {
        ps.push_str(&testkit::ps_line(s, st, h, e));
    }
    runner.first("ps -a --format json", 0, &ps);
    let r = collect_with(
        &runner,
        dir.path(),
        &net(false, Ok(())),
        testkit::fixed_now,
        &Options::default(),
    );
    assert_eq!(r.overall, "problems");
    let all = r.problems.join("\n");
    for needle in [
        "api is running and unhealthy",
        "web is exited",
        "proxy does not exist",
        "the job init failed (exit 1)",
    ] {
        assert!(all.contains(needle), "{needle}\n{all}");
    }
}

#[test]
fn a_running_proxy_that_does_not_answer_https_is_a_problem() {
    let (dir, runner) = healthy_installation(&production_env());
    let dead = FakeNet::default();
    dead.on_https(
        "/healthz",
        Err("cannot connect to 127.0.0.1:443: refused".into()),
    );
    let r = collect_with(
        &runner,
        dir.path(),
        &dead,
        testkit::fixed_now,
        &Options::default(),
    );
    assert!(
        r.problems.iter().any(|p| p.contains("did not answer")),
        "{:?}",
        r.problems
    );
}

#[test]
fn an_untrusted_chain_outside_evaluation_is_a_warning_that_names_ca_file() {
    let (dir, runner) = healthy_installation(&production_env());
    let r = collect_with(
        &runner,
        dir.path(),
        &net(
            false,
            Err("not issued by an authority this tool trusts".into()),
        ),
        testkit::fixed_now,
        &Options::default(),
    );
    assert!(
        r.warnings.iter().any(|w| w.contains("--ca-file")),
        "{:?}",
        r.warnings
    );
}

#[test]
fn leftover_scratch_databases_are_reported() {
    let (dir, runner) = healthy_installation(&production_env());
    runner.first("datname like", 0, "dynodoc_restore_20261021t010101z\n");
    let r = collect_with(
        &runner,
        dir.path(),
        &net(false, Ok(())),
        testkit::fixed_now,
        &Options::default(),
    );
    assert!(
        r.warnings
            .iter()
            .any(|w| w.contains("dynodoc_restore_20261021t010101z")),
        "{:?}",
        r.warnings
    );
}

#[test]
fn nothing_started_is_a_problem_with_the_command_to_start() {
    let (dir, runner) = healthy_installation(&production_env());
    runner.first("ps -a --format json", 0, "");
    let r = collect_with(
        &runner,
        dir.path(),
        &net(false, Ok(())),
        testkit::fixed_now,
        &Options::default(),
    );
    assert!(r
        .problems
        .iter()
        .any(|p| p.contains("has not been started")));
}

#[test]
fn not_installed_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    testkit::bundle(dir.path());
    let runner = testkit::healthy_runner();
    let confirm = ScriptedConfirm::new(&[]);
    let ctx = testkit::ctx(&runner, dir.path(), &confirm);
    let e = collect(&ctx, &FakeNet::default(), &Options::default()).unwrap_err();
    assert_eq!(e.code(), crate::error::exit::REFUSED);
}

#[test]
fn status_output_never_contains_a_secret() {
    let (dir, runner) = healthy_installation(&format!(
        "{}SMTP_PASSWORD=SEEDED-smtp-password-zz99yy88\n",
        production_env()
    ));
    let r = collect_with(
        &runner,
        dir.path(),
        &net(false, Ok(())),
        testkit::fixed_now,
        &Options::default(),
    );
    assert_no_seeded_secret(&r.render(), "status text");
    assert_no_seeded_secret(&serde_json::to_string(&r).unwrap(), "status json");
}
