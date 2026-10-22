//! Tests for the review fixes in `install`: M1 (b) the overlay gate, M9 the project-name guard.

use super::*;

#[test]
fn an_overlay_on_an_existing_installation_is_refused_and_nothing_runs() {
    // M1 (b): `install --overlay X` on an installation that exists replaced the overlay and ran
    // `up`, so `init` migrated with no backup, no rehearsal and no digest check.
    let dir = tempfile::tempdir().unwrap();
    testkit::installed(dir.path(), testkit::ENV_TEXT);
    let overlay = dir.path().join("elsewhere-overlay.yml");
    std::fs::write(&overlay, "services:\n  api:\n    image: r/api@sha256:aa\n").unwrap();
    let runner = testkit::healthy_runner();
    let confirm = ScriptedConfirm::new(&[true]);
    let ctx = testkit::ctx(&runner, dir.path(), &confirm);
    let opts = Options {
        overlay: Some(overlay),
        ..eval_opts()
    };
    let e = run(&ctx, &opts).unwrap_err();
    assert_eq!(e.kind, crate::error::Kind::Refused, "{e}");
    assert!(
        e.what
            .contains("without a backup, a rehearsal or a digest check"),
        "{e}"
    );
    assert!(e.todo.contains("dynodoctl upgrade"), "{e}");
    assert!(
        !dir.path().join(crate::ctx::OVERLAY_NAME).exists(),
        "the overlay was not installed"
    );
    assert!(!runner.ran("up -d"), "{:?}", runner.lines());
    assert!(!runner.ran("migrate"));
    // The pure validation says the same, with and without an installation.
    assert!(validate(&opts, Some(&EnvFile::parse(testkit::ENV_TEXT)), false).is_err());
    assert!(validate(&opts, None, false).is_ok());
}

#[test]
fn a_fresh_install_may_still_take_an_overlay() {
    let dir = tempfile::tempdir().unwrap();
    testkit::bundle(dir.path());
    let overlay = dir.path().join("release-overlay.yml");
    std::fs::write(&overlay, "services: {}\n").unwrap();
    let runner = testkit::healthy_runner();
    runner.on_effect("generate-env", generator_effect(dir.path().to_path_buf()));
    let confirm = ScriptedConfirm::new(&[true]);
    let ctx = testkit::ctx(&runner, dir.path(), &confirm);
    let opts = Options {
        overlay: Some(overlay),
        registry: Some("registry.example.org/dynodoc".into()),
        version: Some("1.0.0".into()),
        ..eval_opts()
    };
    let report = run(&ctx, &opts);
    // Whatever else happens on the way (images are not scripted here), the gate does not stop it.
    if let Err(e) = &report {
        assert!(!e.what.contains("without a backup"), "{e}");
    }
    assert!(dir.path().join(crate::ctx::OVERLAY_NAME).exists());
}

#[test]
fn a_project_name_that_belongs_to_another_directory_stops_the_install_before_any_change() {
    // M9: a second copy of the bundle under the same Compose project name would take over,
    // stop and migrate the first installation's containers.
    let dir = tempfile::tempdir().unwrap();
    testkit::bundle(dir.path());
    let runner = testkit::healthy_runner();
    runner.first(
        "compose.project.working_dir",
        0,
        "/srv/the-first-installation\n",
    );
    runner.on_effect("generate-env", generator_effect(dir.path().to_path_buf()));
    let confirm = ScriptedConfirm::new(&[true]);
    let ctx = testkit::ctx(&runner, dir.path(), &confirm);
    let e = run(&ctx, &eval_opts()).unwrap_err();
    assert_eq!(e.kind, crate::error::Kind::Refused, "{e}");
    assert!(e.what.contains("another directory"), "{e}");
    assert!(e.todo.contains("COMPOSE_PROJECT_NAME"), "{e}");
    assert!(
        !runner.ran("generate-env") && !runner.ran("up -d"),
        "{:?}",
        runner.lines()
    );
    assert!(!dir.path().join(".env").exists());
}
