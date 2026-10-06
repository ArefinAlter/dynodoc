//! The document role ladder and review rules.
//!
//! Engine capability ([`Role`]: author, reviewer, auditor) decides which operations
//! the governance gate accepts. Product roles name what a person may do with a
//! document and its change requests:
//!
//! | Role        | Stored as                        | Adds                                     |
//! |-------------|----------------------------------|------------------------------------------|
//! | Viewer      | `auditor`                        | read, download                           |
//! | Contributor | `reviewer` ("comment & suggest") | comment, drafts, push files, send requests |
//! | Reviewer    | `approver`                       | approve or request changes               |
//! | Editor      | `author`                         | edit the team version, merge, decline    |
//! | Manager     | workspace owner/manager          | share, review rules, trash, ownership    |
//! | Owner       | `document.created_by`            | everything; notified about copies        |
//!
//! Review rules ([`Policy`]) can protect the team version, require approvals and
//! reserve merging for the owner and managers.
use crate::error::ApiError;
use engine_shared::EventPayload;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::PgConnection;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MemberRole {
    Viewer,
    Contributor,
    Reviewer,
    Editor,
    Manager,
    Owner,
}
impl MemberRole {
    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "viewer" => Self::Viewer,
            "contributor" => Self::Contributor,
            "reviewer" => Self::Reviewer,
            "editor" => Self::Editor,
            "manager" => Self::Manager,
            "owner" => Self::Owner,
            _ => return None,
        })
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Viewer => "viewer",
            Self::Contributor => "contributor",
            Self::Reviewer => "reviewer",
            Self::Editor => "editor",
            Self::Manager => "manager",
            Self::Owner => "owner",
        }
    }
    /// The `document_access.role` value that grants this role directly.
    pub fn stored(self) -> Option<&'static str> {
        match self {
            Self::Viewer => Some("auditor"),
            Self::Contributor => Some("reviewer"),
            Self::Reviewer => Some("approver"),
            Self::Editor => Some("author"),
            Self::Manager | Self::Owner => None,
        }
    }
    /// Accept product names and the historical stored names in sharing requests.
    pub fn from_request(value: &str) -> Option<Self> {
        Self::parse(value).or(match value {
            "author" => Some(Self::Editor),
            "approver" => Some(Self::Reviewer),
            "auditor" => Some(Self::Viewer),
            _ => None,
        })
    }
    pub fn can_approve(self) -> bool {
        self >= Self::Reviewer
    }
    pub fn can_manage(self) -> bool {
        self >= Self::Manager
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Policy {
    pub protect_team_version: bool,
    pub required_approvals: i16,
    /// `editors` or `owners`.
    pub merge_roles: String,
}
impl Default for Policy {
    fn default() -> Self {
        Self {
            protect_team_version: false,
            required_approvals: 0,
            merge_roles: "editors".into(),
        }
    }
}
impl Policy {
    pub fn can_merge(&self, role: MemberRole) -> bool {
        role.can_manage() || (role == MemberRole::Editor && self.merge_roles == "editors")
    }
    pub fn can_edit_team(&self, role: MemberRole) -> bool {
        role.can_manage() || (role == MemberRole::Editor && !self.protect_team_version)
    }
}

/// The person's product role, or `None` without access (or in Trash).
pub async fn member_role(
    db: &mut PgConnection,
    document: Uuid,
    person: Uuid,
) -> Result<Option<MemberRole>, ApiError> {
    let role: Option<String> = sqlx::query_scalar("select document_member_role($1,$2)")
        .bind(document)
        .bind(person)
        .fetch_one(&mut *db)
        .await?;
    Ok(role.as_deref().and_then(MemberRole::parse))
}
pub async fn require_member(
    db: &mut PgConnection,
    document: Uuid,
    person: Uuid,
) -> Result<MemberRole, ApiError> {
    member_role(db, document, person)
        .await?
        .ok_or(ApiError::Forbidden)
}

pub async fn policy(db: &mut PgConnection, document: Uuid) -> Result<Policy, ApiError> {
    Ok(policy_with_source(db, document).await?.0)
}

/// A file override wins; otherwise the containing project's rules apply.
pub async fn policy_with_source(
    db: &mut PgConnection,
    document: Uuid,
) -> Result<(Policy, Value), ApiError> {
    let row: Option<(bool, i16, String)> = sqlx::query_as(
        "select protect_team_version,required_approvals,merge_roles from document_policy where document_id=$1",
    )
    .bind(document)
    .fetch_optional(&mut *db)
    .await?;
    if let Some((protect, approvals, merge)) = row {
        return Ok((
            Policy {
                protect_team_version: protect,
                required_approvals: approvals,
                merge_roles: merge,
            },
            json!({"kind":"file"}),
        ));
    }
    let project: Option<(Uuid, String, bool, i16, String)> = sqlx::query_as("select s.id,s.name,p.protect_team_version,p.required_approvals,p.merge_roles from project_settings p join access_space s on s.id=p.space_id where p.space_id=document_project($1)")
        .bind(document).fetch_optional(db).await?;
    Ok(match project {
        Some((id, name, protect, approvals, merge)) => (
            Policy {
                protect_team_version: protect,
                required_approvals: approvals,
                merge_roles: merge,
            },
            json!({"kind":"project","id":id,"name":name}),
        ),
        None => (Policy::default(), json!({"kind":"default"})),
    })
}

/// Operations that change the team version's content, as opposed to comments,
/// suggestions and review decisions.
pub fn changes_content(op: &EventPayload) -> bool {
    matches!(
        op,
        EventPayload::NodeCreated { .. }
            | EventPayload::FieldEdited { .. }
            | EventPayload::RichTextPatched { .. }
            | EventPayload::NodeMoved { .. }
            | EventPayload::NodeDeleted { .. }
            | EventPayload::NodeRestored { .. }
            | EventPayload::ChoiceAdded { .. }
            | EventPayload::ChoiceRemoved { .. }
            | EventPayload::Deployed { .. }
    )
}

/// Direct changes to the team version (editing, restoring, sharing a draft,
/// accepting suggestions) are refused when the rules protect it.
pub async fn ensure_team_edit(
    db: &mut PgConnection,
    document: Uuid,
    person: Uuid,
) -> Result<(), ApiError> {
    let role = require_member(db, document, person).await?;
    let rules = policy(db, document).await?;
    if rules.can_edit_team(role) {
        return Ok(());
    }
    Err(ApiError::Denied {
        reason: if rules.protect_team_version && role == MemberRole::Editor {
            "This document's team version is protected. Make your changes in a personal draft and send them for review.".into()
        } else {
            "Your role can't change the team version. Work in a personal draft and send it for review.".into()
        },
    })
}

/// What each role may do, for the sharing panel and client-side affordances.
pub fn capabilities(role: MemberRole, rules: &Policy) -> Value {
    json!({
        "read": true,
        "comment": role >= MemberRole::Contributor,
        "propose": role >= MemberRole::Contributor,
        "approve": role.can_approve(),
        "edit_team": rules.can_edit_team(role),
        "merge": rules.can_merge(role),
        "share": role.can_manage(),
        "manage_rules": role.can_manage(),
        "transfer": role.can_manage(),
        "trash": role.can_manage(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn roles_order_and_translate_to_stored_values() {
        assert!(MemberRole::Owner > MemberRole::Manager);
        assert!(MemberRole::Reviewer > MemberRole::Contributor);
        assert_eq!(MemberRole::from_request("author"), Some(MemberRole::Editor));
        assert_eq!(MemberRole::Reviewer.stored(), Some("approver"));
        assert_eq!(MemberRole::Contributor.stored(), Some("reviewer"));
        assert!(MemberRole::Manager.stored().is_none());
    }
    #[test]
    fn rules_limit_merging_and_direct_editing() {
        let open = Policy::default();
        assert!(open.can_merge(MemberRole::Editor));
        assert!(open.can_edit_team(MemberRole::Editor));
        assert!(!open.can_merge(MemberRole::Reviewer));
        let strict = Policy {
            protect_team_version: true,
            required_approvals: 2,
            merge_roles: "owners".into(),
        };
        assert!(!strict.can_merge(MemberRole::Editor));
        assert!(!strict.can_edit_team(MemberRole::Editor));
        assert!(strict.can_merge(MemberRole::Owner));
        assert!(strict.can_edit_team(MemberRole::Manager));
    }
}
