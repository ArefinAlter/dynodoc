use super::*;
use crate::ctx::ScriptedConfirm;
use crate::runner::FakeRunner;
use crate::testkit;

const SCRATCH: &str = "dynodoc_restore_20261022t120000z";

fn setup() -> (tempfile::TempDir, FakeRunner) {
    let dir = tempfile::tempdir().unwrap();
    testkit::installed(dir.path(), testkit::ENV_TEXT);
    testkit::make_backup(
        &dir.path().join("backups"),
        "dynodoc-20261021T000000Z",
        "2026-10-21T00:00:00Z",
        "routine",
        Some(dir.path()),
    );
    (dir, testkit::healthy_runner())
}

#[test]
fn the_rehearsal_runs_the_target_migrations_on_a_copy_and_removes_it() {
    let (dir, runner) = setup();
    let confirm = ScriptedConfirm::new(&[]);
    let ctx = testkit::ctx(&runner, dir.path(), &confirm);
    let target = Target {
        version: Some("1.1.0".into()),
        registry: Some("registry.example.org/dynodoc".into()),
        overlay: None,
    };
    let report = run(&ctx, None, None, &target).unwrap();
    assert!(report.ok, "{}", report.render());
    assert!(report.api_smoke && report.scratch_removed);
    assert_eq!(
        (
            report.installed_version.as_str(),
            report.target_version.as_str()
        ),
        ("dev", "1.1.0")
    );
    let pos = |needle: &str| {
        runner
            .position(needle)
            .unwrap_or_else(|| panic!("`{needle}` did not run: {:?}", runner.lines()))
    };
    let order = [
        pos(&format!("create database {SCRATCH}")),
        pos(&format!("pg_restore -h 127.0.0.1 -U dynodoc -d {SCRATCH}")),
        pos(&format!("revoke all on database {SCRATCH} from public")),
        pos(&format!(
            "grant connect on database {SCRATCH} to dynodoc_runtime"
        )),
        pos("sh dynodoc_restore_20261022t120000z migrate"),
        pos("grant-runtime dynodoc_runtime"),
        pos("check-runtime"),
        pos("verify-snapshots --all"),
        pos("engine-api"),
        pos(&format!("drop database if exists {SCRATCH} with (force)")),
    ];
    assert!(
        order.windows(2).all(|w| w[0] < w[1]),
        "{order:?}\n{:?}",
        runner.lines()
    );
}

#[test]
fn the_target_images_are_used_through_process_variables_and_nothing_is_written_to_env() {
    let (dir, runner) = setup();
    let before = std::fs::read_to_string(dir.path().join(".env")).unwrap();
    let confirm = ScriptedConfirm::new(&[]);
    let ctx = testkit::ctx(&runner, dir.path(), &confirm);
    let target = Target {
        version: Some("1.1.0".into()),
        registry: Some("registry.example.org/dynodoc".into()),
        overlay: None,
    };
    run(&ctx, None, None, &target).unwrap();
    let migrate = runner
        .calls()
        .into_iter()
        .find(|c| c.line().contains(" migrate"))
        .unwrap();
    assert!(migrate
        .env
        .contains(&("DYNODOC_VERSION".to_string(), "1.1.0".to_string())));
    assert!(migrate.env.contains(&(
        "DYNODOC_IMAGE_REGISTRY".to_string(),
        "registry.example.org/dynodoc".to_string()
    )));
    let psql = runner
        .calls()
        .into_iter()
        .find(|c| c.line().contains("create database"))
        .unwrap();
    assert!(
        psql.env.is_empty(),
        "database work uses the installed postgres, not the target"
    );
    assert_eq!(
        std::fs::read_to_string(dir.path().join(".env")).unwrap(),
        before
    );
}

#[test]
fn a_release_overlay_replaces_the_compose_files_of_the_target_commands() {
    let (dir, runner) = setup();
    let overlay = dir.path().join("new-overlay.yml");
    std::fs::write(&overlay, "services: {}\n").unwrap();
    let confirm = ScriptedConfirm::new(&[]);
    let ctx = testkit::ctx(&runner, dir.path(), &confirm);
    run(
        &ctx,
        None,
        None,
        &Target {
            overlay: Some(overlay),
            ..Target::default()
        },
    )
    .unwrap();
    let migrate = runner
        .lines()
        .into_iter()
        .find(|l| l.contains(" migrate"))
        .unwrap();
    assert!(migrate.contains("new-overlay.yml"), "{migrate}");
}

#[test]
fn a_migration_failure_fails_the_rehearsal_and_still_removes_the_copy() {
    let (dir, runner) = setup();
    runner.first(
        "migrate",
        1,
        "Error: migration 53 failed: column does not exist",
    );
    let confirm = ScriptedConfirm::new(&[]);
    let ctx = testkit::ctx(&runner, dir.path(), &confirm);
    let report = run(&ctx, None, None, &Target::default()).unwrap();
    assert!(!report.ok && !report.api_smoke);
    assert!(
        report.scratch_removed
            && runner.ran(&format!("drop database if exists {SCRATCH} with (force)"))
    );
    assert!(
        report
            .notes
            .iter()
            .any(|n| n.contains("target release cannot migrate")),
        "{:?}",
        report.notes
    );
    assert!(
        !runner.ran("engine-api"),
        "nothing runs after a failed migration"
    );
}

#[test]
fn a_failing_history_check_or_api_smoke_fails_the_rehearsal() {
    let (dir, runner) = setup();
    runner.first("verify-snapshots --all", 1, "Error: chain broken");
    let confirm = ScriptedConfirm::new(&[]);
    let ctx = testkit::ctx(&runner, dir.path(), &confirm);
    assert!(!run(&ctx, None, None, &Target::default()).unwrap().ok);

    let (dir2, runner2) = setup();
    runner2.first("engine-api", 1, "api smoke: /healthz did not answer 200");
    let ctx2 = testkit::ctx(&runner2, dir2.path(), &confirm);
    let r2 = run(&ctx2, None, None, &Target::default()).unwrap();
    assert!(!r2.ok && !r2.api_smoke);
}

#[test]
fn the_report_is_saved_for_the_change_record() {
    let (dir, runner) = setup();
    let confirm = ScriptedConfirm::new(&[]);
    let ctx = testkit::ctx(&runner, dir.path(), &confirm);
    run(&ctx, None, None, &Target::default()).unwrap();
    let saved = std::fs::read_to_string(
        dir.path()
            .join("state")
            .join("rehearsal-20261022T120000Z.json"),
    )
    .unwrap();
    let parsed: RehearsalReport = serde_json::from_str(&saved).unwrap();
    assert!(
        parsed.ok && parsed.kind == "rehearse-lite" && parsed.backup == "dynodoc-20261021T000000Z"
    );
    assert_eq!(parsed.backup_sha256.len(), 64);
}

#[test]
fn it_needs_a_backup_and_a_running_database() {
    let dir = tempfile::tempdir().unwrap();
    testkit::installed(dir.path(), testkit::ENV_TEXT);
    let runner = testkit::healthy_runner();
    let confirm = ScriptedConfirm::new(&[]);
    let ctx = testkit::ctx(&runner, dir.path(), &confirm);
    assert!(run(&ctx, None, None, &Target::default())
        .unwrap_err()
        .todo
        .contains("dynodoctl backup"));
    let (dir2, runner2) = setup();
    runner2.first("ps -a --format json", 0, "");
    let ctx2 = testkit::ctx(&runner2, dir2.path(), &confirm);
    assert_eq!(
        run(&ctx2, None, None, &Target::default())
            .unwrap_err()
            .code(),
        crate::error::exit::REFUSED
    );
}

#[test]
fn the_rehearsal_never_names_the_live_database_in_a_destructive_command() {
    let (dir, runner) = setup();
    let confirm = ScriptedConfirm::new(&[]);
    let ctx = testkit::ctx(&runner, dir.path(), &confirm);
    run(&ctx, None, None, &Target::default()).unwrap();
    for line in runner.lines() {
        assert!(
            !line.contains("alter database") && !line.contains("rename to"),
            "{line}"
        );
        assert!(
            !line.contains("drop database if exists dynodoc with")
                && !line.contains("create database dynodoc "),
            "{line}"
        );
        assert!(!line.contains("stop proxy"), "{line}");
    }
}
