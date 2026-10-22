use super::*;
use crate::ctx::ScriptedConfirm;
use crate::runner::FakeRunner;
use crate::testkit::{self, assert_no_seeded_secret};

const SCRATCH: &str = "dynodoc_restore_20261022t120000z";
const RESTORING: &str = "dynodoc_restoring_20261022t120000z";
const PREVIOUS: &str = "dynodoc_prev_20261022t120000z";

fn setup() -> (tempfile::TempDir, FakeRunner, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    testkit::installed(dir.path(), testkit::ENV_TEXT);
    let backup = testkit::make_backup(
        &dir.path().join("backups"),
        "dynodoc-20261021T000000Z",
        "2026-10-21T00:00:00Z",
        "routine",
        Some(dir.path()),
    );
    (dir, testkit::healthy_runner(), backup)
}

#[test]
fn restore_verify_loads_the_backup_into_a_throwaway_database_verifies_and_removes_it() {
    let (dir, runner, _) = setup();
    let confirm = ScriptedConfirm::new(&[]);
    let ctx = testkit::ctx(&runner, dir.path(), &confirm);
    let report = restore_verify(&ctx, None, None).unwrap();
    assert!(report.ok, "{}", report.render());
    assert!(report.counts_match && report.history_verified && report.scratch_removed == Some(true));
    assert_eq!(report.documents_verified, 2);
    assert_eq!(report.database, SCRATCH);
    let create = runner
        .position(&format!("create database {SCRATCH}"))
        .unwrap();
    let load = runner
        .position(&format!(
            "pg_restore -h 127.0.0.1 -U dynodoc -d {SCRATCH} --no-owner --no-acl --exit-on-error"
        ))
        .unwrap();
    let verify = runner.position("verify-snapshots --all").unwrap();
    let each = runner.position("verified $n documents").unwrap();
    let drop = runner
        .position(&format!("drop database if exists {SCRATCH} with (force)"))
        .unwrap();
    assert!(
        create < load && load < verify && verify < each && each < drop,
        "{:?}",
        runner.lines()
    );
    let text = report.render();
    assert!(
        text.contains("PASSED")
            && text.contains("hash chain        verified (2 documents verified)")
            && text.contains("removed"),
        "{text}"
    );
}

#[test]
fn restore_verify_never_touches_the_live_database() {
    let (dir, runner, _) = setup();
    let confirm = ScriptedConfirm::new(&[]);
    let ctx = testkit::ctx(&runner, dir.path(), &confirm);
    restore_verify(&ctx, None, None).unwrap();
    for line in runner.lines() {
        assert!(!line.contains("alter database"), "{line}");
        assert!(
            !line.contains("-d dynodoc --no-owner"),
            "restore into the live database: {line}"
        );
        assert!(
            !line.contains("drop database if exists dynodoc with"),
            "{line}"
        );
        assert!(
            !line.contains("create database dynodoc ") && !line.contains("stop proxy"),
            "{line}"
        );
    }
}

#[test]
fn the_throwaway_database_is_removed_even_when_the_restore_fails() {
    let (dir, runner, _) = setup();
    runner.first(
        "pg_restore -h 127.0.0.1",
        1,
        "pg_restore: error: could not execute query",
    );
    let confirm = ScriptedConfirm::new(&[]);
    let ctx = testkit::ctx(&runner, dir.path(), &confirm);
    let report = restore_verify(&ctx, None, None).unwrap();
    assert!(!report.ok && !report.history_verified);
    assert_eq!(report.scratch_removed, Some(true));
    assert!(runner.ran(&format!("drop database if exists {SCRATCH} with (force)")));
    assert!(report.render().contains("FAILED"));
}

#[test]
fn a_history_that_does_not_verify_fails_the_report() {
    let (dir, runner, _) = setup();
    runner.first(
        "verify-snapshots --all",
        1,
        "Error: snapshot backfill failed for document x",
    );
    let confirm = ScriptedConfirm::new(&[]);
    let ctx = testkit::ctx(&runner, dir.path(), &confirm);
    let report = restore_verify(&ctx, None, None).unwrap();
    assert!(!report.ok && !report.history_verified);
    assert_eq!(report.scratch_removed, Some(true), "teardown still runs");
}

#[test]
fn the_restore_verify_report_requires_both_verification_commands() {
    let (dir, runner, _) = setup();
    let confirm = ScriptedConfirm::new(&[]);
    let ctx = testkit::ctx(&runner, dir.path(), &confirm);
    let report = restore_verify(&ctx, None, None).unwrap();
    let names: Vec<&str> = report.steps.iter().map(|s| s.name.as_str()).collect();
    assert!(
        names.contains(&"engine-cli verify-snapshots --all")
            && names.contains(&"engine-cli verify (each document)"),
        "{names:?}"
    );
    assert_eq!(runner.count("verify-snapshots --all"), 1);
    assert_eq!(runner.count("verified $n documents"), 1);
}

#[test]
fn a_per_document_failure_fails_the_report() {
    let (dir, runner, _) = setup();
    runner.first(
        "verified $n documents",
        1,
        "OK: document a\nError: event/snapshot verification failed for document b\n",
    );
    let confirm = ScriptedConfirm::new(&[]);
    let ctx = testkit::ctx(&runner, dir.path(), &confirm);
    let report = restore_verify(&ctx, None, None).unwrap();
    assert!(!report.ok);
    assert!(
        report
            .notes
            .iter()
            .any(|n| n.contains("verify failed (1 of 2 documents passed)")),
        "{:?}",
        report.notes
    );
}

#[test]
fn counts_that_differ_from_the_manifest_fail_the_restore() {
    let (dir, runner, _) = setup();
    runner.first("_sqlx_migrations where success", 0, "52|52|35|2|9|16.4\n");
    let confirm = ScriptedConfirm::new(&[]);
    let ctx = testkit::ctx(&runner, dir.path(), &confirm);
    let report = restore_verify(&ctx, None, None).unwrap();
    assert!(!report.ok && !report.counts_match);
    assert!(
        report
            .notes
            .iter()
            .any(|n| n.contains("events: backup 36..36, restored 35")),
        "{:?}",
        report.notes
    );
}

#[test]
fn counts_between_the_before_and_after_readings_are_accepted() {
    let mut m = crate::manifest::sample("routine", "2026-10-21T00:00:00Z");
    m.facts = testkit::facts();
    m.facts_after = DbFacts {
        events: 40,
        ..testkit::facts()
    };
    assert!(compare(
        &m,
        &DbFacts {
            events: 38,
            ..testkit::facts()
        }
    )
    .is_empty());
    assert!(!compare(
        &m,
        &DbFacts {
            events: 41,
            ..testkit::facts()
        }
    )
    .is_empty());
    assert!(!compare(
        &m,
        &DbFacts {
            events: 35,
            ..testkit::facts()
        }
    )
    .is_empty());
}

#[test]
fn a_damaged_backup_is_refused_before_any_database_command() {
    let (dir, runner, backup) = setup();
    std::fs::write(backup.join("database.dump"), b"PGDMP tampered").unwrap();
    let confirm = ScriptedConfirm::new(&[]);
    let ctx = testkit::ctx(&runner, dir.path(), &confirm);
    let report = restore_verify(&ctx, None, None).unwrap();
    assert!(!report.ok);
    assert!(
        report
            .notes
            .iter()
            .any(|n| n.contains("damaged or incomplete")),
        "{:?}",
        report.notes
    );
    assert!(
        !runner.ran("pg_restore -h"),
        "a damaged dump is never loaded"
    );
}

#[test]
fn no_backup_at_all_is_refused_with_the_command_to_make_one() {
    let dir = tempfile::tempdir().unwrap();
    testkit::installed(dir.path(), testkit::ENV_TEXT);
    let runner = testkit::healthy_runner();
    let confirm = ScriptedConfirm::new(&[]);
    let ctx = testkit::ctx(&runner, dir.path(), &confirm);
    let e = restore_verify(&ctx, None, None).unwrap_err();
    assert_eq!(e.code(), crate::error::exit::REFUSED);
    assert!(e.todo.contains("dynodoctl backup"));
}

#[test]
fn restore_verify_needs_a_running_database() {
    let (dir, runner, _) = setup();
    runner.first(
        "ps -a --format json",
        0,
        &testkit::ps_line("postgres", "exited", "", 0),
    );
    let confirm = ScriptedConfirm::new(&[]);
    let ctx = testkit::ctx(&runner, dir.path(), &confirm);
    let e = restore_verify(&ctx, None, None).unwrap_err();
    assert_eq!(e.code(), crate::error::exit::REFUSED);
    assert!(e.todo.contains("--into-live"));
}

#[test]
fn a_backup_can_be_named_by_directory_or_by_name() {
    let (dir, runner, backup) = setup();
    testkit::make_backup(
        &dir.path().join("backups"),
        "dynodoc-20261022T000000Z",
        "2026-10-22T00:00:00Z",
        "routine",
        None,
    );
    let confirm = ScriptedConfirm::new(&[]);
    let ctx = testkit::ctx(&runner, dir.path(), &confirm);
    assert_eq!(
        resolve_backup(&ctx, None).unwrap().name(),
        "dynodoc-20261022T000000Z"
    );
    assert_eq!(
        resolve_backup(&ctx, Some("latest")).unwrap().name(),
        "dynodoc-20261022T000000Z"
    );
    assert_eq!(
        resolve_backup(&ctx, Some("dynodoc-20261021T000000Z"))
            .unwrap()
            .name(),
        "dynodoc-20261021T000000Z"
    );
    assert_eq!(
        resolve_backup(&ctx, Some(&backup.display().to_string()))
            .unwrap()
            .name(),
        "dynodoc-20261021T000000Z"
    );
    assert!(resolve_backup(&ctx, Some("nope")).is_err());
}

// ---- restore --into-live ---------------------------------------------------------------------

fn live_opts() -> LiveOptions {
    LiveOptions {
        no_safety_backup: true,
        ..LiveOptions::default()
    }
}

#[test]
fn restoring_over_the_live_database_without_confirmation_changes_nothing() {
    let (dir, runner, _) = setup();
    let confirm = ScriptedConfirm::new(&[false]);
    let ctx = testkit::ctx(&runner, dir.path(), &confirm);
    let e = restore_live(&ctx, &live_opts()).unwrap_err();
    assert_eq!(e.code(), crate::error::exit::REFUSED);
    assert!(e.what.contains("not confirmed") && e.todo.contains(LIVE_PHRASE));
    assert_eq!(*confirm.asked.borrow(), vec![LIVE_PHRASE.to_string()]);
    for line in runner.lines() {
        for forbidden in [
            "create database",
            "drop database",
            "alter database",
            "pg_restore -h",
            "stop proxy",
            "up -d",
            "pg_dump",
        ] {
            assert!(
                !line.contains(forbidden),
                "ran `{forbidden}` without confirmation: {line}"
            );
        }
    }
}

#[test]
fn a_confirmed_live_restore_loads_beside_the_live_database_then_swaps_verifies_and_cleans_up() {
    let (dir, runner, _) = setup();
    let confirm = ScriptedConfirm::new(&[true]);
    let ctx = testkit::ctx(&runner, dir.path(), &confirm);
    let report = restore_live(&ctx, &live_opts()).unwrap();
    assert!(report.ok, "{}", report.render());
    let pos = |needle: &str| {
        runner
            .position(needle)
            .unwrap_or_else(|| panic!("`{needle}` did not run: {:?}", runner.lines()))
    };
    let order = [
        pos("up -d --wait --wait-timeout 180 postgres"),
        pos("stop proxy web api"),
        pos(&format!("create database {RESTORING}")),
        pos(&format!(
            "pg_restore -h 127.0.0.1 -U dynodoc -d {RESTORING}"
        )),
        pos("pg_terminate_backend"),
        pos(&format!("alter database dynodoc rename to {PREVIOUS}")),
        pos(&format!("alter database {RESTORING} rename to dynodoc")),
        pos("up -d --wait --wait-timeout 300"),
        pos("verify-snapshots --all"),
        pos("check-runtime"),
        pos(&format!("drop database if exists {PREVIOUS} with (force)")),
    ];
    assert!(
        order.windows(2).all(|w| w[0] < w[1]),
        "{order:?}\n{:?}",
        runner.lines()
    );
    assert!(report.counts_match && report.history_verified && report.documents_verified == 2);
    assert!(report
        .render()
        .contains("Restore report (restore into the live installation)"));
}

#[test]
fn a_damaged_dump_leaves_the_live_database_untouched() {
    let (dir, runner, _) = setup();
    runner.first(
        &format!("pg_restore -h 127.0.0.1 -U dynodoc -d {RESTORING}"),
        1,
        "pg_restore: error: invalid archive",
    );
    let confirm = ScriptedConfirm::new(&[true]);
    let ctx = testkit::ctx(&runner, dir.path(), &confirm);
    let report = restore_live(&ctx, &live_opts()).unwrap();
    assert!(!report.ok);
    assert!(
        !runner.ran("alter database"),
        "the live database was never renamed: {:?}",
        runner.lines()
    );
    assert!(runner.ran(&format!("drop database if exists {RESTORING} with (force)")));
    assert!(
        report
            .notes
            .iter()
            .any(|n| n.contains("live database was NOT changed")),
        "{:?}",
        report.notes
    );
}

#[test]
fn a_restore_that_does_not_verify_keeps_the_previous_database() {
    let (dir, runner, _) = setup();
    runner.first("verify-snapshots --all", 1, "Error: chain broken");
    let confirm = ScriptedConfirm::new(&[true]);
    let ctx = testkit::ctx(&runner, dir.path(), &confirm);
    let report = restore_live(&ctx, &live_opts()).unwrap();
    assert!(!report.ok);
    assert!(
        !runner.ran(&format!("drop database if exists {PREVIOUS}")),
        "the old data is the only copy left"
    );
}

#[test]
fn keep_previous_leaves_the_old_database_and_says_how_to_drop_it() {
    let (dir, runner, _) = setup();
    let confirm = ScriptedConfirm::new(&[true]);
    let ctx = testkit::ctx(&runner, dir.path(), &confirm);
    let report = restore_live(
        &ctx,
        &LiveOptions {
            keep_previous: true,
            ..live_opts()
        },
    )
    .unwrap();
    assert!(report.ok);
    assert!(!runner.ran(&format!("drop database if exists {PREVIOUS}")));
    assert!(report.notes.iter().any(|n| n.contains(PREVIOUS)));
}

#[test]
fn a_caller_that_already_confirmed_is_not_asked_again() {
    let (dir, runner, _) = setup();
    let confirm = ScriptedConfirm::new(&[]);
    let ctx = testkit::ctx(&runner, dir.path(), &confirm);
    restore_live(
        &ctx,
        &LiveOptions {
            confirmed: true,
            ..live_opts()
        },
    )
    .unwrap();
    assert!(confirm.asked.borrow().is_empty());
}

#[test]
fn the_current_database_is_saved_first_unless_it_is_empty_or_skipped() {
    let (dir, runner, _) = setup();
    let confirm = ScriptedConfirm::new(&[true]);
    let ctx = testkit::ctx(&runner, dir.path(), &confirm);
    let report = restore_live(&ctx, &LiveOptions::default()).unwrap();
    assert!(
        runner.position("pg_dump").unwrap()
            < runner
                .position(&format!("create database {RESTORING}"))
                .unwrap(),
        "{:?}",
        runner.lines()
    );
    assert!(report.notes.iter().any(|n| n.contains("saved in")));
    assert!(dir
        .path()
        .join("backups")
        .join("dynodoc-20261022T120000Z-before-restore")
        .is_dir());

    let (dir2, runner2, _) = setup();
    runner2.first("to_regclass", 0, "f\n");
    let confirm2 = ScriptedConfirm::new(&[true]);
    let ctx2 = testkit::ctx(&runner2, dir2.path(), &confirm2);
    restore_live(&ctx2, &LiveOptions::default()).unwrap();
    assert!(
        !runner2.ran("pg_dump"),
        "an empty database (a fresh volume) has nothing to save"
    );
}

#[test]
fn after_the_volume_is_deleted_the_settings_and_secrets_come_back_from_the_backup() {
    let (dir, runner, _) = setup();
    let secrets_before: Vec<(String, Vec<u8>)> = testkit::SEEDED
        .iter()
        .map(|(n, _)| {
            (
                n.to_string(),
                std::fs::read(dir.path().join("secrets").join(n)).unwrap(),
            )
        })
        .collect();
    std::fs::remove_dir_all(dir.path().join("secrets")).unwrap();
    std::fs::remove_file(dir.path().join(".env")).unwrap();
    assert!(!crate::ctx::Layout::new(dir.path()).is_installed());
    let confirm = ScriptedConfirm::new(&[true]);
    let ctx = testkit::ctx(&runner, dir.path(), &confirm);
    let report = restore_live(&ctx, &live_opts()).unwrap();
    assert!(report.ok, "{}", report.render());
    for (name, bytes) in secrets_before {
        assert_eq!(
            std::fs::read(dir.path().join("secrets").join(&name)).unwrap(),
            bytes,
            "{name}"
        );
    }
    #[cfg(unix)]
    assert_eq!(
        crate::util::unix_mode(&dir.path().join("secrets").join("session_secret")),
        Some(0o600)
    );
    assert!(runner
        .position("up -d --wait --wait-timeout 180 postgres")
        .is_some());
    assert_no_seeded_secret(&report.render(), "restore report");
}

#[test]
fn a_directory_with_no_settings_and_a_backup_without_a_copy_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    testkit::bundle(dir.path());
    testkit::make_backup(
        &dir.path().join("backups"),
        "dynodoc-20261021T000000Z",
        "2026-10-21T00:00:00Z",
        "routine",
        None,
    );
    let runner = testkit::healthy_runner();
    let confirm = ScriptedConfirm::new(&[true]);
    let ctx = testkit::ctx(&runner, dir.path(), &confirm);
    let e = restore_live(&ctx, &live_opts()).unwrap_err();
    assert_eq!(e.code(), crate::error::exit::REFUSED);
    assert!(e.todo.contains("--config-from"));
    assert!(runner.calls().is_empty());
}

#[test]
fn a_secret_that_differs_from_the_backup_is_kept_and_reported() {
    let (dir, runner, _) = setup();
    std::fs::write(
        dir.path().join("secrets").join("session_secret"),
        "a-newer-secret-from-another-install\n",
    )
    .unwrap();
    let confirm = ScriptedConfirm::new(&[true]);
    let ctx = testkit::ctx(&runner, dir.path(), &confirm);
    let report = restore_live(&ctx, &live_opts()).unwrap();
    assert_eq!(
        std::fs::read_to_string(dir.path().join("secrets").join("session_secret")).unwrap(),
        "a-newer-secret-from-another-install\n"
    );
    assert!(
        report
            .notes
            .iter()
            .any(|n| n.contains("secrets/session_secret differs")),
        "{:?}",
        report.notes
    );
}

#[test]
fn a_damaged_backup_is_refused_before_the_confirmation() {
    let (dir, runner, backup) = setup();
    std::fs::write(backup.join("database.dump"), b"tampered").unwrap();
    let confirm = ScriptedConfirm::new(&[true]);
    let ctx = testkit::ctx(&runner, dir.path(), &confirm);
    let e = restore_live(&ctx, &live_opts()).unwrap_err();
    assert_eq!(e.code(), crate::error::exit::REFUSED);
    assert!(confirm.asked.borrow().is_empty());
    assert!(runner.calls().is_empty());
}
