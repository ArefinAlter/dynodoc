//! Tests for the review fixes: H1 (confinement, SQL screen), H3 (journal, unwind, interrupts),
//! M2 (what may be dropped), M3 (safety backup), M9 (project guard).

use super::*;

fn alter_calls(runner: &FakeRunner) -> Vec<String> {
    runner
        .lines()
        .into_iter()
        .filter(|l| l.contains("alter database"))
        .collect()
}

fn journal_file(dir: &std::path::Path) -> PathBuf {
    dir.join("state").join("restore-journal.json")
}

fn last_index(runner: &FakeRunner, needle: &str) -> Option<usize> {
    runner.lines().iter().rposition(|l| l.contains(needle))
}

const SWAP_IN: &str = "begin; alter database dynodoc rename to dynodoc_prev_20261022t120000z_ab12cd; alter database dynodoc_restoring_20261022t120000z_ab12cd rename to dynodoc; commit;";
const SWAP_BACK: &str = "begin; alter database dynodoc rename to dynodoc_restoring_20261022t120000z_ab12cd; alter database dynodoc_prev_20261022t120000z_ab12cd rename to dynodoc; commit;";

type Inject = fn(&FakeRunner);

#[test]
fn a_failure_at_any_step_leaves_the_old_database_live_and_the_services_running() {
    // H3 (a), (e), (f): fault injection at every step of a live restore. Each case must end with
    // the previous data live (never swapped, or swapped and swapped back), the stack started
    // again after the writers were stopped, and no journal left behind.
    let cases: Vec<(&str, Inject, usize)> = vec![
        (
            "the writers cannot be stopped",
            |r| {
                r.first("stop proxy web api", 1, "boom");
            },
            0,
        ),
        (
            "the new database cannot be created",
            |r| {
                r.first("create database dynodoc_restoring", 1, "boom");
            },
            0,
        ),
        (
            "pg_restore fails",
            |r| {
                r.first(
                    "pg_restore -h 127.0.0.1 -U dynodoc -d dynodoc_restoring",
                    1,
                    "boom",
                );
            },
            0,
        ),
        (
            "the restored copy has the wrong counts",
            |r| {
                r.first_seq(
                    "_sqlx_migrations where success",
                    &[(0, "52|52|1|2|9|16.4\n")],
                );
            },
            0,
        ),
        (
            "the swap itself fails (nothing is renamed)",
            |r| {
                r.first("alter database", 1, "boom");
            },
            1,
        ),
        (
            "the stack does not start on the restored copy",
            |r| {
                r.first_seq("up -d --wait --wait-timeout 300", &[(1, "boom"), (0, "")]);
            },
            2,
        ),
        (
            "the live counts differ after the swap",
            |r| {
                r.first_seq(
                    "_sqlx_migrations where success",
                    &[(0, testkit::FACTS_LINE), (0, "52|52|1|2|9|16.4\n")],
                );
            },
            2,
        ),
        (
            "verify-snapshots fails on the live database",
            |r| {
                r.first("verify-snapshots --all", 1, "chain broken");
            },
            2,
        ),
        (
            "a per-document verify fails on the live database",
            |r| {
                r.first("verified $n documents", 1, "chain broken");
            },
            2,
        ),
        (
            "check-runtime fails after the swap",
            |r| {
                r.first("check-runtime", 1, "too much access");
            },
            2,
        ),
    ];
    for (label, inject, alters) in cases {
        let (dir, runner, _) = setup();
        inject(&runner);
        let confirm = ScriptedConfirm::new(&[true]);
        let ctx = testkit::ctx(&runner, dir.path(), &confirm);
        let report = restore_live(&ctx, &live_opts()).unwrap();
        let ctxt = format!("{label}\n{}\n{:?}", report.render(), runner.lines());
        assert!(!report.ok, "{ctxt}");
        assert_eq!(alter_calls(&runner).len(), alters, "{ctxt}");
        if alters == 2 {
            assert!(alter_calls(&runner)[0].contains(SWAP_IN), "{ctxt}");
            assert!(alter_calls(&runner)[1].contains(SWAP_BACK), "{ctxt}");
            assert!(report.swapped_back && !report.live_changed, "{ctxt}");
            assert!(
                report
                    .notes
                    .iter()
                    .any(|n| n.contains("previous database is live again")),
                "{ctxt}"
            );
        } else {
            assert!(!report.swapped_back && !report.live_changed, "{ctxt}");
            assert!(
                report
                    .notes
                    .iter()
                    .any(|n| n.contains("live database was NOT changed")),
                "{ctxt}"
            );
        }
        // The services were stopped, then started again, last.
        if let Some(stopped) = last_index(&runner, "stop proxy web api") {
            let started = last_index(&runner, "up -d --wait --wait-timeout 300")
                .unwrap_or_else(|| panic!("the stack was never started again: {ctxt}"));
            assert!(started > stopped, "{ctxt}");
        }
        assert!(!journal_file(dir.path()).exists(), "{ctxt}");
        assert!(!runner.ran("drop database if exists dynodoc "), "{ctxt}");
        // The previous database is never dropped on a failure.
        assert!(
            !runner.ran("drop database if exists dynodoc_prev"),
            "{ctxt}"
        );
    }
}

#[test]
fn the_journal_exists_while_the_swap_is_open_and_is_gone_afterwards() {
    // H3: written BEFORE the swap, removed only when the installation is consistent.
    let (dir, runner, _) = setup();
    let seen: std::rc::Rc<std::cell::RefCell<Vec<String>>> = Default::default();
    let (path, sink) = (journal_file(dir.path()), seen.clone());
    runner.on_effect("begin; alter database", move |_| {
        sink.borrow_mut()
            .push(std::fs::read_to_string(&path).unwrap_or_default());
    });
    let (path2, sink2) = (journal_file(dir.path()), seen.clone());
    runner.on_effect("up -d --wait --wait-timeout 300", move |_| {
        sink2
            .borrow_mut()
            .push(std::fs::read_to_string(&path2).unwrap_or_default());
    });
    let confirm = ScriptedConfirm::new(&[true]);
    let ctx = testkit::ctx(&runner, dir.path(), &confirm);
    let report = restore_live(&ctx, &live_opts()).unwrap();
    assert!(report.ok, "{}", report.render());
    let seen = seen.borrow();
    assert!(
        seen[0].contains("\"phase\": \"prepared\"") && seen[0].contains("dynodoc_restoring_"),
        "{seen:?}"
    );
    assert!(seen[1].contains("\"phase\": \"swapped\""), "{seen:?}");
    assert!(!journal_file(dir.path()).exists());
}

#[test]
fn a_swap_back_that_fails_keeps_the_journal_and_prints_the_exact_commands() {
    // H3 (f): the put-back result is not ignored.
    let (dir, runner, _) = setup();
    runner.first("up -d --wait --wait-timeout 300", 1, "boom");
    runner.first_seq("alter database", &[(0, ""), (1, "ERROR: could not rename")]);
    let confirm = ScriptedConfirm::new(&[true]);
    let ctx = testkit::ctx(&runner, dir.path(), &confirm);
    let report = restore_live(&ctx, &live_opts()).unwrap();
    assert!(
        !report.ok && report.live_changed && !report.swapped_back,
        "{}",
        report.render()
    );
    assert!(
        journal_file(dir.path()).exists(),
        "the journal stays for `dynodoctl recover`"
    );
    let text = report.notes.join("\n");
    assert!(
        text.contains("COULD NOT put the previous database back"),
        "{text}"
    );
    assert!(
        text.contains("dynodoctl recover") && text.contains(SWAP_BACK),
        "{text}"
    );
    assert!(
        !report
            .notes
            .iter()
            .any(|n| n.contains("previous database is live again")),
        "{text}"
    );
    // And now every destructive command refuses until it has been recovered.
    let confirm2 = ScriptedConfirm::new(&[true]);
    let runner2 = testkit::healthy_runner();
    let ctx2 = testkit::ctx(&runner2, dir.path(), &confirm2);
    let e = restore_live(&ctx2, &live_opts()).unwrap_err();
    assert_eq!(e.kind, crate::error::Kind::Refused);
    assert!(e.todo.contains("dynodoctl recover"), "{e}");
    assert!(runner2.calls().is_empty());
}

#[test]
fn an_interrupt_after_the_swap_swaps_back_and_restarts() {
    // H3 (b): a signal sets a flag; the next step does not start, and the unwind puts things back.
    let (dir, runner, _) = setup();
    let confirm = ScriptedConfirm::new(&[true]);
    let ctx = testkit::ctx(&runner, dir.path(), &confirm);
    let flag = ctx.interrupt.clone();
    runner.on_effect("up -d --wait --wait-timeout 300", move |_| {
        flag.store(true, std::sync::atomic::Ordering::SeqCst);
    });
    let report = restore_live(&ctx, &live_opts()).unwrap();
    let text = report.render();
    assert!(!report.ok && report.swapped_back, "{text}");
    assert!(
        report.notes.iter().any(|n| n.contains("interrupted")),
        "{text}"
    );
    assert_eq!(alter_calls(&runner).len(), 2, "{text}");
    assert!(alter_calls(&runner)[1].contains(SWAP_BACK));
    assert!(
        last_index(&runner, "up -d --wait --wait-timeout 300").unwrap()
            > runner.position(SWAP_BACK).unwrap()
    );
    assert!(!journal_file(dir.path()).exists());
}

#[test]
fn an_interrupt_before_the_swap_removes_the_loaded_copy_and_changes_nothing() {
    let (dir, runner, _) = setup();
    let confirm = ScriptedConfirm::new(&[true]);
    let ctx = testkit::ctx(&runner, dir.path(), &confirm);
    let flag = ctx.interrupt.clone();
    runner.on_effect("pg_restore -h 127.0.0.1", move |_| {
        flag.store(true, std::sync::atomic::Ordering::SeqCst);
    });
    let report = restore_live(&ctx, &live_opts()).unwrap();
    assert!(!report.ok && !report.live_changed, "{}", report.render());
    assert!(alter_calls(&runner).is_empty());
    assert!(runner
        .ran("drop database if exists dynodoc_restoring_20261022t120000z_ab12cd with (force)"));
    assert!(
        last_index(&runner, "up -d --wait --wait-timeout 300").unwrap()
            > last_index(&runner, "stop proxy web api").unwrap()
    );
}

#[test]
fn a_dump_with_dangerous_sql_is_refused_before_any_service_is_stopped() {
    // H1: pg_restore runs the dump's SQL as the superuser. The screen refuses it, with no outage.
    for hostile in [
        "COPY t FROM PROGRAM 'id > /tmp/x';",
        "ALTER SYSTEM SET archive_command = 'sh -c evil';",
        "CREATE EXTENSION file_fdw;",
        "CREATE EVENT TRIGGER t ON ddl_command_start EXECUTE FUNCTION f();",
        "CREATE ROLE evil SUPERUSER;",
        "CREATE FUNCTION f() RETURNS void LANGUAGE plperlu AS $$ system('id') $$;",
    ] {
        let (dir, runner, _) = setup();
        runner.on_file(
            "pg_restore --no-owner -f -",
            format!("{hostile}\n").as_bytes(),
        );
        let confirm = ScriptedConfirm::new(&[true]);
        let ctx = testkit::ctx(&runner, dir.path(), &confirm);
        let e = restore_live(&ctx, &live_opts()).unwrap_err();
        assert_eq!(e.kind, crate::error::Kind::Refused, "{hostile}: {e}");
        assert!(
            e.what.contains("SQL that a Dynodoc backup never has"),
            "{hostile}: {e}"
        );
        for forbidden in [
            "stop proxy",
            "create database",
            "pg_restore -h",
            "alter database",
            "pg_dump",
        ] {
            assert!(
                !runner.ran(forbidden),
                "{hostile}: ran {forbidden}: {:?}",
                runner.lines()
            );
        }
        assert!(!journal_file(dir.path()).exists());
        // The rendered script (a readable copy of the database) is not left behind.
        let leftovers: Vec<String> = std::fs::read_dir(dir.path().join("state"))
            .map(|d| {
                d.flatten()
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default();
        assert!(
            !leftovers.iter().any(|n| n.contains("review")),
            "{leftovers:?}"
        );
    }
}

#[test]
fn restore_verify_refuses_a_hostile_dump_without_creating_a_database() {
    let (dir, runner, _) = setup();
    runner.on_file("pg_restore --no-owner -f -", b"COPY t FROM PROGRAM 'id';\n");
    let confirm = ScriptedConfirm::new(&[]);
    let ctx = testkit::ctx(&runner, dir.path(), &confirm);
    let report = restore_verify(&ctx, None, None).unwrap();
    assert!(!report.ok);
    assert!(
        report
            .notes
            .iter()
            .any(|n| n.contains("SQL that a Dynodoc backup never has")),
        "{:?}",
        report.notes
    );
    assert!(!runner.ran("create database"), "{:?}", runner.lines());
    assert!(
        !runner.ran("drop database"),
        "nothing was created, so nothing is dropped"
    );
}

#[test]
fn the_screen_lets_an_ordinary_dump_through() {
    let (dir, runner, _) = setup();
    runner.on_file(
        "pg_restore --no-owner -f -",
        b"SET statement_timeout = 0;\nCREATE EXTENSION IF NOT EXISTS pgcrypto WITH SCHEMA public;\nCREATE TABLE public.event (id bigint);\nCOPY public.event (id) FROM stdin;\n1\n\\.\n",
    );
    let confirm = ScriptedConfirm::new(&[]);
    let ctx = testkit::ctx(&runner, dir.path(), &confirm);
    let report = restore_verify(&ctx, None, None).unwrap();
    assert!(report.ok, "{}", report.render());
}

#[test]
fn a_manifest_that_names_a_path_outside_the_bundle_is_refused_and_nothing_is_written() {
    // H1: `../escaped.txt` and absolute paths.
    for evil in [
        "../escaped.txt",
        "/tmp/escaped.txt",
        "secrets/../../escaped.txt",
        "C:/escaped.txt",
        "\\\\host\\share\\x",
        "secrets/.hidden",
        "other/x",
    ] {
        let (dir, runner, backup) = setup();
        let mut m = crate::manifest::read_manifest(&backup).unwrap();
        m.config.files.push(crate::manifest::FileEntry {
            path: evil.into(),
            bytes: 1,
            sha256: "0".repeat(64),
        });
        crate::manifest::write_manifest(&backup, &m).unwrap();
        let confirm = ScriptedConfirm::new(&[true]);
        let ctx = testkit::ctx(&runner, dir.path(), &confirm);
        let opts = LiveOptions {
            backup: Some(backup.display().to_string()),
            ..live_opts()
        };
        let e = restore_live(&ctx, &opts).unwrap_err();
        assert_eq!(e.kind, crate::error::Kind::Refused, "{evil}: {e}");
        assert!(runner.calls().is_empty(), "{evil}: {:?}", runner.lines());
        assert!(
            !dir.path().parent().unwrap().join("escaped.txt").exists(),
            "{evil}"
        );
    }
}

#[test]
fn restoring_settings_never_writes_outside_the_bundle_even_if_the_manifest_was_not_validated() {
    // Defence in depth: the writer itself checks every path again.
    let (dir, runner, backup) = setup();
    let confirm = ScriptedConfirm::new(&[]);
    let ctx = testkit::ctx(&runner, dir.path(), &confirm);
    let root = backup.join("config");
    // An absolute path that really exists, outside the bundle: `Path::join` would replace the base.
    let outside = tempfile::tempdir().unwrap();
    let absolute = outside.path().join("abs.txt");
    std::fs::write(&absolute, b"original").unwrap();
    let absolute_text = absolute.display().to_string();
    // Each source file EXISTS where the unchecked join would look for it, so that only the path
    // check itself (not "the source is not a regular file") can refuse the entry.
    let escaped = dir
        .path()
        .parent()
        .unwrap()
        .join("escaped-by-the-writer.txt");
    let _ = std::fs::remove_file(&escaped);
    std::fs::write(backup.join("escaped-by-the-writer.txt"), b"x").unwrap(); // root/../escaped-by-the-writer.txt
    std::fs::write(root.join("x"), b"x").unwrap(); // root/secrets/../x
    std::fs::create_dir_all(root.join(".git")).unwrap();
    std::fs::write(root.join(".git").join("config"), b"x").unwrap();
    for evil in [
        "../escaped-by-the-writer.txt",
        absolute_text.as_str(),
        "secrets/../x",
        ".git/config",
    ] {
        let mut m = crate::manifest::read_manifest(&backup).unwrap();
        m.config.files = vec![crate::manifest::FileEntry {
            path: evil.into(),
            bytes: 1,
            sha256: String::new(),
        }];
        let info = BackupInfo {
            path: backup.clone(),
            manifest: m,
        };
        let mut notes = Vec::new();
        let e = restore_config_files(&ctx, &info, &root, &mut notes).unwrap_err();
        assert_eq!(e.kind, crate::error::Kind::Refused, "{evil}");
        assert!(
            !escaped.exists(),
            "{evil}: a file was written outside the bundle"
        );
        assert!(!dir.path().join(".git").exists(), "{evil}");
    }
    assert_eq!(std::fs::read(&absolute).unwrap(), b"original");
    let _ = std::fs::remove_file(&escaped);
}

#[cfg(unix)]
#[test]
fn settings_are_never_written_through_a_symbolic_link() {
    let (dir, runner, backup) = setup();
    let outside = tempfile::tempdir().unwrap();
    std::fs::remove_dir_all(dir.path().join("secrets")).unwrap();
    std::fs::remove_file(dir.path().join(".env")).unwrap();
    std::os::unix::fs::symlink(outside.path(), dir.path().join("secrets")).unwrap();
    let m = crate::manifest::read_manifest(&backup).unwrap();
    let info = BackupInfo {
        path: backup.clone(),
        manifest: m,
    };
    let confirm = ScriptedConfirm::new(&[]);
    let ctx = testkit::ctx(&runner, dir.path(), &confirm);
    let mut notes = Vec::new();
    let e = restore_config_files(&ctx, &info, &backup.join("config"), &mut notes).unwrap_err();
    assert_eq!(e.kind, crate::error::Kind::Refused);
    assert!(
        std::fs::read_dir(outside.path()).unwrap().next().is_none(),
        "nothing was written through the link"
    );
    // A dangling link at the target is refused too (creation is exclusive).
    std::fs::remove_file(dir.path().join("secrets")).unwrap();
    std::fs::create_dir(dir.path().join("secrets")).unwrap();
    std::os::unix::fs::symlink(
        outside.path().join("nowhere"),
        dir.path().join("secrets").join("session_secret"),
    )
    .unwrap();
    assert!(restore_config_files(&ctx, &info, &backup.join("config"), &mut notes).is_err());
    assert!(!outside.path().join("nowhere").exists());
}

#[test]
fn pg_restore_messages_are_neither_stored_nor_shown() {
    // LOW: pg_restore quotes rows of the data in its errors.
    let (dir, runner, _) = setup();
    runner.first(
        "pg_restore -h 127.0.0.1",
        1,
        "pg_restore: error: COPY failed for table \"identity\": ERROR: duplicate key (email)=(alice@example.org)",
    );
    let confirm = ScriptedConfirm::new(&[]);
    let ctx = testkit::ctx(&runner, dir.path(), &confirm);
    let report = restore_verify(&ctx, None, None).unwrap();
    assert!(!report.ok);
    let json = serde_json::to_string(&report).unwrap();
    let all = format!("{json}\n{}", report.render());
    assert!(
        !all.contains("alice@example.org") && !all.contains("duplicate key"),
        "{all}"
    );
    assert!(all.contains("not shown or stored"), "{all}");
    let saved = save_report(&ctx, &report, "restore-verify").unwrap();
    assert!(!std::fs::read_to_string(saved)
        .unwrap()
        .contains("alice@example.org"));
}

#[test]
fn a_failed_create_never_leads_to_a_drop_of_a_database_this_run_did_not_make() {
    // M2: the name could belong to another run; only a successful create makes it ours to drop.
    let (dir, runner, _) = setup();
    runner.first(
        "create database dynodoc_restore_",
        1,
        "ERROR: database already exists",
    );
    let confirm = ScriptedConfirm::new(&[]);
    let ctx = testkit::ctx(&runner, dir.path(), &confirm);
    let report = restore_verify(&ctx, None, None).unwrap();
    assert!(!report.ok);
    assert!(!runner.ran("drop database"), "{:?}", runner.lines());
}

#[test]
fn scratch_names_carry_random_text_so_two_runs_in_one_second_differ() {
    let runner = testkit::healthy_runner();
    let confirm = ScriptedConfirm::new(&[]);
    let dir = tempfile::tempdir().unwrap();
    let mut ctx = Ctx::new(&runner, crate::ctx::Layout::new(dir.path()), &confirm);
    ctx.now = testkit::fixed_now;
    let (a, b) = (
        ctx.scratch_name("dynodoc_restore_"),
        ctx.scratch_name("dynodoc_restore_"),
    );
    assert_ne!(a, b);
    assert!(crate::db::is_scratch_name(&a) && a.len() <= 63, "{a}");
}

#[test]
fn a_failed_schema_query_aborts_instead_of_skipping_the_safety_backup() {
    // M3: `has_schema(..).unwrap_or(false)` treated any psql error as an empty database.
    let (dir, runner, _) = setup();
    runner.first("to_regclass", 1, "psql: error: connection to server failed");
    let confirm = ScriptedConfirm::new(&[true]);
    let ctx = testkit::ctx(&runner, dir.path(), &confirm);
    let e = restore_live(&ctx, &LiveOptions::default()).unwrap_err();
    assert!(e.what.contains("could not read the database"), "{e}");
    for forbidden in ["stop proxy", "create database", "pg_dump", "alter database"] {
        assert!(
            !runner.ran(forbidden),
            "ran {forbidden}: {:?}",
            runner.lines()
        );
    }
}

#[test]
fn the_writers_stop_before_the_safety_backup_is_taken() {
    // M3: a backup of a database that is still being written is not a safety copy.
    let (dir, runner, _) = setup();
    let confirm = ScriptedConfirm::new(&[true]);
    let ctx = testkit::ctx(&runner, dir.path(), &confirm);
    let report = restore_live(&ctx, &LiveOptions::default()).unwrap();
    assert!(report.ok, "{}", report.render());
    assert!(runner.position("stop proxy web api").unwrap() < runner.position("pg_dump").unwrap());
    assert!(runner.ran(SWAP_IN));
}

#[test]
fn a_server_with_no_live_database_yet_gets_an_empty_one_to_swap_with() {
    let (dir, runner, _) = setup();
    runner.first("select 1 from pg_database", 0, "");
    let confirm = ScriptedConfirm::new(&[true]);
    let ctx = testkit::ctx(&runner, dir.path(), &confirm);
    let report = restore_live(&ctx, &LiveOptions::default()).unwrap();
    assert!(report.ok, "{}", report.render());
    assert!(
        runner.ran("-c \"create database dynodoc\""),
        "{:?}",
        runner.lines()
    );
    assert!(!runner.ran("pg_dump"), "there was nothing to save");
    assert!(report
        .steps
        .iter()
        .any(|s| s.detail.contains("no live database yet")));
}

#[test]
fn a_project_name_that_belongs_to_another_directory_stops_the_restore_before_any_change() {
    // M9
    let (dir, runner, _) = setup();
    runner.first(
        "compose.project.working_dir",
        0,
        "/srv/someone-elses-bundle\n",
    );
    let confirm = ScriptedConfirm::new(&[true]);
    let ctx = testkit::ctx(&runner, dir.path(), &confirm);
    let e = restore_live(&ctx, &live_opts()).unwrap_err();
    assert_eq!(e.kind, crate::error::Kind::Refused);
    assert!(e.what.contains("another directory"), "{e}");
    assert!(
        runner
            .lines()
            .iter()
            .all(|l| !l.contains("compose --project-directory")),
        "{:?}",
        runner.lines()
    );
}

#[test]
fn the_report_says_the_live_database_was_replaced_on_success() {
    let (dir, runner, _) = setup();
    let confirm = ScriptedConfirm::new(&[true]);
    let ctx = testkit::ctx(&runner, dir.path(), &confirm);
    let report = restore_live(&ctx, &live_opts()).unwrap();
    assert!(report.ok && report.live_changed && !report.swapped_back);
    assert_eq!(alter_calls(&runner).len(), 1);
}
