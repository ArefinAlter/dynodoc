//! `engine-cli auth ...`: operator-issued sign-in links (docs/decisions/018-operator-issued-sign-in-links.md).
//!
//! These commands connect with the database OWNER credential (`MIGRATION_DATABASE_URL`, like
//! `migrate` and `grant-runtime`), which the API never holds: that is what makes issuing a link an
//! operator act and keeps it off the network. In the self-hosted bundle they run through
//! `dynodoctl admin-link`, in a one-off container.
//!
//! The link is a credential. `issue-link` prints it exactly once, on standard output and nowhere
//! else (no log line, no file, no argument), and its `Debug` shows nothing of it. In text mode
//! standard output is the link alone and the explanation goes to standard error, so
//! `engine-cli auth issue-link ... | clip` copies just the link.

use anyhow::{bail, Context as _};
use clap::{Args, Subcommand, ValueEnum};
use engine_api::auth::operator::{
    self, Expect, IssueError, IssueRequest, NeedsConfirmation, RevokeError,
};
use serde_json::json;

/// Exit status of an `issue-link` that needs the second confirmation (nothing was changed).
pub const EXIT_NEEDS_CONFIRMATION: i32 = 3;

#[derive(Subcommand)]
pub enum Action {
    /// Make a one-time sign-in link for an address, without sending any email. The link works once,
    /// in any browser, until it expires. Anyone who holds it can sign in as that person.
    IssueLink(IssueLink),
    /// List the outstanding operator links (a prefix of each hash, never a token).
    ListLinks {
        /// Also the used, revoked and expired ones (newest 200).
        #[arg(long)]
        all: bool,
    },
    /// End an outstanding link, named by the prefix `list-links` shows.
    RevokeLink {
        /// At least 8 hexadecimal digits.
        prefix: String,
        /// Who is doing this (default: the operating-system user).
        #[arg(long)]
        operator: Option<String>,
    },
}

#[derive(Clone, Copy, ValueEnum)]
pub enum RoleArg {
    /// The address must be in ADMIN_EMAILS.
    Admin,
    /// The address must not be.
    User,
}

#[derive(Args)]
pub struct IssueLink {
    /// The person's email address.
    #[arg(long)]
    email: String,
    /// How long the link works: 60s to 24h, written 90s, 15m or 2h.
    #[arg(long, default_value = "15m")]
    ttl: String,
    /// Make the account if there is none. Without a second confirmation this works only for the
    /// first administrator: an address in ADMIN_EMAILS while no administrator has an account.
    #[arg(long)]
    create: bool,
    /// With --create: the address typed again, to make an account the rule above does not allow
    /// by itself (another administrator exists, or the address is not an administrator).
    #[arg(long, value_name = "EMAIL")]
    confirm_create: Option<String>,
    /// Refuse unless the installation agrees about the person's role. Never changes a role:
    /// administrators are the addresses in ADMIN_EMAILS.
    #[arg(long, value_enum)]
    role: Option<RoleArg>,
    /// Why (kept in the audit trail for good: no personal data).
    #[arg(long, default_value = "")]
    reason: String,
    /// Who is doing this (default: the operating-system user). A label, not a login.
    #[arg(long)]
    operator: Option<String>,
    /// What people type in the browser, such as https://docs.example.org (default: $ORIGIN).
    #[arg(long)]
    origin: Option<String>,
    /// The addresses in ADMIN_EMAILS, comma separated (default: $ADMIN_EMAILS).
    #[arg(long)]
    admin_emails: Option<String>,
    /// One JSON object on standard output (it contains the link).
    #[arg(long)]
    json: bool,
}

/// `90s`, `15m` or `2h` in seconds.
pub fn parse_ttl(text: &str) -> anyhow::Result<i64> {
    let text = text.trim();
    let (digits, factor) = if let Some(d) = text.strip_suffix('s') {
        (d, 1)
    } else if let Some(d) = text.strip_suffix('m') {
        (d, 60)
    } else if let Some(d) = text.strip_suffix('h') {
        (d, 3600)
    } else {
        bail!("write the lifetime as 90s, 15m or 2h")
    };
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        bail!("write the lifetime as 90s, 15m or 2h");
    }
    let number: i64 = digits
        .parse()
        .context("write the lifetime as 90s, 15m or 2h")?;
    Ok(number.checked_mul(factor).unwrap_or(i64::MAX))
}

/// The operating-system user name, for the audit trail. A label: anyone can set the variable, and
/// the person with the owner credential can write any label anyway.
fn os_user() -> String {
    ["USER", "USERNAME", "LOGNAME"]
        .iter()
        .find_map(|name| std::env::var(name).ok().filter(|v| !v.trim().is_empty()))
        .map(|v| {
            v.trim()
                .chars()
                .take(operator::MAX_OPERATOR_BYTES / 4)
                .collect()
        })
        .unwrap_or_else(|| "unknown".to_string())
}

pub async fn run(action: Action) -> anyhow::Result<()> {
    match action {
        Action::IssueLink(args) => issue(args).await,
        Action::ListLinks { all } => {
            let pool = connect().await?;
            let rows = operator::list(&pool, all).await?;
            if rows.is_empty() {
                println!(
                    "No {}operator sign-in links.",
                    if all { "" } else { "outstanding " }
                );
            }
            for row in rows {
                println!(
                    "{}  {:<8}  {}  until {}  by {}{}",
                    row.token_prefix,
                    row.state,
                    row.email,
                    row.expires_at.to_rfc3339(),
                    row.operator,
                    if row.reason.is_empty() {
                        String::new()
                    } else {
                        format!("  ({})", row.reason)
                    }
                );
            }
            Ok(())
        }
        Action::RevokeLink {
            prefix,
            operator: who,
        } => {
            let who = who.unwrap_or_else(os_user);
            let pool = connect().await?;
            match operator::revoke(&pool, &prefix, &who).await {
                Ok(_) => {
                    println!("Ended the link {}.", prefix.trim().to_ascii_lowercase());
                    Ok(())
                }
                Err(RevokeError::Db(error)) => Err(error.into()),
                Err(other) => bail!("{other}"),
            }
        }
    }
}

/// The owner connection, after the schema has been checked (a clear message instead of a missing table).
async fn connect() -> anyhow::Result<sqlx::PgPool> {
    let pool = crate::connect_migration().await?;
    engine_core::database::check_schema(&pool).await?;
    Ok(pool)
}

async fn issue(args: IssueLink) -> anyhow::Result<()> {
    // Everything that can be checked without the database is checked first.
    let ttl = parse_ttl(&args.ttl)?;
    operator::check_ttl(ttl).map_err(|e| anyhow::anyhow!("{e}"))?;
    let origin = args
        .origin
        .clone()
        .or_else(|| std::env::var("ORIGIN").ok())
        .filter(|o| !o.trim().is_empty())
        .context("the web address is needed to write the link: pass --origin https://docs.example.org (or set ORIGIN)")?;
    let origin = operator::check_origin(&origin).map_err(|e| anyhow::anyhow!("{e}"))?;
    let admin_emails = args
        .admin_emails
        .clone()
        .or_else(|| std::env::var("ADMIN_EMAILS").ok())
        .unwrap_or_default();
    let who = args.operator.clone().unwrap_or_else(os_user);
    let pool = connect().await?;
    let request = IssueRequest {
        email: &args.email,
        ttl_seconds: ttl,
        create: args.create,
        confirm_create: args.confirm_create.as_deref(),
        expect: args.role.map(|r| match r {
            RoleArg::Admin => Expect::Admin,
            RoleArg::User => Expect::User,
        }),
        operator: &who,
        reason: &args.reason,
        admin_emails: &admin_emails,
    };
    let issued = match operator::issue(&pool, &request).await {
        Ok(issued) => issued,
        Err(IssueError::NeedsConfirmation(why)) => {
            let (code, text) = match why {
                NeedsConfirmation::AdministratorExists => (
                    "administrator_exists",
                    "an administrator already has an account on this installation",
                ),
                NeedsConfirmation::NotAConfiguredAdministrator => (
                    "not_a_configured_administrator",
                    "this address is not in ADMIN_EMAILS",
                ),
            };
            if args.json {
                println!("{}", json!({"ok": false, "needs_confirmation": code}));
            }
            eprintln!(
                "Nothing was changed: no account exists for this address, and {text}, so making the account needs a second confirmation: add --confirm-create {}",
                args.email.trim()
            );
            std::process::exit(EXIT_NEEDS_CONFIRMATION);
        }
        Err(IssueError::Db(error)) => return Err(error.into()),
        Err(other) => bail!("{other}"),
    };
    let link = operator::sign_in_url(&origin, &issued.token).map_err(|e| anyhow::anyhow!("{e}"))?;
    let minutes = ttl / 60;
    if args.json {
        println!(
            "{}",
            json!({
                "ok": true,
                "link": link,
                "link_id": issued.link_id,
                "token_prefix": issued.token_prefix,
                "email": issued.email,
                "expires_at": issued.expires_at.to_rfc3339(),
                "ttl_seconds": ttl,
                "created_account": issued.created_account,
                "administrator": issued.administrator,
                "superseded": issued.superseded,
            })
        );
        return Ok(());
    }
    eprintln!("Sign-in link for {}", issued.email);
    eprintln!(
        "  Works once, in any browser, until {} ({} {}).",
        issued.expires_at.to_rfc3339(),
        if minutes >= 1 { minutes } else { ttl },
        if minutes >= 1 { "minutes" } else { "seconds" }
    );
    if issued.created_account {
        eprintln!("  The account was created.");
    }
    if issued.administrator {
        eprintln!("  This address is in ADMIN_EMAILS: the person will be an administrator.");
    }
    if issued.superseded > 0 {
        eprintln!(
            "  {} earlier link(s) for this account were ended.",
            issued.superseded
        );
    }
    eprintln!(
        "  Anyone who holds the link can sign in as this person. Pass it on privately; if it goes astray: engine-cli auth revoke-link {}",
        issued.token_prefix
    );
    println!("{link}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lifetimes_are_written_with_a_unit() {
        assert_eq!(parse_ttl("90s").unwrap(), 90);
        assert_eq!(parse_ttl("15m").unwrap(), 900);
        assert_eq!(parse_ttl("2h").unwrap(), 7200);
        assert_eq!(parse_ttl(" 24h ").unwrap(), 86_400);
        for bad in [
            "", "15", "m", "-5m", "1.5h", "5 m", "5d", "0x10m", "+5m", "٣m",
        ] {
            assert!(parse_ttl(bad).is_err(), "{bad:?}");
        }
        // Too long is parsed but refused by the range check.
        assert!(operator::check_ttl(parse_ttl("25h").unwrap()).is_err());
        assert!(operator::check_ttl(parse_ttl("59s").unwrap()).is_err());
        assert!(operator::check_ttl(parse_ttl("0s").unwrap()).is_err());
        assert!(operator::check_ttl(parse_ttl("99999999999999999h").unwrap()).is_err());
    }
}
