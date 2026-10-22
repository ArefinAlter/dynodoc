//! CLI-level tests for the review fixes: the run lock (M2), the `recover` command (H3) and the
//! panic guard (M4).

use super::*;

fn lock_file(dir: &std::path::Path) -> std::path::PathBuf {
    dir.join("state").join(".lock")
}

#[test]
fn a_second_mutating_run_fails_fast_while_the_lock_is_held() {
    let dir = production_dir();
    std::fs::create_dir_all(dir.path().join("state")).unwrap();
    // The lock of a live process: this very test process.
    std::fs::write(
        lock_file(dir.path()),
        format!(
            "pid={}\nstarted=2026-10-22T11:59:00Z\ncommand=backup\n",
            std::process::id()
        ),
    )
    .unwrap();
    for args in [
        &["backup"][..],
        &["restore-verify"][..],
        &["restore", "--into-live"][..],
        &["rehearse-lite"][..],
        &["upgrade", "--local"][..],
        &["rollback"][..],
        &["recover"][..],
        &["configure", "set", "org-name=x"][..],
        &["configure", "apply"][..],
        &["install"][..],
    ] {
        let runner = testkit::healthy_runner();
        let (code, _, err) = run_cli(args, dir.path(), &runner, &[true, true]);
        assert_eq!(code, crate::error::exit::REFUSED, "{args:?}: {err}");
        assert!(
            err.contains("another dynodoctl is already running"),
            "{args:?}: {err}"
        );
        assert!(runner.calls().is_empty(), "{args:?}: {:?}", runner.lines());
    }
    // The lock is not removed by a run that did not take it.
    assert!(lock_file(dir.path()).exists());
    // Read-only commands do not need it.
    let runner = testkit::healthy_runner();
    let (_, _, err) = run_cli(&["status"], dir.path(), &runner, &[]);
    assert!(!err.contains("another dynodoctl"), "{err}");
}

#[test]
fn a_run_takes_the_lock_while_it_works_and_releases_it_after() {
    let dir = production_dir();
    let runner = testkit::healthy_runner();
    let path = lock_file(dir.path());
    let seen = std::rc::Rc::new(std::cell::Cell::new(false));
    let flag = seen.clone();
    runner.on_effect("pg_dump", move |_| flag.set(path.exists()));
    let (code, _, err) = run_cli(&["backup"], dir.path(), &runner, &[]);
    assert_eq!(code, crate::error::exit::OK, "{err}");
    assert!(seen.get(), "the lock existed while the backup ran");
    assert!(!lock_file(dir.path()).exists(), "and is gone afterwards");
}

#[test]
fn a_lock_left_by_a_process_that_no_longer_exists_is_replaced() {
    let dir = production_dir();
    std::fs::create_dir_all(dir.path().join("state")).unwrap();
    // Process ids this large do not exist on any supported system.
    std::fs::write(
        lock_file(dir.path()),
        "pid=4294967290\nstarted=2026-10-22T11:59:00Z\ncommand=backup\n",
    )
    .unwrap();
    let runner = testkit::healthy_runner();
    let (code, _, err) = run_cli(&["backup"], dir.path(), &runner, &[]);
    assert_eq!(code, crate::error::exit::OK, "{err}");
}

#[test]
fn the_lock_is_released_even_when_the_command_fails() {
    let dir = production_dir();
    let runner = testkit::healthy_runner();
    runner.first("pg_dump", 1, "boom");
    let (code, _, _) = run_cli(&["backup"], dir.path(), &runner, &[]);
    assert_ne!(code, crate::error::exit::OK);
    assert!(!lock_file(dir.path()).exists());
}

#[test]
fn recover_with_nothing_recorded_says_so() {
    let dir = production_dir();
    let runner = testkit::healthy_runner();
    let (code, out, err) = run_cli(&["recover"], dir.path(), &runner, &[]);
    assert_eq!(code, crate::error::exit::OK, "{err}");
    assert!(out.contains("Nothing to recover"), "{out}");
}

#[test]
fn recover_runs_the_recorded_plan_and_exits_zero() {
    let dir = production_dir();
    let layout = crate::ctx::Layout::new(dir.path());
    crate::recover::write_journal(
        &layout,
        &crate::recover::Journal {
            version: 1,
            started_utc: "2026-10-22T12:00:00Z".into(),
            operation: "restore-into-live".into(),
            backup: "b".into(),
            live: "dynodoc".into(),
            restoring: "dynodoc_restoring_20261022t120000z_ab12cd".into(),
            previous: "dynodoc_prev_20261022t120000z_ab12cd".into(),
            phase: "swapped".into(),
        },
    )
    .unwrap();
    let runner = testkit::healthy_runner();
    runner.first(
        "select datname from pg_database order by 1",
        0,
        "dynodoc\ndynodoc_prev_20261022t120000z_ab12cd\npostgres\n",
    );
    let (code, out, err) = run_cli(&["recover"], dir.path(), &runner, &[]);
    assert_eq!(code, crate::error::exit::OK, "{out}\n{err}");
    assert!(out.contains("consistent again"), "{out}");
    assert!(!crate::recover::journal_path(&layout).exists());
}

#[test]
fn a_recorded_journal_makes_every_destructive_command_refuse_with_exit_3() {
    let dir = production_dir();
    let layout = crate::ctx::Layout::new(dir.path());
    crate::recover::write_journal(
        &layout,
        &crate::recover::new_journal(
            &{
                let r = FakeRunner::new();
                let c = ScriptedConfirm::new(&[]);
                let mut ctx = crate::ctx::Ctx::new(
                    Box::leak(Box::new(r)),
                    layout.clone(),
                    Box::leak(Box::new(c)),
                );
                ctx.now = testkit::fixed_now;
                ctx
            },
            "b",
            "dynodoc_restoring_x_ab12cd",
            "dynodoc_prev_x_ab12cd",
        ),
    )
    .unwrap();
    for args in [
        &["restore", "--into-live"][..],
        &["upgrade", "--local"][..],
        &["rollback"][..],
    ] {
        let runner = testkit::healthy_runner();
        let (code, _, err) = run_cli(args, dir.path(), &runner, &[true, true]);
        assert_eq!(code, crate::error::exit::REFUSED, "{args:?}: {err}");
        assert!(err.contains("dynodoctl recover"), "{args:?}: {err}");
        assert!(
            !runner.ran("stop proxy") && !runner.ran("alter database"),
            "{args:?}"
        );
    }
}

struct PanickingRunner;

impl crate::runner::Runner for PanickingRunner {
    fn run(&self, _: &crate::runner::Cmd) -> std::io::Result<crate::runner::Output> {
        panic!("deliberate test panic");
    }
}

#[test]
fn a_panic_is_reported_as_a_structured_error_not_a_silent_exit() {
    let dir = production_dir();
    for json in [false, true] {
        let mut full: Vec<String> = vec![
            "dynodoctl".into(),
            "--dir".into(),
            dir.path().display().to_string(),
        ];
        if json {
            full.push("--json".into());
        }
        full.push("status".into());
        let confirm = ScriptedConfirm::new(&[]);
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let code = run(
            full,
            &PanickingRunner,
            &|_| Box::new(good_net()),
            Some(&confirm),
            &mut out,
            &mut err,
        );
        let (out, err) = (
            String::from_utf8(out).unwrap(),
            String::from_utf8(err).unwrap(),
        );
        assert_eq!(code, crate::error::exit::FAILED);
        assert!(err.contains("internal error"), "{err}");
        if json {
            let v: serde_json::Value = serde_json::from_str(out.trim()).unwrap();
            assert_eq!(v["ok"], false);
            assert_eq!(v["error"]["exit_code"], 1);
            assert!(v["error"]["what"]
                .as_str()
                .unwrap()
                .contains("internal error"));
        }
    }
}
