//! `dynodoctl support-bundle`: one `.tar.gz` an operator can attach to a support request.
//!
//! What goes in: versions, `docker compose ps` and `config`, recent logs, `.env` with secret-looking
//! values hidden, disk and memory, certificate summaries (names and dates, never a key), the
//! migration level, backup names and ages, and `status --json`. Every text passes the redactor.
//! What never goes in: any file from `secrets/` (the guard below refuses the read), a private key,
//! backup contents, document content. The installation's own secret values are read only to
//! build the in-memory list of strings to hide; they are never written out. The command prints
//! exactly what the archive contains, with a checksum for each entry.

use crate::compose::{docker, Compose};
use crate::ctx::{Ctx, Layout};
use crate::error::{CliError, Result};
use crate::manifest;
use crate::net::Net;
use crate::redact::Redactor;
use crate::status;
use crate::tarball::TarBuilder;
use crate::util::{create_private_dir, free_bytes, human_bytes, iso, sha256_hex, stamp, write_private};
use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct Options {
    pub out_dir: Option<PathBuf>,
    pub log_lines: usize,
    pub keep_emails: bool,
    pub list_only: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self { out_dir: None, log_lines: 400, keep_emails: false, list_only: false }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct EntryInfo {
    pub name: String,
    pub bytes: u64,
    pub sha256: String,
    pub source: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct SupportReport {
    pub path: Option<PathBuf>,
    pub archive_bytes: u64,
    pub entries: Vec<EntryInfo>,
    pub excluded: Vec<String>,
    pub secret_values_hidden: usize,
    pub emails_hidden: bool,
}

impl SupportReport {
    pub fn render(&self) -> String {
        let mut o = String::new();
        match &self.path {
            Some(p) => o.push_str(&format!("Support bundle written: {} ({})\n", p.display(), human_bytes(self.archive_bytes))),
            None => o.push_str("Support bundle (listing only, nothing was written)\n"),
        }
        o.push_str("\nIncluded (each text passed the redaction rules):\n");
        for e in &self.entries {
            o.push_str(&format!("  {:<34} {:>9}  sha256 {}  <- {}\n", e.name, human_bytes(e.bytes), &e.sha256[..12], e.source));
        }
        o.push_str("\nNever included:\n");
        for x in &self.excluded {
            o.push_str(&format!("  - {x}\n"));
        }
        o.push_str(&format!(
            "\nRedaction: {} secret value(s) of this installation are hidden wherever they appear, plus URL passwords, tokens, key=value pairs, private keys and long hex keys; email addresses are {}.\n",
            self.secret_values_hidden,
            if self.emails_hidden { "replaced by [email]" } else { "KEPT (--keep-emails)" }
        ));
        o.push_str("Read the archive before you send it: `tar -tzf` lists it and `tar -xzOf <file> <entry>` shows one entry.\n");
        o
    }
}

/// The one door every file read goes through. Nothing under `secrets/`, no private key.
pub fn read_allowed(layout: &Layout, path: &Path) -> Result<Vec<u8>> {
    let canon = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let secrets = std::fs::canonicalize(layout.secrets_dir()).unwrap_or_else(|_| layout.secrets_dir());
    let name = path.file_name().map(|n| n.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
    if canon.starts_with(&secrets) || path.starts_with(layout.secrets_dir()) || name == "privkey.pem" || name.ends_with(".key") || name.contains("private") {
        return Err(CliError::failed(format!("refusing to put {} in a support bundle", path.display()), "support bundles never contain secrets or private keys; this is a bug in dynodoctl if you did not ask for it"));
    }
    Ok(std::fs::read(path)?)
}

/// Load this installation's secret values into the redactor (memory only).
pub fn load_secrets(layout: &Layout, redactor: &mut Redactor) {
    let Ok(entries) = std::fs::read_dir(layout.secrets_dir()) else { return };
    for e in entries.flatten() {
        if let (Ok(text), true) = (std::fs::read_to_string(e.path()), e.path().is_file()) {
            let name = e.file_name().to_string_lossy().into_owned();
            redactor.add_secret(&name, &text);
            // The password inside a connection URL is a secret on its own.
            if let Some((_, rest)) = text.trim().split_once("://") {
                if let Some((creds, _)) = rest.split_once('@') {
                    if let Some((_, pw)) = creds.split_once(':') {
                        redactor.add_secret(&format!("{name}-password"), pw);
                    }
                }
            }
        }
    }
}

struct Bundle {
    entries: Vec<(EntryInfo, Vec<u8>)>,
    redactor: Redactor,
}

impl Bundle {
    fn add_text(&mut self, name: &str, source: &str, text: &str) {
        let clean = self.redactor.apply(text);
        let bytes = clean.into_bytes();
        self.entries.push((EntryInfo { name: name.to_string(), bytes: bytes.len() as u64, sha256: sha256_hex(&bytes), source: source.to_string() }, bytes));
    }

    /// For the manifest only: it holds nothing but our own fixed text, names and checksums
    /// (64-digit hex values that the redaction would, correctly, take for keys).
    fn add_own_json(&mut self, name: &str, source: &str, text: &str) {
        let bytes = text.as_bytes().to_vec();
        self.entries.push((EntryInfo { name: name.to_string(), bytes: bytes.len() as u64, sha256: sha256_hex(&bytes), source: source.to_string() }, bytes));
    }

    fn add_command(&mut self, name: &str, source: &str, result: Result<crate::runner::Output>) {
        let text = match result {
            Ok(o) if o.ok() => o.stdout_text() + &o.stderr_text(),
            Ok(o) => format!("[command failed with exit code {}]\n{}", o.status, o.text()),
            Err(e) => format!("[command could not run]\n{}", e.what),
        };
        self.add_text(name, source, &text);
    }
}

pub fn excluded_list() -> Vec<String> {
    vec![
        "any file from secrets/ (the values are used only to hide them if they appear in a log)".to_string(),
        "private keys (tls/privkey.pem and every other key file)".to_string(),
        "backups/ (database dumps and the copy of the secrets)".to_string(),
        "document content and the database itself (only counts and the schema level appear)".to_string(),
    ]
}

pub fn run(ctx: &Ctx<'_>, net: &dyn Net, opts: &Options) -> Result<SupportReport> {
    ctx.require_bundle()?;
    let now = (ctx.now)();
    let mut redactor = Redactor::new(!opts.keep_emails);
    load_secrets(&ctx.layout, &mut redactor);
    let secret_values_hidden = redactor.known_count();
    let mut bundle = Bundle { entries: Vec::new(), redactor };
    let c = Compose::new(ctx);

    let mut versions = format!("dynodoctl {}\nhost {} {}\n", crate::VERSION, std::env::consts::OS, std::env::consts::ARCH);
    for args in [&["version"][..], &["compose", "version"][..]] {
        match docker(ctx, args) {
            Ok(o) => versions.push_str(&format!("\n$ docker {}\n{}\n", args.join(" "), o.text())),
            Err(e) => versions.push_str(&format!("\n$ docker {}\n[could not run] {}\n", args.join(" "), e.what)),
        }
    }
    bundle.add_text("versions.txt", "dynodoctl, docker version, docker compose version", &versions);

    let mut host = format!("generated {}\n", iso(now));
    for (label, p) in [("install directory", ctx.layout.dir.clone()), ("backups directory", ctx.layout.backups_dir())] {
        if let Some(free) = free_bytes(&p).or_else(|| p.parent().and_then(free_bytes)) {
            host.push_str(&format!("free disk at {label} {}: {}\n", p.display(), human_bytes(free)));
        }
    }
    bundle.add_text("host.txt", "dynodoctl", &host);
    bundle.add_command("docker-info.txt", "docker info (memory, CPUs, storage driver)", docker(ctx, &["info"]));
    bundle.add_command("docker-disk.txt", "docker system df", docker(ctx, &["system", "df"]));

    if ctx.layout.is_installed() {
        match ctx.env() {
            Ok(env) => bundle.add_text("env.redacted", ".env with secret-looking values hidden", &env.render_redacted()),
            Err(e) => bundle.add_text("env.redacted", ".env", &format!("[could not read] {}", e.what)),
        }
        match status::collect(ctx, net, &status::Options::default()) {
            Ok(report) => bundle.add_text("status.json", "dynodoctl status --json", &serde_json::to_string_pretty(&report)?),
            Err(e) => bundle.add_text("status.json", "dynodoctl status --json", &format!("[status failed] {}", e.what)),
        }
        // Certificate summary: names and dates from the public chain, never a key.
        let mut certs = String::new();
        let chain = ctx.layout.tls_dir().join("fullchain.pem");
        if chain.is_file() {
            match read_allowed(&ctx.layout, &chain) {
                Ok(bytes) => {
                    for (i, der) in crate::x509::pem_certificates(&String::from_utf8_lossy(&bytes)).iter().enumerate() {
                        match crate::x509::parse_cert(der) {
                            Some(c) => certs.push_str(&format!("tls/fullchain.pem #{}: subject {}, issuer {}, valid {} to {}, names {}\n", i + 1, c.subject, c.issuer, c.not_before, c.not_after, c.dns_names.join(", "))),
                            None => certs.push_str(&format!("tls/fullchain.pem #{}: not readable\n", i + 1)),
                        }
                    }
                }
                Err(e) => certs.push_str(&format!("[{}]\n", e.what)),
            }
        }
        certs.push_str("The certificate the proxy serves is in status.json (tls section).\n");
        bundle.add_text("certificates.txt", "tls/fullchain.pem (public certificates only) and the proxy probe", &certs);
    }

    bundle.add_command("compose-ps.json", "docker compose ps -a", c.run(&["ps", "-a", "--format", "json"]));
    bundle.add_command("compose-config.yml", "docker compose config (interpolated)", c.run(&["config"]));
    let services: Vec<String> = c.ps().map(|s| s.into_iter().map(|x| x.service).collect()).unwrap_or_default();
    let tail = opts.log_lines.to_string();
    for service in &services {
        bundle.add_command(&format!("logs/{service}.log"), &format!("docker compose logs --tail {}", opts.log_lines), c.run(&["logs", "--no-color", "--tail", &tail, service]));
    }
    if ctx.layout.is_installed() && services.iter().any(|s| s == "postgres") {
        if let Ok(f) = crate::db::read_facts(&c, crate::db::LIVE_DB) {
            bundle.add_text("migration-level.txt", "SELECT over _sqlx_migrations, event, document, identity (counts only)", &format!("schema level {} ({} migrations applied)\nPostgreSQL {}\n{} events, {} documents, {} identities\n", f.latest_migration, f.migrations_applied, f.server_version, f.events, f.documents, f.identities));
        }
    }
    let backups = manifest::list_backups(&ctx.layout.backups_dir());
    let mut list = String::new();
    for b in &backups {
        list.push_str(&format!("{}  kind {}  taken {}  dump {} bytes  config {}\n", b.name(), b.manifest.kind, b.manifest.created_utc, b.manifest.database.bytes, b.manifest.config.location));
    }
    if list.is_empty() {
        list.push_str("no backups found\n");
    }
    bundle.add_text("backups.txt", "names and dates from backups/*/manifest.json (no contents)", &list);

    let name = format!("dynodoc-support-{}", stamp(now));
    let excluded = excluded_list();
    let readme = format!(
        "Dynodoc support bundle {name}\nGenerated {} by dynodoctl {}.\n\nThis archive holds diagnostic text only. Secrets, private keys, backups and document content are never included (see manifest.json, \"excluded\"). Every text was passed through redaction; read it before you send it.\n",
        iso(now),
        crate::VERSION
    );
    bundle.add_text("README.txt", "dynodoctl", &readme);

    let infos: Vec<EntryInfo> = bundle.entries.iter().map(|(i, _)| i.clone()).collect();
    let manifest_json = serde_json::to_string_pretty(&serde_json::json!({
        "format": 1,
        "tool": format!("dynodoctl {}", crate::VERSION),
        "generated_utc": iso(now),
        "entries": infos,
        "excluded": excluded,
        "redaction": { "secret_values_hidden": secret_values_hidden, "emails_hidden": !opts.keep_emails },
    }))?;
    bundle.add_own_json("manifest.json", "dynodoctl", &manifest_json);

    let entries: Vec<EntryInfo> = bundle.entries.iter().map(|(i, _)| i.clone()).collect();
    if opts.list_only {
        return Ok(SupportReport { path: None, archive_bytes: 0, entries, excluded, secret_values_hidden, emails_hidden: !opts.keep_emails });
    }
    let mut tar = TarBuilder::new(now.timestamp().max(0) as u64);
    for (info, bytes) in &bundle.entries {
        tar.add_file(&format!("{name}/{}", info.name), bytes)?;
    }
    let gz = tar.finish_gz()?;
    let dir = opts.out_dir.clone().unwrap_or_else(|| ctx.layout.dir.join("support"));
    create_private_dir(&dir)?;
    let path = dir.join(format!("{name}.tar.gz"));
    if path.exists() {
        return Err(CliError::refused(format!("{} already exists", path.display()), "wait a second, or choose another --out"));
    }
    write_private(&path, &gz)?;
    Ok(SupportReport { path: Some(path), archive_bytes: gz.len() as u64, entries, excluded, secret_values_hidden, emails_hidden: !opts.keep_emails })
}

#[cfg(test)]
#[path = "tests/support.rs"]
mod tests;
