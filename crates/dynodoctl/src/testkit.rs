//! Shared helpers for unit tests: a fake bundle directory, seeded fake secrets, canned Docker answers.

use crate::ctx::{Ctx, Layout, ScriptedConfirm};
use crate::runner::FakeRunner;
use chrono::{DateTime, TimeZone, Utc};
use std::path::Path;

/// Fake secrets, written into `secrets/`. Tests prove none of these ever reaches an output.
pub const SEEDED: [(&str, &str); 7] = [
    ("session_secret", "SEEDED-session-secret-0123456789abcdef0123456789abcdef"),
    ("web_service_key", "SEEDED-web-service-key-fedcba9876543210fedcba9876543210"),
    ("paseto_local_key", "SEEDEDpasetolocalkey00112233445566778899aabbccddeeff00"),
    ("postgres_password", "SEEDED-owner-password-aabbccddeeff"),
    ("runtime_database_password", "SEEDED-runtime-password-112233445566"),
    ("database_url", "postgres://dynodoc_runtime:SEEDED-runtime-password-112233445566@postgres:5432/dynodoc"),
    ("smtp_password", "SEEDED-smtp-password-zz99yy88"),
];

pub fn fixed_now() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 10, 22, 12, 0, 0).unwrap()
}

pub const ENV_TEXT: &str = "# generated\nDYNODOC_MODE=selfhosted\nORIGIN=https://docs.example.org\nDYNODOC_TLS_MODE=evaluation\nADMIN_EMAILS=admin@example.org\nDYNODOC_MAIL_TRANSPORT=smtp\nSMTP_HOST=smtp.example.org\nSMTP_FROM=docs@example.org\nSESSION_SECRET_FILE=/run/secrets/session_secret\nSMTP_PASSWORD_FILE=/run/secrets/smtp_password\n";

/// `compose.yml` and the usual directories.
pub fn bundle(dir: &Path) {
    std::fs::write(dir.join("compose.yml"), "name: dynodoc-selfhosted\nservices: {}\n").unwrap();
    for d in ["jobs", "tls", "config"] {
        std::fs::create_dir_all(dir.join(d)).unwrap();
    }
    std::fs::write(dir.join("tls").join("README.md"), "tls\n").unwrap();
}

/// `.env` and `secrets/` as an installation has them.
pub fn installed(dir: &Path, env_text: &str) {
    bundle(dir);
    std::fs::write(dir.join(".env"), env_text).unwrap();
    std::fs::create_dir_all(dir.join("secrets")).unwrap();
    for (name, value) in SEEDED {
        std::fs::write(dir.join("secrets").join(name), format!("{value}\n")).unwrap();
    }
}

pub fn ps_line(service: &str, state: &str, health: &str, exit: i32) -> String {
    format!("{{\"Service\":\"{service}\",\"State\":\"{state}\",\"Health\":\"{health}\",\"ExitCode\":{exit},\"Image\":\"dynodoc-local/{service}:dev\"}}\n")
}

/// `docker compose ps -a --format json` for a healthy installation.
pub fn ps_healthy() -> String {
    let mut text = String::new();
    for s in ["postgres", "api", "web", "proxy"] {
        text.push_str(&ps_line(s, "running", "healthy", 0));
    }
    for s in ["secrets-init", "preflight", "db-roles", "init"] {
        text.push_str(&ps_line(s, "exited", "", 0));
    }
    text
}

pub const FACTS_LINE: &str = "52|52|36|2|9|16.4\n";

/// A runner that answers like a healthy Docker host with a healthy installation.
pub fn healthy_runner() -> FakeRunner {
    let r = FakeRunner::new();
    r.on("version --format {{.Server.Version}}", 0, "29.5.3\n");
    r.on("compose version --short", 0, "v5.1.4\n");
    r.on("info --format", 0, "8589934592|2026-10-22T12:00:00Z\n");
    r.on("ps -a --format json", 0, &ps_healthy());
    r.on("pg_database_size", 0, "123456\n");
    r.on("_sqlx_migrations where success", 0, FACTS_LINE);
    r.on("to_regclass", 0, "t\n");
    r.on("select 1 from pg_database", 0, "1\n");
    r.on("select id from document", 0, "0190a1b2-0000-7000-8000-000000000001\n0190a1b2-0000-7000-8000-000000000002\n");
    r.on("pg_restore --list", 0, "; Archive\n1; 1 1 SCHEMA - public x\n2; 1 2 TABLE public event x\n");
    r.on("verify-snapshots --all", 0, "OK: document a; 1 snapshots; 3 replayed events; through seq 3\nOK: document b; 1 snapshots; 2 replayed events; through seq 2\n");
    r.on("verified $n documents", 0, "OK: event chain and 1 legacy snapshots verified through seq 3 for document a; head 00\nOK: event chain and 1 legacy snapshots verified through seq 2 for document b; head 11\nverified 2 documents\n");
    r.on("engine-cli", 0, "");
    r.on_file("pg_dump", b"PGDMP\x01\x0e fake custom-format dump");
    r
}

pub fn facts() -> crate::db::DbFacts {
    crate::db::DbFacts { migrations_applied: 52, latest_migration: 52, events: 36, documents: 2, identities: 9, server_version: "16.4".into() }
}

/// A valid backup directory (real dump bytes, checksums that match). With `config_from`, the
/// settings and secrets of that installation are copied in as `config/`.
pub fn make_backup(backups: &Path, name: &str, created: &str, kind: &str, config_from: Option<&Path>) -> std::path::PathBuf {
    use crate::manifest::{self, ConfigEntry, DumpEntry, FileEntry};
    let path = backups.join(name);
    std::fs::create_dir_all(&path).unwrap();
    let dump = b"PGDMP\x01\x0e fake custom-format dump";
    std::fs::write(path.join(manifest::DUMP_NAME), dump).unwrap();
    let mut m = manifest::sample(kind, created);
    m.database = DumpEntry { file: manifest::DUMP_NAME.into(), bytes: dump.len() as u64, sha256: crate::util::sha256_hex(dump), toc_entries: 2 };
    m.facts = facts();
    m.facts_after = facts();
    if let Some(from) = config_from {
        let root = path.join("config");
        let mut files: Vec<FileEntry> = Vec::new();
        let mut rels = vec![".env".to_string()];
        for (n, _) in SEEDED {
            rels.push(format!("secrets/{n}"));
        }
        for rel in rels {
            let target = root.join(&rel);
            std::fs::create_dir_all(target.parent().unwrap()).unwrap();
            std::fs::copy(from.join(&rel), &target).unwrap();
            files.push(manifest::file_entry(&root, &rel).unwrap());
        }
        m.config = ConfigEntry { location: "inside".into(), dir_name: None, files };
    }
    manifest::write_manifest(&path, &m).unwrap();
    path
}

pub fn confirm_yes() -> ScriptedConfirm {
    ScriptedConfirm::new(&[true, true, true, true])
}

pub fn ctx<'a>(runner: &'a FakeRunner, dir: &Path, confirm: &'a ScriptedConfirm) -> Ctx<'a> {
    let mut c = Ctx::new(runner, Layout::new(dir), confirm);
    c.now = fixed_now;
    c.progress_to_stderr = true;
    c
}

/// Every seeded secret value, to assert that none appears in an output.
pub fn assert_no_seeded_secret(text: &str, context: &str) {
    for (name, value) in SEEDED {
        assert!(!text.contains(value), "{context}: the value of {name} leaked");
    }
}
