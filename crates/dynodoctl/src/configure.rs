//! `dynodoctl configure`: show, validate, change and apply a small, named set of settings in
//! `.env`. Every change keeps a dated copy of the previous file (mode 0600), is validated by the
//! web tier's own `check-config` (the `preflight` job) and, for mail, by `check-mail` before it
//! is applied; a change that fails validation is taken back. No setting here is a secret, and no
//! value is ever printed that the name says is one.

use crate::compose::{short, Compose};
use crate::ctx::Ctx;
use crate::envfile::{display_value, EnvFile};
use crate::error::{CliError, Result};
use crate::install::TLS_MODES;
use crate::origin::Origin;
use crate::util::{stamp, write_private};
use serde::Serialize;
use std::net::IpAddr;

pub struct Setting {
    pub name: &'static str,
    pub key: &'static str,
    pub help: &'static str,
    validate: fn(&str) -> std::result::Result<(), String>,
}

fn nonempty_line(v: &str) -> std::result::Result<(), String> {
    if v.is_empty() || v.len() > 2000 || v.contains('\n') || v.contains('\r') {
        Err("give a value of 1 to 2000 characters on one line".into())
    } else {
        Ok(())
    }
}
fn one_of(v: &str, allowed: &[&str]) -> std::result::Result<(), String> {
    if allowed.contains(&v) {
        Ok(())
    } else {
        Err(format!("use one of: {}", allowed.join(", ")))
    }
}
fn port(v: &str) -> std::result::Result<(), String> {
    v.parse::<u16>()
        .ok()
        .filter(|p| *p != 0)
        .map(|_| ())
        .ok_or_else(|| "use a port number from 1 to 65535".to_string())
}
fn email(v: &str) -> std::result::Result<(), String> {
    if v.contains('@') && !v.contains(' ') && !v.contains(',') && v.len() <= 254 {
        Ok(())
    } else {
        Err("use one address such as docs@example.org".into())
    }
}

pub const SETTINGS: [Setting; 19] = [
    Setting {
        name: "origin",
        key: "ORIGIN",
        help: "the address people type, https://host[:port]",
        validate: |v| Origin::parse(v).map(|_| ()).map_err(|e| e.what),
    },
    Setting {
        name: "tls-mode",
        key: "DYNODOC_TLS_MODE",
        help: "evaluation | provided | acme",
        validate: |v| one_of(v, &TLS_MODES),
    },
    Setting {
        name: "bind",
        key: "DYNODOC_BIND",
        help: "address the proxy listens on (127.0.0.1 = this machine only)",
        validate: |v| {
            v.parse::<IpAddr>()
                .map(|_| ())
                .map_err(|_| "use an IP address such as 127.0.0.1 or 0.0.0.0".to_string())
        },
    },
    Setting {
        name: "http-port",
        key: "DYNODOC_HTTP_PORT",
        help: "published HTTP port",
        validate: port,
    },
    Setting {
        name: "https-port",
        key: "DYNODOC_HTTPS_PORT",
        help: "published HTTPS port",
        validate: port,
    },
    Setting {
        name: "smtp-host",
        key: "SMTP_HOST",
        help: "mail server for sign-in links",
        validate: |v| {
            if nonempty_line(v).is_ok() && !v.contains(' ') {
                Ok(())
            } else {
                Err("use a host name or address without spaces".into())
            }
        },
    },
    Setting {
        name: "smtp-port",
        key: "SMTP_PORT",
        help: "mail server port",
        validate: port,
    },
    Setting {
        name: "smtp-security",
        key: "SMTP_SECURITY",
        help: "starttls | tls | none-local",
        validate: |v| one_of(v, &["starttls", "tls", "none-local"]),
    },
    Setting {
        name: "smtp-from",
        key: "SMTP_FROM",
        help: "sender address of sign-in mail",
        validate: email,
    },
    Setting {
        name: "smtp-user",
        key: "SMTP_USER",
        help: "mail server login name (the password stays in secrets/smtp_password)",
        validate: |v| {
            if nonempty_line(v).is_ok() && !v.contains(' ') {
                Ok(())
            } else {
                Err("use a login name without spaces".into())
            }
        },
    },
    Setting {
        name: "brand",
        key: "DYNODOC_BRAND_NAME",
        help: "product name shown to people",
        validate: |v| {
            nonempty_line(v).and_then(|_| {
                if v.len() <= 100 {
                    Ok(())
                } else {
                    Err("at most 100 characters".into())
                }
            })
        },
    },
    Setting {
        name: "org-name",
        key: "DYNODOC_ORG_NAME",
        help: "your organisation, shown on the sign-in page",
        validate: nonempty_line,
    },
    Setting {
        name: "telemetry",
        key: "DYNODOC_TELEMETRY",
        help: "on | off (page metrics; off by default when self-hosted)",
        validate: |v| one_of(v, &["on", "off"]),
    },
    Setting {
        name: "auth-email-link",
        key: "AUTH_EMAIL_LINK",
        help: "on | off (email sign-in)",
        validate: |v| one_of(v, &["on", "off"]),
    },
    Setting {
        name: "admin-emails",
        key: "ADMIN_EMAILS",
        help: "comma-separated addresses allowed to use /admin",
        validate: |v| {
            if v.split(',').all(|a| email(a.trim()).is_ok()) {
                Ok(())
            } else {
                Err("use addresses separated by commas".into())
            }
        },
    },
    Setting {
        name: "dev-auth",
        key: "DEV_AUTH",
        help: "true | false (test stacks only: shows the sign-in link instead of mailing it)",
        validate: |v| one_of(v, &["true", "false"]),
    },
    Setting {
        name: "acme-email",
        key: "DYNODOC_ACME_EMAIL",
        help: "contact address for the certificate authority",
        validate: email,
    },
    Setting {
        name: "acme-ca",
        key: "DYNODOC_ACME_CA",
        help: "letsencrypt-staging | letsencrypt | an ACME directory URL",
        validate: |v| {
            if v.contains(' ') || nonempty_line(v).is_err() {
                Err("use a short name or a URL without spaces".into())
            } else {
                Ok(())
            }
        },
    },
    Setting {
        name: "acme-accept-tos",
        key: "DYNODOC_ACME_ACCEPT_TOS",
        help: "true after you read the certificate authority's terms",
        validate: |v| one_of(v, &["true", "false"]),
    },
];

pub fn find(name: &str) -> Option<&'static Setting> {
    let wanted = name.to_ascii_lowercase().replace('_', "-");
    SETTINGS
        .iter()
        .find(|s| s.name == wanted || s.key.eq_ignore_ascii_case(name))
}

pub fn parse_assignments(items: &[String]) -> Result<Vec<(&'static Setting, String)>> {
    let mut out = Vec::new();
    for item in items {
        let (name, value) = item.split_once('=').ok_or_else(|| {
            CliError::usage(
                format!("`{item}` is not NAME=VALUE"),
                "write the setting and its new value, for example origin=https://docs.example.org",
            )
        })?;
        let setting = find(name.trim()).ok_or_else(|| {
            CliError::usage(
                format!("`{name}` is not a setting dynodoctl changes"),
                format!("the settings are: {}. Secrets are never set here; edit other settings in .env by hand", SETTINGS.iter().map(|s| s.name).collect::<Vec<_>>().join(", ")),
            )
        })?;
        let value = value.trim().to_string();
        (setting.validate)(&value).map_err(|why| {
            CliError::usage(
                format!(
                    "{} cannot be `{}`: {why}",
                    setting.name,
                    display_value(setting.key, &value)
                ),
                format!(
                    "{} ({})",
                    setting.help, "see docs/SELF-HOSTED-OPERATIONS.md"
                ),
            )
        })?;
        out.push((setting, value));
    }
    Ok(out)
}

/// What else must follow from a change, so the file stays consistent. Pure.
pub fn derive(env: &mut EnvFile, changes: &[(&'static Setting, String)]) -> Result<Vec<String>> {
    let mut notes = Vec::new();
    for (setting, value) in changes {
        match setting.key {
            "SMTP_HOST" => {
                if env.get("DYNODOC_MAIL_TRANSPORT") != Some("smtp") {
                    env.set("DYNODOC_MAIL_TRANSPORT", "smtp")?;
                    notes.push("DYNODOC_MAIL_TRANSPORT set to smtp".to_string());
                }
                if env.get("SMTP_FROM").is_none() && !changes.iter().any(|(s, _)| s.key == "SMTP_FROM") {
                    return Err(CliError::usage("smtp-host needs smtp-from", "set both in one command: configure set smtp-host=... smtp-from=..."));
                }
            }
            "ORIGIN" => {
                let o = Origin::parse(value)?;
                if o.explicit_port && !changes.iter().any(|(s, _)| s.key == "DYNODOC_HTTPS_PORT") {
                    env.set("DYNODOC_HTTPS_PORT", &o.port.to_string())?;
                    notes.push(format!("DYNODOC_HTTPS_PORT set to {} to match the origin", o.port));
                }
                notes.push("a new origin changes links, cookies, the certificate name and the add-in manifests: see the deployment guide, 9.9".to_string());
            }
            "DYNODOC_TLS_MODE" => {
                let profiles = env.get("COMPOSE_PROFILES").unwrap_or("").to_string();
                let mut list: Vec<&str> = profiles.split(',').filter(|p| !p.is_empty() && *p != "acme").collect();
                if value == "acme" {
                    list.push("acme");
                }
                if list.is_empty() {
                    env.unset("COMPOSE_PROFILES");
                } else {
                    env.set("COMPOSE_PROFILES", &list.join(","))?;
                }
                if value == "acme" {
                    for need in ["DYNODOC_ACME_EMAIL", "DYNODOC_ACME_CA"] {
                        if env.get(need).is_none() && !changes.iter().any(|(s, _)| s.key == need) {
                            return Err(CliError::usage(format!("tls-mode=acme needs {}", need), "set acme-email, acme-ca and acme-accept-tos=true in the same command"));
                        }
                    }
                    if env.get("DYNODOC_ACME_ACCEPT_TOS") != Some("true") && !changes.iter().any(|(s, v)| s.key == "DYNODOC_ACME_ACCEPT_TOS" && v == "true") {
                        return Err(CliError::usage("the certificate authority's terms are not accepted", "read them, then add acme-accept-tos=true"));
                    }
                }
                if value == "provided" {
                    notes.push("tls/fullchain.pem and tls/privkey.pem must be in place; the proxy checks them at start and refuses to run with a wrong pair (README, \"provided\")".to_string());
                }
            }
            "DEV_AUTH" if value == "true" => notes.push("DEV_AUTH=true lets anyone who can reach the sign-in page sign in as anyone: test stacks only".to_string()),
            _ => {}
        }
    }
    Ok(notes)
}

#[derive(Clone, Debug, Serialize)]
pub struct ShownSetting {
    pub name: String,
    pub key: String,
    pub value: Option<String>,
    pub problem: Option<String>,
}

pub fn show(ctx: &Ctx<'_>) -> Result<Vec<ShownSetting>> {
    ctx.require_installed()?;
    let env = ctx.env()?;
    Ok(SETTINGS
        .iter()
        .map(|s| {
            let value = env.get(s.key).map(|v| display_value(s.key, v));
            let problem = env.get(s.key).and_then(|v| (s.validate)(v).err());
            ShownSetting {
                name: s.name.into(),
                key: s.key.into(),
                value,
                problem,
            }
        })
        .collect())
}

pub fn render_show(settings: &[ShownSetting]) -> String {
    let mut out = String::new();
    for s in settings {
        let value = s.value.clone().unwrap_or_else(|| "(not set)".into());
        out.push_str(&format!(
            "  {:<16} {:<28} {}{}\n",
            s.name,
            s.key,
            value,
            s.problem
                .as_ref()
                .map(|p| format!("   <-- INVALID: {p}"))
                .unwrap_or_default()
        ));
    }
    out.push_str(
        "\nSecrets are not settings: they live in secrets/ and are never shown or changed here.\n",
    );
    out
}

/// Stage `.env` and the secrets and run the web tier's own `check-config` over them (the
/// `preflight` job). Returns the problems it lists; empty means valid.
pub fn validate_env(ctx: &Ctx<'_>) -> Result<Vec<String>> {
    let c = Compose::new(ctx);
    let stage = c.run(&["run", "--rm", "--no-deps", "-T", "secrets-init"])?;
    if !stage.ok() {
        return Err(CliError::failed(
            format!("secrets-init failed: {}", short(&stage.text())),
            "run `docker compose logs secrets-init`; it names the missing or wrong file",
        ));
    }
    let check = c.run(&["run", "--rm", "--no-deps", "-T", "preflight"])?;
    if check.ok() {
        return Ok(Vec::new());
    }
    let text = check.text();
    let problems: Vec<String> = text
        .lines()
        .filter(|l| l.trim_start().starts_with("- "))
        .map(|l| l.trim().trim_start_matches("- ").to_string())
        .collect();
    Ok(if problems.is_empty() {
        vec![short(&text)]
    } else {
        problems
    })
}

#[derive(Clone, Debug, Serialize)]
pub struct MailCheck {
    pub ran: bool,
    pub ok: bool,
    pub message: String,
}

/// `check-mail` from the web server's network position: the real handshake, no message text.
pub fn check_mail(ctx: &Ctx<'_>, to: &str, send: bool) -> Result<MailCheck> {
    let c = Compose::new(ctx);
    let mut args = vec![
        "--profile",
        "maintenance",
        "run",
        "--rm",
        "-T",
        "mailcheck",
        "--to",
        to,
    ];
    if send {
        args.push("--send");
    }
    let out = c.run(&args)?;
    Ok(MailCheck {
        ran: true,
        ok: out.ok(),
        message: short(&out.text()),
    })
}

#[derive(Clone, Debug, Serialize)]
pub struct ConfigureReport {
    pub changes: Vec<String>,
    pub notes: Vec<String>,
    pub previous_copy: Option<String>,
    pub applied: bool,
    pub mail: Option<MailCheck>,
    pub ok: bool,
}

impl ConfigureReport {
    pub fn render(&self) -> String {
        let mut o = String::new();
        for c in &self.changes {
            o.push_str(&format!("  changed  {c}\n"));
        }
        if let Some(p) = &self.previous_copy {
            o.push_str(&format!("  the previous .env is kept in {p} (mode 0600)\n"));
        }
        if let Some(m) = &self.mail {
            o.push_str(&format!(
                "  mail check: {}{}\n",
                if m.ok { "passed" } else { "FAILED - " },
                if m.ok {
                    String::new()
                } else {
                    m.message.clone()
                }
            ));
        }
        for n in &self.notes {
            o.push_str(&format!("  note     {n}\n"));
        }
        o.push_str(if self.applied {
            "  applied: the services were restarted with the new settings\n"
        } else {
            "  saved, not applied yet: run `dynodoctl configure apply` (or add --apply)\n"
        });
        o
    }
}

pub struct SetOptions {
    pub assignments: Vec<String>,
    pub apply: bool,
    pub mail_to: Option<String>,
    pub send_test_mail: bool,
}

fn restore_previous(ctx: &Ctx<'_>, previous: &str) -> Result<()> {
    write_private(&ctx.layout.env_file(), previous.as_bytes())?;
    // The staged copy must follow, or the next `up` would not matter but the next check would see the bad file.
    let _ = Compose::new(ctx).run(&["run", "--rm", "--no-deps", "-T", "secrets-init"]);
    Ok(())
}

pub fn set(ctx: &Ctx<'_>, opts: &SetOptions) -> Result<ConfigureReport> {
    ctx.require_installed()?;
    let changes = parse_assignments(&opts.assignments)?;
    if changes.is_empty() {
        return Err(CliError::usage("nothing to change", "give at least one NAME=VALUE, for example `configure set smtp-host=smtp.example.org smtp-from=docs@example.org`"));
    }
    let previous = std::fs::read_to_string(ctx.layout.env_file())?;
    let mut env = EnvFile::parse(&previous);
    let mut lines = Vec::new();
    for (setting, value) in &changes {
        let old = env.get(setting.key).map(|v| display_value(setting.key, v));
        if old.as_deref() == Some(value.as_str()) {
            continue;
        }
        lines.push(format!(
            "{}: {} -> {}",
            setting.key,
            old.unwrap_or_else(|| "(not set)".into()),
            display_value(setting.key, value)
        ));
        env.set(setting.key, value)?;
    }
    let notes = derive(&mut env, &changes)?;
    if lines.is_empty() {
        return Ok(ConfigureReport {
            changes: vec![],
            notes: vec!["every setting already has that value; nothing was changed".into()],
            previous_copy: None,
            applied: false,
            mail: None,
            ok: true,
        });
    }
    let smtp_touched = changes
        .iter()
        .any(|(s, _)| s.key.starts_with("SMTP_") || s.key == "AUTH_EMAIL_LINK");

    // The dated copy first, then the change.
    let copy = ctx
        .layout
        .state_dir()
        .join(format!("env-{}.bak", stamp((ctx.now)())));
    write_private(&copy, previous.as_bytes())?;
    write_private(&ctx.layout.env_file(), env.render().as_bytes())?;

    let problems = validate_env(ctx).inspect_err(|_| {
        let _ = restore_previous(ctx, &previous);
    })?;
    if !problems.is_empty() {
        restore_previous(ctx, &previous)?;
        return Err(CliError::refused(
            format!(
                "the web server's own check rejects the new settings:\n{}",
                problems
                    .iter()
                    .map(|p| format!("  - {p}"))
                    .collect::<Vec<_>>()
                    .join("\n")
            ),
            "nothing was changed (the previous .env is back); fix the values and try again",
        ));
    }
    let mut report = ConfigureReport {
        changes: lines,
        notes,
        previous_copy: Some(copy.display().to_string()),
        applied: false,
        mail: None,
        ok: true,
    };
    if let Some(to) = &opts.mail_to {
        let m = check_mail(ctx, to, opts.send_test_mail)?;
        if !m.ok {
            restore_previous(ctx, &previous)?;
            return Err(CliError::refused(format!("the mail check failed, so the settings were taken back: {}", m.message), "fix the mail settings (the message says what is wrong) or run again without --mail-to to save them anyway"));
        }
        report.mail = Some(m);
    } else if smtp_touched {
        report.notes.push("the mail settings changed but were not tested: run `dynodoctl configure check --mail-to you@example.org`".into());
    }
    if opts.apply {
        let a = apply(ctx);
        match a {
            Ok(()) => report.applied = true,
            Err(e) => {
                // Configuration rollback (guide 9.10, level 1): the previous file and a fresh start.
                restore_previous(ctx, &previous)?;
                let again = apply(ctx);
                return Err(CliError::failed(
                    format!("{}; the previous settings were put back{}", e.what, if again.is_ok() { " and the stack is running with them" } else { ", but the stack still does not start" }),
                    "see `dynodoctl status` and `docker compose logs`; the rejected file is not kept",
                ));
            }
        }
    }
    Ok(report)
}

/// Apply the saved settings: `up -d --wait` re-runs the jobs (secrets-init, preflight, db-roles,
/// init) and recreates the services whose configuration changed.
pub fn apply(ctx: &Ctx<'_>) -> Result<()> {
    ctx.require_installed()?;
    let out = Compose::new(ctx).run(&["up", "-d", "--wait", "--wait-timeout", "300"])?;
    if out.ok() {
        Ok(())
    } else {
        Err(CliError::failed(
            format!(
                "the stack did not come up with these settings: {}",
                short(&out.text())
            ),
            "run `docker compose logs <service>` for the failed job",
        ))
    }
}

pub fn check(
    ctx: &Ctx<'_>,
    mail_to: Option<&str>,
    send: bool,
) -> Result<(Vec<String>, Option<MailCheck>)> {
    ctx.require_installed()?;
    let problems = validate_env(ctx)?;
    let mail = match mail_to {
        Some(to) if problems.is_empty() => Some(check_mail(ctx, to, send)?),
        _ => None,
    };
    Ok((problems, mail))
}

#[cfg(test)]
#[path = "tests/configure.rs"]
mod tests;
