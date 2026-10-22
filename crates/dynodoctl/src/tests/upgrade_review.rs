//! Tests for the review fixes in `upgrade` and `rollback`: H2 (validate before changing files),
//! H3 (c) (the record before the switch), M1 (the gates cannot be bypassed), M9.

use super::*;

fn state_records(dir: &Path) -> Vec<String> {
    std::fs::read_dir(dir.join("state"))
        .map(|d| {
            d.flatten()
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .filter(|n| n.starts_with("upgrade-"))
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn local_with_a_version_is_refused_because_it_would_skip_every_gate() {
    // M1 (a): `upgrade --local --version X` switched DYNODOC_VERSION with no overlay, no digest
    // check and no --allow-unpinned.
    let (dir, runner, _) = setup();
    let before = std::fs::read_to_string(dir.path().join(".env")).unwrap();
    let e = run_with(
        &runner,
        dir.path(),
        &[],
        &Options {
            local: true,
            version: Some("9.9.9".into()),
            ..Options::default()
        },
    )
    .unwrap_err();
    assert_eq!(e.kind, crate::error::Kind::Usage, "{e}");
    assert!(e.what.contains("--local"), "{e}");
    assert!(runner.calls().is_empty(), "{:?}", runner.lines());
    assert_eq!(
        std::fs::read_to_string(dir.path().join(".env")).unwrap(),
        before
    );
    assert!(state_records(dir.path()).is_empty());
}

#[test]
fn an_upgrade_by_tag_over_an_installed_overlay_is_refused_not_half_applied() {
    // M1 (c): the old pinned digests would keep running while DYNODOC_VERSION moves on.
    let (dir, runner, _) = setup();
    std::fs::write(dir.path().join(crate::ctx::OVERLAY_NAME), overlay_text()).unwrap();
    let before = std::fs::read_to_string(dir.path().join(".env")).unwrap();
    let e = run_with(
        &runner,
        dir.path(),
        &[],
        &Options {
            version: Some("1.1.0".into()),
            registry: Some("registry.example.org/dynodoc".into()),
            allow_unpinned: true,
            ..Options::default()
        },
    )
    .unwrap_err();
    assert_eq!(e.kind, crate::error::Kind::Refused, "{e}");
    assert!(e.what.contains("pins the images by digest"), "{e}");
    assert!(e.todo.contains("--overlay"), "{e}");
    assert!(runner.calls().is_empty(), "{:?}", runner.lines());
    assert_eq!(
        std::fs::read_to_string(dir.path().join(".env")).unwrap(),
        before
    );
}

#[test]
fn only_a_full_sha256_digest_counts() {
    assert!(valid_digest(DIGEST));
    for bad in [
        "sha256:abc",
        &DIGEST[..DIGEST.len() - 1],
        &format!("{DIGEST}0"),
        &DIGEST.replace('a', "A"),
        &DIGEST.replace("sha256", "sha512"),
        "sha256:",
        "",
        &format!("{DIGEST} "),
        &DIGEST.replacen('a', "g", 1),
    ] {
        assert!(!valid_digest(bad), "{bad}");
    }
    let short = "services:\n  api:\n    image: r/api@sha256:abcdef\n";
    assert!(parse_overlay(short)[0].digest.is_none());
    let long = format!("services:\n  api:\n    image: r/api@{DIGEST}0\n");
    assert!(parse_overlay(&long)[0].digest.is_none());
}

#[test]
fn the_pulled_digest_must_match_exactly_in_repository_and_digest() {
    // M1 (d): the check was a substring match on the whole RepoDigests text.
    let reference = format!("registry.example.org/dynodoc/api@{DIGEST}");
    let good = format!("[\"registry.example.org/dynodoc/api@{DIGEST}\"]");
    assert!(repo_digests_contain(&good, &reference, DIGEST));
    // A longer digest that merely contains the wanted one.
    let longer = format!("[\"registry.example.org/dynodoc/api@{DIGEST}00\"]");
    assert!(!repo_digests_contain(&longer, &reference, DIGEST));
    // The right digest under another repository.
    let other_repo = format!("[\"registry.example.org/evil/api@{DIGEST}\"]");
    assert!(!repo_digests_contain(&other_repo, &reference, DIGEST));
    // The wanted text only inside a different string in the array.
    let embedded = format!("[\"x registry.example.org/dynodoc/api@{DIGEST}\"]");
    assert!(!repo_digests_contain(&embedded, &reference, DIGEST));
    // Not JSON at all (the old code would have matched it as text).
    assert!(!repo_digests_contain(
        &format!("{reference} oops"),
        &reference,
        DIGEST
    ));
    assert!(!repo_digests_contain("[]", &reference, DIGEST));
    // Docker Hub's default names agree with the short form.
    let pg = format!("postgres:16@{PG_DIGEST}");
    assert!(repo_digests_contain(
        &format!("[\"postgres@{PG_DIGEST}\"]"),
        &pg,
        PG_DIGEST
    ));
    assert!(repo_digests_contain(
        &format!("[\"docker.io/library/postgres@{PG_DIGEST}\"]"),
        &pg,
        PG_DIGEST
    ));
}

#[test]
fn a_service_the_overlay_does_not_pin_is_named_and_refused() {
    // M1 (d): the overlay must cover every service image of the project.
    let (dir, runner, overlay) = setup();
    let mut config: serde_json::Value = serde_json::from_str(&config_json()).unwrap();
    config["services"]["web"]["image"] = "registry.example.org/dynodoc/web:1.1.0".into();
    config["services"]["converter"] =
        serde_json::json!({"image": "registry.example.org/dynodoc/converter:1.1.0"});
    runner.first("config --format json", 0, &config.to_string());
    let e = run_with(&runner, dir.path(), &[], &release_opts(&overlay)).unwrap_err();
    assert_eq!(e.kind, crate::error::Kind::Refused, "{e}");
    assert!(
        e.what
            .contains("web (registry.example.org/dynodoc/web:1.1.0)")
            && e.what.contains("converter"),
        "{e}"
    );
    assert!(
        !e.what.contains("api ("),
        "only the unpinned ones are listed: {e}"
    );
    assert!(!runner.ran("pg_dump") && !runner.ran("up -d --wait --wait-timeout 300"));
    assert!(state_records(dir.path()).len() <= 1);
}

#[test]
fn an_overlay_that_does_not_apply_as_written_is_refused() {
    let (dir, runner, overlay) = setup();
    let mut config: serde_json::Value = serde_json::from_str(&config_json()).unwrap();
    config["services"]["web"]["image"] =
        format!("registry.example.org/dynodoc/other@{DIGEST}").into();
    runner.first("config --format json", 0, &config.to_string());
    let e = run_with(&runner, dir.path(), &[], &release_opts(&overlay)).unwrap_err();
    assert!(e.what.contains("but Compose would run"), "{e}");
    // A service in the overlay that the installation does not have.
    let (dir2, runner2, overlay2) = setup();
    let mut config2: serde_json::Value = serde_json::from_str(&config_json()).unwrap();
    config2["services"].as_object_mut().unwrap().remove("proxy");
    runner2.first("config --format json", 0, &config2.to_string());
    let e2 = run_with(&runner2, dir2.path(), &[], &release_opts(&overlay2)).unwrap_err();
    assert!(e2.what.contains("does not have: proxy"), "{e2}");
}

#[test]
fn when_the_effective_configuration_cannot_be_read_the_images_are_not_trusted() {
    for (status, text) in [
        (1, "error: invalid compose file"),
        (0, "not json"),
        (0, "{\"name\":\"x\"}"),
    ] {
        let (dir, runner, overlay) = setup();
        runner.first("config --format json", status, text);
        let e = run_with(&runner, dir.path(), &[], &release_opts(&overlay)).unwrap_err();
        assert_eq!(e.kind, crate::error::Kind::Refused, "{status} {text}: {e}");
        assert!(e.what.contains("could not read the configuration"), "{e}");
        assert!(!runner.ran("pg_dump"));
    }
}

#[test]
fn every_pinned_image_is_inspected_once_including_third_party_ones() {
    let (dir, runner, overlay) = setup();
    let report = run_with(&runner, dir.path(), &[], &release_opts(&overlay)).unwrap();
    assert!(report.ok, "{}", report.render());
    // api, web, tools, proxy and postgres: five distinct references (several services share one).
    assert_eq!(
        runner.count("--format {{json .RepoDigests}}"),
        5,
        "{:?}",
        runner.lines()
    );
}

#[test]
fn the_record_exists_before_the_settings_change_and_says_switching() {
    // H3 (c): an upgrade interrupted after the switch must not read as "nothing to roll back".
    let (dir, runner, overlay) = setup();
    let (env_path, state) = (dir.path().join(".env"), dir.path().join("state"));
    let seen: std::rc::Rc<std::cell::RefCell<Vec<(String, String)>>> = Default::default();
    let sink = seen.clone();
    runner.on_effect("up -d --wait --wait-timeout 300", move |_| {
        let record = std::fs::read_dir(&state)
            .unwrap()
            .flatten()
            .find(|e| e.file_name().to_string_lossy().starts_with("upgrade-"))
            .map(|e| std::fs::read_to_string(e.path()).unwrap())
            .unwrap_or_default();
        sink.borrow_mut()
            .push((record, std::fs::read_to_string(&env_path).unwrap()));
    });
    let report = run_with(&runner, dir.path(), &[], &release_opts(&overlay)).unwrap();
    assert!(report.ok);
    let seen = seen.borrow();
    let (record, env) = &seen[0];
    assert!(record.contains("\"result\": \"switching\""), "{record}");
    assert!(
        record.contains("pre-upgrade-"),
        "the saved settings are named: {record}"
    );
    assert!(env.contains("DYNODOC_VERSION=1.1.0"));
    // And a record in that state is what `rollback` picks up.
    let r: UpgradeRecord = serde_json::from_str(record).unwrap();
    assert_eq!(r.result, "switching");
    let confirm = ScriptedConfirm::new(&[]);
    let (dir2, runner2, _) = setup();
    std::fs::create_dir_all(dir2.path().join("state")).unwrap();
    std::fs::write(
        dir2.path()
            .join("state")
            .join("upgrade-20261022T120000Z.json"),
        record,
    )
    .unwrap();
    let ctx2 = testkit::ctx(&runner2, dir2.path(), &confirm);
    let (found, _) = latest_record(&ctx2, None).unwrap();
    assert_eq!(
        found.result, "switching",
        "an interrupted upgrade can be rolled back"
    );
}

#[test]
fn an_interrupt_before_the_switch_changes_no_setting() {
    let (dir, runner, overlay) = setup();
    let confirm = ScriptedConfirm::new(&[]);
    let ctx = testkit::ctx(&runner, dir.path(), &confirm);
    let flag = ctx.interrupt.clone();
    runner.on_effect("engine-api", move |_| {
        flag.store(true, std::sync::atomic::Ordering::SeqCst);
    });
    let before = std::fs::read_to_string(dir.path().join(".env")).unwrap();
    let e = run(&ctx, &release_opts(&overlay)).unwrap_err();
    assert!(e.what.contains("interrupted"), "{e}");
    assert_eq!(
        std::fs::read_to_string(dir.path().join(".env")).unwrap(),
        before
    );
    assert!(!dir.path().join(crate::ctx::OVERLAY_NAME).exists());
    assert!(!runner.ran("up -d --wait --wait-timeout 300"));
}

#[test]
fn a_failed_switch_puts_the_settings_back() {
    // The overlay cannot be written (the target is a directory): .env must not stay half-switched.
    let (dir, runner, overlay) = setup();
    std::fs::create_dir(dir.path().join(crate::ctx::OVERLAY_NAME)).unwrap();
    let before = std::fs::read_to_string(dir.path().join(".env")).unwrap();
    let e = run_with(&runner, dir.path(), &[], &release_opts(&overlay)).unwrap_err();
    let _ = e;
    assert_eq!(
        std::fs::read_to_string(dir.path().join(".env")).unwrap(),
        before,
        "the old .env is back"
    );
    assert!(!runner.ran("up -d --wait --wait-timeout 300"));
}

fn upgraded() -> (tempfile::TempDir, FakeRunner, String, Vec<u8>) {
    let (dir, runner, overlay) = setup();
    run_with(&runner, dir.path(), &[], &release_opts(&overlay)).unwrap();
    let env = std::fs::read_to_string(dir.path().join(".env")).unwrap();
    let overlay_bytes = std::fs::read(dir.path().join(crate::ctx::OVERLAY_NAME)).unwrap();
    (dir, runner, env, overlay_bytes)
}

#[test]
fn rollback_with_a_damaged_backup_changes_no_file_and_says_so_truthfully() {
    // H2: .env and the overlay were rewritten BEFORE the backup was validated, then the error said
    // "nothing was changed".
    let (dir, runner, env, overlay_bytes) = upgraded();
    let record_backup = std::fs::read_dir(dir.path().join("backups"))
        .unwrap()
        .flatten()
        .find(|e| e.file_name().to_string_lossy().ends_with("before-upgrade"))
        .unwrap()
        .path();
    std::fs::write(record_backup.join("database.dump"), b"PGDMP tampered").unwrap();
    let calls = runner.calls().len();
    let confirm = ScriptedConfirm::new(&[true]);
    let ctx = testkit::ctx(&runner, dir.path(), &confirm);
    let e = rollback(&ctx, None).err().unwrap();
    assert_eq!(e.kind, crate::error::Kind::Refused, "{e}");
    assert!(e.what.contains("damaged or incomplete"), "{e}");
    assert!(e.todo.contains("nothing was changed"));
    assert_eq!(
        std::fs::read_to_string(dir.path().join(".env")).unwrap(),
        env
    );
    assert_eq!(
        std::fs::read(dir.path().join(crate::ctx::OVERLAY_NAME)).unwrap(),
        overlay_bytes
    );
    assert_eq!(runner.calls().len(), calls, "no command ran");
    assert!(
        confirm.asked.borrow().is_empty(),
        "not even asked: it would be refused anyway"
    );
}

#[test]
fn rollback_whose_restore_fails_before_changing_the_database_puts_the_settings_back() {
    let (dir, runner, env, overlay_bytes) = upgraded();
    runner.first("pg_restore -h 127.0.0.1", 1, "boom");
    let confirm = ScriptedConfirm::new(&[true]);
    let ctx = testkit::ctx(&runner, dir.path(), &confirm);
    let report = rollback(&ctx, None).unwrap();
    assert!(!report.ok && !report.restore.live_changed);
    assert_eq!(
        std::fs::read_to_string(dir.path().join(".env")).unwrap(),
        env
    );
    assert_eq!(
        std::fs::read(dir.path().join(crate::ctx::OVERLAY_NAME)).unwrap(),
        overlay_bytes
    );
    assert!(
        report
            .restore
            .notes
            .iter()
            .any(|n| n.contains("settings from before it were put back")),
        "{:?}",
        report.restore.notes
    );
    // And the record still says the upgrade is in place, so a later rollback can be tried again.
    let (record, _) = latest_record(&ctx, None).unwrap();
    assert_eq!(record.result, "completed");
}

#[test]
fn rollback_refused_by_the_sql_screen_puts_the_settings_back() {
    let (dir, runner, env, overlay_bytes) = upgraded();
    runner.on_file("pg_restore --no-owner -f -", b"COPY t FROM PROGRAM 'id';\n");
    let confirm = ScriptedConfirm::new(&[true]);
    let ctx = testkit::ctx(&runner, dir.path(), &confirm);
    let e = rollback(&ctx, None).err().unwrap();
    assert!(
        e.what.contains("SQL that a Dynodoc backup never has"),
        "{e}"
    );
    assert_eq!(
        std::fs::read_to_string(dir.path().join(".env")).unwrap(),
        env
    );
    assert_eq!(
        std::fs::read(dir.path().join(crate::ctx::OVERLAY_NAME)).unwrap(),
        overlay_bytes
    );
}

#[test]
fn an_upgrade_and_a_rollback_refuse_while_a_swap_is_unfinished() {
    let (dir, runner, overlay) = setup();
    let layout = crate::ctx::Layout::new(dir.path());
    crate::recover::write_journal(
        &layout,
        &crate::recover::Journal {
            version: 1,
            started_utc: "2026-10-22T12:00:00Z".into(),
            operation: "restore-into-live".into(),
            backup: "b".into(),
            live: "dynodoc".into(),
            restoring: "dynodoc_restoring_x_ab12cd".into(),
            previous: "dynodoc_prev_x_ab12cd".into(),
            phase: "swapped".into(),
        },
    )
    .unwrap();
    let e = run_with(&runner, dir.path(), &[], &release_opts(&overlay)).unwrap_err();
    assert_eq!(e.kind, crate::error::Kind::Refused);
    assert!(e.todo.contains("dynodoctl recover"), "{e}");
    let confirm = ScriptedConfirm::new(&[true]);
    let ctx = testkit::ctx(&runner, dir.path(), &confirm);
    assert!(rollback(&ctx, None)
        .err()
        .unwrap()
        .todo
        .contains("dynodoctl recover"));
    assert!(runner.calls().is_empty());
}

#[test]
fn a_project_name_that_belongs_to_another_directory_stops_the_upgrade() {
    // M9
    let (dir, runner, overlay) = setup();
    runner.first("compose.project.working_dir", 0, "/srv/other-bundle\n");
    let e = run_with(&runner, dir.path(), &[], &release_opts(&overlay)).unwrap_err();
    assert_eq!(e.kind, crate::error::Kind::Refused);
    assert!(e.what.contains("another directory"), "{e}");
    assert!(!runner.ran("pg_dump"));
}
