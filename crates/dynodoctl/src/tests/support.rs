use super::*;
use crate::ctx::ScriptedConfirm;
use crate::net::FakeNet;
use crate::runner::FakeRunner;
use crate::tarball::read_tar_gz;
use crate::testkit::{self, assert_no_seeded_secret, SEEDED};

fn seeded_runner() -> FakeRunner {
    let r = testkit::healthy_runner();
    // Every place a secret could plausibly leak from: logs, resolved config, docker info, ps.
    r.first("compose-ps-marker", 0, "");
    r.first("logs --no-color --tail", 0, &format!(
        "web | boot ok\nweb | SESSION_SECRET={} loaded\napi | connecting postgres://dynodoc_runtime:{}@postgres:5432/dynodoc\napi | Authorization: Bearer {}\nweb | sign-in link sent to person@example.org\nweb | smtp password {} rejected\nweb | owner pw {}\n",
        SEEDED[0].1, "SEEDED-runtime-password-112233445566", SEEDED[1].1, SEEDED[6].1, SEEDED[3].1
    ));
    r.first(" config", 0, &format!(
        "services:\n  web:\n    environment:\n      SESSION_SECRET: {}\n      SMTP_PASSWORD: {}\n      ADMIN_EMAILS: admin@example.org\n      DATABASE_URL: {}\n",
        SEEDED[0].1, SEEDED[6].1, SEEDED[5].1
    ));
    r.first("info", 0, &format!("Server Version: 29.5.3\nTotal Memory: 8GiB\nHTTP Proxy: http://user:{}@proxy.corp:3128\n", SEEDED[4].1));
    r
}

fn setup() -> (tempfile::TempDir, FakeRunner) {
    let dir = tempfile::tempdir().unwrap();
    // A pasted secret in .env, as the redaction must also catch.
    testkit::installed(dir.path(), &format!("{}SMTP_PASSWORD={}\n", testkit::ENV_TEXT, SEEDED[6].1));
    (dir, seeded_runner())
}

fn fake_net() -> FakeNet {
    let n = FakeNet::default();
    n.on_https("/healthz", Ok((testkit::response(200, &[], "ok"), testkit::tls_facts(false, Ok(())))));
    n
}

fn build(dir: &Path, runner: &FakeRunner, opts: &Options) -> SupportReport {
    let confirm = ScriptedConfirm::new(&[]);
    let ctx = testkit::ctx(runner, dir, &confirm);
    run(&ctx, &fake_net(), opts).unwrap()
}

fn all_text(report: &SupportReport) -> String {
    let files = read_tar_gz(&std::fs::read(report.path.as_ref().unwrap()).unwrap()).unwrap();
    files.iter().map(|(n, b)| format!("== {n}\n{}\n", String::from_utf8_lossy(b))).collect()
}

#[test]
fn no_seeded_secret_survives_anywhere_in_the_archive() {
    let (dir, runner) = setup();
    let report = build(dir.path(), &runner, &Options::default());
    let text = all_text(&report);
    assert_no_seeded_secret(&text, "support bundle");
    for needle in ["Seeded-0001", "SEEDED-runtime", "SEEDED-owner", "SEEDED-smtp", "SEEDED-session", "SEEDED-web-service"] {
        assert!(!text.contains(needle), "{needle} leaked");
    }
    assert!(text.contains("[redacted"), "the redaction left its marks");
    assert!(report.secret_values_hidden >= 7, "{}", report.secret_values_hidden);
}

#[test]
fn the_listing_and_the_json_never_contain_a_secret_either() {
    let (dir, runner) = setup();
    let report = build(dir.path(), &runner, &Options::default());
    assert_no_seeded_secret(&report.render(), "support listing");
    assert_no_seeded_secret(&serde_json::to_string(&report).unwrap(), "support json");
}

#[test]
fn emails_are_hidden_by_default_and_kept_on_request() {
    let (dir, runner) = setup();
    let hidden = all_text(&build(dir.path(), &runner, &Options::default()));
    assert!(!hidden.contains("person@example.org") && !hidden.contains("admin@example.org"), "an address survived");
    assert!(hidden.contains("[email]"));
    let dir2 = tempfile::tempdir().unwrap();
    testkit::installed(dir2.path(), testkit::ENV_TEXT);
    let kept = build(dir2.path(), &seeded_runner(), &Options { keep_emails: true, ..Options::default() });
    let text = all_text(&kept);
    assert!(text.contains("person@example.org"));
    assert_no_seeded_secret(&text, "keep-emails bundle");
    assert!(!kept.emails_hidden);
}

#[test]
fn nothing_from_secrets_or_a_private_key_is_ever_read_into_the_bundle() {
    let layout = Layout::new("/srv/dynodoc");
    for forbidden in ["/srv/dynodoc/secrets/session_secret", "/srv/dynodoc/secrets/anything", "/srv/dynodoc/tls/privkey.pem", "/srv/dynodoc/tls/server.key", "/srv/dynodoc/tls/private-key.pem"] {
        let e = read_allowed(&layout, Path::new(forbidden)).unwrap_err();
        assert!(e.what.contains("refusing"), "{forbidden}");
    }
    let dir = tempfile::tempdir().unwrap();
    testkit::installed(dir.path(), testkit::ENV_TEXT);
    let real = Layout::new(dir.path());
    assert!(read_allowed(&real, &dir.path().join("secrets").join("session_secret")).is_err(), "an existing secret file is refused too");
    std::fs::write(dir.path().join("config").join("notice.txt"), "hello").unwrap();
    assert_eq!(read_allowed(&real, &dir.path().join("config").join("notice.txt")).unwrap(), b"hello");
}

#[test]
fn no_entry_comes_from_secrets_and_a_private_key_in_tls_is_not_read() {
    let (dir, runner) = setup();
    std::fs::write(dir.path().join("tls").join("privkey.pem"), "-----BEGIN PRIVATE KEY-----\nPRIVATE-KEY-MARKER-0042\n-----END PRIVATE KEY-----\n").unwrap();
    std::fs::write(dir.path().join("tls").join("fullchain.pem"), format!("-----BEGIN CERTIFICATE-----\n{}\n-----END CERTIFICATE-----\n", crate::x509::TEST_CERT2_B64)).unwrap();
    let report = build(dir.path(), &runner, &Options::default());
    assert!(report.entries.iter().all(|e| !e.source.contains("secrets/") && !e.name.starts_with("secrets")), "{:?}", report.entries);
    let text = all_text(&report);
    assert!(!text.contains("PRIVATE-KEY-MARKER-0042"));
    assert!(text.contains("CN=docs.example.org") && text.contains("2046-10-17"), "the public certificate is summarised");
    assert!(!text.contains(crate::x509::TEST_CERT2_B64), "only the summary, not the certificate");
}

#[test]
fn the_archive_has_the_expected_entries_each_listed_with_a_matching_checksum() {
    let (dir, runner) = setup();
    let report = build(dir.path(), &runner, &Options::default());
    let path = report.path.clone().unwrap();
    assert!(path.starts_with(dir.path().join("support")) && path.to_string_lossy().ends_with("dynodoc-support-20261022T120000Z.tar.gz"));
    let files = read_tar_gz(&std::fs::read(&path).unwrap()).unwrap();
    let names: Vec<String> = files.iter().map(|(n, _)| n.trim_start_matches("dynodoc-support-20261022T120000Z/").to_string()).collect();
    for expected in ["versions.txt", "host.txt", "docker-info.txt", "docker-disk.txt", "env.redacted", "status.json", "certificates.txt", "compose-ps.json", "compose-config.yml", "logs/web.log", "logs/postgres.log", "migration-level.txt", "backups.txt", "README.txt", "manifest.json"] {
        assert!(names.contains(&expected.to_string()), "{expected} missing from {names:?}");
    }
    let manifest: serde_json::Value = serde_json::from_slice(&files.iter().find(|(n, _)| n.ends_with("manifest.json")).unwrap().1).unwrap();
    assert_eq!(manifest["format"], 1);
    for entry in manifest["entries"].as_array().unwrap() {
        let name = entry["name"].as_str().unwrap();
        let (_, bytes) = files.iter().find(|(n, _)| n.ends_with(&format!("/{name}"))).unwrap_or_else(|| panic!("{name} listed but not archived"));
        assert_eq!(entry["sha256"].as_str().unwrap(), sha256_hex(bytes), "{name}");
    }
    assert!(manifest["excluded"].as_array().unwrap().iter().any(|x| x.as_str().unwrap().contains("secrets/")));
    assert!(report.render().contains("Never included"));
}

#[test]
fn listing_only_writes_nothing() {
    let (dir, runner) = setup();
    let report = build(dir.path(), &runner, &Options { list_only: true, ..Options::default() });
    assert!(report.path.is_none() && !report.entries.is_empty());
    assert!(!dir.path().join("support").exists());
    assert!(report.render().contains("listing only"));
}

#[test]
fn a_failing_command_is_recorded_in_the_bundle_not_hidden() {
    let (dir, runner) = setup();
    runner.first("logs --no-color --tail", 1, "Error response from daemon: No such container");
    let text = all_text(&build(dir.path(), &runner, &Options::default()));
    assert!(text.contains("[command failed with exit code 1]") && text.contains("No such container"));
}

#[test]
fn an_existing_archive_is_not_overwritten() {
    let (dir, runner) = setup();
    build(dir.path(), &runner, &Options::default());
    let confirm = ScriptedConfirm::new(&[]);
    let ctx = testkit::ctx(&runner, dir.path(), &confirm);
    assert_eq!(run(&ctx, &fake_net(), &Options::default()).unwrap_err().code(), crate::error::exit::REFUSED);
}

#[cfg(unix)]
#[test]
fn the_archive_is_private_to_its_owner() {
    let (dir, runner) = setup();
    let report = build(dir.path(), &runner, &Options::default());
    assert_eq!(crate::util::unix_mode(report.path.as_ref().unwrap()), Some(0o600));
}

#[test]
fn log_lines_are_limited_by_the_option() {
    let (dir, runner) = setup();
    build(dir.path(), &runner, &Options { log_lines: 25, ..Options::default() });
    assert!(runner.ran("logs --no-color --tail 25 web"));
}

#[test]
fn a_bundle_directory_that_is_not_installed_still_produces_a_bundle() {
    let dir = tempfile::tempdir().unwrap();
    testkit::bundle(dir.path());
    let runner = testkit::healthy_runner();
    let report = build(dir.path(), &runner, &Options::default());
    assert!(report.entries.iter().any(|e| e.name == "versions.txt") && !report.entries.iter().any(|e| e.name == "env.redacted"));
}
