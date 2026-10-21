//! Errors of the agent routes (docs/AI-AGENT-PROTOCOL-SPEC.md sections 10 and 12.4).
//!
//! The body keeps the envelope of [`crate::error`], `{error, code, details}`, and
//! `details.agent_code` carries the fine-grained code, so a client written for the
//! rest of the API keeps working. No message holds document text: the library errors
//! carry refs, hashes, counts and one fixed hint sentence each.

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use engine_agent::error::{AgentError, CompileError};
use serde_json::{json, Value};

use crate::error::{ApiError, ErrorCode};

/// Everything an agent route can fail with.
#[derive(Debug)]
pub enum AgentFail {
    /// Authorisation, database and admission failures of the shared API vocabulary.
    Api(ApiError),
    /// A protocol error produced by `engine-agent` or by request validation.
    Agent(AgentError),
    /// A protocol error of `propose` with the op that caused it: `details.op` is its index
    /// and `details.id` the agent's own label for it, when it gave one (spec 7.7).
    Compile(CompileError),
}

impl From<ApiError> for AgentFail {
    fn from(error: ApiError) -> Self {
        Self::Api(error)
    }
}
impl From<AgentError> for AgentFail {
    fn from(error: AgentError) -> Self {
        Self::Agent(error)
    }
}
impl From<CompileError> for AgentFail {
    fn from(error: CompileError) -> Self {
        Self::Compile(error)
    }
}
impl From<sqlx::Error> for AgentFail {
    fn from(error: sqlx::Error) -> Self {
        Self::Api(error.into())
    }
}
impl From<engine_core::snapshot::SnapshotError> for AgentFail {
    fn from(error: engine_core::snapshot::SnapshotError) -> Self {
        Self::Api(error.into())
    }
}

impl AgentFail {
    /// The `agent_code` this failure reports (also the ledger's `outcome`).
    pub fn agent_code(&self) -> &'static str {
        match self {
            AgentFail::Api(error) => api_agent_code(error),
            AgentFail::Agent(error) => error.agent_code(),
            AgentFail::Compile(error) => error.error.agent_code(),
        }
    }
}

/// `details.agent_code` for an error of the shared vocabulary.
fn api_agent_code(error: &ApiError) -> &'static str {
    match error {
        ApiError::NotFound => "not_found",
        ApiError::Unauthorized => "unauthorized",
        ApiError::Forbidden | ApiError::Denied { .. } => "forbidden",
        ApiError::BadRequest { .. } => "bad_request",
        ApiError::Conflict { .. } => "conflict",
        // Only the review route makes one; if it ever reached an agent route, say what it is.
        ApiError::Coded { status, .. } if *status == axum::http::StatusCode::CONFLICT => "conflict",
        ApiError::Coded { .. } => "bad_request",
        ApiError::RateLimited { .. } => "rate_limited",
        ApiError::Deadline => "deadline_exceeded",
        ApiError::Busy => "busy",
        ApiError::StorageLimit => "storage_limit",
        // A document that does not replay is an internal fault for the agent; the
        // envelope keeps the existing `replay_failed` code and names the document.
        ApiError::ReplayFailed { .. } | ApiError::Internal(_) => "internal",
    }
}

/// `details` of a protocol error: typed values only.
fn agent_details(error: &AgentError) -> Value {
    let refs = |refs: &[engine_agent::refs::Ref]| -> Value {
        refs.iter().map(|r| r.as_str()).collect::<Vec<_>>().into()
    };
    let mut details = json!({ "agent_code": error.agent_code() });
    let map = details.as_object_mut().expect("object literal");
    match error {
        AgentError::PolicyDenied { policy } => {
            map.insert("policy".into(), (*policy).into());
        }
        AgentError::BadRequest { field } => {
            map.insert("field".into(), (*field).into());
        }
        AgentError::NotFound { refs: r }
        | AgentError::UnknownRef { refs: r }
        | AgentError::AmbiguousRef { refs: r }
        | AgentError::RedactedBlock { refs: r }
        | AgentError::Conflict { refs: r } => {
            if !r.is_empty() {
                map.insert("refs".into(), refs(r));
            }
        }
        AgentError::StaleBase { target, current } => {
            map.insert("ref".into(), target.as_str().into());
            map.insert("current".into(), current.as_str().into());
        }
        AgentError::MatchCount {
            found,
            expected,
            refs: r,
        } => {
            map.insert("found".into(), (*found).into());
            map.insert("expected".into(), (*expected).into());
            if !r.is_empty() {
                map.insert("refs".into(), refs(r));
            }
        }
        AgentError::LimitExceeded {
            field,
            limit,
            value,
        } => {
            map.insert("field".into(), (*field).into());
            map.insert("limit".into(), (*limit).into());
            map.insert("value".into(), (*value).into());
        }
        AgentError::BudgetExceeded { retry_after } | AgentError::RateLimited { retry_after } => {
            map.insert("retry_after".into(), (*retry_after).into());
        }
        _ => {}
    }
    details
}

/// The response of a protocol error, `details` as built by the caller.
fn agent_response(error: &AgentError, details: Value) -> Response {
    if matches!(error, AgentError::Internal) {
        tracing::error!("agent route internal error");
    }
    let status =
        StatusCode::from_u16(error.http_status()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    let body = json!({
        "error": error.hint(),
        "code": error.code(),
        "details": details,
    });
    let mut response = (status, Json(body)).into_response();
    response.extensions_mut().insert(ErrorCode(error.code()));
    if let AgentError::BudgetExceeded { retry_after } | AgentError::RateLimited { retry_after } =
        error
    {
        if let Ok(value) = retry_after.to_string().parse() {
            response.headers_mut().insert("retry-after", value);
        }
    }
    response
}

impl IntoResponse for AgentFail {
    fn into_response(self) -> Response {
        match self {
            AgentFail::Api(error) => {
                let code = api_agent_code(&error);
                error.respond(json!({ "agent_code": code }))
            }
            AgentFail::Agent(error) => agent_response(&error, agent_details(&error)),
            AgentFail::Compile(failed) => {
                let mut details = agent_details(&failed.error);
                if let Some(map) = details.as_object_mut() {
                    if let Some(op) = failed.op {
                        map.insert("op".into(), op.into());
                    }
                    // The label was restricted to `A-Za-z0-9_.:-` by the grammar, so it is a
                    // token, not text.
                    if let Some(id) = &failed.id {
                        map.insert("id".into(), id.clone().into());
                    }
                }
                agent_response(&failed.error, details)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::to_bytes;

    async fn body(response: Response) -> (StatusCode, Value) {
        let status = response.status();
        let bytes = to_bytes(response.into_body(), 1 << 20).await.unwrap();
        (status, serde_json::from_slice(&bytes).unwrap())
    }

    #[tokio::test]
    async fn every_protocol_error_keeps_the_envelope_and_names_its_agent_code() {
        for error in engine_agent::corpus::error_table() {
            let expected_status = error.http_status();
            let agent_code = error.agent_code();
            let (status, json) = body(AgentFail::Agent(error).into_response()).await;
            assert_eq!(status.as_u16(), expected_status, "{agent_code}");
            assert_eq!(json["details"]["agent_code"], agent_code);
            assert!(json["error"].is_string() && json["code"].is_string());
        }
    }

    #[tokio::test]
    async fn shared_errors_gain_an_agent_code_and_keep_their_details() {
        let (status, json) = body(AgentFail::Api(ApiError::Forbidden).into_response()).await;
        assert_eq!((status.as_u16(), &json["code"]), (403, &json!("forbidden")));
        assert_eq!(json["details"]["agent_code"], "forbidden");
        let limited = AgentFail::Api(ApiError::RateLimited { retry_after: 60 }).into_response();
        assert_eq!(limited.headers()["retry-after"], "60");
        let (status, json) = body(limited).await;
        assert_eq!(status.as_u16(), 429);
        assert_eq!(json["details"]["retry_after"], 60);
        assert_eq!(json["details"]["agent_code"], "rate_limited");
        let replay = AgentFail::Api(ApiError::ReplayFailed {
            document_id: uuid::Uuid::nil(),
            seq: Some(3),
        });
        let (status, json) = body(replay.into_response()).await;
        assert_eq!(status.as_u16(), 500);
        assert_eq!(json["code"], "replay_failed");
        assert_eq!(json["details"]["agent_code"], "internal");
    }
}
