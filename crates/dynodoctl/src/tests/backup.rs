use super::*;
use crate::ctx::ScriptedConfirm;
use crate::manifest::{self, read_manifest};
use crate::runner::FakeRunner;
use crate::testkit::{self, assert_no_seeded_secret};

fn setup() -> (tempfile::TempDir, FakeRunner) {
    let dir = tempfile::tempdir().unwrap();
    testkit::installed(dir.path(), testkit::ENV_TEXT);
    (dir, testkit::healthy_runner())
}

#[test]
fn a_backup_writes_a_dump_a_manifest_checksums_and_the_settings() {
    let (dir, runner) = setup();
    let confirm = ScriptedConfirm::new(&[]);
    let ctx = testkit::ctx(&runner, dir.path(), &confirm);
    let report = run(&ctx, &Options::default()).unwrap();
    assert!(report.ok);
    let path = &report.path;
    assert!(path.starts_with(dir.path().join("backups")));
    assert_eq!(
        path.file_name().unwrap().to_string_lossy(),
        "dynodoc-20261022T120000Z"
    );
    let m = read_manifest(path).unwrap();
    assert_eq!(m.kind, "routine");
    assert_eq!(m.facts.events, 36);
    assert_eq!(m.facts_after.documents, 2);
    assert_eq!(m.database.toc_entries, 2);
    assert_eq!(m.origin, "https://docs.example.org");
    assert_eq!(m.config.location, "inside");
    let sums = std::fs::read_to_string(path.join("SHA256SUMS")).unwrap();
    assert!(
        sums.contains("  database.dump\n")
            && sums.contains("  manifest.json\n")
            && sums.contains("  config/.env\n"),
        "{sums}"
    );
    // LOW: no unsalted hash of a secret file anywhere; one salted aggregate digest instead.
    assert!(!sums.contains("config/secrets/"), "{sums}");
    assert!(m.config.secrets_salt.is_some() && m.config.secrets_digest.is_some());
    assert!(m
        .config
        .files
        .iter()
        .filter(|f| f.path.starts_with("secrets/"))
        .all(|f| f.sha256.is_empty()));
    let info = manifest::BackupInfo {
        path: path.clone(),
        manifest: m,
    };
    assert!(manifest::verify_files(&info, None).is_ok());
    #[cfg(unix)]
    for rel in [
        "config/.env",
        "config/secrets/session_secret",
        "database.dump",
        "manifest.json",
    ] {
        assert_eq!(
            crate::util::unix_mode(&path.join(rel)),
            Some(0o600),
            "{rel}"
        );
    }
}

#[test]
fn the_dump_is_taken_inside_the_postgres_container_and_no_password_is_on_a_command_line() {
    let (dir, runner) = setup();
    let confirm = ScriptedConfirm::new(&[]);
    let ctx = testkit::ctx(&runner, dir.path(), &confirm);
    run(&ctx, &Options::default()).unwrap();
    let dump = runner
        .lines()
        .into_iter()
        .find(|l| l.contains("pg_dump"))
        .expect("pg_dump ran");
    assert!(
        dump.contains("exec -T postgres sh -c")
            && dump.contains("pg_dump -h 127.0.0.1 -U dynodoc -Fc dynodoc"),
        "{dump}"
    );
    assert!(
        dump.contains("$(cat /run/secrets/postgres_password)"),
        "the password is read inside the container"
    );
    for line in runner.lines() {
        assert_no_seeded_secret(&line, "command line");
    }
    assert!(
        runner.calls().iter().any(|c| c.stdout_file.is_some()),
        "the dump streams to a file, not through memory"
    );
}

#[test]
fn the_report_and_the_json_never_contain_a_secret_value() {
    let (dir, runner) = setup();
    let confirm = ScriptedConfirm::new(&[]);
    let ctx = testkit::ctx(&runner, dir.path(), &confirm);
    let report = run(&ctx, &Options::default()).unwrap();
    assert_no_seeded_secret(&report.render(), "backup report");
    assert_no_seeded_secret(&serde_json::to_string(&report).unwrap(), "backup json");
    assert_no_seeded_secret(
        &std::fs::read_to_string(report.path.join("manifest.json")).unwrap(),
        "manifest",
    );
}

#[test]
fn the_warnings_are_loud_about_secrets_next_to_the_dump_and_same_disk_backups() {
    let (dir, runner) = setup();
    let confirm = ScriptedConfirm::new(&[]);
    let ctx = testkit::ctx(&runner, dir.path(), &confirm);
    let report = run(&ctx, &Options::default()).unwrap();
    let all = report.warnings.join("\n");
    assert!(
        all.contains("AND the secrets") && all.contains("not encrypted"),
        "{all}"
    );
    assert!(all.contains("same disk"), "{all}");
}

#[test]
fn secrets_can_be_sent_to_another_place_and_the_dump_directory_then_holds_none() {
    let (dir, runner) = setup();
    let elsewhere = tempfile::tempdir().unwrap();
    let confirm = ScriptedConfirm::new(&[]);
    let ctx = testkit::ctx(&runner, dir.path(), &confirm);
    let report = run(
        &ctx,
        &Options {
            secrets_to: Some(elsewhere.path().to_path_buf()),
            ..Options::default()
        },
    )
    .unwrap();
    assert!(!report.path.join("config").exists());
    let m = read_manifest(&report.path).unwrap();
    assert_eq!(m.config.location, "external");
    let copy = elsewhere.path().join("dynodoc-20261022T120000Z-config");
    assert!(copy.join("secrets").join("session_secret").is_file() && copy.join(".env").is_file());
    assert!(!report.warnings.join("\n").contains("AND the secrets"));
    let info = manifest::BackupInfo {
        path: report.path.clone(),
        manifest: m,
    };
    assert!(manifest::verify_files(&info, Some(&copy)).is_ok());
    assert!(
        manifest::verify_files(&info, None).is_ok(),
        "an external copy that is not named is not checked (the installation has its own)"
    );
    // Named, it is checked: a tampered secret is caught.
    std::fs::write(copy.join("secrets").join("session_secret"), "tampered\n").unwrap();
    let problems = manifest::verify_files(&info, Some(&copy)).unwrap_err();
    assert!(
        problems.iter().any(|p| p.contains("session_secret")),
        "{problems:?}"
    );
}

#[test]
fn no_config_skips_the_secrets_and_says_so() {
    let (dir, runner) = setup();
    let confirm = ScriptedConfirm::new(&[]);
    let ctx = testkit::ctx(&runner, dir.path(), &confirm);
    let report = run(
        &ctx,
        &Options {
            no_config: true,
            ..Options::default()
        },
    )
    .unwrap();
    assert_eq!(read_manifest(&report.path).unwrap().config.location, "none");
    assert!(report.warnings.iter().any(|w| w.contains("NOT copied")));
}

#[test]
fn a_stopped_database_is_refused_and_nothing_is_written() {
    let (dir, runner) = setup();
    runner.first(
        "ps -a --format json",
        0,
        &testkit::ps_line("postgres", "exited", "", 0),
    );
    let confirm = ScriptedConfirm::new(&[]);
    let ctx = testkit::ctx(&runner, dir.path(), &confirm);
    let e = run(&ctx, &Options::default()).unwrap_err();
    assert_eq!(e.code(), crate::error::exit::REFUSED);
    assert!(e.what.contains("not running") && e.todo.contains("start"));
    assert!(!dir
        .path()
        .join("backups")
        .join("dynodoc-20261022T120000Z")
        .exists());
}

#[test]
fn a_failing_pg_dump_leaves_no_half_written_backup() {
    let (dir, runner) = setup();
    runner.first("pg_dump", 1, "pg_dump: error: connection to server failed");
    let confirm = ScriptedConfirm::new(&[]);
    let ctx = testkit::ctx(&runner, dir.path(), &confirm);
    let e = run(&ctx, &Options::default()).unwrap_err();
    assert_eq!(e.code(), crate::error::exit::FAILED);
    assert!(e.what.contains("pg_dump failed"));
    assert!(
        !dir.path()
            .join("backups")
            .join("dynodoc-20261022T120000Z")
            .exists(),
        "a half-written backup looks like a real one"
    );
}

#[test]
fn output_that_is_not_a_custom_format_dump_is_rejected() {
    let (dir, runner) = setup();
    runner.first("pg_dump", 0, "");
    let confirm = ScriptedConfirm::new(&[]);
    let ctx = testkit::ctx(&runner, dir.path(), &confirm);
    let e = run(&ctx, &Options::default()).unwrap_err();
    assert!(e.what.contains("not a custom-format dump"), "{e}");
    assert!(!dir
        .path()
        .join("backups")
        .join("dynodoc-20261022T120000Z")
        .exists());
}

#[test]
fn an_unreadable_dump_is_rejected_by_pg_restore_list() {
    let (dir, runner) = setup();
    runner.first(
        "pg_restore --list",
        1,
        "pg_restore: error: unsupported version",
    );
    let confirm = ScriptedConfirm::new(&[]);
    let ctx = testkit::ctx(&runner, dir.path(), &confirm);
    let e = run(&ctx, &Options::default()).unwrap_err();
    assert!(e.what.contains("cannot be read by pg_restore"));
    assert!(!dir
        .path()
        .join("backups")
        .join("dynodoc-20261022T120000Z")
        .exists());
}

#[test]
fn an_existing_backup_of_the_same_second_is_never_overwritten() {
    let (dir, runner) = setup();
    std::fs::create_dir_all(dir.path().join("backups").join("dynodoc-20261022T120000Z")).unwrap();
    let confirm = ScriptedConfirm::new(&[]);
    let ctx = testkit::ctx(&runner, dir.path(), &confirm);
    let e = run(&ctx, &Options::default()).unwrap_err();
    assert_eq!(e.code(), crate::error::exit::REFUSED);
    assert!(e.what.contains("already exists"));
}

#[test]
fn labels_are_restricted_to_safe_characters() {
    let (dir, runner) = setup();
    let confirm = ScriptedConfirm::new(&[]);
    let ctx = testkit::ctx(&runner, dir.path(), &confirm);
    for bad in ["", "UPPER", "a b", "../x", "a/b", &"x".repeat(41)] {
        let e = run(
            &ctx,
            &Options {
                label: Some(bad.to_string()),
                ..Options::default()
            },
        )
        .unwrap_err();
        assert_eq!(e.code(), crate::error::exit::USAGE, "{bad}");
    }
    let ok = run(
        &ctx,
        &Options {
            label: Some("before-upgrade".into()),
            ..Options::default()
        },
    )
    .unwrap();
    assert!(ok.name.ends_with("-before-upgrade"));
}

#[test]
fn retention_keeps_the_newest_routine_backups_and_never_the_one_just_made_last() {
    let (dir, runner) = setup();
    let backups = dir.path().join("backups");
    testkit::make_backup(
        &backups,
        "dynodoc-20261019T000000Z",
        "2026-10-19T00:00:00Z",
        "routine",
        None,
    );
    testkit::make_backup(
        &backups,
        "dynodoc-20261020T000000Z",
        "2026-10-20T00:00:00Z",
        "routine",
        None,
    );
    testkit::make_backup(
        &backups,
        "dynodoc-20261021T000000Z",
        "2026-10-21T00:00:00Z",
        "routine",
        None,
    );
    testkit::make_backup(
        &backups,
        "dynodoc-20261018T000000Z-before-upgrade",
        "2026-10-18T00:00:00Z",
        "pre-upgrade",
        None,
    );
    let confirm = ScriptedConfirm::new(&[]);
    let ctx = testkit::ctx(&runner, dir.path(), &confirm);
    let report = run(
        &ctx,
        &Options {
            keep: 2,
            ..Options::default()
        },
    )
    .unwrap();
    let remaining: Vec<String> = manifest::list_backups(&backups)
        .iter()
        .map(|b| b.name())
        .collect();
    assert_eq!(
        remaining,
        vec![
            "dynodoc-20261018T000000Z-before-upgrade",
            "dynodoc-20261021T000000Z",
            "dynodoc-20261022T120000Z"
        ]
    );
    assert_eq!(report.removed.len(), 2);
    assert!(report.render().contains("removed 2 older backup"));
}

#[test]
fn verify_restores_the_new_backup_into_a_throwaway_database_and_reports_it() {
    let (dir, runner) = setup();
    let confirm = ScriptedConfirm::new(&[]);
    let ctx = testkit::ctx(&runner, dir.path(), &confirm);
    let report = run(
        &ctx,
        &Options {
            verify: true,
            ..Options::default()
        },
    )
    .unwrap();
    let v = report.verification.as_ref().expect("verified");
    assert!(
        report.ok
            && v.ok
            && v.counts_match
            && v.history_verified
            && v.scratch_removed == Some(true)
    );
    assert!(runner.ran("create database dynodoc_restore_20261022t120000z_ab12cd"));
    assert!(
        runner.ran("drop database if exists dynodoc_restore_20261022t120000z_ab12cd with (force)")
    );
    assert!(report.render().contains("Verification of this backup"));
}

#[test]
fn a_failed_verification_marks_the_backup_failed_and_keeps_older_backups() {
    let (dir, runner) = setup();
    let backups = dir.path().join("backups");
    testkit::make_backup(
        &backups,
        "dynodoc-20261019T000000Z",
        "2026-10-19T00:00:00Z",
        "routine",
        None,
    );
    runner.first(
        "verify-snapshots --all",
        1,
        "Error: snapshot backfill failed for document x",
    );
    let confirm = ScriptedConfirm::new(&[]);
    let ctx = testkit::ctx(&runner, dir.path(), &confirm);
    let report = run(
        &ctx,
        &Options {
            verify: true,
            keep: 1,
            ..Options::default()
        },
    )
    .unwrap();
    assert!(!report.ok);
    assert!(
        report.removed.is_empty() && backups.join("dynodoc-20261019T000000Z").exists(),
        "a backup that did not verify must not cost an older one"
    );
    assert!(report
        .warnings
        .iter()
        .any(|w| w.contains("verification failed")));
}

#[test]
fn the_age_of_the_newest_backup_is_counted_in_days() {
    let dir = tempfile::tempdir().unwrap();
    testkit::installed(dir.path(), testkit::ENV_TEXT);
    let runner = FakeRunner::new();
    let confirm = ScriptedConfirm::new(&[]);
    let ctx = testkit::ctx(&runner, dir.path(), &confirm);
    let backups = dir.path().join("backups");
    assert!(newest_age_days(&ctx, &backups).is_none());
    testkit::make_backup(
        &backups,
        "dynodoc-a",
        "2026-10-15T12:00:00Z",
        "routine",
        None,
    );
    assert_eq!(newest_age_days(&ctx, &backups).unwrap().1, 7);
}
