//! Tests for the review fixes in `verify-delivery` (M4): the mode comes from the operator only,
//! redirects are parsed, outside origins are found whatever their name, an incomplete crawl is a
//! warning, bracketed IPv6 works, and the TLS version claim is honest.

use super::*;

fn ok_page(body: &str) -> std::result::Result<(HttpResponse, crate::net::TlsFacts), String> {
    Ok((
        page(200, &headers("max-age=31536000"), body),
        testkit::tls_facts(false, Ok(())),
    ))
}

fn detail<'a>(report: &'a DeliveryReport, id: &str) -> &'a str {
    &report.checks.iter().find(|c| c.id == id).unwrap().detail
}

#[test]
fn an_evaluation_certificate_cannot_turn_a_provided_installation_into_a_warning() {
    // M4: `evaluation` used to be true if the certificate said so or a header said so.
    let net = site(true);
    let report = check(&net, &opts()); // the operator says `provided`
    assert_eq!(level(&report, "tls-chain"), Level::Fail);
    assert!(
        detail(&report, "tls-chain").contains("evaluation certificate"),
        "the failure names what it is"
    );
    assert_eq!(report.overall, "failed");
    // The HSTS leniency of evaluation mode is not granted either.
    assert_eq!(level(&report, "hsts"), Level::Warn);
}

#[test]
fn a_header_that_claims_evaluation_mode_changes_nothing() {
    let net = site(false);
    let untrusted = testkit::tls_facts(false, Err("not trusted".into()));
    let mut h = headers("max-age=300");
    h.push(("x-dynodoc-tls-mode", "evaluation".into()));
    net.on_https("/login", Ok((page(200, &h, LOGIN_HTML), untrusted)));
    let report = check(&net, &opts());
    assert_eq!(level(&report, "tls-chain"), Level::Fail);
    assert_eq!(level(&report, "hsts"), Level::Warn);
}

#[test]
fn the_redirect_is_parsed_not_matched_as_text() {
    for location in [
        "https://docs.example.org.evil.example/",
        "https://docs.example.org@evil.example/",
        "https://evil.example\\@docs.example.org/",
        "https://docs.example.org:8443/",
        "http://docs.example.org/",
        "//docs.example.org/",
        "/login",
        "https://docs.example.orgevil/",
        "",
    ] {
        let net = site(false);
        net.http.borrow_mut().clear();
        net.on_http(
            "/",
            Ok(testkit::response(301, &[("location", location)], "")),
        );
        assert_eq!(
            level(&check(&net, &opts()), "redirect"),
            Level::Fail,
            "{location:?}"
        );
    }
    for location in [
        "https://docs.example.org/",
        "https://DOCS.example.org/login?x=1",
        "https://docs.example.org:443/",
        "HTTPS://docs.example.org",
    ] {
        let net = site(false);
        net.http.borrow_mut().clear();
        net.on_http(
            "/",
            Ok(testkit::response(308, &[("location", location)], "")),
        );
        assert_eq!(
            level(&check(&net, &opts()), "redirect"),
            Level::Pass,
            "{location:?}"
        );
    }
}

#[test]
fn urls_are_split_the_way_a_browser_decides_where_they_go() {
    let o = crate::origin::Origin::parse(ORIGIN).unwrap();
    let own = |u: &str| split_url(u).is_some_and(|p| p.is_own(&o));
    assert!(own("https://docs.example.org"));
    assert!(own("https://docs.example.org/a/b.js?x=1#y"));
    assert!(own("HTTPS://Docs.Example.ORG:443/"));
    assert!(!own("http://docs.example.org/"));
    assert!(!own("https://docs.example.org:8443/"));
    assert!(!own("https://docs.example.org.evil.example/"));
    assert!(!own("https://docs.example.org@evil.example/"));
    assert!(!own("https://evil.example\\@docs.example.org/"));
    assert!(!own("https://evil.example?@docs.example.org/"));
    assert!(!own("https://evil.example#@docs.example.org/"));
    assert!(!own("https://user:pw@docs.example.org/"));
    assert!(!own("https://docs.example.org\t.evil.example/"));
    assert!(!own("ftp://docs.example.org/"));
    let v6 = split_url("https://[2001:db8::1]:8443/x").unwrap();
    assert_eq!((v6.host.as_str(), v6.port), ("2001:db8::1", 8443));
    assert_eq!(v6.origin(), "https://[2001:db8::1]:8443");
    assert_eq!(
        split_url("//cdn.example/x.js").unwrap().scheme,
        "https",
        "protocol-relative takes the page's scheme"
    );
    assert_eq!(split_url("javascript:alert(1)"), None);
    assert_eq!(split_url("data:text/plain,hi"), None);
}

#[test]
fn connect_takes_a_host_a_host_and_port_and_ipv6_in_either_form() {
    assert_eq!(parse_connect("127.0.0.1", 443), ("127.0.0.1".into(), 443));
    assert_eq!(
        parse_connect("127.0.0.1:8443", 443),
        ("127.0.0.1".into(), 8443)
    );
    assert_eq!(
        parse_connect("docs.internal", 443),
        ("docs.internal".into(), 443)
    );
    assert_eq!(
        parse_connect("docs.internal:444", 443),
        ("docs.internal".into(), 444)
    );
    // Used to read `::1` as host `:` with port 1.
    assert_eq!(parse_connect("::1", 443), ("::1".into(), 443));
    assert_eq!(parse_connect("[::1]", 8443), ("::1".into(), 8443));
    assert_eq!(parse_connect("[::1]:9443", 443), ("::1".into(), 9443));
    assert_eq!(
        parse_connect("2001:db8::7", 443),
        ("2001:db8::7".into(), 443)
    );
}

#[test]
fn an_ipv6_connect_reaches_the_right_target() {
    let net = site(false);
    let report = check(
        &net,
        &Options {
            connect: Some("::1".into()),
            ..opts()
        },
    );
    assert_eq!(level(&report, "reachable"), Level::Pass);
    assert!(
        net.requested
            .borrow()
            .iter()
            .all(|r| r.ends_with("via ::1:443") || r.ends_with("via ::1:80")),
        "{:?}",
        net.requested.borrow()
    );
}

#[test]
fn any_outside_origin_is_found_whatever_its_name_and_listed_once() {
    // M4: the deny-list knew twelve strings. A tracker it had never heard of passed.
    for needle in [
        "https://tracker.never-heard-of.example/pixel.gif",
        "http://plain.http.example:8080/x",
        "https://203.0.113.9/collect",
        "https://[2001:db8::5]/collect",
        "https://docs.example.org.evil.example/x",
        "https://docs.example.org@evil.example/x",
    ] {
        let net = FakeNet::default();
        net.on_https("/login", ok_page(LOGIN_HTML));
        net.on_https(
            "/_app/immutable/entry.js",
            ok_page(&format!("fetch('{needle}'); fetch('{needle}');")),
        );
        net.on_https("/_app/immutable/app.css", ok_page("a{}"));
        let report = check(&net, &opts());
        assert_eq!(
            level(&report, "outbound-references"),
            Level::Fail,
            "{needle}"
        );
        let text = detail(&report, "outbound-references");
        assert!(text.contains("entry.js"), "{needle}: {text}");
        let origin = split_url(needle).unwrap().origin();
        assert_eq!(text.matches(&origin).count(), 1, "listed once: {text}");
    }
}

#[test]
fn every_distinct_origin_is_reported() {
    let net = FakeNet::default();
    net.on_https("/login", ok_page(LOGIN_HTML));
    net.on_https(
        "/_app/immutable/entry.js",
        ok_page(
            "a('https://one.example/x'); b('https://two.example/y'); c('https://one.example/z')",
        ),
    );
    net.on_https("/_app/immutable/app.css", ok_page("a{}"));
    let report = check(&net, &opts());
    let text = detail(&report, "outbound-references");
    assert!(
        text.contains("https://one.example") && text.contains("https://two.example"),
        "{text}"
    );
}

#[test]
fn the_installations_own_origin_and_the_documented_inert_ones_are_not_outside() {
    let net = FakeNet::default();
    net.on_https("/login", ok_page(LOGIN_HTML));
    net.on_https(
        "/_app/immutable/entry.js",
        ok_page(
            "const a = 'https://docs.example.org/api'; const ns = 'http://www.w3.org/2000/svg'; const e = 'https://svelte.dev/e/lifecycle_outside_component';",
        ),
    );
    net.on_https("/_app/immutable/app.css", ok_page("a{}"));
    let report = check(&net, &opts());
    assert_eq!(
        level(&report, "outbound-references"),
        Level::Pass,
        "{:?}",
        report.checks
    );
}

#[test]
fn the_operators_own_support_link_is_not_outside() {
    let dir = tempfile::tempdir().unwrap();
    testkit::installed(
        dir.path(),
        &format!(
            "{}DYNODOC_SUPPORT_URL=https://help.example.net/dynodoc\n",
            testkit::ENV_TEXT.replace("DYNODOC_TLS_MODE=evaluation", "DYNODOC_TLS_MODE=provided")
        ),
    );
    let net = FakeNet::default();
    net.on_https(
        "/login",
        ok_page(&LOGIN_HTML.replace("/help", "https://help.example.net/dynodoc")),
    );
    net.on_https("/_app/immutable/entry.js", ok_page("export {}"));
    net.on_https("/_app/immutable/app.css", ok_page("a{}"));
    let runner = FakeRunner::new();
    let confirm = ScriptedConfirm::new(&[]);
    let ctx = testkit::ctx(&runner, dir.path(), &confirm);
    let report = run(
        &ctx,
        &net,
        &Options {
            origin: Some(ORIGIN.into()),
            ..Options::default()
        },
    )
    .unwrap();
    assert_eq!(
        level(&report, "outbound-references"),
        Level::Pass,
        "{:?}",
        report.checks
    );
    // Another origin, same page: not allowed.
    let net2 = FakeNet::default();
    net2.on_https(
        "/login",
        ok_page(&LOGIN_HTML.replace("/help", "https://other.example.net/x")),
    );
    net2.on_https("/_app/immutable/entry.js", ok_page("export {}"));
    net2.on_https("/_app/immutable/app.css", ok_page("a{}"));
    let report2 = run(
        &ctx,
        &net2,
        &Options {
            origin: Some(ORIGIN.into()),
            ..Options::default()
        },
    )
    .unwrap();
    assert_eq!(level(&report2, "outbound-references"), Level::Fail);
}

#[test]
fn an_absolute_reference_to_the_installations_own_origin_is_crawled() {
    // M4: it used to be skipped, so what that file names was never looked at.
    let net = FakeNet::default();
    net.on_https(
        "/login",
        ok_page(
            "<html><script src=\"https://docs.example.org/_app/abs.js\"></script><script src=\"//docs.example.org/_app/rel.js\"></script></html>",
        ),
    );
    net.on_https(
        "/_app/abs.js",
        ok_page("fetch('https://tracker.example/a')"),
    );
    net.on_https("/_app/rel.js", ok_page("export {}"));
    let report = check(&net, &opts());
    assert_eq!(level(&report, "outbound-references"), Level::Fail);
    assert!(detail(&report, "outbound-references").contains("https://tracker.example"));
    let requested = net.requested.borrow();
    assert!(
        requested.iter().any(|r| r.contains("/_app/abs.js")),
        "{requested:?}"
    );
    assert!(
        requested.iter().any(|r| r.contains("/_app/rel.js")),
        "{requested:?}"
    );
}

#[test]
fn a_file_that_cannot_be_read_makes_the_result_a_warning_not_a_pass() {
    let net = FakeNet::default();
    net.on_https("/login", ok_page(LOGIN_HTML));
    net.on_https("/_app/immutable/entry.js", Err("read timed out".into()));
    net.on_https("/_app/immutable/app.css", ok_page("a{}"));
    let report = check(&net, &opts());
    assert_eq!(level(&report, "outbound-references"), Level::Warn);
    assert!(detail(&report, "outbound-references").contains("could not be read"));
    let net2 = FakeNet::default();
    net2.on_https("/login", ok_page(LOGIN_HTML));
    net2.on_https(
        "/_app/immutable/entry.js",
        Ok((page(500, &[], "x"), testkit::tls_facts(false, Ok(())))),
    );
    net2.on_https("/_app/immutable/app.css", ok_page("a{}"));
    assert_eq!(
        level(&check(&net2, &opts()), "outbound-references"),
        Level::Warn
    );
}

#[test]
fn the_tls_version_check_only_claims_what_it_tested() {
    let report = check(&site(false), &opts());
    let c = report
        .checks
        .iter()
        .find(|c| c.id == "tls-version")
        .unwrap();
    assert!(c.title.contains("negotiated"), "{}", c.title);
    assert!(c.detail.contains("not tested"), "{}", c.detail);
    assert!(!c.title.contains("only"));
}

#[test]
fn hsts_max_age_is_found_whatever_the_case_or_quoting() {
    assert_eq!(
        max_age("max-age=31536000; includeSubDomains"),
        Some(31_536_000)
    );
    assert_eq!(max_age("Max-Age=15552000"), Some(15_552_000));
    assert_eq!(
        max_age("includeSubDomains; MAX-AGE=\"15552000\""),
        Some(15_552_000)
    );
    assert_eq!(max_age("max-age = 100"), Some(100));
    assert_eq!(max_age("includeSubDomains"), None);
    let net = site(false);
    let mut h = headers("MAX-AGE=31536000; includeSubDomains");
    h.retain(|(k, _)| *k != "server");
    h.push(("server", "nginx".into()));
    net.on_https(
        "/login",
        Ok((page(200, &h, LOGIN_HTML), testkit::tls_facts(false, Ok(())))),
    );
    assert_eq!(level(&check(&net, &opts()), "hsts"), Level::Pass);
}
