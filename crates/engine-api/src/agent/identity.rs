//! The agent identity (docs/AI-AGENT-CONNECTORS.md section 4.1, migration 0044): the
//! identity row under which an AI agent's drafts, edits and audit rows are recorded, and
//! the rules that keep it from ever being a person.
//!
//! * **One per (principal, client product).** Made when a person first grants an agent key
//!   a write scope ([`ensure`], inside the grant transaction); found again per request by
//!   [`resolve`] from the key's own class row, never from anything the client says.
//! * **Not a person.** The row is born disabled (`identity.disabled_at`, which every
//!   sign-in, session, magic-link and notification path already tests), its address is in
//!   the reserved domain [`RESERVED_DOMAIN`] that [`is_reserved_email`] lets no sign-up or
//!   invitation use, and the database refuses it as a member, a reviewer or a key holder.
//! * **Its authority is its principal's.** Nothing here grants anything: every request
//!   re-reads the principal's role, the key's grant and the project policy.
//! * **It ends with the principal.** Disabling or erasing the person revokes it (a trigger,
//!   next to the one that revokes their keys); a revoked identity is never found again.
//!
//! The SQL fragments at the bottom are the one definition of "whose draft is this" that the
//! draft and review code share: a draft belongs to its creator and, when the creator is an
//! agent identity, to the person it acts for.

use serde_json::json;
use sqlx::{PgConnection, Postgres, Transaction};
use uuid::Uuid;

use crate::{error::ApiError, product::audit};

/// Addresses in this domain (and its subdomains) belong to agent identities alone.
pub const RESERVED_DOMAIN: &str = "agents.invalid";

/// The client products an agent identity can be made for, as `device_key_class.client_kind`
/// and `agent_identity.client_kind` store them. An agent key enrolled without a product is
/// `device`.
pub const CLIENT_KINDS: [&str; 8] = [
    "claude", "chatgpt", "gemini", "copilot", "cursor", "codex", "device", "generic",
];

/// Is `email` an address of the reserved domain? Case-insensitive; the domain itself, any
/// subdomain and the trailing-dot spelling count. Only the part after the last `@` is the
/// domain, so a quoted local part cannot hide a real domain behind it or fake the reserved
/// one. Sign-in, magic links, invitations and the database all use this rule.
pub fn is_reserved_email(email: &str) -> bool {
    let Some((_, domain)) = email.trim().rsplit_once('@') else {
        return false;
    };
    let domain = domain.trim_end_matches('.').to_ascii_lowercase();
    domain == RESERVED_DOMAIN || domain.ends_with(&format!(".{RESERVED_DOMAIN}"))
}

/// The address of agent identity `id`.
pub fn agent_email(id: Uuid) -> String {
    format!("agent+{id}@{RESERVED_DOMAIN}")
}

/// How a client product is named in the display name ("Claude for Olivia").
fn product_name(kind: &str) -> &'static str {
    match kind {
        "claude" => "Claude",
        "chatgpt" => "ChatGPT",
        "gemini" => "Gemini",
        "copilot" => "Copilot",
        "cursor" => "Cursor",
        "codex" => "Codex",
        "device" => "Agent key",
        _ => "AI agent",
    }
}

/// `Claude for Olivia`: the product and the person it acts for, so a reviewer sees who
/// delegated. The person is their display name, else the part of their address before the
/// `@`; control characters are dropped and the whole is at most 100 characters.
pub fn display_name(kind: &str, display: Option<&str>, email: &str) -> String {
    let clean = |s: &str| -> String {
        s.chars()
            .filter(|c| !c.is_control())
            .collect::<String>()
            .trim()
            .to_owned()
    };
    let person = display
        .map(clean)
        .filter(|s| !s.is_empty())
        .or_else(|| email.split('@').next().map(clean).filter(|s| !s.is_empty()))
        .unwrap_or_else(|| "someone".to_owned());
    let name = format!("{} for {}", product_name(kind), person);
    name.chars().take(100).collect()
}

/// A live agent identity and the key's product.
#[derive(Debug, Clone)]
pub struct AgentIdentity {
    pub id: Uuid,
    pub client_kind: String,
    pub display_name: String,
}

type Row = (Uuid, String, Option<String>);

impl From<Row> for AgentIdentity {
    fn from((id, client_kind, display_name): Row) -> Self {
        Self {
            id,
            client_kind,
            display_name: display_name.unwrap_or_default(),
        }
    }
}

/// The live agent identity of the person `principal` for the agent key `key`: the one for
/// the key's product, or `None` when the key is not an agent key, was granted no write
/// scope yet, or the identity was revoked. Read on every write request.
pub async fn resolve(
    db: &mut PgConnection,
    principal: Uuid,
    key: Uuid,
) -> Result<Option<AgentIdentity>, sqlx::Error> {
    let row: Option<Row> = sqlx::query_as("select a.identity_id,a.client_kind,i.display_name from device_key_class c join agent_identity a on a.principal_id=$1 and a.client_kind=coalesce(c.client_kind,'device') and a.revoked_at is null join identity i on i.id=a.identity_id where c.key_id=$2 and c.class='agent'")
        .bind(principal)
        .bind(key)
        .fetch_optional(db)
        .await?;
    Ok(row.map(Into::into))
}

/// The agent identity for `key`'s product, made if the person has none yet. Called in the
/// transaction that grants a write scope, which holds the account row lock, so two grants
/// cannot both make one; the unique index on live identities is the backstop. The new row
/// is born disabled and with the reserved address (the database checks both at commit).
pub async fn ensure(
    tx: &mut Transaction<'_, Postgres>,
    principal: Uuid,
    key: Uuid,
) -> Result<AgentIdentity, ApiError> {
    let kind: Option<String> = sqlx::query_scalar(
        "select coalesce(client_kind,'device') from device_key_class where key_id=$1 and class='agent'",
    )
    .bind(key)
    .fetch_optional(&mut **tx)
    .await?;
    let Some(kind) = kind else {
        return Err(ApiError::BadRequest {
            reason: "Only an agent key can be granted a write scope".into(),
        });
    };
    if let Some(found) = resolve(&mut **tx, principal, key).await? {
        return Ok(found);
    }
    let (display, email): (Option<String>, String) =
        sqlx::query_as("select display_name,email from identity where id=$1")
            .bind(principal)
            .fetch_one(&mut **tx)
            .await?;
    let id = Uuid::new_v4();
    let name = display_name(&kind, display.as_deref(), &email);
    sqlx::query("insert into identity(id,email,display_name,disabled_at) values($1,$2,$3,now())")
        .bind(id)
        .bind(agent_email(id))
        .bind(&name)
        .execute(&mut **tx)
        .await?;
    sqlx::query(
        "insert into agent_identity(identity_id,principal_id,client_kind,client_ref) values($1,$2,$3,$4)",
    )
    .bind(id)
    .bind(principal)
    .bind(&kind)
    .bind(key.to_string())
    .execute(&mut **tx)
    .await?;
    audit(
        tx,
        principal,
        "agent.identity.created",
        id,
        json!({"principal":principal,"client_kind":kind,"key_id":key}),
    )
    .await?;
    Ok(AgentIdentity {
        id,
        client_kind: kind,
        display_name: name,
    })
}

/// Is `identity` an agent identity? The explicit check of invariant I4 in the review code.
pub async fn is_agent(db: &mut PgConnection, identity: Uuid) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar("select exists(select 1 from agent_identity where identity_id=$1)")
        .bind(identity)
        .fetch_one(db)
        .await
}

// --- SQL shared with the draft and review code (docs/AI-AGENT-CONNECTORS.md 6.1, 7.3) ---

/// SQL boolean: is the draft row `draft` (a table alias or name) the viewer's own? It is when
/// the viewer (an expression, such as `$3`) created it, or when it was created by an agent
/// identity acting for the viewer. So a person sees, edits, submits and withdraws their
/// agents' drafts, nobody else sees an unsubmitted draft, and an agent's own id (the viewer
/// of its routes) matches only its own drafts.
pub fn owned_by(draft: &str, viewer: &str) -> String {
    format!("({draft}.created_by={viewer} or exists(select 1 from agent_identity ai where ai.identity_id={draft}.created_by and ai.principal_id={viewer}))")
}

/// SQL expression, a `jsonb` object: `{"agent": {...}}` for a draft row `draft` created by an
/// agent identity (the badge data: its id, client product, display name, the person it acts
/// for, whether it has been revoked), else `{}`. Appended to the request and draft JSON, so
/// existing fields do not change and non-agent rows gain nothing.
pub fn agent_json(draft: &str) -> String {
    format!("coalesce((select jsonb_build_object('agent',jsonb_build_object('id',ai.identity_id,'client_kind',ai.client_kind,'display_name',ag.display_name,'principal',jsonb_build_object('id',pr.id,'name',pr.display_name),'revoked',ai.revoked_at is not null)) from agent_identity ai join identity ag on ag.id=ai.identity_id join identity pr on pr.id=ai.principal_id where ai.identity_id={draft}.created_by),'{{}}'::jsonb)")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_reserved_domain_is_refused_in_every_spelling() {
        for reserved in [
            "agent+1@agents.invalid",
            "a@AGENTS.INVALID",
            "a@agents.invalid.",
            "a@x.agents.invalid",
            "  a@agents.invalid  ",
            "\"b@example.com\"@agents.invalid",
        ] {
            assert!(is_reserved_email(reserved), "{reserved}");
        }
        for fine in [
            "a@example.com",
            "a@notagents.invalid",
            "a@agents.invalid.example.com",
            "agents.invalid@example.com",
            "\"a@agents.invalid\"@example.com",
            "a@invalid",
            "agents.invalid",
        ] {
            assert!(!is_reserved_email(fine), "{fine}");
        }
        assert!(is_reserved_email(&agent_email(Uuid::nil())));
    }

    #[test]
    fn the_display_name_names_the_product_and_the_person() {
        assert_eq!(
            display_name("claude", Some("Olivia"), "o@example.com"),
            "Claude for Olivia"
        );
        assert_eq!(
            display_name("chatgpt", None, "olivia.w@example.com"),
            "ChatGPT for olivia.w"
        );
        assert_eq!(
            display_name("device", Some("  "), "x@example.com"),
            "Agent key for x"
        );
        assert_eq!(
            display_name("nonsense", Some("Ol\nivia\u{7}"), "o@example.com"),
            "AI agent for Olivia"
        );
        let long = display_name("claude", Some(&"n".repeat(300)), "o@example.com");
        assert_eq!(long.chars().count(), 100);
        assert!(CLIENT_KINDS
            .iter()
            .all(|k| display_name(k, Some("P"), "p@x").ends_with("for P")));
    }

    #[test]
    fn the_ownership_fragment_names_both_owners_and_nobody_else() {
        let sql = owned_by("d", "$3");
        assert!(sql.starts_with("(d.created_by=$3 or exists("), "{sql}");
        assert!(sql.contains("ai.principal_id=$3"), "{sql}");
        assert!(agent_json("d").contains("ai.identity_id=d.created_by"));
    }
}
