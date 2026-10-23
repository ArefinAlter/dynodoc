use super::*;
use crate::ctx::ScriptedConfirm;
use crate::runner::FakeRunner;
use crate::testkit::{self, assert_no_seeded_secret};

fn eval_opts() -> Options {
    Options {
        origin: Some("https://localhost:18443".into()),
        tls: Some("evaluation".into()),
        https_port: Some(0),
        http_port: Some(0),
        ..Options::default()
    }
}

/// What the real generator does: it writes into the work directory the tool names (the tests use
/// a fixed clock, so the name is fixed); `generate` then moves the result into `dir`.
fn generator_effect(dir: std::path::PathBuf) -> impl Fn(&crate::runner::Cmd) + 'static {
    let dir = dir.join(".generate-20261022T120000Z");
    move |_| {
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join(".env"),
            "DYNODOC_MODE=selfhosted\nORIGIN=https://localhost:18443\nAUTH_EMAIL_LINK=off\n",
        )
        .unwrap();
        std::fs::create_dir_all(dir.join("secrets")).unwrap();
        for (name, value) in testkit::SEEDED {
            std::fs::write(dir.join("secrets").join(name), format!("{value}\n")).unwrap();
        }
    }
}

#[test]
fn the_certificate_mode_has_no_default() {
    let e = validate(&Options::default(), None, false).unwrap_err();
    assert_eq!(e.code(), crate::error::exit::USAGE);
    assert!(e.what.contains("not chosen") && e.todo.contains("evaluation"));
    let bad = Options {
        tls: Some("self-signed".into()),
        origin: Some("https://x.example".into()),
        ..Options::default()
    };
    assert!(validate(&bad, None, false).is_err());
}

#[test]
fn every_option_conflict_is_refused_with_a_next_step() {
    let base = || Options {
        origin: Some("https://x.example".into()),
        tls: Some("evaluation".into()),
        ..Options::default()
    };
    let cases: Vec<(Options, &str)> = vec![
        (
            Options {
                origin: None,
                ..base()
            },
            "origin",
        ),
        (
            Options {
                tls: Some("provided".into()),
                ..base()
            },
            "certificate",
        ),
        (
            Options {
                cert: Some("/nonexistent/c.pem".into()),
                key: Some("/nonexistent/k.pem".into()),
                ..base()
            },
            "only apply",
        ),
        (
            Options {
                tls: Some("acme".into()),
                ..base()
            },
            "acme-email",
        ),
        (
            Options {
                tls: Some("acme".into()),
                acme_email: Some("a@x.example".into()),
                ..base()
            },
            "acme-ca",
        ),
        (
            Options {
                tls: Some("acme".into()),
                acme_email: Some("a@x.example".into()),
                acme_ca: Some("letsencrypt-staging".into()),
                ..base()
            },
            "terms",
        ),
        (
            Options {
                acme_email: Some("a@x.example".into()),
                ..base()
            },
            "only apply",
        ),
        (
            Options {
                smtp_from: Some("d@x.example".into()),
                ..base()
            },
            "smtp-host",
        ),
        (
            Options {
                smtp_host: Some("smtp.x.example".into()),
                ..base()
            },
            "smtp-from",
        ),
        (
            Options {
                build: true,
                registry: Some("r.example/dynodoc".into()),
                ..base()
            },
            "choose one",
        ),
    ];
    for (opts, needle) in cases {
        let e = validate(&opts, None, false).unwrap_err();
        assert!(
            format!("{} {}", e.what, e.todo).contains(needle),
            "{needle}: {e}"
        );
        assert!(!e.todo.is_empty());
    }
    let ok = Options {
        tls: Some("acme".into()),
        acme_email: Some("a@x.example".into()),
        acme_ca: Some("letsencrypt-staging".into()),
        acme_accept_tos: true,
        ..base()
    };
    assert!(validate(&ok, None, false).is_ok());
}

#[test]
fn settings_follow_the_chosen_mode_and_the_origin_port() {
    let origin = Origin::parse("https://docs.example.org:8443").unwrap();
    let opts = Options {
        tls: Some("acme".into()),
        acme_email: Some("a@x.example".into()),
        acme_ca: Some("letsencrypt".into()),
        acme_accept_tos: true,
        with_converter: true,
        bind: Some("0.0.0.0".into()),
        ..Options::default()
    };
    let s = desired_settings(&opts, &origin, None);
    let get = |k: &str| s.iter().find(|(key, _)| key == k).map(|(_, v)| v.as_str());
    assert_eq!(get("DYNODOC_TLS_MODE"), Some("acme"));
    assert_eq!(
        get("DYNODOC_HTTPS_PORT"),
        Some("8443"),
        "the origin's port is also the published port"
    );
    assert_eq!(get("COMPOSE_PROFILES"), Some("acme,converter"));
    assert_eq!(get("DYNODOC_ACME_ACCEPT_TOS"), Some("true"));
    assert_eq!(get("DYNODOC_BIND"), Some("0.0.0.0"));
    assert_eq!(get("DYNODOC_HTTP_PORT"), None);
}

#[test]
fn applying_settings_is_idempotent_and_refuses_a_conflict() {
    let mut env = EnvFile::parse("ORIGIN=https://x.example\n# ADMIN_EMAILS=\n");
    let settings = vec![
        ("DYNODOC_TLS_MODE".to_string(), "evaluation".to_string()),
        ("ADMIN_EMAILS".to_string(), "a@x.example".to_string()),
    ];
    assert_eq!(apply_settings(&mut env, &settings).unwrap(), 2);
    let once = env.render();
    assert_eq!(once.matches("written by dynodoctl install").count(), 1);
    assert_eq!(apply_settings(&mut env, &settings).unwrap(), 0);
    assert_eq!(env.render(), once);
    let conflict = vec![("DYNODOC_TLS_MODE".to_string(), "acme".to_string())];
    let e = apply_settings(&mut env, &conflict).unwrap_err();
    assert_eq!(e.code(), crate::error::exit::REFUSED);
    assert!(e.todo.contains("configure set"));
}

#[test]
fn a_dry_run_changes_nothing_and_starts_nothing() {
    let dir = tempfile::tempdir().unwrap();
    testkit::bundle(dir.path());
    let runner = testkit::healthy_runner();
    let confirm = ScriptedConfirm::new(&[]);
    let ctx = testkit::ctx(&runner, dir.path(), &confirm);
    let report = run(
        &ctx,
        &Options {
            dry_run: true,
            ..eval_opts()
        },
    )
    .unwrap();
    assert!(report.dry_run && report.steps.is_empty());
    assert!(
        report.plan.iter().any(|p| p.contains("generator")),
        "{:?}",
        report.plan
    );
    assert!(!dir.path().join(".env").exists() && !dir.path().join("secrets").exists());
    assert!(
        !runner.ran(" up ") && !runner.ran("generate-env") && !runner.ran(" pull"),
        "{:?}",
        runner.lines()
    );
    assert!(confirm.asked.borrow().is_empty(), "a dry run asks nothing");
}

#[test]
fn a_fresh_install_runs_the_steps_in_order_and_explains_the_first_administrator() {
    let dir = tempfile::tempdir().unwrap();
    testkit::bundle(dir.path());
    let runner = testkit::healthy_runner();
    runner.on_effect("generate-env", generator_effect(dir.path().to_path_buf()));
    let confirm = ScriptedConfirm::new(&[true]);
    let ctx = testkit::ctx(&runner, dir.path(), &confirm);
    let opts = Options {
        admin_emails: Some("you@example.org".into()),
        ..eval_opts()
    };
    let report = run(&ctx, &opts).unwrap();
    assert!(report.ok && report.started && report.generated_secrets);
    let gen = runner.position("generate-env").expect("generator ran");
    let up = runner.position("up -d --wait").expect("stack started");
    assert!(
        gen < up,
        "settings and secrets exist before the stack starts"
    );
    let env = std::fs::read_to_string(dir.path().join(".env")).unwrap();
    assert!(
        env.contains("DYNODOC_TLS_MODE=evaluation") && env.contains("ADMIN_EMAILS=you@example.org"),
        "{env}"
    );
    let text = report.next_steps.join("\n");
    // No mail server was given: the way in is the operator-issued link, with the exact command.
    assert!(
        text.contains("dynodoctl admin-link --email you@example.org --create")
            && text.contains("Email sign-in is OFF"),
        "{text}"
    );
    assert!(
        !text.contains("nobody can sign in") && !text.contains("no operator-issued sign-in link"),
        "{text}"
    );
    assert!(report
        .warnings
        .iter()
        .any(|w| w.contains("NOT FOR PRODUCTION")));
    assert_no_seeded_secret(&report.render(), "install report");
}

#[test]
fn an_existing_installation_is_never_regenerated() {
    let dir = tempfile::tempdir().unwrap();
    testkit::installed(dir.path(), testkit::ENV_TEXT);
    let before = std::fs::read(dir.path().join("secrets").join("session_secret")).unwrap();
    let runner = testkit::healthy_runner();
    let confirm = ScriptedConfirm::new(&[true]);
    let ctx = testkit::ctx(&runner, dir.path(), &confirm);
    let opts = Options {
        origin: None,
        tls: None,
        https_port: Some(0),
        http_port: Some(0),
        ..Options::default()
    };
    let report = run(&ctx, &opts).unwrap();
    assert!(!report.generated_secrets);
    assert!(!runner.ran("generate-env"));
    assert!(
        runner.ran("up -d --wait"),
        "the repeat run still brings the stack up"
    );
    assert_eq!(
        std::fs::read(dir.path().join("secrets").join("session_secret")).unwrap(),
        before
    );
}

#[test]
fn a_conflicting_setting_is_refused_before_anything_runs() {
    let dir = tempfile::tempdir().unwrap();
    testkit::installed(dir.path(), testkit::ENV_TEXT);
    let runner = testkit::healthy_runner();
    let confirm = ScriptedConfirm::new(&[true]);
    let ctx = testkit::ctx(&runner, dir.path(), &confirm);
    let opts = Options {
        tls: Some("acme".into()),
        acme_email: Some("a@x.example".into()),
        acme_ca: Some("letsencrypt-staging".into()),
        acme_accept_tos: true,
        https_port: Some(0),
        http_port: Some(0),
        ..Options::default()
    };
    let e = run(&ctx, &opts).unwrap_err();
    assert_eq!(e.code(), crate::error::exit::REFUSED);
    assert!(e.what.contains("already set"));
    assert!(!runner.ran("up -d"));
}

#[test]
fn a_half_installed_directory_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    testkit::bundle(dir.path());
    std::fs::write(dir.path().join(".env"), "DYNODOC_MODE=selfhosted\n").unwrap();
    let runner = testkit::healthy_runner();
    let confirm = ScriptedConfirm::new(&[true]);
    let ctx = testkit::ctx(&runner, dir.path(), &confirm);
    let e = run(&ctx, &eval_opts()).unwrap_err();
    assert!(e.what.contains("half installed"));
    assert!(runner.calls().is_empty());
}

#[test]
fn declining_the_confirmation_changes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    testkit::bundle(dir.path());
    let runner = testkit::healthy_runner();
    let confirm = ScriptedConfirm::new(&[false]);
    let ctx = testkit::ctx(&runner, dir.path(), &confirm);
    let e = run(&ctx, &eval_opts()).unwrap_err();
    assert_eq!(e.code(), crate::error::exit::REFUSED);
    assert!(!dir.path().join(".env").exists());
    assert!(!runner.ran("generate-env") && !runner.ran("up -d"));
}

#[test]
fn a_failed_preflight_refuses_and_names_the_problem() {
    let dir = tempfile::tempdir().unwrap();
    testkit::bundle(dir.path());
    let runner = FakeRunner::new();
    runner.on(
        "version --format {{.Server.Version}}",
        1,
        "Cannot connect to the Docker daemon",
    );
    let confirm = ScriptedConfirm::new(&[true]);
    let ctx = testkit::ctx(&runner, dir.path(), &confirm);
    let e = run(&ctx, &eval_opts()).unwrap_err();
    assert_eq!(e.code(), crate::error::exit::REFUSED);
    assert!(e.what.contains("Docker daemon is not reachable"), "{e}");
    assert!(!dir.path().join(".env").exists());
}

#[test]
fn a_directory_without_a_bundle_asks_for_one() {
    let dir = tempfile::tempdir().unwrap();
    let runner = testkit::healthy_runner();
    let confirm = ScriptedConfirm::new(&[true]);
    let ctx = testkit::ctx(&runner, dir.path(), &confirm);
    let e = run(&ctx, &eval_opts()).unwrap_err();
    assert!(e.what.contains("no compose.yml") && e.todo.contains("--from"));
}

#[test]
fn the_bundle_can_be_copied_from_a_release_directory() {
    let release = tempfile::tempdir().unwrap();
    testkit::bundle(release.path());
    std::fs::write(release.path().join("jobs").join("init.sh"), "#!/bin/sh\n").unwrap();
    let target = tempfile::tempdir().unwrap();
    let runner = testkit::healthy_runner();
    runner.on_effect(
        "generate-env",
        generator_effect(target.path().to_path_buf()),
    );
    let confirm = ScriptedConfirm::new(&[true]);
    let ctx = testkit::ctx(&runner, target.path(), &confirm);
    run(
        &ctx,
        &Options {
            from: Some(release.path().to_path_buf()),
            ..eval_opts()
        },
    )
    .unwrap();
    assert!(
        target.path().join("compose.yml").is_file()
            && target.path().join("jobs").join("init.sh").is_file()
    );
}

#[test]
fn missing_images_stop_the_install_with_the_build_command() {
    let dir = tempfile::tempdir().unwrap();
    testkit::bundle(dir.path());
    let runner = testkit::healthy_runner();
    runner.on("image inspect", 1, "Error: No such image");
    let confirm = ScriptedConfirm::new(&[true]);
    let ctx = testkit::ctx(&runner, dir.path(), &confirm);
    let e = run(&ctx, &eval_opts()).unwrap_err();
    assert_eq!(e.code(), crate::error::exit::REFUSED);
    assert!(
        e.what.contains("dynodoc-local/api:dev") && e.todo.contains("--build"),
        "{e}"
    );
}

#[test]
fn registry_installs_pull_the_images_before_the_generator_needs_the_tools_image() {
    let dir = tempfile::tempdir().unwrap();
    testkit::bundle(dir.path());
    let runner = testkit::healthy_runner();
    // The four image lookups fail (nothing pulled yet); later lookups, after the pull, succeed.
    runner.on_seq(
        "image inspect",
        &[(1, ""), (1, ""), (1, ""), (1, ""), (0, "")],
    );
    runner.on_effect("generate-env", generator_effect(dir.path().to_path_buf()));
    let confirm = ScriptedConfirm::new(&[true]);
    let ctx = testkit::ctx(&runner, dir.path(), &confirm);
    let opts = Options {
        registry: Some("registry.example.org/dynodoc".into()),
        version: Some("1.0.0".into()),
        ..eval_opts()
    };
    run(&ctx, &opts).unwrap();
    assert!(
        runner.position(" pull").unwrap() < runner.position("generate-env").unwrap(),
        "{:?}",
        runner.lines()
    );
    let env = std::fs::read_to_string(dir.path().join(".env")).unwrap();
    assert!(
        env.contains("DYNODOC_IMAGE_REGISTRY=registry.example.org/dynodoc")
            && env.contains("DYNODOC_VERSION=1.0.0")
    );
}

#[test]
fn a_stack_that_does_not_come_up_reports_the_failed_job_logs_without_secrets() {
    let dir = tempfile::tempdir().unwrap();
    testkit::bundle(dir.path());
    let runner = testkit::healthy_runner();
    runner.on_effect("generate-env", generator_effect(dir.path().to_path_buf()));
    runner.first(
        "up -d --wait",
        1,
        "dependency failed to start: container init exited (1)",
    );
    let mut ps = testkit::ps_line("init", "exited", "", 1);
    ps.push_str(&testkit::ps_line("postgres", "running", "healthy", 0));
    runner.first("ps -a --format json", 0, &ps);
    runner.first("logs --no-color --tail 12 init", 0, "init: applying migrations\nERROR: database schema differs from this binary postgres://u:SEEDED-runtime-password-112233445566@postgres/db\n");
    let confirm = ScriptedConfirm::new(&[true]);
    let ctx = testkit::ctx(&runner, dir.path(), &confirm);
    let e = run(&ctx, &eval_opts()).unwrap_err();
    assert_eq!(e.code(), crate::error::exit::FAILED);
    assert!(e.what.contains("database schema differs"), "{e}");
    assert_no_seeded_secret(&e.to_string(), "install failure");
}

#[test]
fn a_mail_user_without_a_password_stops_before_the_stack_starts() {
    let dir = tempfile::tempdir().unwrap();
    testkit::bundle(dir.path());
    let runner = testkit::healthy_runner();
    let d = dir.path().to_path_buf();
    runner.on_effect("generate-env", generator_effect(d.clone()));
    runner.on_effect("generate-env", move |_| {
        std::fs::write(
            d.join(".generate-20261022T120000Z")
                .join("secrets")
                .join("smtp_password"),
            "",
        )
        .unwrap()
    });
    let confirm = ScriptedConfirm::new(&[true]);
    let ctx = testkit::ctx(&runner, dir.path(), &confirm);
    let opts = Options {
        smtp_host: Some("smtp.x.example".into()),
        smtp_from: Some("d@x.example".into()),
        smtp_user: Some("mailer".into()),
        ..eval_opts()
    };
    let e = run(&ctx, &opts).unwrap_err();
    assert_eq!(e.code(), crate::error::exit::REFUSED);
    assert!(e.what.contains("smtp_password"));
    assert!(!runner.ran("up -d"));
}

#[test]
fn a_given_mail_password_is_stored_privately_and_never_echoed() {
    let dir = tempfile::tempdir().unwrap();
    testkit::bundle(dir.path());
    let runner = testkit::healthy_runner();
    let d = dir.path().to_path_buf();
    runner.on_effect("generate-env", generator_effect(d.clone()));
    let confirm = ScriptedConfirm::new(&[true]);
    let ctx = testkit::ctx(&runner, dir.path(), &confirm);
    let opts = Options {
        smtp_host: Some("smtp.x.example".into()),
        smtp_from: Some("d@x.example".into()),
        smtp_user: Some("mailer".into()),
        smtp_password: Some("fresh-mail-password-77\n".into()),
        ..eval_opts()
    };
    let report = run(&ctx, &opts).unwrap();
    assert_eq!(
        std::fs::read_to_string(dir.path().join("secrets").join("smtp_password")).unwrap(),
        "fresh-mail-password-77\n"
    );
    assert!(!report.render().contains("fresh-mail-password-77"));
    assert!(
        !runner
            .lines()
            .iter()
            .any(|l| l.contains("fresh-mail-password-77")),
        "the password is never on a command line"
    );
}

#[path = "install_review.rs"]
mod review;
