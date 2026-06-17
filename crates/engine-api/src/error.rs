//! The single API error type and its one [`IntoResponse`] (docs/18 §4).
//!
//! Every handler returns `Result<_, ApiError>`. [`ApiError`] is the one place an
//! engine/auth error becomes an HTTP status + JSON body, so the wire contract is
//! uniform: `{ "error": "...", "code": "...", "details": {...} }`. Internal failures
//! are logged server-side with full context and surfaced to clients only as a generic
//! 500 — internals never leak (the same discipline as [`crate::auth::AuthError`]).
//!
//! The engine's own error vocabularies ([`engine_core`] governance/ops/log/snapshot,
//! [`crate::auth::AuthError`]) are translated here so handlers can use `?` directly.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::{json, Value};

use engine_core::governance::GovernanceError;
use engine_core::log::EventError;
use engine_core::ops::OpError;
use engine_core::snapshot::SnapshotError;

use crate::auth::AuthError;

/// Everything an API handler can fail with, mapped to one HTTP status + JSON body.
#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    /// The resource (document, node, snapshot, event) does not exist → 404.
    #[error("not found")]
    NotFound,
    /// No/invalid credentials → 401.
    #[error("unauthorized")]
    Unauthorized,
    /// Authenticated but lacking the capability/role for this action → 403.
    #[error("forbidden")]
    Forbidden,
    /// The request was structurally or semantically invalid → 400.
    #[error("bad request: {reason}")]
    BadRequest { reason: String },
    /// The request conflicts with current state (e.g. deploy refused on integrity
    /// violations, a duplicate id) → 409.
    #[error("conflict: {reason}")]
    Conflict { reason: String },
    /// The caller exceeded a rate limit → 429, with a `Retry-After` hint (seconds).
    #[error("rate limited")]
    RateLimited { retry_after: u64 },
    /// An unexpected server-side failure → 500. The detail is logged, never returned.
    #[error("internal error: {0}")]
    Internal(String),
}

impl ApiError {
    /// The HTTP status this error maps to.
    fn status(&self) -> StatusCode {
        match self {
            ApiError::NotFound => StatusCode::NOT_FOUND,
            ApiError::Unauthorized => StatusCode::UNAUTHORIZED,
            ApiError::Forbidden => StatusCode::FORBIDDEN,
            ApiError::BadRequest { .. } => StatusCode::BAD_REQUEST,
            ApiError::Conflict { .. } => StatusCode::CONFLICT,
            ApiError::RateLimited { .. } => StatusCode::TOO_MANY_REQUESTS,
            ApiError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    /// A stable, machine-readable error code for clients/generated TS types.
    fn code(&self) -> &'static str {
        match self {
            ApiError::NotFound => "not_found",
            ApiError::Unauthorized => "unauthorized",
            ApiError::Forbidden => "forbidden",
            ApiError::BadRequest { .. } => "bad_request",
            ApiError::Conflict { .. } => "conflict",
            ApiError::RateLimited { .. } => "rate_limited",
            ApiError::Internal(_) => "internal",
        }
    }

    /// Structured detail returned to the client. Never includes internal text.
    fn details(&self) -> Value {
        match self {
            ApiError::BadRequest { reason } | ApiError::Conflict { reason } => {
                json!({ "reason": reason })
            }
            ApiError::RateLimited { retry_after } => json!({ "retry_after": retry_after }),
            _ => json!({}),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = self.status();
        // Internal errors carry detail that must not reach the client; log it here.
        if status == StatusCode::INTERNAL_SERVER_ERROR {
            tracing::error!(error = %self, "api internal error");
        }
        // The public message is the generic status text for 500s, the variant message
        // otherwise (the variant messages above are deliberately non-leaking).
        let message = if status == StatusCode::INTERNAL_SERVER_ERROR {
            "internal error".to_string()
        } else {
            self.to_string()
        };
        let body = json!({
            "error": message,
            "code": self.code(),
            "details": self.details(),
        });

        let mut response = (status, Json(body)).into_response();
        if let ApiError::RateLimited { retry_after } = self {
            if let Ok(value) = retry_after.to_string().parse() {
                response.headers_mut().insert("retry-after", value);
            }
        }
        response
    }
}

// --- translations from the engine/auth error vocabularies -----------------------

impl From<AuthError> for ApiError {
    fn from(err: AuthError) -> Self {
        match err {
            AuthError::TokenInvalid | AuthError::MagicLinkInvalid | AuthError::RefreshInvalid => {
                ApiError::Unauthorized
            }
            AuthError::Forbidden => ApiError::Forbidden,
            AuthError::InvalidKey | AuthError::TokenMint(_) => ApiError::Internal(err.to_string()),
            AuthError::Db(e) => ApiError::Internal(e.to_string()),
        }
    }
}

impl From<sqlx::Error> for ApiError {
    fn from(err: sqlx::Error) -> Self {
        ApiError::Internal(err.to_string())
    }
}

impl From<OpError> for ApiError {
    fn from(err: OpError) -> Self {
        // A validation failure is the client's fault (a bad/stale op), so 400 — except
        // an id collision, which is a conflict with existing state.
        match err {
            OpError::NodeExists(_) | OpError::SuggestionExists(_) => ApiError::Conflict {
                reason: err.to_string(),
            },
            other => ApiError::BadRequest {
                reason: other.to_string(),
            },
        }
    }
}

impl From<GovernanceError> for ApiError {
    fn from(err: GovernanceError) -> Self {
        match err {
            // A capability violation is a role/authorization failure, not a 400.
            GovernanceError::Unauthorized { .. } => ApiError::Forbidden,
            GovernanceError::Op(op) => ApiError::from(op),
            // A deploy refused on referential-integrity violations conflicts with the
            // current (broken) instrument state.
            GovernanceError::Integrity(_) => ApiError::Conflict {
                reason: err.to_string(),
            },
        }
    }
}

impl From<EventError> for ApiError {
    fn from(err: EventError) -> Self {
        match err {
            EventError::DocumentNotFound(_) => ApiError::NotFound,
            other => ApiError::Internal(other.to_string()),
        }
    }
}

impl From<SnapshotError> for ApiError {
    fn from(err: SnapshotError) -> Self {
        match err {
            SnapshotError::Event(EventError::DocumentNotFound(_)) => ApiError::NotFound,
            other => ApiError::Internal(other.to_string()),
        }
    }
}
