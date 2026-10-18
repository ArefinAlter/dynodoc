//! Project device sessions: the extractor for the explicit route allow-list.
//!
//! [`ProjectAuth`] accepts either an ordinary account login (identical to
//! [`AuthContext`]) or a project device token. Everything else in the API keeps
//! the account extractor, which cannot decrypt a device token, so a route is
//! reachable by a device only because its handler names `ProjectAuth<Scope>`.
//!
//! The device path is fail closed and does not trust the token for authority:
//! the signature/assertion is checked, then scope, then the route's `:id`
//! equals the token's project, then the database must still show the key
//! unrevoked, the account active in the same session generation and (for a key
//! that is not in `all` mode) an active grant for this project and scope. Project and
//! file roles are read by the handler's existing checks. Transactional handlers
//! recheck the key under the account row lock (`devices::account_lock`).

use std::{marker::PhantomData, ops::Deref};

use axum::{
    extract::{FromRequestParts, RawPathParams},
    http::{header::AUTHORIZATION, request::Parts},
};
use uuid::Uuid;

use super::{token, AuthContext, DeviceBinding};
use crate::{error::ApiError, AppState};

/// A scope a handler requires of a device token. Scopes are independent.
pub trait Need: Send + Sync + 'static {
    const SCOPE: &'static str;
}
/// `GET /projects/:id/files` and `/activity`.
pub struct ProjectRead;
/// Published commits, objects, refs, reviews, receipts, authority and storage reads.
pub struct HistoryRead;
/// Private staging, publication and ref advancement.
pub struct HistoryWrite;
/// Recording a review decision. Separate from [`HistoryWrite`] so a key can be
/// allowed to push without also being able to approve.
pub struct HistoryReview;
impl Need for ProjectRead {
    const SCOPE: &'static str = "project:read";
}
impl Need for HistoryRead {
    const SCOPE: &'static str = "history:read";
}
impl Need for HistoryWrite {
    const SCOPE: &'static str = "history:write";
}
impl Need for HistoryReview {
    const SCOPE: &'static str = "history:review";
}

/// SQL boolean: may the key `k` (alias of `device_signing_key`) use every scope in
/// the `text[]` expression `scopes` on the project expression `project`? A key in
/// `all` mode may; a `granted` key (also a key with no access row at all) needs an
/// active grant that lists every scope. Shared by the request extractor, the
/// account lock and issuance so they can never disagree.
pub(crate) fn key_reaches(project: &str, scopes: &str) -> String {
    format!("(coalesce((select a.project_access from device_key_access a where a.key_id=k.id),'granted')='all' or exists(select 1 from device_key_project_grant g where g.key_id=k.id and g.project_id={project} and g.revoked_at is null and g.scopes @> {scopes}))")
}

/// The caller of an allow-listed project route. Dereferences to [`AuthContext`]
/// so the existing helpers need no change; a device login has
/// `AuthContext::device` set and `session_id: None`.
pub struct ProjectAuth<S: Need>(AuthContext, PhantomData<S>);

impl<S: Need> Deref for ProjectAuth<S> {
    type Target = AuthContext;
    fn deref(&self) -> &AuthContext {
        &self.0
    }
}

impl<S: Need> FromRequestParts<AppState> for ProjectAuth<S> {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, ApiError> {
        let raw = parts
            .headers
            .get(AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .map(str::trim)
            .ok_or(ApiError::Unauthorized)?;
        if !token::is_project_device_token(raw) {
            // Ordinary logins are exactly as for every other account route.
            let account = AuthContext::from_request_parts(parts, state).await?;
            return Ok(Self(account, PhantomData));
        }
        let claims = token::verify_project_device(&state.auth.paseto_key, raw)?;
        if !claims.scopes.iter().any(|scope| scope == S::SCOPE) {
            return Err(ApiError::Forbidden);
        }
        // The token's project must be the route's project. A route without an
        // `id` path parameter is never reachable by a device token.
        let params = RawPathParams::from_request_parts(parts, state)
            .await
            .map_err(|_| ApiError::Forbidden)?;
        let route = params
            .iter()
            .find(|(name, _)| *name == "id")
            .and_then(|(_, value)| Uuid::parse_str(value).ok());
        if route != Some(claims.project) {
            return Err(ApiError::Forbidden);
        }
        // The key's reach (account-wide mode or an active grant for this project
        // and this route's scope) is read again on every request: revoking or
        // narrowing a grant applies to the next call, nothing is cached.
        let current: Option<(i64, bool)> = sqlx::query_as(&format!(
            "select i.session_generation,{} from identity i join device_signing_key k on k.identity_id=i.id where i.id=$1 and k.id=$2 and i.disabled_at is null and i.erased_at is null and not exists(select 1 from device_key_revocation r where r.key_id=k.id)",
            key_reaches("$3", "$4::text[]")
        ))
        .bind(claims.identity)
        .bind(claims.key)
        .bind(claims.project)
        .bind(vec![S::SCOPE])
        .fetch_optional(&state.pool)
        .await?;
        let Some((generation, reaches)) = current else {
            return Err(ApiError::Unauthorized);
        };
        if generation != claims.generation {
            return Err(ApiError::Unauthorized);
        }
        if !reaches {
            return Err(ApiError::Forbidden);
        }
        Ok(Self(
            AuthContext {
                identity_id: claims.identity,
                session_generation: claims.generation,
                session_id: None,
                device: Some(DeviceBinding {
                    key_id: claims.key,
                    project: claims.project,
                    session: claims.session,
                    scope: S::SCOPE,
                }),
            },
            PhantomData,
        ))
    }
}
