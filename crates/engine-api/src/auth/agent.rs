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
//! disabling the account, therefore ends access at the next request. The project and
//! file roles are read by the handler.

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
/// Every read route of the agent data plane.
pub struct ContentRead;
impl AgentScope for ContentRead {
    const SCOPE: &'static str = "content:read";
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
        let current: Option<(i64, bool, bool)> = sqlx::query_as(&format!(
            "select i.session_generation,exists(select 1 from device_key_class c where c.key_id=k.id and c.class='agent'),{} from identity i join device_signing_key k on k.identity_id=i.id where i.id=$1 and k.id=$2 and i.disabled_at is null and i.erased_at is null and not exists(select 1 from device_key_revocation r where r.key_id=k.id)",
            key_reaches("$3", "$4::text[]")
        ))
        .bind(claims.identity)
        .bind(claims.key)
        .bind(claims.project)
        .bind(vec![S::SCOPE])
        .fetch_optional(&state.pool)
        .await
        .map_err(ApiError::from)?;
        let Some((generation, agent_key, reaches)) = current else {
            return Err(ApiError::Unauthorized.into());
        };
        if generation != claims.generation {
            return Err(ApiError::Unauthorized.into());
        }
        if !agent_key || !reaches {
            return Err(ApiError::Forbidden.into());
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
