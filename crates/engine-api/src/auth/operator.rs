//! Operator-issued sign-in links (docs/decisions/018-operator-issued-sign-in-links.md).
//!
//! A self-hosted installation with no working mail (or with email sign-in turned off) still
//! needs a first administrator and a break-glass way in. The OPERATOR, the person with shell access
//! to the host and the database owner credential, mints a one-time link with
//! `engine-cli auth issue-link` and hands it over out of band.
//!
//! * **Issuing is not reachable over the network.** [`issue`] is called only by the CLI, with the
//!   owner credential. No HTTP route calls it, and the API's restricted runtime login has no
//!   INSERT on `operator_sign_in_link` (`engine_core::database`, `CONSUME_PRUNE`), so a bug or a
//!   compromise in the API cannot mint a link either.
//! * **Using one is the ordinary path.** `POST /auth/verify` tries the emailed (magic) links first;
//!   a token that is not one of those is looked up here ([`redeem`]). The link is consumed, the
//!   login made (the same function an emailed link uses) and the audit row written in
//!   ONE transaction, so two simultaneous uses have exactly one winner and a failed login leaves
//!   the link live. Expiry, the account being active and its session generation are checked by the
//!   same statement that consumes the row (and expiry again by a trigger).
//! * **The email-link switch is not involved.** `AUTH_EMAIL_LINK=off` stops the public request for
//!   a NEW emailed link (a web-tier route); it has never stopped confirming a link already issued,
//!   and an operator link is a different kind of credential on purpose: that is what makes the flag
//!   usable on an installation that has no mail. Nothing here is rate limited per client address:
//!   such a limit would let the public lock the operator out, and a 256-bit token needs none.
//! * **Roles are not touched.** Administrators are the addresses in `ADMIN_EMAILS` (see
//!   [`crate::admin::is_admin`]); a link signs the person in as who they already are. `--role`
//!   only asserts what the operator expects and refuses on a mismatch.
//!
//! Only the SHA-256 of a token is stored. The token exists in the issuing process, is printed once
//! by the CLI, and is wiped when dropped ([`Token`]); its `Debug` prints nothing of it. Audit rows
//! (`auth.operator_link.issued`, `.used`, `.refused`, `.revoked`, `.expired`) hold the operator's
//! label, a reason, ids and counts, never a token or an address.

use std::fmt;

use chrono::{DateTime, Utc};
use serde_json::{json, Value};
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;
use zeroize::Zeroizing;

use super::routes::{mint_session_with, TokenResponse};
use super::{random_token, sha256, AuthError};
use crate::agent::identity::is_reserved_email;
use crate::AppState;

/// A link made without `--ttl` lives this long (the same as an emailed one).
pub const DEFAULT_TTL_SECONDS: i64 = 15 * 60;
/// The shortest life an operator can ask for.
pub const MIN_TTL_SECONDS: i64 = 60;
/// The longest: the table refuses more.
pub const MAX_TTL_SECONDS: i64 = 24 * 60 * 60;
/// Longest email address (RFC 5321).
pub const MAX_EMAIL_BYTES: usize = 254;
/// Longest operator label and reason (the table checks the same).
pub const MAX_OPERATOR_BYTES: usize = 64;
pub const MAX_REASON_BYTES: usize = 200;
/// A prefix of a token's hash must be at least this many hex digits to name a link.
pub const MIN_PREFIX_DIGITS: usize = 8;
/// How many hex digits of the hash `list-links` shows.
const SHOWN_PREFIX_DIGITS: usize = 8;

/// A sign-in token: 64 lower-case hexadecimal digits. Wiped when dropped; `Debug` shows nothing.
pub struct Token(Zeroizing<String>);

impl Token {
    /// The secret text. Call only to print it once or to build the link.
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Token([redacted])")
    }
}

/// What the operator expects the person's role to be. It never changes a role.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Expect {
    /// The address must be in `ADMIN_EMAILS`.
    Admin,
    /// The address must not be.
    User,
}

/// Why an account that does not exist may not be created by this command alone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NeedsConfirmation {
    /// An administrator already has an account on this installation.
    AdministratorExists,
    /// No administrator has an account yet, but this address is not a configured administrator.
    NotAConfiguredAdministrator,
}

#[derive(Debug, thiserror::Error)]
pub enum IssueError {
    /// The input is not acceptable; the text says what to fix and holds no secret.
    #[error("{0}")]
    Invalid(String),
    /// The account exists and is disabled (removed). Restore it in the administration screen first.
    #[error("the account for this address is disabled; restore it in the administration screen before issuing a link")]
    Disabled,
    /// No account for the address, and `create` was not asked for.
    #[error("there is no account for this address; pass --create to make one")]
    NoAccount,
    /// The account would be created, which needs the address typed again (`--confirm-create`).
    #[error("creating this account needs a second confirmation")]
    NeedsConfirmation(NeedsConfirmation),
    /// `--role` disagreed with `ADMIN_EMAILS`.
    #[error("{0}")]
    RoleMismatch(String),
    #[error("database error: {0}")]
    Db(#[from] sqlx::Error),
}

#[derive(Clone, Debug)]
pub struct IssueRequest<'a> {
    pub email: &'a str,
    pub ttl_seconds: i64,
    /// Make the account when there is none.
    pub create: bool,
    /// The address again, accepted as the second confirmation of an account creation that the
    /// first-administrator rule does not allow on its own.
    pub confirm_create: Option<&'a str>,
    pub expect: Option<Expect>,
    /// The operator's operating-system user name as the caller saw it (a label).
    pub operator: &'a str,
    pub reason: &'a str,
    /// `ADMIN_EMAILS` as the API runs with it (comma separated).
    pub admin_emails: &'a str,
}

#[derive(Debug)]
pub struct Issued {
    pub token: Token,
    pub link_id: Uuid,
    /// The first hex digits of the token's hash: how `list-links` and `revoke-link` name it.
    pub token_prefix: String,
    pub identity_id: Uuid,
    /// The normalised address.
    pub email: String,
    pub created_account: bool,
    /// The address is in `ADMIN_EMAILS`: the person will be an administrator.
    pub administrator: bool,
    /// Earlier live links of the same account that this one ended.
    pub superseded: u64,
    pub expires_at: DateTime<Utc>,
}

/// Trim, lower-case and check an address. The same shape the web form accepts, plus no control
/// characters and none of the characters that would confuse an address list or a terminal.
pub fn normalise_email(raw: &str) -> Result<String, IssueError> {
    let email = raw.trim().to_lowercase();
    let shaped = !email.is_empty()
        && email.len() <= MAX_EMAIL_BYTES
        && email.chars().all(|c| {
            !c.is_whitespace()
                && !c.is_control()
                && !matches!(c, ',' | ';' | '<' | '>' | '"' | '\\')
        })
        && email.matches('@').count() == 1
        && email.split_once('@').is_some_and(|(local, domain)| {
            !local.is_empty()
                && domain.contains('.')
                && !domain.starts_with('.')
                && !domain.ends_with('.')
        });
    if shaped {
        Ok(email)
    } else {
        Err(IssueError::Invalid(
            "the email address is not valid (one @, a domain with a dot, no spaces, at most 254 characters)".into(),
        ))
    }
}

/// A label or a reason: bounded, no control characters (they would reach the terminal and the audit).
fn free_text(what: &str, text: &str, max: usize, allow_empty: bool) -> Result<(), IssueError> {
    if (text.is_empty() && !allow_empty) || text.len() > max || text.chars().any(char::is_control) {
        return Err(IssueError::Invalid(format!(
            "{what} must be {} to {max} bytes of text without control characters",
            if allow_empty { 0 } else { 1 }
        )));
    }
    Ok(())
}

/// `ttl` in seconds, within the allowed range.
pub fn check_ttl(seconds: i64) -> Result<i64, IssueError> {
    if (MIN_TTL_SECONDS..=MAX_TTL_SECONDS).contains(&seconds) {
        Ok(seconds)
    } else {
        Err(IssueError::Invalid(format!(
            "the lifetime must be from {MIN_TTL_SECONDS} seconds to {} hours",
            MAX_TTL_SECONDS / 3600
        )))
    }
}

/// `https://host[:port]` (or `http://` for localhost only), with no user, path, query or fragment,
/// as the web server's `ORIGIN` is written. Returns it without a trailing slash.
pub fn check_origin(origin: &str) -> Result<String, String> {
    let bad = |why: &str| format!("the origin `{origin}` is not usable: {why}");
    let trimmed = origin.trim().trim_end_matches('/');
    let (scheme, rest) = trimmed
        .split_once("://")
        .ok_or_else(|| bad("it needs https:// at the front"))?;
    if rest.is_empty()
        || rest
            .chars()
            .any(|c| !(c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | ':' | '[' | ']')))
    {
        return Err(bad("write only a host name and an optional port"));
    }
    let host = if let Some(inner) = rest.strip_prefix('[') {
        inner.split_once(']').map_or("", |(h, _)| h)
    } else {
        rest.rsplit_once(':').map_or(rest, |(h, _)| h)
    };
    let loopback = matches!(host, "localhost" | "127.0.0.1" | "::1");
    match scheme {
        "https" => {}
        "http" if loopback => {}
        _ => return Err(bad("the scheme must be https (http only for localhost)")),
    }
    if host.is_empty() {
        return Err(bad("the host name is empty"));
    }
    Ok(trimmed.to_string())
}

/// The link a person opens: the sign-in page with the token, which asks for one click before it
/// uses the token (so a mail scanner or a link preview cannot use it up).
pub fn sign_in_url(origin: &str, token: &Token) -> Result<String, String> {
    Ok(format!(
        "{}/login?token={}",
        check_origin(origin)?,
        token.expose()
    ))
}

fn shaped_token(token: &str) -> bool {
    token.len() == 64
        && token
            .bytes()
            .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// One row of `operation_audit`. The detail never holds a secret.
async fn audit(
    connection: &mut PgConnection,
    actor: Uuid,
    action: &str,
    resource: Uuid,
    detail: Value,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "insert into operation_audit(actor_id,action,resource_id,detail) values($1,$2,$3,$4)",
    )
    .bind(actor)
    .bind(action)
    .bind(resource)
    .bind(detail)
    .execute(connection)
    .await?;
    Ok(())
}

/// Does any configured administrator have an active account? (`ADMIN_EMAILS` is the only thing
/// that makes an administrator, so "no administrator yet" means none of those addresses has one.)
async fn administrator_has_account(
    connection: &mut PgConnection,
    admin_emails: &str,
) -> Result<bool, sqlx::Error> {
    for address in admin_emails
        .split(',')
        .map(str::trim)
        .filter(|a| !a.is_empty())
    {
        let exists: bool = sqlx::query_scalar(
            "select exists(select 1 from identity where lower(email)=lower($1) and disabled_at is null)",
        )
        .bind(address)
        .fetch_one(&mut *connection)
        .await?;
        if exists {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Make a one-time sign-in link. Operator credential only (the caller connects as the database
/// owner); the runtime login cannot do this. Everything happens in one transaction, serialised per
/// address, so a refusal changes nothing.
pub async fn issue(pool: &PgPool, request: &IssueRequest<'_>) -> Result<Issued, IssueError> {
    let email = normalise_email(request.email)?;
    if is_reserved_email(&email) {
        return Err(IssueError::Invalid(
            "that address belongs to the reserved domain of the AI-agent identities".into(),
        ));
    }
    let ttl = check_ttl(request.ttl_seconds)?;
    free_text(
        "the operator label",
        request.operator,
        MAX_OPERATOR_BYTES,
        false,
    )?;
    free_text("the reason", request.reason, MAX_REASON_BYTES, true)?;
    let administrator = crate::admin::is_admin(&email, request.admin_emails);
    match request.expect {
        Some(Expect::Admin) if !administrator => {
            return Err(IssueError::RoleMismatch(
                "--role admin, but this address is not in ADMIN_EMAILS; an operator link never makes anyone an administrator (add the address to ADMIN_EMAILS first)".into(),
            ))
        }
        Some(Expect::User) if administrator => {
            return Err(IssueError::RoleMismatch(
                "--role user, but this address is in ADMIN_EMAILS: the link would sign in an administrator".into(),
            ))
        }
        _ => {}
    }
    let confirmed = match request.confirm_create {
        Some(typed) => normalise_email(typed)? == email,
        None => false,
    };

    let mut tx = pool.begin().await?;
    sqlx::query("select pg_advisory_xact_lock(hashtextextended($1, 47))")
        .bind(format!("dynodoc/operator-link/{email}"))
        .execute(&mut *tx)
        .await?;
    let found: Option<(Uuid, bool, i64)> = sqlx::query_as(
        "select id, disabled_at is not null, session_generation from identity where lower(email)=lower($1) for update",
    )
    .bind(&email)
    .fetch_optional(&mut *tx)
    .await?;
    let (identity_id, generation, created_account) = match found {
        Some((_, true, _)) => return Err(IssueError::Disabled),
        Some((id, false, generation)) => (id, generation, false),
        None => {
            if !request.create {
                return Err(IssueError::NoAccount);
            }
            // The first administrator needs no second confirmation; any other new account does.
            let administrator_exists =
                administrator_has_account(&mut tx, request.admin_emails).await?;
            if (administrator_exists || !administrator) && !confirmed {
                return Err(IssueError::NeedsConfirmation(if administrator_exists {
                    NeedsConfirmation::AdministratorExists
                } else {
                    NeedsConfirmation::NotAConfiguredAdministrator
                }));
            }
            let (id, generation): (Uuid, i64) = sqlx::query_as(
                "insert into identity(email) values($1) returning id, session_generation",
            )
            .bind(&email)
            .fetch_one(&mut *tx)
            .await?;
            (id, generation, true)
        }
    };
    // One live link per account: a new one ends the earlier ones, so bearer credentials do not pile up.
    let superseded = sqlx::query(
        "update operator_sign_in_link set consumed_at=now(), outcome='revoked'
         where identity_id=$1 and consumed_at is null and expires_at > now()",
    )
    .bind(identity_id)
    .execute(&mut *tx)
    .await?
    .rows_affected();
    let (plain, hash) =
        random_token().map_err(|_| IssueError::Invalid("no randomness available".into()))?;
    let token = Token(Zeroizing::new(plain));
    let (link_id, expires_at): (Uuid, DateTime<Utc>) = sqlx::query_as(
        "insert into operator_sign_in_link(identity_id,token_hash,session_generation,operator,reason,expires_at)
         values($1,$2,$3,$4,$5, now()+make_interval(secs => $6)) returning id, expires_at",
    )
    .bind(identity_id)
    .bind(&hash)
    .bind(generation)
    .bind(request.operator)
    .bind(request.reason)
    .bind(ttl as f64)
    .fetch_one(&mut *tx)
    .await?;
    audit(
        &mut tx,
        identity_id,
        "auth.operator_link.issued",
        link_id,
        json!({
            "operator": request.operator,
            "reason": request.reason,
            "ttl_seconds": ttl,
            "created_account": created_account,
            "administrator": administrator,
            "superseded": superseded,
            "token_prefix": hex::encode(&hash[..SHOWN_PREFIX_DIGITS / 2]),
        }),
    )
    .await?;
    tx.commit().await?;
    Ok(Issued {
        token,
        link_id,
        token_prefix: hex::encode(&hash[..SHOWN_PREFIX_DIGITS / 2]),
        identity_id,
        email,
        created_account,
        administrator,
        superseded,
        expires_at,
    })
}

/// One link as `list-links` shows it. No token, and only a prefix of its hash.
#[derive(Debug, sqlx::FromRow)]
pub struct LinkRow {
    pub id: Uuid,
    pub token_prefix: String,
    pub email: String,
    pub operator: String,
    pub reason: String,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub consumed_at: Option<DateTime<Utc>>,
    pub outcome: Option<String>,
    /// `live`, `used`, `revoked`, `expired`, or `lapsed` (past its time, not yet marked).
    pub state: String,
}

/// The newest links (live ones only unless `all`), at most 200.
pub async fn list(pool: &PgPool, all: bool) -> Result<Vec<LinkRow>, sqlx::Error> {
    sqlx::query_as::<_, LinkRow>(
        "select l.id, encode(substr(l.token_hash,1,4),'hex') as token_prefix, i.email, l.operator, l.reason,
                l.created_at, l.expires_at, l.consumed_at, l.outcome,
                case when l.outcome is not null then l.outcome
                     when l.expires_at <= now() then 'lapsed' else 'live' end as state
         from operator_sign_in_link l join identity i on i.id=l.identity_id
         where $1 or (l.consumed_at is null and l.expires_at > now())
         order by l.created_at desc, l.id limit 200",
    )
    .bind(all)
    .fetch_all(pool)
    .await
}

#[derive(Debug, thiserror::Error)]
pub enum RevokeError {
    #[error(
        "give at least {MIN_PREFIX_DIGITS} hexadecimal digits of the link's hash (see list-links)"
    )]
    BadPrefix,
    #[error("no live link has that prefix")]
    NotFound,
    #[error("{0} live links have that prefix; give more digits")]
    Ambiguous(usize),
    #[error("the operator label is not acceptable")]
    BadOperator,
    #[error("database error: {0}")]
    Db(#[from] sqlx::Error),
}

/// End one live link, named by a prefix of its hash. Operator credential only.
pub async fn revoke(pool: &PgPool, prefix: &str, operator: &str) -> Result<Uuid, RevokeError> {
    let prefix = prefix.trim().to_ascii_lowercase();
    if prefix.len() < MIN_PREFIX_DIGITS
        || prefix.len() > 64
        || !prefix
            .bytes()
            .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
    {
        return Err(RevokeError::BadPrefix);
    }
    free_text("the operator label", operator, MAX_OPERATOR_BYTES, false)
        .map_err(|_| RevokeError::BadOperator)?;
    let mut tx = pool.begin().await?;
    let live: Vec<(Uuid, Uuid)> = sqlx::query_as(
        "select id, identity_id from operator_sign_in_link
         where consumed_at is null and expires_at > now() and starts_with(encode(token_hash,'hex'), $1)
         for update",
    )
    .bind(&prefix)
    .fetch_all(&mut *tx)
    .await?;
    let [(link, identity)] = live.as_slice() else {
        return Err(if live.is_empty() {
            RevokeError::NotFound
        } else {
            RevokeError::Ambiguous(live.len())
        });
    };
    sqlx::query(
        "update operator_sign_in_link set consumed_at=now(), outcome='revoked' where id=$1",
    )
    .bind(link)
    .execute(&mut *tx)
    .await?;
    audit(
        &mut tx,
        *identity,
        "auth.operator_link.revoked",
        *link,
        json!({"operator": operator}),
    )
    .await?;
    tx.commit().await?;
    Ok(*link)
}

/// Use a token that is not an emailed link, if it is a live operator link: consume it, make the
/// login and write the audit row in ONE transaction. Every refusal is the same
/// [`AuthError::MagicLinkInvalid`]; the reason goes to the audit trail.
pub async fn redeem(state: &AppState, token: &str) -> Result<TokenResponse, AuthError> {
    if !shaped_token(token) {
        return Err(AuthError::MagicLinkInvalid);
    }
    let hash = sha256(token.as_bytes());
    let mut tx = state.pool.begin().await?;
    // The statement that checks the row also uses it up, so two simultaneous uses take turns on
    // the row lock and the second finds it used. Expiry is the database clock's (and a trigger's).
    let used: Option<(Uuid, Uuid, i64)> = sqlx::query_as(
        "update operator_sign_in_link l set consumed_at=now(), outcome='used'
         where l.token_hash=$1 and l.consumed_at is null and l.expires_at > now()
           and exists(select 1 from identity i where i.id=l.identity_id and i.disabled_at is null and i.session_generation=l.session_generation)
         returning l.id, l.identity_id, l.session_generation",
    )
    .bind(&hash)
    .fetch_optional(&mut *tx)
    .await?;
    let Some((link, identity, generation)) = used else {
        drop(tx);
        record_refusal(state, &hash).await;
        return Err(AuthError::MagicLinkInvalid);
    };
    let (tokens, session) = mint_session_with(state, &mut *tx, identity, generation).await?;
    audit(
        &mut tx,
        identity,
        "auth.operator_link.used",
        link,
        json!({"session_id": session}),
    )
    .await?;
    tx.commit().await?;
    Ok(tokens)
}

/// Note a refused use of a KNOWN link (an unknown token has nothing to attribute it to), at most
/// one audit row a minute per link so a replayed link cannot fill the append-only trail. A
/// failure here never changes the answer.
async fn record_refusal(state: &AppState, hash: &[u8]) {
    let result = sqlx::query(
        "insert into operation_audit(actor_id,action,resource_id,detail)
         select l.identity_id, 'auth.operator_link.refused', l.id,
                jsonb_build_object('reason',
                    case when l.outcome='used' then 'used'
                         when l.outcome='revoked' then 'revoked'
                         when l.outcome='expired' or l.expires_at <= now() then 'expired'
                         else 'account' end)
         from operator_sign_in_link l
         where l.token_hash=$1
           and not exists(select 1 from operation_audit a where a.resource_id=l.id
                          and a.action='auth.operator_link.refused' and a.created_at > now() - interval '1 minute')",
    )
    .bind(hash)
    .execute(&state.pool)
    .await;
    if let Err(error) = result {
        tracing::warn!(%error, "could not record a refused operator sign-in link");
    }
}

/// Retention: mark live links past their time as expired, with an audit row each, in one
/// statement. Returns how many. (Pruning the ended rows is the retention job's ordinary spec.)
pub async fn sweep_expired(connection: &mut PgConnection, batch: i64) -> Result<u64, sqlx::Error> {
    sqlx::query(
        "with ended as (
             update operator_sign_in_link set consumed_at=now(), outcome='expired'
             where id in (select id from operator_sign_in_link
                          where consumed_at is null and expires_at <= now()
                          order by expires_at limit $1 for update skip locked)
             returning id, identity_id, operator)
         insert into operation_audit(actor_id,action,resource_id,detail)
         select identity_id, 'auth.operator_link.expired', id, jsonb_build_object('operator', operator) from ended",
    )
    .bind(batch)
    .execute(connection)
    .await
    .map(|done| done.rows_affected())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn addresses_are_trimmed_lowered_and_checked() {
        assert_eq!(
            normalise_email("  You@Example.ORG ").unwrap(),
            "you@example.org"
        );
        for bad in [
            "",
            "plain",
            "a@b",
            "@example.org",
            "a@@example.org",
            "a@b@example.org",
            "a b@example.org",
            "a@example.org,b@example.org",
            "a@example.org;b",
            "<a@example.org>",
            "a@.example.org",
            "a@example.org.",
            "a\u{7}@example.org",
            "a@exa\nmple.org",
            "\"a\"@example.org",
        ] {
            assert!(normalise_email(bad).is_err(), "{bad:?}");
        }
        let long = format!("{}@example.org", "a".repeat(250));
        assert!(normalise_email(&long).is_err());
    }

    #[test]
    fn the_lifetime_has_bounds() {
        assert!(check_ttl(MIN_TTL_SECONDS).is_ok());
        assert!(check_ttl(MAX_TTL_SECONDS).is_ok());
        assert!(check_ttl(DEFAULT_TTL_SECONDS).is_ok());
        for bad in [
            i64::MIN,
            -1,
            0,
            MIN_TTL_SECONDS - 1,
            MAX_TTL_SECONDS + 1,
            i64::MAX,
        ] {
            assert!(check_ttl(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn free_text_is_bounded_and_has_no_control_characters() {
        assert!(free_text("x", "alice", 64, false).is_ok());
        assert!(free_text("x", "", 64, false).is_err());
        assert!(free_text("x", "", 64, true).is_ok());
        assert!(free_text("x", &"a".repeat(65), 64, false).is_err());
        assert!(free_text("x", "a\u{1b}[31m", 64, false).is_err());
        assert!(free_text("x", "line\nbreak", 200, true).is_err());
    }

    #[test]
    fn the_origin_is_what_the_web_server_calls_itself() {
        assert_eq!(
            check_origin("https://docs.example.org").unwrap(),
            "https://docs.example.org"
        );
        assert_eq!(
            check_origin("https://docs.example.org:8443/").unwrap(),
            "https://docs.example.org:8443"
        );
        assert_eq!(
            check_origin("http://localhost:4175").unwrap(),
            "http://localhost:4175"
        );
        assert_eq!(
            check_origin("http://127.0.0.1:3000").unwrap(),
            "http://127.0.0.1:3000"
        );
        for bad in [
            "",
            "docs.example.org",
            "http://docs.example.org",
            "ftp://x.example",
            "https://",
            "https://user@docs.example.org",
            "https://docs.example.org/login",
            "https://docs.example.org?x=1",
            "https://docs.example.org#frag",
            "https://docs.example.org evil",
            "javascript://x",
            "https://docs.example.org\\@evil.example",
        ] {
            assert!(check_origin(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn a_token_shows_nothing_of_itself_and_the_link_has_one_shape() {
        let token = Token(Zeroizing::new("ab".repeat(32)));
        assert_eq!(format!("{token:?}"), "Token([redacted])");
        let url = sign_in_url("https://docs.example.org", &token).unwrap();
        assert_eq!(
            url,
            format!("https://docs.example.org/login?token={}", "ab".repeat(32))
        );
        assert!(sign_in_url("http://docs.example.org", &token).is_err());
        let (plain, _) = random_token().unwrap();
        assert!(
            shaped_token(&plain),
            "the token the emailed flow makes has the shape redeem accepts"
        );
        for bad in [
            "",
            "AB",
            &"AB".repeat(32),
            &"a".repeat(63),
            &"a".repeat(65),
            &"g".repeat(64),
        ] {
            assert!(!shaped_token(bad), "{bad:?}");
        }
    }
}
