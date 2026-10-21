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

/// The error code of a response, for counting ([`crate::counters`]).
#[derive(Clone, Copy)]
pub struct ErrorCode(pub &'static str);

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
    /// Like [`ApiError::Forbidden`], with a reason the person can act on → 403.
    #[error("forbidden: {reason}")]
    Denied { reason: String },
    /// The request was structurally or semantically invalid → 400.
    #[error("bad request: {reason}")]
    BadRequest { reason: String },
    /// The request conflicts with current state (e.g. deploy refused on integrity
    /// violations, a duplicate id) → 409.
    #[error("conflict: {reason}")]
    Conflict { reason: String },
    /// A refusal that has a stable code of its own, for the client to act on without parsing
    /// the sentence: `request_changed` (409) and `revision_required` (400) of the review
    /// route. `details` is merged into the usual `details.reason`.
    #[error("{reason}")]
    Coded {
        status: StatusCode,
        code: &'static str,
        reason: String,
        details: Value,
    },
    /// The caller exceeded a rate limit → 429, with a `Retry-After` hint (seconds).
    #[error("rate limited")]
    RateLimited { retry_after: u64 },
    /// Read/database deadline reached; no query internals are returned.
    #[error("operation deadline exceeded")]
    Deadline,
    /// Database pool admission unavailable.
    #[error("service busy; retry later")]
    Busy,
    /// Encoded shared history reached its retained-storage budget.
    #[error("Shared history storage limit reached. Your change was not saved. Ask the workspace operator to increase the storage budget.")]
    StorageLimit,
    /// The document's event history no longer replays → 500, naming the document and
    /// the event, so an operator can find and recover it.
    #[error("document {document_id} does not replay at event {seq:?}")]
    ReplayFailed {
        document_id: uuid::Uuid,
        seq: Option<i64>,
    },
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
            ApiError::Forbidden | ApiError::Denied { .. } => StatusCode::FORBIDDEN,
            ApiError::BadRequest { .. } => StatusCode::BAD_REQUEST,
            ApiError::Conflict { .. } => StatusCode::CONFLICT,
            ApiError::Coded { status, .. } => *status,
            ApiError::RateLimited { .. } => StatusCode::TOO_MANY_REQUESTS,
            ApiError::Internal(_) | ApiError::ReplayFailed { .. } => {
                StatusCode::INTERNAL_SERVER_ERROR
            }
            ApiError::Deadline => StatusCode::GATEWAY_TIMEOUT,
            ApiError::Busy => StatusCode::SERVICE_UNAVAILABLE,
            ApiError::StorageLimit => StatusCode::INSUFFICIENT_STORAGE,
        }
    }

    /// A stable, machine-readable error code for clients/generated TS types.
    fn code(&self) -> &'static str {
        match self {
            ApiError::NotFound => "not_found",
            ApiError::Unauthorized => "unauthorized",
            ApiError::Forbidden | ApiError::Denied { .. } => "forbidden",
            ApiError::BadRequest { .. } => "bad_request",
            ApiError::Conflict { .. } => "conflict",
            ApiError::Coded { code, .. } => code,
            ApiError::RateLimited { .. } => "rate_limited",
            ApiError::Internal(_) => "internal",
            ApiError::ReplayFailed { .. } => "replay_failed",
            ApiError::Deadline => "deadline_exceeded",
            ApiError::Busy => "busy",
            ApiError::StorageLimit => "storage_limit",
        }
    }

    /// Structured detail returned to the client. Never includes internal text.
    fn details(&self) -> Value {
        match self {
            ApiError::BadRequest { reason }
            | ApiError::Conflict { reason }
            | ApiError::Denied { reason } => {
                json!({ "reason": reason })
            }
            ApiError::Coded {
                reason, details, ..
            } => {
                let mut merged = json!({ "reason": reason });
                if let (Some(into), Value::Object(more)) = (merged.as_object_mut(), details) {
                    into.extend(more.clone());
                }
                merged
            }
            ApiError::RateLimited { retry_after } => json!({ "retry_after": retry_after }),
            ApiError::ReplayFailed { document_id, seq } => {
                json!({ "document_id": document_id, "seq": seq })
            }
            _ => json!({}),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        self.respond(Value::Null)
    }
}

impl ApiError {
    /// The response, with the members of the object `extra` merged into `details`
    /// (the agent routes add `agent_code`; docs/AI-AGENT-PROTOCOL-SPEC.md 12.4).
    pub(crate) fn respond(self, extra: Value) -> Response {
        let status = self.status();
        // Internal errors carry detail that must not reach the client; log it here.
        if status == StatusCode::INTERNAL_SERVER_ERROR {
            tracing::error!(error = %self, "api internal error");
        }
        // The public message is the generic status text for internal errors, the variant
        // message otherwise (the variant messages above are deliberately non-leaking).
        let message = if matches!(self, ApiError::Internal(_)) {
            "internal error".to_string()
        } else {
            self.to_string()
        };
        let mut details = self.details();
        if let (Some(into), Value::Object(extra)) = (details.as_object_mut(), extra) {
            into.extend(extra);
        }
        let body = json!({
            "error": message,
            "code": self.code(),
            "details": details,
        });

        let mut response = (status, Json(body)).into_response();
        response.extensions_mut().insert(ErrorCode(self.code()));
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
            AuthError::Db(e) => e.into(),
        }
    }
}

impl From<sqlx::Error> for ApiError {
    fn from(err: sqlx::Error) -> Self {
        if matches!(err, sqlx::Error::PoolTimedOut) {
            return ApiError::Busy;
        }
        if let sqlx::Error::Database(db) = &err {
            if db.code().as_deref() == Some("23514")
                && matches!(
                    db.constraint(),
                    Some(
                        "checkpoint_storage_quota"
                            | "project_staging_quota"
                            | "project_history_quota"
                    )
                )
            {
                return ApiError::StorageLimit;
            }
            if matches!(db.code().as_deref(), Some("57014" | "55P03")) {
                return ApiError::Deadline;
            }
        }
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
            GovernanceError::Integrity(_) | GovernanceError::SheetProtection(_) => {
                ApiError::Conflict {
                    reason: err.to_string(),
                }
            }
        }
    }
}

impl From<EventError> for ApiError {
    fn from(err: EventError) -> Self {
        match err {
            EventError::DocumentNotFound(_) => ApiError::NotFound,
            EventError::Db(e) => e.into(),
            other => ApiError::Internal(other.to_string()),
        }
    }
}

impl From<SnapshotError> for ApiError {
    fn from(err: SnapshotError) -> Self {
        match err {
            SnapshotError::Event(e) => e.into(),
            SnapshotError::Db(e) => e.into(),
            SnapshotError::Shared(e) => (*e).into(),
            SnapshotError::Replay { document, source } => ApiError::ReplayFailed {
                document_id: document.0,
                seq: source.seq(),
            },
            other => ApiError::Internal(other.to_string()),
        }
    }
}

impl From<engine_core::shared_checkpoint::Error> for ApiError {
    fn from(err: engine_core::shared_checkpoint::Error) -> Self {
        match err {
            engine_core::shared_checkpoint::Error::Db(e) => e.into(),
            engine_core::shared_checkpoint::Error::History(e) => e.into(),
            other => ApiError::Internal(other.to_string()),
        }
    }
}
