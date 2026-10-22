use super::*;
use crate::ctx::ScriptedConfirm;
use crate::net::{FakeNet, HttpResponse};
use crate::runner::FakeRunner;
use crate::testkit;
use chrono::{DateTime, TimeZone, Utc};

const ORIGIN: &str = "https://docs.example.org";

const LOGIN_HTML: &str = "<!doctype html><html><head><link rel=\"stylesheet\" href=\"/_app/immutable/app.css\"><link rel=\"icon\" href=\"/favicon.png\"><script type=\"module\" src=\"/_app/immutable/entry.js\"></script></head><body><form action=\"/auth/email\"></form><a href=\"https://example.org/help\">help</a>Example Docs</body></html>";

fn headers(hsts: &str) -> Vec<(&'static str, String)> {
    vec![
        (
            "content-security-policy",
            format!("default-src 'self'; script-src 'self'; connect-src 'self' {ORIGIN}"),
        ),
        ("strict-transport-security", hsts.to_string()),
        ("x-content-type-options", "nosniff".into()),
        ("referrer-policy", "no-referrer".into()),
        ("x-frame-options", "SAMEORIGIN".into()),
        ("server", "nginx".into()),
        ("x-robots-tag", "noindex, nofollow".into()),
    ]
}

fn page(status: u16, hs: &[(&str, String)], body: &str) -> HttpResponse {
    let pairs: Vec<(&str, &str)> = hs.iter().map(|(k, v)| (*k, v.as_str())).collect();
    testkit::response(status, &pairs, body)
}

/// A correctly delivered installation. `evaluation` switches to the self-signed variant.
fn site(evaluation: bool) -> FakeNet {
    let net = FakeNet::default();
    let facts = if evaluation {
        testkit::tls_facts(
            true,
            Err("not issued by an authority this tool trusts".into()),
        )
    } else {
        testkit::tls_facts(false, Ok(()))
    };
    let hsts = if evaluation {
        "max-age=300"
    } else {
        "max-age=31536000"
    };
    let ok = |r: HttpResponse| Ok((r, facts.clone()));
    net.on_https("/login", ok(page(200, &headers(hsts), LOGIN_HTML)));
    net.on_https(
        "/_app/immutable/app.css",
        ok(page(200, &[], "body{color:#222}")),
    );
    net.on_https(
        "/_app/immutable/entry.js",
        ok(page(
            200,
            &[],
            "export const start = () => 1; // see https://svelte.dev/e/x",
        )),
    );
    net.on_https("/healthz", ok(page(200, &[], "{\"status\":\"ok\"}")));
    net.on_https(
        "@dynodoc-verify-delivery.invalid/",
        ok(page(421, &[], "Misdirected")),
    );
    net.on_https("/api/engine/documents", ok(page(401, &[], "{}")));
    net.on_https(
        "/",
        ok(testkit::response(302, &[("location", "/login")], "")),
    );
    for p in [
        "/features",
        "/privacy",
        "/terms",
        "/pricing",
        "/sitemap.xml",
        "/llms.txt",
    ] {
        net.on_https(p, ok(page(404, &[], "not found")));
    }
    net.on_http(
        "/",
        Ok(testkit::response(
            301,
            &[("location", "https://docs.example.org/")],
            "",
        )),
    );
    net
}

fn level(report: &DeliveryReport, id: &str) -> Level {
    report
        .checks
        .iter()
        .find(|c| c.id == id)
        .unwrap_or_else(|| {
            panic!(
                "no check {id}: {:?}",
                report.checks.iter().map(|c| &c.id).collect::<Vec<_>>()
            )
        })
        .level
}

fn opts() -> Options {
    Options {
        origin: Some(ORIGIN.into()),
        tls_mode: Some("provided".into()),
        ..Options::default()
    }
}

fn check(net: &FakeNet, o: &Options) -> DeliveryReport {
    let dir = tempfile::tempdir().unwrap();
    testkit::bundle(dir.path());
    let runner = FakeRunner::new();
    let confirm = ScriptedConfirm::new(&[]);
    let ctx = testkit::ctx(&runner, dir.path(), &confirm);
    run(&ctx, net, o).unwrap()
}

fn check_at(net: &FakeNet, o: &Options, now: fn() -> DateTime<Utc>) -> DeliveryReport {
    let dir = tempfile::tempdir().unwrap();
    testkit::bundle(dir.path());
    let runner = FakeRunner::new();
    let confirm = ScriptedConfirm::new(&[]);
    let mut ctx = testkit::ctx(&runner, dir.path(), &confirm);
    ctx.now = now;
    run(&ctx, net, o).unwrap()
}

#[test]
fn a_correctly_delivered_installation_passes_every_check_that_can_run() {
    let report = check(&site(false), &opts());
    let bad: Vec<_> = report
        .checks
        .iter()
        .filter(|c| matches!(c.level, Level::Fail | Level::Warn))
        .collect();
    assert!(bad.is_empty(), "{bad:?}");
    assert_eq!((report.overall.as_str(), report.exit_code()), ("passed", 0));
    for id in [
        "reachable",
        "tls-version",
        "tls-chain",
        "tls-expiry",
        "redirect",
        "signin",
        "csp",
        "hsts",
        "nosniff",
        "referrer",
        "frame",
        "banner",
        "robots",
        "healthz",
        "host-check",
        "api-private",
        "marketing",
        "front-page",
        "outbound-references",
    ] {
        assert_eq!(level(&report, id), Level::Pass, "{id}");
    }
}

#[test]
fn the_add_in_check_is_a_named_stub_that_says_not_built_and_does_not_fail_the_run() {
    let report = check(&site(false), &opts());
    let stub = report
        .checks
        .iter()
        .find(|c| c.id == "addin-manifests")
        .unwrap();
    assert_eq!(stub.level, Level::Skip);
    assert!(stub.detail.contains("NOT BUILT") && stub.detail.contains("P0-7"));
    assert_eq!(report.overall, "passed");
}

#[test]
fn an_evaluation_installation_warns_instead_of_failing() {
    let report = check(
        &site(true),
        &Options {
            tls_mode: Some("evaluation".into()),
            ..opts()
        },
    );
    assert_eq!(level(&report, "tls-chain"), Level::Warn);
    assert_eq!(
        level(&report, "hsts"),
        Level::Pass,
        "five-minute HSTS is the design in evaluation mode"
    );
    assert_eq!(
        (report.overall.as_str(), report.exit_code()),
        ("warnings", crate::error::exit::WARNINGS)
    );
    assert!(report
        .checks
        .iter()
        .any(|c| c.detail.contains("Expected in evaluation mode")));
}

#[test]
fn the_same_untrusted_chain_is_a_failure_when_the_mode_is_provided_or_acme() {
    let net = site(false);
    let untrusted = testkit::tls_facts(
        false,
        Err("not issued by an authority this tool trusts (pass it with --ca-file)".into()),
    );
    net.on_https(
        "/login",
        Ok((
            page(200, &headers("max-age=31536000"), LOGIN_HTML),
            untrusted,
        )),
    );
    let report = check(&net, &opts());
    assert_eq!(level(&report, "tls-chain"), Level::Fail);
    assert_eq!(report.exit_code(), crate::error::exit::FAILED);
}

#[test]
fn an_unreachable_installation_fails_once_with_what_to_check() {
    let net = FakeNet::default();
    net.on_https(
        "/login",
        Err("cannot connect to docs.example.org:443: refused".into()),
    );
    let report = check(&net, &opts());
    assert_eq!(report.checks.len(), 1);
    assert_eq!(report.checks[0].level, Level::Fail);
    assert!(
        report.checks[0].detail.contains("DYNODOC_BIND")
            && report.checks[0].detail.contains("dynodoctl status")
    );
}

#[test]
fn missing_or_doubled_headers_fail() {
    for (mutate, id) in [
        (
            Box::new(|h: &mut Vec<(&'static str, String)>| {
                h.retain(|(k, _)| *k != "strict-transport-security")
            }) as Box<dyn Fn(&mut Vec<(&'static str, String)>)>,
            "hsts",
        ),
        (
            Box::new(|h| h.push(("content-security-policy", "default-src *".into()))),
            "csp",
        ),
        (
            Box::new(|h| h.retain(|(k, _)| *k != "content-security-policy")),
            "csp",
        ),
        (
            Box::new(|h| h.retain(|(k, _)| *k != "referrer-policy")),
            "referrer",
        ),
        (
            Box::new(|h| h.retain(|(k, _)| *k != "x-content-type-options")),
            "nosniff",
        ),
        (
            Box::new(|h| h.retain(|(k, _)| *k != "x-frame-options")),
            "frame",
        ),
    ] {
        let net = site(false);
        let mut h = headers("max-age=31536000");
        mutate(&mut h);
        net.on_https(
            "/login",
            Ok((page(200, &h, LOGIN_HTML), testkit::tls_facts(false, Ok(())))),
        );
        assert_eq!(level(&check(&net, &opts()), id), Level::Fail, "{id}");
    }
}

#[test]
fn a_short_hsts_and_a_server_banner_are_warnings() {
    let net = site(false);
    let mut h = headers("max-age=86400");
    h.retain(|(k, _)| *k != "server");
    h.push(("server", "nginx/1.27.0".into()));
    net.on_https(
        "/login",
        Ok((page(200, &h, LOGIN_HTML), testkit::tls_facts(false, Ok(())))),
    );
    let report = check(&net, &opts());
    assert_eq!(level(&report, "hsts"), Level::Warn);
    assert_eq!(level(&report, "banner"), Level::Warn);
}

#[test]
fn the_redirect_must_go_to_the_origin_and_a_closed_port_80_is_only_a_warning() {
    let net = site(false);
    net.http.borrow_mut().clear();
    net.on_http(
        "/",
        Ok(testkit::response(
            301,
            &[("location", "https://evil.example/")],
            "",
        )),
    );
    assert_eq!(level(&check(&net, &opts()), "redirect"), Level::Fail);
    net.http.borrow_mut().clear();
    net.on_http("/", Ok(testkit::response(200, &[], "hello")));
    assert_eq!(level(&check(&net, &opts()), "redirect"), Level::Fail);
    net.http.borrow_mut().clear();
    net.on_http("/", Err("refused".into()));
    assert_eq!(level(&check(&net, &opts()), "redirect"), Level::Warn);
}

#[test]
fn marketing_pages_the_open_api_and_a_name_it_does_not_own_are_failures() {
    let net = site(false);
    let facts = testkit::tls_facts(false, Ok(()));
    net.on_https("/pricing", Ok((page(200, &[], "Plans"), facts.clone())));
    net.on_https(
        "/api/engine/documents",
        Ok((page(200, &[], "[]"), facts.clone())),
    );
    net.on_https(
        "@dynodoc-verify-delivery.invalid/",
        Ok((page(200, &[], "app"), facts)),
    );
    // The first registered answer for a path wins in the fake, so register on a fresh site instead.
    let fresh = FakeNet::default();
    let ok = |s: u16, b: &str| {
        Ok((
            page(s, &headers("max-age=31536000"), b),
            testkit::tls_facts(false, Ok(())),
        ))
    };
    fresh.on_https("/login", ok(200, LOGIN_HTML));
    fresh.on_https("/healthz", ok(200, "{\"status\":\"ok\"}"));
    fresh.on_https("/pricing", ok(200, "Plans"));
    fresh.on_https("/api/engine/documents", ok(200, "[]"));
    fresh.on_https("@dynodoc-verify-delivery.invalid/", ok(200, "app"));
    fresh.on_https("/", ok(200, "home"));
    let report = check(&fresh, &opts());
    assert_eq!(level(&report, "marketing"), Level::Fail);
    assert!(report
        .checks
        .iter()
        .find(|c| c.id == "marketing")
        .unwrap()
        .detail
        .contains("/pricing (200)"));
    assert_eq!(level(&report, "api-private"), Level::Fail);
    assert_eq!(level(&report, "host-check"), Level::Fail);
    assert_eq!(level(&report, "front-page"), Level::Fail);
}

#[test]
fn a_served_script_that_names_the_hosted_site_or_an_analytics_host_fails() {
    for needle in [
        "https://app.dynodoc.online/x",
        "https://www.google-analytics.com/collect",
        "https://cdn.jsdelivr.net/npm/x",
    ] {
        let net = FakeNet::default();
        let ok = |b: &str| {
            Ok((
                page(200, &headers("max-age=31536000"), b),
                testkit::tls_facts(false, Ok(())),
            ))
        };
        net.on_https("/login", ok(LOGIN_HTML));
        net.on_https(
            "/_app/immutable/entry.js",
            ok(&format!("fetch('{needle}')")),
        );
        net.on_https("/_app/immutable/app.css", ok("a{}"));
        let report = check(&net, &opts());
        assert_eq!(
            level(&report, "outbound-references"),
            Level::Fail,
            "{needle}"
        );
        assert!(report
            .checks
            .iter()
            .find(|c| c.id == "outbound-references")
            .unwrap()
            .detail
            .contains("/_app/immutable/entry.js"));
    }
}

#[test]
fn a_page_that_loads_a_script_from_another_host_fails() {
    let net = FakeNet::default();
    let html = "<html><head><script src=\"https://static.thirdparty.example/lib.js\"></script></head></html>";
    net.on_https(
        "/login",
        Ok((
            page(200, &headers("max-age=31536000"), html),
            testkit::tls_facts(false, Ok(())),
        )),
    );
    let report = check(&net, &opts());
    assert_eq!(level(&report, "outbound-references"), Level::Fail);
    assert!(report.checks.iter().any(|c| c
        .detail
        .contains("<script> loads https://static.thirdparty.example/lib.js")));
}

#[test]
fn documented_optional_references_are_warnings_not_failures() {
    let net = FakeNet::default();
    let ok = |b: &str| {
        Ok((
            page(200, &headers("max-age=31536000"), b),
            testkit::tls_facts(false, Ok(())),
        ))
    };
    net.on_https("/login", ok(LOGIN_HTML));
    net.on_https(
        "/_app/immutable/entry.js",
        ok("const office = 'https://appsforoffice.microsoft.com/lib/1/hosted/office.js'"),
    );
    net.on_https("/_app/immutable/app.css", ok("a{}"));
    let report = check(&net, &opts());
    assert_eq!(level(&report, "outbound-references"), Level::Pass);
    assert_eq!(level(&report, "known-references"), Level::Warn);
}

#[test]
fn the_certificate_horizon_warns_inside_thirty_days_and_fails_inside_seven_or_when_expired() {
    fn late_sept() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2046, 9, 25, 12, 0, 0).unwrap()
    }
    fn mid_oct() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2046, 10, 12, 12, 0, 0).unwrap()
    }
    fn after() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2046, 10, 20, 12, 0, 0).unwrap()
    }
    assert_eq!(
        level(&check_at(&site(false), &opts(), late_sept), "tls-expiry"),
        Level::Warn
    );
    assert_eq!(
        level(&check_at(&site(false), &opts(), mid_oct), "tls-expiry"),
        Level::Fail
    );
    let expired = check_at(&site(false), &opts(), after);
    assert_eq!(level(&expired, "tls-expiry"), Level::Fail);
    assert!(expired
        .checks
        .iter()
        .any(|c| c.detail.contains("expired") && c.detail.contains("days ago")));
}

#[test]
fn the_origin_comes_from_env_or_must_be_given() {
    let dir = tempfile::tempdir().unwrap();
    testkit::bundle(dir.path());
    let runner = FakeRunner::new();
    let confirm = ScriptedConfirm::new(&[]);
    let ctx = testkit::ctx(&runner, dir.path(), &confirm);
    let e = run(&ctx, &site(false), &Options::default()).unwrap_err();
    assert_eq!(e.code(), crate::error::exit::USAGE);
    assert!(e.todo.contains("--origin"));
    testkit::installed(dir.path(), testkit::ENV_TEXT);
    let report = run(
        &ctx,
        &site(false),
        &Options {
            tls_mode: Some("provided".into()),
            ..Options::default()
        },
    )
    .unwrap();
    assert_eq!(report.origin, ORIGIN);
}

#[test]
fn connect_overrides_where_to_connect_but_not_the_names() {
    let net = site(false);
    check(
        &net,
        &Options {
            connect: Some("10.0.0.5:8443".into()),
            ..opts()
        },
    );
    let requested = net.requested.borrow().clone();
    assert!(
        requested
            .iter()
            .any(|r| r == "https://docs.example.org/login via 10.0.0.5:8443"),
        "{requested:?}"
    );
    assert!(requested
        .iter()
        .all(|r| !r.contains("via docs.example.org")));
}

#[test]
fn the_mail_check_runs_from_the_server_when_smtp_is_configured() {
    let dir = tempfile::tempdir().unwrap();
    testkit::installed(dir.path(), testkit::ENV_TEXT);
    let runner = testkit::healthy_runner();
    let confirm = ScriptedConfirm::new(&[]);
    let ctx = testkit::ctx(&runner, dir.path(), &confirm);
    let report = run(
        &ctx,
        &site(false),
        &Options {
            tls_mode: Some("provided".into()),
            ..Options::default()
        },
    )
    .unwrap();
    assert_eq!(level(&report, "mail"), Level::Pass);
    assert!(
        runner.ran("mailcheck --to admin@example.org"),
        "{:?}",
        runner.lines()
    );
    assert!(
        !runner.ran("--send"),
        "verify-delivery never sends a message"
    );
    let failing = testkit::healthy_runner();
    failing.first(
        "mailcheck",
        1,
        "TLS handshake failed: check SMTP_PORT and SMTP_SECURITY",
    );
    let ctx2 = testkit::ctx(&failing, dir.path(), &confirm);
    let report2 = run(
        &ctx2,
        &site(false),
        &Options {
            tls_mode: Some("provided".into()),
            ..Options::default()
        },
    )
    .unwrap();
    assert_eq!(level(&report2, "mail"), Level::Fail);
    assert!(report2
        .checks
        .iter()
        .find(|c| c.id == "mail")
        .unwrap()
        .detail
        .contains("TLS handshake failed"));
}

#[test]
fn without_an_install_directory_or_smtp_the_mail_check_is_skipped_with_a_reason() {
    let report = check(&site(false), &opts());
    assert_eq!(level(&report, "mail"), Level::Skip);
    let dir = tempfile::tempdir().unwrap();
    testkit::installed(
        dir.path(),
        &testkit::ENV_TEXT.replace("DYNODOC_MAIL_TRANSPORT=smtp", "DYNODOC_MAIL_TRANSPORT=none"),
    );
    let runner = testkit::healthy_runner();
    let confirm = ScriptedConfirm::new(&[]);
    let ctx = testkit::ctx(&runner, dir.path(), &confirm);
    let r = run(
        &ctx,
        &site(false),
        &Options {
            tls_mode: Some("provided".into()),
            ..Options::default()
        },
    )
    .unwrap();
    assert!(r
        .checks
        .iter()
        .find(|c| c.id == "mail")
        .unwrap()
        .detail
        .contains("not SMTP"));
    assert!(!runner.ran("mailcheck"));
}

#[test]
fn references_are_taken_from_the_tags_a_browser_loads_by_itself() {
    let refs = loaded_references(LOGIN_HTML);
    assert!(refs.contains(&("script".into(), "/_app/immutable/entry.js".into())));
    assert!(refs.contains(&("link".into(), "/_app/immutable/app.css".into())));
    assert!(refs.contains(&("form".into(), "/auth/email".into())));
    assert!(
        !refs.iter().any(|(_, r)| r.contains("example.org/help")),
        "an <a href> is not loaded by itself"
    );
    assert_eq!(
        host_of("https://Static.Example/x"),
        Some("static.example".into())
    );
    assert_eq!(host_of("//cdn.example/x.js"), Some("cdn.example".into()));
    assert_eq!(host_of("/local"), None);
}

#[test]
fn relative_references_resolve_against_the_file_that_names_them() {
    assert_eq!(
        resolve_path("/login", "./_app/a.js").as_deref(),
        Some("/_app/a.js")
    );
    assert_eq!(
        resolve_path("/login", "_app/a.js").as_deref(),
        Some("/_app/a.js")
    );
    assert_eq!(
        resolve_path("/login", "/x/y.css?v=2#top").as_deref(),
        Some("/x/y.css")
    );
    assert_eq!(
        resolve_path("/_app/immutable/entry/start.js", "../chunks/b.js").as_deref(),
        Some("/_app/immutable/chunks/b.js")
    );
    assert_eq!(
        resolve_path("/_app/a.js", "./b.js").as_deref(),
        Some("/_app/b.js")
    );
    assert_eq!(
        resolve_path("/a.js", "../../escape.js"),
        None,
        "a path above the root is not a path"
    );
    for no in [
        "https://x.example/a.js",
        "//x.example/a.js",
        "data:text/plain,hi",
        "mailto:a@x.example",
        "#top",
        "",
    ] {
        assert_eq!(resolve_path("/login", no), None, "{no}");
    }
}

#[test]
fn imports_are_found_in_inline_scripts_and_in_modules() {
    let text = r#"Promise.all([import("./_app/immutable/entry/start.js"), import('./_app/immutable/entry/app.js')]); import {a} from "../chunks/b.js"; import "./c.mjs"; const s = "not-a-module.txt";"#;
    let found = script_imports(text);
    assert_eq!(
        found
            .iter()
            .map(|(s, d)| (s.as_str(), *d))
            .collect::<Vec<_>>(),
        vec![
            ("./_app/immutable/entry/start.js", true),
            ("./_app/immutable/entry/app.js", true),
            ("../chunks/b.js", false),
            ("./c.mjs", false)
        ]
    );
}

#[test]
fn the_crawl_follows_a_sveltekit_style_page_into_its_modules_and_finds_what_they_name() {
    let net = FakeNet::default();
    let ok = |b: &str| {
        Ok((
            page(200, &headers("max-age=31536000"), b),
            testkit::tls_facts(false, Ok(())),
        ))
    };
    let html = r#"<html><head><link href="./_app/immutable/assets/0.css" rel="stylesheet"><script src="./theme-init.js"></script></head><body><script>Promise.all([import("./_app/immutable/entry/start.js")])</script></body></html>"#;
    net.on_https("/login", ok(html));
    net.on_https("/_app/immutable/assets/0.css", ok("a{}"));
    net.on_https(
        "/theme-init.js",
        ok("document.documentElement.dataset.theme='dark'"),
    );
    net.on_https(
        "/_app/immutable/entry/start.js",
        ok("import {x} from '../chunks/deep.js'; export {x}"),
    );
    net.on_https(
        "/_app/immutable/chunks/deep.js",
        ok("fetch('https://app.dynodoc.online/telemetry')"),
    );
    let report = check(&net, &opts());
    let c = report
        .checks
        .iter()
        .find(|c| c.id == "outbound-references")
        .unwrap();
    assert_eq!(c.level, Level::Fail, "{c:?}");
    assert!(
        c.detail.contains("/_app/immutable/chunks/deep.js") && c.detail.contains("dynodoc.online"),
        "{}",
        c.detail
    );
    let clean = FakeNet::default();
    clean.on_https("/login", ok(html));
    clean.on_https("/_app/immutable/assets/0.css", ok("a{}"));
    clean.on_https("/theme-init.js", ok("x"));
    clean.on_https(
        "/_app/immutable/entry/start.js",
        ok("import {x} from '../chunks/deep.js'"),
    );
    clean.on_https("/_app/immutable/chunks/deep.js", ok("export const x = 1"));
    let report = check(&clean, &opts());
    let c = report
        .checks
        .iter()
        .find(|c| c.id == "outbound-references")
        .unwrap();
    assert_eq!(c.level, Level::Pass);
    assert!(c.detail.starts_with("5 files read"), "{}", c.detail);
}

#[test]
fn the_hosted_name_in_a_route_module_that_sign_in_does_not_load_is_a_warning_not_a_failure() {
    let net = FakeNet::default();
    let ok = |b: &str| {
        Ok((
            page(200, &headers("max-age=31536000"), b),
            testkit::tls_facts(false, Ok(())),
        ))
    };
    let html =
        r#"<html><body><script>import("./_app/immutable/entry/app.js")</script></body></html>"#;
    net.on_https("/login", ok(html));
    // The application entry lists every route module as a lazy import, as SvelteKit does.
    net.on_https("/_app/immutable/entry/app.js", ok(r#"export const nodes = [() => import("../nodes/0.js"), () => import("../nodes/3.js")];"#));
    net.on_https("/_app/immutable/nodes/0.js", ok("export default 'shell'"));
    net.on_https(
        "/_app/immutable/nodes/3.js",
        ok("const site = 'https://dynodoc.online/pricing'"),
    );
    let report = check(&net, &opts());
    assert_eq!(
        level(&report, "outbound-references"),
        Level::Pass,
        "{:?}",
        report.checks
    );
    let dormant = report
        .checks
        .iter()
        .find(|c| c.id == "dormant-references")
        .expect("reported");
    assert_eq!(dormant.level, Level::Warn);
    assert!(
        dormant.detail.contains("nodes/3.js") && dormant.detail.contains("criterion 6"),
        "{}",
        dormant.detail
    );
}

#[test]
fn the_same_name_in_a_file_the_sign_in_page_loads_eagerly_fails() {
    let net = FakeNet::default();
    let ok = |b: &str| {
        Ok((
            page(200, &headers("max-age=31536000"), b),
            testkit::tls_facts(false, Ok(())),
        ))
    };
    let html =
        r#"<html><body><script>import("./_app/immutable/entry/app.js")</script></body></html>"#;
    net.on_https("/login", ok(html));
    net.on_https(
        "/_app/immutable/entry/app.js",
        ok(r#"import {a} from "../chunks/shared.js";"#),
    );
    net.on_https(
        "/_app/immutable/chunks/shared.js",
        ok("fetch('https://app.dynodoc.online/telemetry')"),
    );
    let report = check(&net, &opts());
    assert_eq!(level(&report, "outbound-references"), Level::Fail);
    assert!(report.checks.iter().all(|c| c.id != "dormant-references"));
}

#[test]
fn the_crawl_is_bounded_and_says_how_much_it_left_unread() {
    let net = FakeNet::default();
    let ok = |b: &str| {
        Ok((
            page(200, &headers("max-age=31536000"), b),
            testkit::tls_facts(false, Ok(())),
        ))
    };
    net.on_https(
        "/login",
        ok(r#"<html><script>import("./_app/entry.js")</script></html>"#),
    );
    let many: String = (0..100)
        .map(|i| format!("import(\"./n{i}.js\");"))
        .collect();
    net.on_https("/_app/entry.js", ok(&many));
    for i in 0..100 {
        net.on_https(&format!("/_app/n{i}.js"), ok("export {}"));
    }
    let report = check(&net, &opts());
    let c = report
        .checks
        .iter()
        .find(|c| c.id == "outbound-references")
        .unwrap();
    assert_eq!(c.level, Level::Pass);
    assert!(
        c.detail.contains("more not read (limit 60)"),
        "{}",
        c.detail
    );
    assert!(
        net.requested.borrow().len() < 90,
        "the number of requests is bounded"
    );
}

#[test]
fn the_json_has_the_documented_shape() {
    let v = serde_json::to_value(check(&site(false), &opts())).unwrap();
    for key in ["schema_version", "origin", "tls_mode", "checks", "overall"] {
        assert!(v.get(key).is_some(), "{key}");
    }
    let first = &v["checks"][0];
    for key in ["id", "level", "title", "detail"] {
        assert!(first.get(key).is_some(), "{key}");
    }
    assert!(["pass", "warn", "fail", "skip"].contains(&first["level"].as_str().unwrap()));
}

#[test]
fn the_text_report_names_the_result() {
    let text = check(&site(false), &opts()).render();
    assert!(text.contains("Result: PASSED") && text.contains("NOT BUILT"));
}
