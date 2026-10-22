//! `dynodoctl admin-link`: an operator-issued sign-in link, for the first administrator and for
//! break-glass access (docs/decisions/018-operator-issued-sign-in-links.md).
//!
//! A thin wrapper: it runs `engine-cli auth issue-link` in a one-off `database-admin` container
//! (the one place that holds the database OWNER credential besides the migration job; the API never
//! does), and shows the link it gets back. Everything that decides anything is in `engine-cli`.
//!
//! The link is a credential, so:
//!
//! * It is shown ONCE, on this terminal, in the command's own output. It is never written to a
//!   file, a report, the lock or the journal, and there is no `--json` mode (a machine-readable
//!   output is how a credential ends up in a log). The command refuses unless a person is typing at
//!   a terminal.
//! * The person must type the address (the same address, exactly) before anything is made. A phrase
//!   passed in advance with `--confirm` does not count. Making an ACCOUNT the first-administrator
//!   rule does not cover needs a second typed phrase.
//! * It never appears in an argument (the container prints it; only the address, the lifetime and a
//!   reason, none of them secret, are arguments).

use crate::compose::{short, Compose};
use crate::ctx::Ctx;
use crate::db::{engine_cli_owner, LIVE_DB};
use crate::error::{CliError, Result};
use serde::Serialize;

#[derive(Clone, Debug, Default)]
pub struct Options {
    pub email: String,
    /// `90s`, `15m`, `2h`; engine-cli's own default (15 minutes) when absent.
    pub ttl: Option<String>,
    pub create: bool,
    /// `admin` or `user`: refuse unless the installation agrees (never changes a role).
    pub role: Option<String>,
    pub reason: Option<String>,
    /// The operating-system user, for the audit trail (a label).
    pub operator: String,
}

/// What the command did. The link is held apart, never serialised, and never shown by `Debug`.
#[derive(Serialize)]
pub struct Report {
    pub ok: bool,
    pub email: String,
    pub expires_at: String,
    pub ttl_seconds: i64,
    pub created_account: bool,
    pub administrator: bool,
    pub superseded: u64,
    /// The first digits of the link's hash: how `engine-cli auth revoke-link` names it.
    pub token_prefix: String,
    #[serde(skip)]
    link: String,
}

impl std::fmt::Debug for Report {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Report")
            .field("email", &self.email)
            .field("expires_at", &self.expires_at)
            .field("link", &"[not shown]")
            .finish()
    }
}

impl Report {
    /// The text for the terminal: the one place the link is written.
    pub fn render(&self) -> String {
        let minutes = self.ttl_seconds / 60;
        let mut s = format!("Sign-in link for {}\n\n  {}\n\n", self.email, self.link);
        s.push_str(&format!(
            "  Works once, in any browser, until {} ({} {}).\n",
            self.expires_at,
            if minutes >= 1 {
                minutes
            } else {
                self.ttl_seconds
            },
            if minutes >= 1 { "minutes" } else { "seconds" }
        ));
        if self.created_account {
            s.push_str("  The account was created.\n");
        }
        if self.administrator {
            s.push_str(
                "  This address is in ADMIN_EMAILS: the person will be an administrator (/admin).\n",
            );
        }
        if self.superseded > 0 {
            s.push_str(&format!(
                "  {} earlier link(s) for this account were ended.\n",
                self.superseded
            ));
        }
        s.push_str(&format!(
            "\n  Anyone who holds this link can sign in as this person. Hand it over privately.\n  It is shown only here and is not saved anywhere. To end it before it is used:\n    docker compose --profile maintenance run --rm database-admin auth revoke-link {}\n",
            self.token_prefix
        ));
        s
    }

    /// What a machine-readable record would hold: everything but the link.
    pub fn public_json(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or_default()
    }
}

/// The address as engine-cli will see it: trimmed and lower-cased, one `@`, no spaces or control
/// characters, at most 254 bytes. (engine-cli checks again; this keeps nonsense out of the prompt.)
pub fn clean_email(raw: &str) -> Result<String> {
    let email = raw.trim().to_lowercase();
    let ok = !email.is_empty()
        && email.len() <= 254
        && !email.chars().any(|c| {
            c.is_whitespace() || c.is_control() || matches!(c, ',' | ';' | '<' | '>' | '"' | '\\')
        })
        && email.matches('@').count() == 1
        && email.split_once('@').is_some_and(|(l, d)| {
            !l.is_empty() && d.contains('.') && !d.starts_with('.') && !d.ends_with('.')
        });
    if ok {
        Ok(email)
    } else {
        Err(CliError::usage(
            format!("`{}` is not an email address", printable(raw)),
            "write it as you@example.org",
        ))
    }
}

/// Text safe to put in a message: no control characters, at most 60 characters.
fn printable(text: &str) -> String {
    text.chars().filter(|c| !c.is_control()).take(60).collect()
}

fn clean_ttl(ttl: &str) -> Result<String> {
    let t = ttl.trim();
    let digits = t.strip_suffix(['s', 'm', 'h']).unwrap_or("");
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) || digits.len() > 9 {
        return Err(CliError::usage(
            format!("`{}` is not a lifetime", printable(ttl)),
            "write it as 90s, 15m or 2h (60 seconds to 24 hours)",
        ));
    }
    Ok(t.to_string())
}

fn clean_text(what: &str, text: &str, max: usize) -> Result<String> {
    if text.len() > max || text.chars().any(char::is_control) {
        return Err(CliError::usage(
            format!("the {what} must be at most {max} characters of plain text"),
            "shorten it and leave out line breaks",
        ));
    }
    Ok(text.to_string())
}

/// The first line of `text` that is a JSON object (engine-cli's answer; Compose may print other lines).
fn json_line(text: &str) -> Option<serde_json::Value> {
    text.lines()
        .rev()
        .map(str::trim)
        .filter(|l| l.starts_with('{'))
        .find_map(|l| serde_json::from_str(l).ok())
}

pub fn run(ctx: &Ctx<'_>, options: &Options) -> Result<Report> {
    ctx.require_installed()?;
    let email = clean_email(&options.email)?;
    let ttl = options.ttl.as_deref().map(clean_ttl).transpose()?;
    let reason = options
        .reason
        .as_deref()
        .map(|r| clean_text("reason", r, 200))
        .transpose()?;
    let operator: String = options
        .operator
        .chars()
        .filter(|c| !c.is_control())
        .take(48)
        .collect();
    let operator = if operator.trim().is_empty() {
        "unknown".to_string()
    } else {
        operator
    };
    if let Some(role) = options.role.as_deref() {
        if !matches!(role, "admin" | "user") {
            return Err(CliError::usage(
                format!("--role `{}` is not known", printable(role)),
                "use admin or user",
            ));
        }
    }
    let env = ctx.env()?;
    let origin = env
        .get("ORIGIN")
        .filter(|o| !o.trim().is_empty())
        .ok_or_else(|| {
            CliError::refused(
                ".env has no ORIGIN, so the link cannot be written",
                "run `dynodoctl configure show`, then `dynodoctl install` again if the settings are missing",
            )
        })?
        .to_string();
    let admin_emails = env.get("ADMIN_EMAILS").unwrap_or("").to_string();
    let c = Compose::new(ctx);
    if !c.ps()?.iter().any(|s| s.service == "postgres" && s.up()) {
        return Err(CliError::refused(
            "PostgreSQL is not running, so no link can be made",
            "start the installation (`docker compose up -d --wait`) and run the command again",
        ));
    }
    let is_admin = admin_emails
        .split(',')
        .any(|a| !a.trim().is_empty() && a.trim().eq_ignore_ascii_case(&email));

    // The first confirmation: the address, typed by a person at a terminal. Nothing is made before it.
    let prompt = format!(
        "A one-time sign-in link will be made for {email}.\n\
         Anyone who holds the link can sign in as that person until it is used or expires.\n\
         It is shown once, on this screen, and is not saved anywhere.\n\
         {}",
        if is_admin {
            "This address is in ADMIN_EMAILS: the person will be an administrator."
        } else {
            "This address is not in ADMIN_EMAILS: the person will not be an administrator."
        }
    );
    if !ctx.confirm.confirm_interactive(&prompt, &email) {
        return Err(CliError::refused(
            "the address was not confirmed, so no link was made",
            "run `dynodoctl admin-link` in a terminal and type the address exactly when asked (--confirm is not accepted here: the link is shown on the screen)",
        ));
    }

    let mut confirmed_creation = false;
    loop {
        let mut args: Vec<String> = vec![
            "auth".into(),
            "issue-link".into(),
            "--json".into(),
            "--email".into(),
            email.clone(),
            "--origin".into(),
            origin.clone(),
            "--admin-emails".into(),
            admin_emails.clone(),
            "--operator".into(),
            operator.clone(),
        ];
        if let Some(t) = &ttl {
            args.extend(["--ttl".into(), t.clone()]);
        }
        if let Some(r) = &reason {
            args.extend(["--reason".into(), r.clone()]);
        }
        if let Some(role) = &options.role {
            args.extend(["--role".into(), role.clone()]);
        }
        if options.create {
            args.push("--create".into());
            if confirmed_creation {
                args.extend(["--confirm-create".into(), email.clone()]);
            }
        }
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        let out = engine_cli_owner(&c, LIVE_DB, &refs)?;
        let answer = json_line(&out.stdout_text());
        match out.status {
            0 => return parse(&email, &origin, answer),
            3 if options.create && !confirmed_creation => {
                let why = answer
                    .as_ref()
                    .and_then(|a| a["needs_confirmation"].as_str())
                    .unwrap_or("");
                let reason_text = match why {
                    "administrator_exists" => {
                        "an administrator already has an account on this installation"
                    }
                    "not_a_configured_administrator" => "this address is not in ADMIN_EMAILS",
                    _ => "the first-administrator rule does not cover it",
                };
                let phrase = format!("CREATE ACCOUNT {email}");
                let prompt = format!(
                    "There is no account for {email}, and {reason_text}.\n\
                     Creating the account needs a second confirmation. Nothing has been made yet."
                );
                if !ctx.confirm.confirm_interactive(&prompt, &phrase) {
                    return Err(CliError::refused(
                        "the account was not confirmed, so nothing was made",
                        format!("run the command again and type `{phrase}` when asked"),
                    ));
                }
                confirmed_creation = true;
            }
            3 => {
                return Err(CliError::refused(
                    "the account cannot be made without a second confirmation",
                    "add --create and type the confirmation when asked",
                ))
            }
            _ => {
                // Standard error only: on a failure standard output holds no link.
                return Err(CliError::failed(
                    format!("the link could not be made: {}", short(&out.stderr_text())),
                    "check `dynodoctl status`; the account must exist (or add --create), and the database schema must be this release's (`dynodoctl upgrade`)",
                ));
            }
        }
    }
}

/// engine-cli's JSON answer, checked: the link must be this installation's own sign-in page.
fn parse(email: &str, origin: &str, answer: Option<serde_json::Value>) -> Result<Report> {
    let bad = || {
        CliError::failed(
            "engine-cli gave an answer dynodoctl does not understand",
            "no link is shown; check that the image and dynodoctl are the same release (`dynodoctl status`)",
        )
    };
    let a = answer.ok_or_else(bad)?;
    let link = a["link"].as_str().ok_or_else(bad)?;
    let token = link
        .strip_prefix(&format!("{}/login?token=", origin.trim_end_matches('/')))
        .ok_or_else(bad)?;
    if a["ok"] != true || token.len() != 64 || !token.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(bad());
    }
    Ok(Report {
        ok: true,
        email: email.to_string(),
        expires_at: a["expires_at"].as_str().unwrap_or_default().to_string(),
        ttl_seconds: a["ttl_seconds"].as_i64().unwrap_or(0),
        created_account: a["created_account"].as_bool().unwrap_or(false),
        administrator: a["administrator"].as_bool().unwrap_or(false),
        superseded: a["superseded"].as_u64().unwrap_or(0),
        token_prefix: a["token_prefix"].as_str().unwrap_or_default().to_string(),
        link: link.to_string(),
    })
}

#[cfg(test)]
#[path = "tests/admin_link.rs"]
mod tests;
