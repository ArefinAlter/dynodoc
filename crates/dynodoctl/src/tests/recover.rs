use super::*;
use crate::ctx::ScriptedConfirm;
use crate::runner::FakeRunner;
use crate::testkit;

const R: &str = "dynodoc_restoring_ab12cd";
const P: &str = "dynodoc_prev_ab12cd";

fn journal(phase: &str) -> Journal {
    Journal {
        version: 1,
        started_utc: "2026-10-22T12:00:00Z".into(),
        operation: "restore-into-live".into(),
        backup: "dynodoc-20261021T000000Z".into(),
        live: "dynodoc".into(),
        restoring: R.into(),
        previous: P.into(),
        phase: phase.into(),
    }
}

fn names(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| s.to_string()).collect()
}

fn setup(phase: Option<&str>, databases: &[&str]) -> (tempfile::TempDir, FakeRunner) {
    let dir = tempfile::tempdir().unwrap();
    testkit::installed(dir.path(), testkit::ENV_TEXT);
    let runner = testkit::healthy_runner();
    runner.first(
        "select datname from pg_database order by 1",
        0,
        &format!("{}\n", databases.join("\n")),
    );
    if let Some(phase) = phase {
        write_journal(&Layout::new(dir.path()), &journal(phase)).unwrap();
    }
    (dir, runner)
}

#[test]
fn the_plan_follows_which_databases_exist() {
    let swap_back = Action::SwapBack {
        live: "dynodoc".into(),
        previous: P.into(),
        restoring: R.into(),
    };
    // The swap never happened: drop the loaded copy, keep the live one.
    assert_eq!(
        plan(
            &journal(PHASE_PREPARED),
            &names(&["dynodoc", R, "postgres"])
        ),
        vec![Action::Drop(R.into()), Action::StartStack]
    );
    assert_eq!(
        plan(&journal(PHASE_PREPARED), &names(&["dynodoc", "postgres"])),
        vec![Action::StartStack]
    );
    // Swapped, not verified: put the old data back (one transaction).
    assert_eq!(
        plan(&journal(PHASE_SWAPPED), &names(&["dynodoc", P])),
        vec![swap_back.clone(), Action::StartStack]
    );
    assert_eq!(
        plan(&journal(PHASE_PREPARED), &names(&["dynodoc", P])),
        vec![swap_back, Action::StartStack]
    );
    // Swapped and verified: finish, never swap back.
    assert_eq!(
        plan(&journal(PHASE_VERIFIED), &names(&["dynodoc", P])),
        vec![Action::Finalize { drop: P.into() }, Action::StartStack]
    );
    // Between two renames (only if they were not one transaction): bring the old one back.
    assert_eq!(
        plan(&journal(PHASE_SWAPPED), &names(&[P, R])),
        vec![
            Action::Rename {
                from: P.into(),
                to: "dynodoc".into()
            },
            Action::Drop(R.into()),
            Action::StartStack
        ]
    );
}

#[test]
fn states_that_cannot_be_decided_automatically_say_so_and_drop_nothing() {
    for existing in [&["postgres"][..], &["dynodoc", R, P][..], &[R][..]] {
        let actions = plan(&journal(PHASE_SWAPPED), &names(existing));
        assert_eq!(actions.len(), 1, "{existing:?}");
        assert!(
            matches!(&actions[0], Action::Manual(_)),
            "{existing:?}: {actions:?}"
        );
        assert!(
            !actions.iter().any(|a| matches!(
                a,
                Action::Drop(_) | Action::Finalize { .. } | Action::SwapBack { .. }
            )),
            "{existing:?}"
        );
    }
}

#[test]
fn the_journal_round_trips_privately_and_a_damaged_one_is_reported_not_deleted() {
    let dir = tempfile::tempdir().unwrap();
    let layout = Layout::new(dir.path());
    assert_eq!(read_journal(&layout).unwrap(), None);
    write_journal(&layout, &journal(PHASE_SWAPPED)).unwrap();
    assert_eq!(read_journal(&layout).unwrap(), Some(journal(PHASE_SWAPPED)));
    #[cfg(unix)]
    assert_eq!(crate::util::unix_mode(&journal_path(&layout)), Some(0o600));
    // A byte order mark (a Windows editor, `Set-Content -Encoding utf8`) does not make it damaged.
    let with_bom = format!(
        "\u{feff}{}",
        std::fs::read_to_string(journal_path(&layout)).unwrap()
    );
    std::fs::write(journal_path(&layout), with_bom).unwrap();
    assert_eq!(read_journal(&layout).unwrap(), Some(journal(PHASE_SWAPPED)));
    std::fs::write(journal_path(&layout), "{ not json").unwrap();
    let err = read_journal(&layout).unwrap_err();
    assert!(
        err.what.contains("damaged") && err.todo.contains("do not delete"),
        "{err}"
    );
    assert!(journal_path(&layout).exists());
    clear_journal(&layout);
    assert!(!journal_path(&layout).exists());
}

#[test]
fn a_recorded_journal_stops_every_destructive_command_and_names_the_way_out() {
    let (dir, runner) = setup(Some(PHASE_SWAPPED), &["dynodoc", P]);
    let confirm = ScriptedConfirm::new(&[true, true]);
    let ctx = testkit::ctx(&runner, dir.path(), &confirm);
    let err = require_no_journal(&ctx).unwrap_err();
    assert_eq!(err.kind, crate::error::Kind::Refused);
    assert!(
        err.what.contains("restore-into-live") && err.what.contains("phase swapped"),
        "{err}"
    );
    assert!(err.todo.contains("dynodoctl recover"), "{err}");
    assert!(
        runner.calls().is_empty(),
        "the check reads a file, nothing else"
    );
}

#[test]
fn recover_with_no_journal_does_nothing() {
    let (dir, runner) = setup(None, &["dynodoc"]);
    let confirm = ScriptedConfirm::new(&[]);
    let ctx = testkit::ctx(&runner, dir.path(), &confirm);
    let report = recover(&ctx).unwrap();
    assert!(report.ok && report.journal.is_none());
    assert!(report.render().contains("Nothing to recover"));
    assert!(runner.calls().is_empty());
}

#[test]
fn recover_puts_the_old_database_back_in_one_transaction_and_clears_the_journal() {
    let (dir, runner) = setup(Some(PHASE_SWAPPED), &["dynodoc", P, "postgres"]);
    let confirm = ScriptedConfirm::new(&[]);
    let ctx = testkit::ctx(&runner, dir.path(), &confirm);
    let report = recover(&ctx).unwrap();
    assert!(report.ok, "{}", report.render());
    let swap = format!("begin; alter database dynodoc rename to {R}; alter database {P} rename to dynodoc; commit;");
    let swap_at = runner
        .position(&swap)
        .expect("the swap back is one transaction");
    assert!(
        runner.position("stop proxy web api").unwrap() < swap_at,
        "writers stop first"
    );
    assert!(runner.position("pg_terminate_backend").unwrap() < swap_at);
    assert!(
        runner.position("up -d --wait --wait-timeout 300").unwrap() > swap_at,
        "the stack starts after"
    );
    assert!(
        !runner.ran(&format!("drop database if exists {R}")),
        "the rejected copy is kept for inspection"
    );
    assert!(!journal_path(&Layout::new(dir.path())).exists());
    assert!(report.render().contains("consistent again"));
}

#[test]
fn recover_after_a_verified_swap_finishes_instead_of_undoing_it() {
    let (dir, runner) = setup(Some(PHASE_VERIFIED), &["dynodoc", P]);
    let confirm = ScriptedConfirm::new(&[]);
    let ctx = testkit::ctx(&runner, dir.path(), &confirm);
    let report = recover(&ctx).unwrap();
    assert!(report.ok, "{}", report.render());
    assert!(runner.ran(&format!("drop database if exists {P} with (force)")));
    assert!(!runner.ran("alter database"), "{:?}", runner.lines());
    assert!(!journal_path(&Layout::new(dir.path())).exists());
}

#[test]
fn recover_before_the_swap_drops_only_the_loaded_copy() {
    let (dir, runner) = setup(Some(PHASE_PREPARED), &["dynodoc", R, "postgres"]);
    let confirm = ScriptedConfirm::new(&[]);
    let ctx = testkit::ctx(&runner, dir.path(), &confirm);
    let report = recover(&ctx).unwrap();
    assert!(report.ok, "{}", report.render());
    assert!(runner.ran(&format!("drop database if exists {R} with (force)")));
    assert!(
        !runner.ran("drop database if exists dynodoc "),
        "{:?}",
        runner.lines()
    );
    assert!(!runner.ran("alter database"));
}

#[test]
fn a_swap_back_that_fails_keeps_the_journal_and_reports_not_finished() {
    let (dir, runner) = setup(Some(PHASE_SWAPPED), &["dynodoc", P]);
    runner.first("alter database", 1, "ERROR: boom");
    let confirm = ScriptedConfirm::new(&[]);
    let ctx = testkit::ctx(&runner, dir.path(), &confirm);
    let report = recover(&ctx).unwrap();
    assert!(!report.ok);
    assert!(
        journal_path(&Layout::new(dir.path())).exists(),
        "the journal stays until the databases are right"
    );
    assert!(report.render().contains("NOT finished"));
    assert!(
        !runner.ran("up -d --wait --wait-timeout 300"),
        "the stack is not started on unknown ground"
    );
}

#[test]
fn a_state_that_needs_a_person_keeps_the_journal_and_drops_nothing() {
    let (dir, runner) = setup(Some(PHASE_SWAPPED), &["postgres"]);
    let confirm = ScriptedConfirm::new(&[]);
    let ctx = testkit::ctx(&runner, dir.path(), &confirm);
    let report = recover(&ctx).unwrap();
    assert!(!report.ok);
    assert!(journal_path(&Layout::new(dir.path())).exists());
    assert!(
        !runner.ran("drop database") && !runner.ran("alter database"),
        "{:?}",
        runner.lines()
    );
    assert!(report.actions.iter().any(|a| a.starts_with("MANUAL")));
}

#[test]
fn the_manual_commands_are_exact_and_come_with_a_condition() {
    let lines = manual_commands(&journal(PHASE_SWAPPED));
    let text = lines.join("\n");
    assert!(text.contains("docker compose stop proxy web api"));
    assert!(text.contains(&format!("begin; alter database dynodoc rename to {R}; alter database {P} rename to dynodoc; commit;")), "{text}");
    assert!(text.contains(&format!("only if {P} exists")));
    assert!(text.contains("rm state/restore-journal.json"));
    assert!(
        text.contains("select datname from pg_database"),
        "look before you rename"
    );
}
