//! `dynodoctl status`: what is running, how healthy, which versions, how long the certificate
//! lasts, how far the schema is, how big the volumes are and when the last backup was taken,
//! with warnings. `--json` prints the same facts as one object (schema_version 1).

use crate::backup::newest_age_days;
use crate::compose::{docker, Compose, ServiceState, LONG_RUNNING};
use crate::ctx::Ctx;
use crate::db::{self, LIVE_DB};
use crate::error::{exit, Result};
use crate::manifest;
use crate::net::{Net, Target};
use crate::origin::Origin;
use crate::util::{human_bytes, iso};
use serde::Serialize;

pub const SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug)]
pub struct Options {
    /// Warn when the newest backup is older than this many days.
    pub max_backup_age_days: i64,
    pub volumes: bool,
    /// Where backups are kept, if not backups/ of the bundle directory.
    pub backup_dir: Option<std::path::PathBuf>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            max_backup_age_days: 2,
            volumes: true,
            backup_dir: None,
        }
    }
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct VersionInfo {
    pub dynodoctl: String,
    pub bundle_version: String,
    pub image_registry: String,
    pub images: std::collections::BTreeMap<String, String>,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct TlsStatus {
    pub mode: String,
    pub origin: String,
    pub probe: String,
    pub reachable: bool,
    pub protocol: String,
    pub subject: String,
    pub issuer: String,
    pub not_after: String,
    pub days_left: Option<i64>,
    pub evaluation_certificate: bool,
    pub chain_trusted: Option<bool>,
    pub trust_note: String,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct DbStatus {
    pub reachable: bool,
    pub migrations_applied: i64,
    pub latest_migration: i64,
    pub events: i64,
    pub documents: i64,
    pub size_bytes: u64,
    pub server_version: String,
    pub leftover_scratch_databases: Vec<String>,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct VolumeUse {
    pub name: String,
    pub size: String,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct BackupStatus {
    pub directory: String,
    pub count: usize,
    pub newest: Option<String>,
    pub newest_utc: Option<String>,
    pub newest_age_days: Option<i64>,
    pub max_age_days: i64,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct StatusReport {
    pub schema_version: u32,
    pub generated_utc: String,
    pub directory: String,
    pub project: String,
    /// `ok`, `warnings` or `problems`.
    pub overall: String,
    pub services: Vec<ServiceState>,
    pub versions: VersionInfo,
    pub tls: TlsStatus,
    pub database: DbStatus,
    pub volumes: Vec<VolumeUse>,
    pub backup: BackupStatus,
    pub warnings: Vec<String>,
    pub problems: Vec<String>,
}

impl StatusReport {
    pub fn exit_code(&self) -> i32 {
        match self.overall.as_str() {
            "problems" => exit::FAILED,
            "warnings" => exit::WARNINGS,
            _ => exit::OK,
        }
    }

    pub fn render(&self) -> String {
        let mut o = String::new();
        o.push_str(&format!(
            "Dynodoc status: {}  ({}, project {})\n\n",
            self.overall.to_uppercase(),
            self.directory,
            self.project
        ));
        o.push_str("Services\n");
        for s in &self.services {
            let health = if s.health.is_empty() {
                String::new()
            } else {
                format!(", {}", s.health)
            };
            let exit = if s.state == "exited" {
                format!(" (exit {})", s.exit_code)
            } else {
                String::new()
            };
            o.push_str(&format!(
                "  {:<14} {}{}{}   {}\n",
                s.service, s.state, exit, health, s.image
            ));
        }
        o.push_str(&format!(
            "\nVersions\n  dynodoctl {}, bundle version {}{}\n",
            self.versions.dynodoctl,
            self.versions.bundle_version,
            if self.versions.image_registry.is_empty() {
                String::new()
            } else {
                format!(", registry {}", self.versions.image_registry)
            }
        ));
        o.push_str(&format!(
            "\nCertificate\n  mode {}, origin {}\n",
            self.tls.mode, self.tls.origin
        ));
        if self.tls.reachable {
            o.push_str(&format!(
                "  {} - subject {}, expires {} ({} days), {}\n",
                self.tls.protocol,
                self.tls.subject,
                self.tls.not_after,
                self.tls
                    .days_left
                    .map(|d| d.to_string())
                    .unwrap_or_else(|| "?".into()),
                match self.tls.chain_trusted {
                    Some(true) => "chain trusted".to_string(),
                    Some(false) => format!("chain NOT trusted ({})", self.tls.trust_note),
                    None => "trust not checked".to_string(),
                }
            ));
        } else {
            o.push_str(&format!("  not reachable at {}\n", self.tls.probe));
        }
        o.push_str("\nDatabase\n");
        if self.database.reachable {
            o.push_str(&format!("  PostgreSQL {}, schema level {} ({} migrations applied), {} events, {} documents, {}\n", self.database.server_version, self.database.latest_migration, self.database.migrations_applied, self.database.events, self.database.documents, human_bytes(self.database.size_bytes)));
        } else {
            o.push_str("  not reachable\n");
        }
        if !self.volumes.is_empty() {
            o.push_str("\nVolumes\n");
            for v in &self.volumes {
                o.push_str(&format!("  {:<40} {}\n", v.name, v.size));
            }
        }
        o.push_str("\nBackups\n");
        match &self.backup.newest {
            Some(n) => o.push_str(&format!(
                "  {} backup(s) in {}; newest {} ({} days ago)\n",
                self.backup.count,
                self.backup.directory,
                n,
                self.backup.newest_age_days.unwrap_or(0)
            )),
            None => o.push_str(&format!("  none in {}\n", self.backup.directory)),
        }
        for p in &self.problems {
            o.push_str(&format!("\nPROBLEM: {p}\n"));
        }
        for w in &self.warnings {
            o.push_str(&format!("\nWARNING: {w}\n"));
        }
        o
    }
}

/// `137.6MB`, `16.38kB`, `0B` as Docker prints them -> bytes (decimal units).
pub fn parse_docker_size(text: &str) -> Option<u64> {
    let t = text.trim();
    let split = t.find(|c: char| c.is_ascii_alphabetic())?;
    let (num, unit) = t.split_at(split);
    let n: f64 = num.trim().parse().ok()?;
    let mult = match unit.to_ascii_lowercase().as_str() {
        "b" => 1.0,
        "kb" => 1e3,
        "mb" => 1e6,
        "gb" => 1e9,
        "tb" => 1e12,
        _ => return None,
    };
    Some((n * mult).round() as u64)
}

fn volume_uses(ctx: &Ctx<'_>, project: &str) -> Vec<VolumeUse> {
    let Ok(out) = docker(ctx, &["system", "df", "-v", "--format", "json"]) else {
        return Vec::new();
    };
    if !out.ok() {
        return Vec::new();
    }
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&out.stdout_text()) else {
        return Vec::new();
    };
    let prefix = format!("{project}_");
    let mut v: Vec<VolumeUse> = value
        .get("Volumes")
        .and_then(|x| x.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|x| {
                    let name = x.get("Name")?.as_str()?;
                    name.starts_with(&prefix).then(|| VolumeUse {
                        name: name.to_string(),
                        size: x
                            .get("Size")
                            .and_then(|s| s.as_str())
                            .unwrap_or("?")
                            .to_string(),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    v.sort_by(|a, b| a.name.cmp(&b.name));
    v
}

pub fn collect(ctx: &Ctx<'_>, net: &dyn Net, opts: &Options) -> Result<StatusReport> {
    ctx.require_installed()?;
    let env = ctx.env()?;
    let c = Compose::new(ctx);
    let mut report = StatusReport {
        schema_version: SCHEMA_VERSION,
        generated_utc: iso((ctx.now)()),
        directory: ctx.layout.dir.display().to_string(),
        project: ctx.project_name(),
        ..StatusReport::default()
    };

    // An unfinished swap or upgrade comes first: it explains most of what follows.
    match crate::recover::read_journal(&ctx.layout) {
        Ok(None) => {}
        Ok(Some(j)) => report.problems.push(format!(
            "an interrupted database swap is recorded in state/{} ({} started {}, backup {}, phase {}): run `dynodoctl recover` before anything else",
            crate::recover::JOURNAL_NAME, j.operation, j.started_utc, j.backup, j.phase
        )),
        Err(e) => report.problems.push(format!("{} - {}", e.what, e.todo)),
    }
    if let Some(r) = crate::upgrade::unfinished(ctx) {
        report.problems.push(format!(
            "the upgrade {} -> {} started {} did not finish (its record says `switching`): check the services below, then `dynodoctl rollback` to go back or `docker compose up -d --wait` to go on",
            r.from_version, r.to_version, r.started_utc
        ));
    }

    // Services.
    let mut docker_unreachable = false;
    report.services = match c.ps() {
        Ok(services) => services,
        Err(e) => {
            docker_unreachable = true;
            report.problems.push(format!(
                "cannot reach Docker, so the services are unknown: {}  What to do: {}",
                e.what, e.todo
            ));
            Vec::new()
        }
    };
    if report.services.is_empty() && !docker_unreachable {
        report.problems.push("no services exist: the installation has not been started (`dynodoctl install`, or `docker compose up -d --wait`)".into());
    }
    for name in LONG_RUNNING {
        match report.services.iter().find(|s| s.service == name) {
            Some(s) if s.up() => {}
            Some(s) => report.problems.push(format!(
                "{name} is {}{}",
                s.state,
                if s.health.is_empty() {
                    String::new()
                } else {
                    format!(" and {}", s.health)
                }
            )),
            None if !report.services.is_empty() => {
                report.problems.push(format!("{name} does not exist"))
            }
            None => {}
        }
    }
    for job in ["secrets-init", "preflight", "db-roles", "init"] {
        if let Some(s) = report.services.iter().find(|s| s.service == job) {
            if s.state == "exited" && s.exit_code != 0 {
                report.problems.push(format!(
                    "the job {job} failed (exit {}): `docker compose logs {job}`",
                    s.exit_code
                ));
            }
        }
    }

    // Versions.
    report.versions = VersionInfo {
        dynodoctl: crate::VERSION.to_string(),
        bundle_version: env.get("DYNODOC_VERSION").unwrap_or("dev").to_string(),
        image_registry: env.get("DYNODOC_IMAGE_REGISTRY").unwrap_or("").to_string(),
        images: report
            .services
            .iter()
            .filter(|s| !s.image.is_empty() && LONG_RUNNING.contains(&s.service.as_str()))
            .map(|s| (s.service.clone(), s.image.clone()))
            .collect(),
    };

    // Certificate.
    let mode = env.get("DYNODOC_TLS_MODE").unwrap_or("").to_string();
    report.tls.mode = mode.clone();
    report.tls.origin = env.get("ORIGIN").unwrap_or("").to_string();
    if let Ok(origin) = Origin::parse(&report.tls.origin) {
        let port: u16 = env
            .get("DYNODOC_HTTPS_PORT")
            .and_then(|p| p.parse().ok())
            .unwrap_or(443);
        report.tls.probe = format!("127.0.0.1:{port}");
        let target = Target {
            connect_host: "127.0.0.1".into(),
            port,
            server_name: origin.host.clone(),
            host_header: origin.host_header(),
        };
        if report
            .services
            .iter()
            .any(|s| s.service == "proxy" && s.running())
        {
            match net.https_get(&target, "/healthz") {
                Ok((_, facts)) => {
                    report.tls.reachable = true;
                    report.tls.protocol = facts.protocol.clone();
                    match &facts.trust {
                        Ok(()) => report.tls.chain_trusted = Some(true),
                        Err(e) => {
                            report.tls.chain_trusted = Some(false);
                            report.tls.trust_note = e.clone();
                        }
                    }
                    if let Some(cert) = facts.leaf() {
                        let days = cert.days_left((ctx.now)());
                        report.tls.subject = cert.subject.clone();
                        report.tls.issuer = cert.issuer.clone();
                        report.tls.not_after = cert.not_after.clone();
                        report.tls.days_left = Some(days);
                        report.tls.evaluation_certificate = cert.is_evaluation();
                        if days < 0 {
                            report.problems.push(format!("the certificate expired {} days ago: renew it (README, \"Renewing without downtime\")", -days));
                        } else if days < 7 {
                            report
                                .problems
                                .push(format!("the certificate expires in {days} days"));
                        } else if days < 30 && !cert.is_evaluation() {
                            report.warnings.push(format!(
                                "the certificate expires in {days} days; renew it before then"
                            ));
                        }
                    }
                }
                Err(e) => report.problems.push(format!(
                    "the proxy is running but HTTPS on {} did not answer: {e}",
                    report.tls.probe
                )),
            }
        }
    }
    if mode == "evaluation" || report.tls.evaluation_certificate {
        report.warnings.push("TLS mode `evaluation` is in use: a self-signed certificate that browsers and Office add-ins do not trust. Not for production: use `provided` or `acme`.".into());
    } else if report.tls.chain_trusted == Some(false) && report.tls.reachable {
        report.warnings.push(format!("the certificate chain is not trusted by the public roots this tool knows ({}); if you use a company authority, pass its certificate with --ca-file to check", report.tls.trust_note));
    }

    // Exposure and development switches.
    let bind = env.get("DYNODOC_BIND").unwrap_or("127.0.0.1");
    if bind != "127.0.0.1" && bind != "::1" {
        report.warnings.push(format!("DYNODOC_BIND is {bind}: the proxy is reachable from other computers. Docker publishes ports around host firewalls such as ufw."));
    }
    if env
        .get("DEV_AUTH")
        .map(|v| v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
    {
        report.warnings.push("DEV_AUTH=true: the sign-in page shows the link instead of mailing it, so anyone who can reach the page can sign in as anyone, administrators included. Never on a machine other people can reach.".into());
    }

    // Database.
    if report
        .services
        .iter()
        .any(|s| s.service == "postgres" && s.up())
    {
        match db::read_facts(&c, LIVE_DB) {
            Ok(f) => {
                report.database = DbStatus {
                    reachable: true,
                    migrations_applied: f.migrations_applied,
                    latest_migration: f.latest_migration,
                    events: f.events,
                    documents: f.documents,
                    size_bytes: db::size_bytes(&c, LIVE_DB).unwrap_or(0),
                    server_version: f.server_version,
                    leftover_scratch_databases: db::list_scratch_databases(&c).unwrap_or_default(),
                };
                if !report.database.leftover_scratch_databases.is_empty() {
                    report.warnings.push(format!(
                        "scratch databases from an interrupted restore check remain: {}. Remove them: docker compose exec postgres sh -c 'PGPASSWORD=$(cat /run/secrets/postgres_password) dropdb -h 127.0.0.1 -U dynodoc <name>'",
                        report.database.leftover_scratch_databases.join(", ")
                    ));
                }
            }
            Err(e) => report
                .problems
                .push(format!("the database did not answer: {}", e.what)),
        }
    }

    // Volumes.
    if opts.volumes {
        report.volumes = volume_uses(ctx, &report.project);
    }

    // Backups.
    let dir = opts
        .backup_dir
        .clone()
        .unwrap_or_else(|| ctx.layout.backups_dir());
    let all = manifest::list_backups(&dir);
    report.backup = BackupStatus {
        directory: dir.display().to_string(),
        count: all.len(),
        max_age_days: opts.max_backup_age_days,
        ..BackupStatus::default()
    };
    if let Some((created, age)) = newest_age_days(ctx, &dir) {
        report.backup.newest = all.last().map(|b| b.name());
        report.backup.newest_utc = Some(created);
        report.backup.newest_age_days = Some(age);
        if age > opts.max_backup_age_days {
            report.warnings.push(format!("the newest backup is {age} days old (warning after {} days): run `dynodoctl backup`", opts.max_backup_age_days));
        }
    } else {
        report.warnings.push("there is no backup yet: run `dynodoctl backup --verify`, and keep a copy of .env and secrets/ somewhere else".into());
    }
    if report.backup.count > 0 && dir.starts_with(&ctx.layout.dir) {
        report.warnings.push("backups are on the same disk as the installation; copy them to another machine or use `backup --to <other disk>`: same-host backups are not disaster recovery.".into());
    }

    report.overall = if !report.problems.is_empty() {
        "problems"
    } else if !report.warnings.is_empty() {
        "warnings"
    } else {
        "ok"
    }
    .into();
    Ok(report)
}

#[cfg(test)]
#[path = "tests/status.rs"]
mod tests;
