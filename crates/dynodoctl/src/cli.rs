//! Argument parsing and the mapping from results to output and exit codes.
//!
//! Exit codes (docs/SELF-HOSTED-OPERATIONS.md): 0 done and every check passed; 1 a check or step
//! failed; 2 wrong arguments; 3 refused before changing anything; 4 finished with warnings only.

use crate::ctx::{Confirm, Ctx, Interactive, Layout};
use crate::error::{exit, CliError, Result};
use crate::net::Net;
use crate::runner::Runner;
use crate::{backup, configure, install, rehearse, restore, status, support, upgrade, verify_delivery};
use clap::{Args, Parser, Subcommand};
use serde::Serialize;
use std::io::{BufRead, Write};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(
    name = "dynodoctl",
    version,
    about = "Install, operate, back up and upgrade a single-node Dynodoc installation (infra/selfhosted).",
    long_about = "Install, operate, back up and upgrade a single-node Dynodoc installation (infra/selfhosted).\n\nEvery command works in the bundle directory (the one with compose.yml): run it there, or pass --dir.\nExit codes: 0 done, all checks passed | 1 a check or step failed | 2 wrong arguments | 3 refused before changing anything | 4 finished with warnings only."
)]
pub struct Cli {
    /// The bundle directory (compose.yml, .env, secrets/). Default: $DYNODOCTL_DIR, else the current directory.
    #[arg(long, global = true, value_name = "DIR")]
    pub dir: Option<PathBuf>,
    /// Docker Compose project name (the bundle's own name applies when omitted).
    #[arg(long, global = true, value_name = "NAME")]
    pub project: Option<String>,
    /// An extra compose file after compose.yml and the release overlay (repeatable).
    #[arg(short = 'f', long = "file", global = true, value_name = "FILE")]
    pub files: Vec<PathBuf>,
    /// Print one JSON object on standard output (progress goes to standard error).
    #[arg(long, global = true)]
    pub json: bool,
    /// Answer a typed confirmation in advance, with the exact phrase the command names (repeatable).
    #[arg(long = "confirm", global = true, value_name = "PHRASE")]
    pub confirm: Vec<String>,
    /// Answer `yes` to the plain yes/no question of `install`.
    #[arg(long, global = true)]
    pub yes: bool,
    /// A PEM file with extra trusted certificate authorities for the checks that look at the certificate (repeatable).
    #[arg(long, global = true, value_name = "FILE")]
    pub ca_file: Vec<PathBuf>,
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Check the machine, create settings and secrets, start the stack and say what to do next.
    Install(InstallArgs),
    /// Show, check, change and apply settings in .env.
    Configure {
        #[command(subcommand)]
        action: ConfigureAction,
    },
    /// Services, health, versions, certificate, schema level, disk, last backup, warnings.
    Status(StatusArgs),
    /// A consistent database backup with a manifest, checksums and retention.
    Backup(BackupArgs),
    /// Restore a backup into a throwaway database, verify the history, report, remove it.
    RestoreVerify(RestoreVerifyArgs),
    /// Replace the live database with a backup (loudly confirmed).
    Restore(RestoreArgs),
    /// Rehearse a release on a copy of the latest backup (migrations, history, API start).
    RehearseLite(RehearseArgs),
    /// Back up, rehearse, switch to a new release, check health, print the way back.
    Upgrade(UpgradeArgs),
    /// Undo the last upgrade by restoring its pre-upgrade backup into the previous version.
    Rollback(RollbackArgs),
    /// Collect diagnostics into one redacted .tar.gz.
    SupportBundle(SupportArgs),
    /// Check a running installation from the operator's side: HTTPS, headers, pages, references.
    VerifyDelivery(VerifyArgs),
}

#[derive(Args, Debug, Default)]
pub struct InstallArgs {
    /// Copy compose.yml, jobs/, tls/, config/ from this bundle directory into --dir first.
    #[arg(long, value_name = "DIR")]
    pub from: Option<PathBuf>,
    /// What people type in the browser, https://host[:port].
    #[arg(long)]
    pub origin: Option<String>,
    /// Certificate mode: evaluation | provided | acme (no default on purpose).
    #[arg(long)]
    pub tls: Option<String>,
    /// Comma-separated addresses allowed to use /admin.
    #[arg(long)]
    pub admin_emails: Option<String>,
    #[arg(long)]
    pub org_name: Option<String>,
    #[arg(long)]
    pub brand_name: Option<String>,
    #[arg(long)]
    pub support_url: Option<String>,
    #[arg(long)]
    pub smtp_host: Option<String>,
    #[arg(long)]
    pub smtp_from: Option<String>,
    #[arg(long)]
    pub smtp_user: Option<String>,
    #[arg(long)]
    pub smtp_port: Option<String>,
    #[arg(long)]
    pub smtp_security: Option<String>,
    /// Path of a CA file as the web container sees it (for example /etc/dynodoc/mail-ca.pem).
    #[arg(long)]
    pub smtp_ca_file: Option<String>,
    /// Read the mail server password from this file (never from an argument).
    #[arg(long, value_name = "FILE")]
    pub smtp_password_file: Option<PathBuf>,
    /// Read the mail server password from one line of standard input.
    #[arg(long)]
    pub smtp_password_stdin: bool,
    /// Also enable the legacy .doc converter.
    #[arg(long)]
    pub with_converter: bool,
    /// --tls provided: the certificate chain, server certificate first.
    #[arg(long, value_name = "FILE")]
    pub cert: Option<PathBuf>,
    /// --tls provided: the private key (PEM, no passphrase).
    #[arg(long, value_name = "FILE")]
    pub key: Option<PathBuf>,
    #[arg(long)]
    pub acme_email: Option<String>,
    /// letsencrypt-staging | letsencrypt | an ACME directory URL (no default).
    #[arg(long)]
    pub acme_ca: Option<String>,
    /// Accept the certificate authority's terms of service (read them first).
    #[arg(long)]
    pub acme_accept_tos: bool,
    /// DYNODOC_BIND: 127.0.0.1 (this machine only, the default) or 0.0.0.0 or one address.
    #[arg(long)]
    pub bind: Option<String>,
    #[arg(long)]
    pub http_port: Option<u16>,
    #[arg(long)]
    pub https_port: Option<u16>,
    #[arg(long)]
    pub registry: Option<String>,
    #[arg(long)]
    pub version: Option<String>,
    /// The release's compose.release.generated.yml (images pinned by digest).
    #[arg(long, value_name = "FILE")]
    pub overlay: Option<PathBuf>,
    /// How to reach the settings generator: auto | node | image.
    #[arg(long, default_value = "auto")]
    pub generator: String,
    #[arg(long, value_name = "FILE")]
    pub generator_script: Option<PathBuf>,
    /// Build the images from this source checkout first (compose.build.yml).
    #[arg(long)]
    pub build: bool,
    /// Create settings and secrets but do not start the stack.
    #[arg(long)]
    pub no_start: bool,
    /// Show the preflight and the plan; change nothing.
    #[arg(long)]
    pub dry_run: bool,
    /// Seconds to wait for the stack to become healthy.
    #[arg(long, default_value_t = 300)]
    pub wait_timeout: u32,
}

#[derive(Subcommand, Debug)]
pub enum ConfigureAction {
    /// List the settings dynodoctl manages and their values (no secret is ever shown).
    Show,
    /// Change settings (NAME=VALUE ...), validate them, keep a dated copy of the previous .env.
    Set {
        /// For example: smtp-host=smtp.example.org smtp-from=docs@example.org
        #[arg(required = true, value_name = "NAME=VALUE")]
        assignments: Vec<String>,
        /// Restart the services with the new settings (a failed start puts the previous ones back).
        #[arg(long)]
        apply: bool,
        /// Run the mail check against this recipient; a failure takes the change back.
        #[arg(long, value_name = "ADDRESS")]
        mail_to: Option<String>,
        /// With --mail-to: also send one test message.
        #[arg(long)]
        send: bool,
    },
    /// Run the web tier's own configuration check (and the mail check with --mail-to).
    Check {
        #[arg(long, value_name = "ADDRESS")]
        mail_to: Option<String>,
        #[arg(long)]
        send: bool,
    },
    /// Re-run the jobs and recreate the services whose configuration changed.
    Apply,
}

#[derive(Args, Debug)]
pub struct StatusArgs {
    /// Warn when the newest backup is older than this many days.
    #[arg(long, default_value_t = 2)]
    pub max_backup_age_days: i64,
    /// Where the backups are, if not backups/ of the bundle directory.
    #[arg(long, value_name = "DIR")]
    pub backup_dir: Option<PathBuf>,
    /// Skip the volume sizes (they take a few seconds).
    #[arg(long)]
    pub no_volumes: bool,
}

#[derive(Args, Debug)]
pub struct BackupArgs {
    /// Directory that receives the backup (default: backups/ of the bundle directory).
    #[arg(long, value_name = "DIR")]
    pub to: Option<PathBuf>,
    /// Send the copy of .env and secrets/ to this other directory (not encrypted).
    #[arg(long, value_name = "DIR")]
    pub secrets_to: Option<PathBuf>,
    #[arg(long)]
    pub label: Option<String>,
    /// Routine backups to keep; older ones are removed after this one succeeded.
    #[arg(long, default_value_t = 7)]
    pub keep: usize,
    /// Restore the new backup into a throwaway database and verify it.
    #[arg(long)]
    pub verify: bool,
    /// Do not copy .env and secrets/ at all.
    #[arg(long)]
    pub no_config: bool,
}

#[derive(Args, Debug)]
pub struct RestoreVerifyArgs {
    /// A backup directory or its name; default: the newest.
    #[arg(long)]
    pub backup: Option<String>,
    /// Where the copy of the settings and secrets is, if it was sent elsewhere (`backup --secrets-to`).
    #[arg(long, value_name = "DIR")]
    pub config_from: Option<PathBuf>,
}

#[derive(Args, Debug)]
pub struct RestoreArgs {
    /// Required: this replaces the live database (use restore-verify to test a backup safely).
    #[arg(long)]
    pub into_live: bool,
    #[arg(long)]
    pub backup: Option<String>,
    #[arg(long, value_name = "DIR")]
    pub config_from: Option<PathBuf>,
    /// Do not save the current database before replacing it.
    #[arg(long)]
    pub no_safety_backup: bool,
    /// Keep the database that was replaced, until you drop it yourself.
    #[arg(long)]
    pub keep_previous: bool,
}

#[derive(Args, Debug)]
pub struct RehearseArgs {
    #[arg(long)]
    pub backup: Option<String>,
    #[arg(long, value_name = "DIR")]
    pub config_from: Option<PathBuf>,
    /// The release to rehearse (default: the installed one).
    #[arg(long)]
    pub version: Option<String>,
    #[arg(long)]
    pub registry: Option<String>,
    #[arg(long, value_name = "FILE")]
    pub overlay: Option<PathBuf>,
}

#[derive(Args, Debug)]
pub struct UpgradeArgs {
    #[arg(long)]
    pub version: Option<String>,
    #[arg(long)]
    pub registry: Option<String>,
    /// The release's compose.release.generated.yml; the pulled images are checked against its digests.
    #[arg(long, value_name = "FILE")]
    pub overlay: Option<PathBuf>,
    /// No new images: exercise backup, rehearsal, restart and rollback with the installed version.
    #[arg(long)]
    pub local: bool,
    /// Trust the registry tag instead of digests (not recommended).
    #[arg(long)]
    pub allow_unpinned: bool,
    /// Upgrade without a backup. Needs --confirm "I HAVE NO BACKUP".
    #[arg(long)]
    pub skip_backup: bool,
    /// Upgrade without a rehearsal. Needs --confirm "SKIP REHEARSAL".
    #[arg(long)]
    pub skip_rehearsal: bool,
    /// Show what would happen; change nothing.
    #[arg(long)]
    pub plan: bool,
    /// Roll back without asking when the new version does not become healthy.
    #[arg(long)]
    pub auto_rollback: bool,
    #[arg(long, value_name = "DIR")]
    pub backup_to: Option<PathBuf>,
    #[arg(long, value_name = "DIR")]
    pub secrets_to: Option<PathBuf>,
}

#[derive(Args, Debug)]
pub struct RollbackArgs {
    /// The upgrade record (id or file in state/); default: the latest.
    #[arg(long)]
    pub record: Option<String>,
}

#[derive(Args, Debug)]
pub struct SupportArgs {
    /// Where to write the archive (default: support/ of the bundle directory).
    #[arg(long, value_name = "DIR")]
    pub out: Option<PathBuf>,
    #[arg(long, default_value_t = 400)]
    pub log_lines: usize,
    /// Keep email addresses (they are replaced by default).
    #[arg(long)]
    pub keep_emails: bool,
    /// List what would be included and write nothing.
    #[arg(long)]
    pub list: bool,
}

#[derive(Args, Debug)]
pub struct VerifyArgs {
    /// The address people use (default: ORIGIN of the bundle directory).
    #[arg(long)]
    pub origin: Option<String>,
    /// Connect to host[:port] instead of resolving the origin's name.
    #[arg(long, value_name = "HOST[:PORT]")]
    pub connect: Option<String>,
    #[arg(long)]
    pub http_port: Option<u16>,
    /// evaluation | provided | acme (default: from .env).
    #[arg(long)]
    pub tls_mode: Option<String>,
    /// Recipient for the mail check (default: the first ADMIN_EMAILS).
    #[arg(long, value_name = "ADDRESS")]
    pub mail_to: Option<String>,
}

/// What a command hands back: text for a person, a value for `--json`, an exit code.
struct Outcome {
    text: String,
    json: serde_json::Value,
    code: i32,
}

fn outcome<T: Serialize>(report: &T, text: String, code: i32) -> Result<Outcome> {
    Ok(Outcome { text, json: serde_json::to_value(report)?, code })
}

fn bool_code(ok: bool) -> i32 {
    if ok {
        exit::OK
    } else {
        exit::FAILED
    }
}

fn dir_from(cli: &Cli) -> PathBuf {
    let chosen = cli.dir.clone().or_else(|| std::env::var_os("DYNODOCTL_DIR").map(PathBuf::from)).unwrap_or_else(|| PathBuf::from("."));
    std::path::absolute(&chosen).unwrap_or(chosen)
}

fn read_roots(files: &[PathBuf]) -> Result<Vec<Vec<u8>>> {
    let mut roots = Vec::new();
    for f in files {
        let text = std::fs::read_to_string(f).map_err(|e| CliError::usage(format!("cannot read the CA file {}: {e}", f.display()), "pass a PEM file with --ca-file"))?;
        let found = crate::x509::pem_certificates(&text);
        if found.is_empty() {
            return Err(CliError::usage(format!("{} contains no certificate", f.display()), "--ca-file takes a PEM file with BEGIN CERTIFICATE blocks"));
        }
        roots.extend(found);
    }
    Ok(roots)
}

fn dispatch(cli: &Cli, ctx: &Ctx<'_>, net: &dyn Net, stdin_password: &dyn Fn() -> Option<String>) -> Result<Outcome> {
    match &cli.command {
        Command::Install(a) => {
            let generator = match a.generator.as_str() {
                "auto" => crate::generator::Choice::Auto,
                "node" => crate::generator::Choice::Node,
                "image" => crate::generator::Choice::Image,
                other => return Err(CliError::usage(format!("--generator `{other}` is not known"), "use auto, node or image")),
            };
            let smtp_password = match (&a.smtp_password_file, a.smtp_password_stdin) {
                (Some(_), true) => return Err(CliError::usage("--smtp-password-file and --smtp-password-stdin together", "choose one")),
                (Some(p), false) => Some(std::fs::read_to_string(p).map_err(|e| CliError::usage(format!("cannot read {}: {e}", p.display()), "check --smtp-password-file"))?),
                (None, true) => Some(stdin_password().ok_or_else(|| CliError::usage("--smtp-password-stdin got no input", "pipe the password in on one line"))?),
                (None, false) => None,
            };
            let opts = install::Options {
                from: a.from.clone(),
                origin: a.origin.clone(),
                tls: a.tls.clone(),
                admin_emails: a.admin_emails.clone(),
                org_name: a.org_name.clone(),
                brand_name: a.brand_name.clone(),
                support_url: a.support_url.clone(),
                smtp_host: a.smtp_host.clone(),
                smtp_from: a.smtp_from.clone(),
                smtp_user: a.smtp_user.clone(),
                smtp_port: a.smtp_port.clone(),
                smtp_security: a.smtp_security.clone(),
                smtp_ca_file: a.smtp_ca_file.clone(),
                smtp_password,
                with_converter: a.with_converter,
                cert: a.cert.clone(),
                key: a.key.clone(),
                acme_email: a.acme_email.clone(),
                acme_ca: a.acme_ca.clone(),
                acme_accept_tos: a.acme_accept_tos,
                bind: a.bind.clone(),
                http_port: a.http_port,
                https_port: a.https_port,
                registry: a.registry.clone(),
                version: a.version.clone(),
                overlay: a.overlay.clone(),
                generator,
                generator_script: a.generator_script.clone(),
                build: a.build,
                no_start: a.no_start,
                dry_run: a.dry_run,
                wait_timeout: a.wait_timeout,
            };
            let r = install::run(ctx, &opts)?;
            let code = if r.dry_run && !r.ok { exit::REFUSED } else { exit::OK };
            outcome(&r, r.render(), code)
        }
        Command::Configure { action } => match action {
            ConfigureAction::Show => {
                let shown = configure::show(ctx)?;
                outcome(&shown, configure::render_show(&shown), if shown.iter().any(|s| s.problem.is_some()) { exit::FAILED } else { exit::OK })
            }
            ConfigureAction::Set { assignments, apply, mail_to, send } => {
                let r = configure::set(ctx, &configure::SetOptions { assignments: assignments.clone(), apply: *apply, mail_to: mail_to.clone(), send_test_mail: *send })?;
                outcome(&r, r.render(), exit::OK)
            }
            ConfigureAction::Check { mail_to, send } => {
                let (problems, mail) = configure::check(ctx, mail_to.as_deref(), *send)?;
                let mail_ok = mail.as_ref().map(|m| m.ok).unwrap_or(true);
                let mut text = String::new();
                if problems.is_empty() {
                    text.push_str("The web server's own check accepts the settings.\n");
                } else {
                    text.push_str(&format!("The web server's own check rejects the settings:\n{}\n", problems.iter().map(|p| format!("  - {p}")).collect::<Vec<_>>().join("\n")));
                }
                if let Some(m) = &mail {
                    text.push_str(&format!("Mail check: {}{}\n", if m.ok { "passed" } else { "FAILED - " }, if m.ok { String::new() } else { m.message.clone() }));
                }
                let json = serde_json::json!({ "problems": problems, "mail": mail });
                Ok(Outcome { text, json, code: bool_code(problems.is_empty() && mail_ok) })
            }
            ConfigureAction::Apply => {
                configure::apply(ctx)?;
                Ok(Outcome { text: "Applied: the jobs ran and the services are healthy.\n".into(), json: serde_json::json!({ "applied": true }), code: exit::OK })
            }
        },
        Command::Status(a) => {
            let r = status::collect(ctx, net, &status::Options { max_backup_age_days: a.max_backup_age_days, volumes: !a.no_volumes, backup_dir: a.backup_dir.clone() })?;
            let code = r.exit_code();
            outcome(&r, r.render(), code)
        }
        Command::Backup(a) => {
            let r = backup::run(ctx, &backup::Options { to: a.to.clone(), secrets_to: a.secrets_to.clone(), label: a.label.clone(), keep: a.keep, verify: a.verify, no_config: a.no_config, ..backup::Options::default() })?;
            outcome(&r, r.render(), bool_code(r.ok))
        }
        Command::RestoreVerify(a) => {
            let r = restore::restore_verify(ctx, a.backup.as_deref(), a.config_from.as_deref())?;
            let _ = restore::save_report(ctx, &r, "restore-verify");
            outcome(&r, r.render(), bool_code(r.ok))
        }
        Command::Restore(a) => {
            if !a.into_live {
                return Err(CliError::usage("`restore` replaces the live database and needs --into-live", "to test a backup safely use `dynodoctl restore-verify`; to replace the live data add --into-live"));
            }
            let r = restore::restore_live(ctx, &restore::LiveOptions { backup: a.backup.clone(), config_from: a.config_from.clone(), no_safety_backup: a.no_safety_backup, keep_previous: a.keep_previous, confirmed: false })?;
            let _ = restore::save_report(ctx, &r, "restore-live");
            outcome(&r, r.render(), bool_code(r.ok))
        }
        Command::RehearseLite(a) => {
            let target = rehearse::Target { version: a.version.clone(), registry: a.registry.clone(), overlay: a.overlay.clone() };
            let r = rehearse::run(ctx, a.backup.as_deref(), a.config_from.as_deref(), &target)?;
            outcome(&r, r.render(), bool_code(r.ok))
        }
        Command::Upgrade(a) => {
            let r = upgrade::run(
                ctx,
                &upgrade::Options {
                    version: a.version.clone(),
                    registry: a.registry.clone(),
                    overlay: a.overlay.clone(),
                    local: a.local,
                    allow_unpinned: a.allow_unpinned,
                    skip_backup: a.skip_backup,
                    skip_rehearsal: a.skip_rehearsal,
                    plan_only: a.plan,
                    auto_rollback: a.auto_rollback,
                    backup_to: a.backup_to.clone(),
                    secrets_to: a.secrets_to.clone(),
                },
            )?;
            outcome(&r, r.render(), exit::OK)
        }
        Command::Rollback(a) => {
            let r = upgrade::rollback(ctx, a.record.as_deref())?;
            outcome(&r.restore, r.render(), bool_code(r.ok))
        }
        Command::SupportBundle(a) => {
            let r = support::run(ctx, net, &support::Options { out_dir: a.out.clone(), log_lines: a.log_lines, keep_emails: a.keep_emails, list_only: a.list })?;
            outcome(&r, r.render(), exit::OK)
        }
        Command::VerifyDelivery(a) => {
            let r = verify_delivery::run(ctx, net, &verify_delivery::Options { origin: a.origin.clone(), connect: a.connect.clone(), http_port: a.http_port, tls_mode: a.tls_mode.clone(), mail_to: a.mail_to.clone() })?;
            let code = r.exit_code();
            outcome(&r, r.render(), code)
        }
    }
}

/// Run one command line. `args` includes the program name. Returns the exit code.
pub fn run(args: Vec<String>, runner: &dyn Runner, make_net: &dyn Fn(Vec<Vec<u8>>) -> Box<dyn Net>, confirm_for_tests: Option<&dyn Confirm>, out: &mut dyn Write, err: &mut dyn Write) -> i32 {
    let cli = match Cli::try_parse_from(&args) {
        Ok(c) => c,
        Err(e) => {
            let code = if e.use_stderr() { exit::USAGE } else { exit::OK };
            let _ = if e.use_stderr() { write!(err, "{e}") } else { write!(out, "{e}") };
            return code;
        }
    };
    let interactive = Interactive { provided: cli.confirm.iter().cloned().chain(cli.yes.then(|| "yes".to_string())).collect() };
    let confirm: &dyn Confirm = confirm_for_tests.unwrap_or(&interactive);
    let mut ctx = Ctx::new(runner, Layout::new(dir_from(&cli)), confirm);
    ctx.project = cli.project.clone();
    ctx.extra_files = cli.files.clone();
    ctx.progress_to_stderr = cli.json;
    let result = read_roots(&cli.ca_file).and_then(|roots| {
        let net = make_net(roots);
        dispatch(&cli, &ctx, net.as_ref(), &|| {
            let mut line = String::new();
            std::io::stdin().lock().read_line(&mut line).ok().filter(|n| *n > 0).map(|_| line.trim_end_matches(['\r', '\n']).to_string())
        })
    });
    match result {
        Ok(o) => {
            if cli.json {
                let _ = writeln!(out, "{}", serde_json::to_string_pretty(&o.json).unwrap_or_default());
            } else {
                let _ = write!(out, "{}", o.text);
            }
            o.code
        }
        Err(e) => {
            if cli.json {
                let _ = writeln!(out, "{}", serde_json::json!({ "ok": false, "error": { "what": e.what, "todo": e.todo, "exit_code": e.code() } }));
            }
            let _ = writeln!(err, "ERROR: {e}");
            e.code()
        }
    }
}

#[cfg(test)]
#[path = "tests/cli.rs"]
mod tests;
