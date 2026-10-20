//! The agent data plane's extractor (docs/AI-AGENT-PROTOCOL-SPEC.md section 14,
//! docs/AI-AGENT-CONNECTORS.md section 4.3).
//!
//! [`AgentAuth`] mirrors [`super::device::ProjectAuth`] and accepts exactly one kind of
//! credential: a project device session whose scopes include the route's agent scope.
//! An ordinary account login, a connector key or anything that is not a device token is
//! refused (`401`), and an agent token is refused by every other extractor because they
//! cannot decrypt it or lack its scope (spec 14.1).
//!
//! Nothing in the token is authority. Each request re-reads from the database that the
//! key is unrevoked, was enrolled for an AI agent (`device_key_class`), belongs to an
//! active account in the same session generation, and holds an active grant for this
//! route's project that lists the scope. An `all`-mode key never reaches an agent scope
//! without a grant (`super::device::key_reaches`). Revoking a grant or the key, or
//! disabling the account, therefore ends access at the next request. After the project's
//! kill switch has been pulled, only a session issued after it is accepted, so granting
//! again revives none that existed before (`project_agent_policy.sessions_valid_after`).
//! The project policy, the project and the file roles are read by the handler.

use std::{marker::PhantomData, ops::Deref};

use axum::{
    extract::{FromRequestParts, RawPathParams},
    http::{header::AUTHORIZATION, request::Parts},
};
use uuid::Uuid;

use super::{device::key_reaches, token, AuthContext, DeviceBinding};
use crate::{agent::error::AgentFail, error::ApiError, AppState};

/// A scope a handler requires of an agent token. Scopes are independent.
pub trait AgentScope: Send + Sync + 'static {
    const SCOPE: &'static str;
}
/// Every read route of the agent data plane, and the status of a proposal.
pub struct ContentRead;
impl AgentScope for ContentRead {
    const SCOPE: &'static str = "content:read";
}
/// `propose`: staging edits in a draft owned by the agent identity.
pub struct ContentPropose;
impl AgentScope for ContentPropose {
    const SCOPE: &'static str = "content:propose";
}
/// `submit` and `withdraw` of the agent's own proposals. Independent of the other scopes.
pub struct ContentSubmit;
impl AgentScope for ContentSubmit {
    const SCOPE: &'static str = "content:submit";
}

/// What a session token claims; none of it is trusted until [`verify_current`] has read it
/// back from the database.
pub(crate) struct Claimed {
    pub identity: Uuid,
    pub key: Uuid,
    pub project: Uuid,
    pub session: Uuid,
    pub generation: i64,
    pub scope: &'static str,
}

/// The facts the database must show now for a session to be honoured: the account active and
/// in the session's generation, the key unrevoked and enrolled for an AI agent, an active
/// grant of the key for the project that lists the scope (`all` mode never covers an agent
/// scope), and the session issued after the project's last kill switch. The last column of
/// the query is that kill switch (migration 0043): once pulled, only a session issued
/// strictly after it counts (a session stamped with the very same instant is treated as
/// older), and a session with no ledger row cannot show that it was.
///
/// `lock` takes the account row `FOR SHARE`. A write transaction passes it, so that a
/// revocation, a grant change, a disablement or an erasure (they take the row `FOR UPDATE`)
/// waits for the write to commit or is seen by it, and no write lands after the change.
pub(crate) async fn verify_current(
    db: &mut sqlx::PgConnection,
    claimed: &Claimed,
    lock: bool,
) -> Result<(), ApiError> {
    let current: Option<(i64, bool, bool, bool)> = sqlx::query_as(&format!(
        "select i.session_generation,exists(select 1 from device_key_class c where c.key_id=k.id and c.class='agent'),{},coalesce((select p.sessions_valid_after is null or exists(select 1 from device_session s where s.id=$5 and s.issued_at>p.sessions_valid_after) from project_agent_policy p where p.project_id=$3),true) from identity i join device_signing_key k on k.identity_id=i.id where i.id=$1 and k.id=$2 and i.disabled_at is null and i.erased_at is null and not exists(select 1 from device_key_revocation r where r.key_id=k.id){}",
        key_reaches("$3", "$4::text[]"),
        if lock { " for share of i" } else { "" }
    ))
    .bind(claimed.identity)
    .bind(claimed.key)
    .bind(claimed.project)
    .bind(vec![claimed.scope])
    .bind(claimed.session)
    .fetch_optional(&mut *db)
    .await?;
    let Some((generation, agent_key, reaches, not_cut_off)) = current else {
        return Err(ApiError::Unauthorized);
    };
    if generation != claimed.generation {
        return Err(ApiError::Unauthorized);
    }
    if !agent_key || !reaches || !not_cut_off {
        return Err(ApiError::Forbidden);
    }
    Ok(())
}

/// The caller of an agent route. Dereferences to [`AuthContext`]; `device` is always set.
pub struct AgentAuth<S: AgentScope>(AuthContext, PhantomData<S>);

impl<S: AgentScope> Deref for AgentAuth<S> {
    type Target = AuthContext;
    fn deref(&self) -> &AuthContext {
        &self.0
    }
}

impl<S: AgentScope> AgentAuth<S> {
    /// The key, project and session this request was authorised for.
    pub fn binding(&self) -> &DeviceBinding {
        self.0
            .device
            .as_ref()
            .expect("an agent login always carries its device binding")
    }
}

impl<S: AgentScope> FromRequestParts<AppState> for AgentAuth<S> {
    type Rejection = AgentFail;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, AgentFail> {
        let raw = parts
            .headers
            .get(AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .map(str::trim)
            .ok_or(ApiError::Unauthorized)?;
        // An account login, connector key or any other credential is the wrong class
        // for this plane: no account validator runs here, so nothing can pass by
        // accident.
        if !token::is_project_device_token(raw) {
            return Err(ApiError::Unauthorized.into());
        }
        let claims = token::verify_project_device(&state.auth.paseto_key, raw)
            .map_err(|_| ApiError::Unauthorized)?;
        if !claims.scopes.iter().any(|scope| scope == S::SCOPE) {
            return Err(ApiError::Forbidden.into());
        }
        // The token's project must be the route's project.
        let params = RawPathParams::from_request_parts(parts, state)
            .await
            .map_err(|_| ApiError::Forbidden)?;
        let route = params
            .iter()
            .find(|(name, _)| *name == "id")
            .and_then(|(_, value)| Uuid::parse_str(value).ok());
        if route != Some(claims.project) {
            return Err(ApiError::Forbidden.into());
        }
        // Read again on every request: nothing is cached but the token's lifetime.
        let mut db = state.pool.acquire().await.map_err(ApiError::from)?;
        verify_current(
            &mut db,
            &Claimed {
                identity: claims.identity,
                key: claims.key,
                project: claims.project,
                session: claims.session,
                generation: claims.generation,
                scope: S::SCOPE,
            },
            false,
        )
        .await?;
        drop(db);
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
