//! Tests for the review fixes in `status`: an unfinished swap or upgrade is a problem, and Docker
//! being down is not reported as "no services exist".

use super::*;

#[test]
fn an_unreachable_docker_is_reported_as_such_not_as_no_services() {
    let (dir, runner) = healthy_installation(&production_env());
    runner.first(
        "ps -a --format json",
        1,
        "Cannot connect to the Docker daemon at unix:///var/run/docker.sock",
    );
    let r = collect_with(
        &runner,
        dir.path(),
        &net(false, Ok(())),
        testkit::fixed_now,
        &Options::default(),
    );
    assert_eq!(r.overall, "problems");
    assert!(
        r.problems.iter().any(|p| p.contains("cannot reach Docker")),
        "{:?}",
        r.problems
    );
    assert!(
        !r.problems.iter().any(|p| p.contains("no services exist")),
        "{:?}",
        r.problems
    );
}

#[test]
fn a_recorded_swap_journal_is_the_first_problem_and_names_the_command() {
    let (dir, runner) = healthy_installation(&production_env());
    crate::recover::write_journal(
        &crate::ctx::Layout::new(dir.path()),
        &crate::recover::Journal {
            version: 1,
            started_utc: "2026-10-22T11:00:00Z".into(),
            operation: "restore-into-live".into(),
            backup: "dynodoc-20261021T000000Z".into(),
            live: "dynodoc".into(),
            restoring: "dynodoc_restoring_x_ab12cd".into(),
            previous: "dynodoc_prev_x_ab12cd".into(),
            phase: "swapped".into(),
        },
    )
    .unwrap();
    let r = collect_with(
        &runner,
        dir.path(),
        &net(false, Ok(())),
        testkit::fixed_now,
        &Options::default(),
    );
    assert_eq!(r.overall, "problems");
    assert!(
        r.problems[0].contains("dynodoctl recover") && r.problems[0].contains("phase swapped"),
        "{:?}",
        r.problems
    );
    assert_eq!(r.exit_code(), crate::error::exit::FAILED);
}

#[test]
fn a_damaged_journal_is_a_problem_too() {
    let (dir, runner) = healthy_installation(&production_env());
    std::fs::create_dir_all(dir.path().join("state")).unwrap();
    std::fs::write(
        crate::recover::journal_path(&crate::ctx::Layout::new(dir.path())),
        "{ nope",
    )
    .unwrap();
    let r = collect_with(
        &runner,
        dir.path(),
        &net(false, Ok(())),
        testkit::fixed_now,
        &Options::default(),
    );
    assert!(
        r.problems.iter().any(|p| p.contains("damaged")),
        "{:?}",
        r.problems
    );
}

#[test]
fn an_upgrade_that_began_and_never_finished_is_a_problem() {
    let (dir, runner) = healthy_installation(&production_env());
    std::fs::create_dir_all(dir.path().join("state")).unwrap();
    let record = serde_json::json!({
        "id": "20261022T110000Z", "started_utc": "2026-10-22T11:00:00Z", "finished_utc": "",
        "from_version": "1.0.0", "from_registry": "r", "to_version": "1.1.0", "to_registry": "r",
        "result": "switching", "backup_dir": null, "backup_name": null,
        "saved_state_dir": null, "skipped_backup": false, "skipped_rehearsal": false,
        "rehearsal_ok": null, "schema_before": null, "schema_after": null, "steps": []
    });
    std::fs::write(
        dir.path()
            .join("state")
            .join("upgrade-20261022T110000Z.json"),
        record.to_string(),
    )
    .unwrap();
    let r = collect_with(
        &runner,
        dir.path(),
        &net(false, Ok(())),
        testkit::fixed_now,
        &Options::default(),
    );
    assert!(
        r.problems
            .iter()
            .any(|p| p.contains("did not finish") && p.contains("1.0.0 -> 1.1.0")),
        "{:?}",
        r.problems
    );
    // A later, finished record clears it.
    let done = record
        .to_string()
        .replace("switching", "completed")
        .replace("110000Z", "120000Z");
    std::fs::write(
        dir.path()
            .join("state")
            .join("upgrade-20261022T120000Z.json"),
        done,
    )
    .unwrap();
    let r2 = collect_with(
        &runner,
        dir.path(),
        &net(false, Ok(())),
        testkit::fixed_now,
        &Options::default(),
    );
    assert!(
        !r2.problems.iter().any(|p| p.contains("did not finish")),
        "{:?}",
        r2.problems
    );
}
