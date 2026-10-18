//! Agent errors (protocol spec section 10).
//!
//! Errors never carry document text: only refs, hashes, counts, field names and codes,
//! and at most one fixed hint sentence. Every payload field is therefore a typed value,
//! never a free `String` that could be filled from a document.

use crate::dct::{HeaderBuilder, Tool};
use crate::hash::ShortHash;
use crate::refs::Ref;

/// How a caller should react (spec section 10, "Retry" column).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Retry {
    No,
    AfterReauth,
    AfterFixing,
    /// Call `outline` (or re-read) first.
    AfterRead,
    /// Wait for `retry_after` seconds.
    AfterDelay,
    Yes,
    /// Reads only; a write with an unknown outcome is retried with the same `request_id`.
    YesForReads,
    Maybe,
}

/// One value per `agent_code` of spec section 10, in table order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentError {
    Unauthorized,
    Forbidden,
    PolicyDenied {
        policy: &'static str,
    },
    NotFound {
        refs: Vec<Ref>,
    },
    BadRequest {
        field: &'static str,
    },
    UnsupportedOp,
    UnknownRef {
        refs: Vec<Ref>,
    },
    AmbiguousRef {
        /// Shortest unique candidates.
        refs: Vec<Ref>,
    },
    StaleBase {
        target: Ref,
        current: ShortHash,
    },
    MatchCount {
        found: u64,
        expected: u64,
        refs: Vec<Ref>,
    },
    NoMatch,
    NoChange,
    UnsupportedStructure,
    RichBlock,
    RedactedBlock {
        refs: Vec<Ref>,
    },
    Protected,
    LimitExceeded {
        field: &'static str,
        limit: u64,
        value: u64,
    },
    IdempotencyConflict,
    ProposalClosed,
    Conflict {
        refs: Vec<Ref>,
    },
    BudgetExceeded {
        retry_after: u64,
    },
    RateLimited {
        retry_after: u64,
    },
    Busy,
    DeadlineExceeded,
    StorageLimit,
    Internal,
}

/// Every `agent_code`, in the order of the spec table (used by conformance tests).
pub const ALL_CODES: [&str; 26] = [
    "unauthorized",
    "forbidden",
    "policy_denied",
    "not_found",
    "bad_request",
    "unsupported_op",
    "unknown_ref",
    "ambiguous_ref",
    "stale_base",
    "match_count",
    "no_match",
    "no_change",
    "unsupported_structure",
    "rich_block",
    "redacted_block",
    "protected",
    "limit_exceeded",
    "idempotency_conflict",
    "proposal_closed",
    "conflict",
    "budget_exceeded",
    "rate_limited",
    "busy",
    "deadline_exceeded",
    "storage_limit",
    "internal",
];

impl AgentError {
    /// The fine-grained `details.agent_code` and the `code=` of the DCT error line.
    pub fn agent_code(&self) -> &'static str {
        match self {
            Self::Unauthorized => "unauthorized",
            Self::Forbidden => "forbidden",
            Self::PolicyDenied { .. } => "policy_denied",
            Self::NotFound { .. } => "not_found",
            Self::BadRequest { .. } => "bad_request",
            Self::UnsupportedOp => "unsupported_op",
            Self::UnknownRef { .. } => "unknown_ref",
            Self::AmbiguousRef { .. } => "ambiguous_ref",
            Self::StaleBase { .. } => "stale_base",
            Self::MatchCount { .. } => "match_count",
            Self::NoMatch => "no_match",
            Self::NoChange => "no_change",
            Self::UnsupportedStructure => "unsupported_structure",
            Self::RichBlock => "rich_block",
            Self::RedactedBlock { .. } => "redacted_block",
            Self::Protected => "protected",
            Self::LimitExceeded { .. } => "limit_exceeded",
            Self::IdempotencyConflict => "idempotency_conflict",
            Self::ProposalClosed => "proposal_closed",
            Self::Conflict { .. } => "conflict",
            Self::BudgetExceeded { .. } => "budget_exceeded",
            Self::RateLimited { .. } => "rate_limited",
            Self::Busy => "busy",
            Self::DeadlineExceeded => "deadline_exceeded",
            Self::StorageLimit => "storage_limit",
            Self::Internal => "internal",
        }
    }

    /// The existing HTTP `code` (`crates/engine-api/src/error.rs`) this maps to.
    pub fn code(&self) -> &'static str {
        match self {
            Self::Unauthorized => "unauthorized",
            Self::Forbidden
            | Self::PolicyDenied { .. }
            | Self::RedactedBlock { .. } => "forbidden",
            Self::NotFound { .. } => "not_found",
            Self::BadRequest { .. }
            | Self::UnsupportedOp
            | Self::UnknownRef { .. }
            | Self::AmbiguousRef { .. }
            | Self::NoChange
            | Self::UnsupportedStructure
            | Self::RichBlock
            | Self::LimitExceeded { .. } => "bad_request",
            Self::StaleBase { .. }
            | Self::MatchCount { .. }
            | Self::NoMatch
            | Self::Protected
            | Self::IdempotencyConflict
            | Self::ProposalClosed
            | Self::Conflict { .. } => "conflict",
            Self::BudgetExceeded { .. } | Self::RateLimited { .. } => "rate_limited",
            Self::Busy => "busy",
            Self::DeadlineExceeded => "deadline_exceeded",
            Self::StorageLimit => "storage_limit",
            Self::Internal => "internal",
        }
    }

    /// HTTP status. `limit_exceeded` is 413 for byte sizes and 400 for counts
    /// (the spec table says "413 or 400"; this crate decides by `field`).
    pub fn http_status(&self) -> u16 {
        match self {
            Self::Unauthorized => 401,
            Self::Forbidden | Self::PolicyDenied { .. } | Self::RedactedBlock { .. } => 403,
            Self::NotFound { .. } => 404,
            Self::BadRequest { .. }
            | Self::UnsupportedOp
            | Self::UnknownRef { .. }
            | Self::AmbiguousRef { .. }
            | Self::NoChange
            | Self::UnsupportedStructure
            | Self::RichBlock => 400,
            Self::LimitExceeded { field, .. } => {
                if matches!(*field, "max_bytes" | "body" | "text") {
                    413
                } else {
                    400
                }
            }
            Self::StaleBase { .. }
            | Self::MatchCount { .. }
            | Self::NoMatch
            | Self::Protected
            | Self::IdempotencyConflict
            | Self::ProposalClosed
            | Self::Conflict { .. } => 409,
            Self::BudgetExceeded { .. } | Self::RateLimited { .. } => 429,
            Self::Busy => 503,
            Self::DeadlineExceeded => 504,
            Self::StorageLimit => 507,
            Self::Internal => 500,
        }
    }

    pub fn retry(&self) -> Retry {
        match self {
            Self::Unauthorized => Retry::AfterReauth,
            Self::BadRequest { .. }
            | Self::AmbiguousRef { .. }
            | Self::MatchCount { .. }
            | Self::LimitExceeded { .. } => Retry::AfterFixing,
            Self::UnknownRef { .. } | Self::StaleBase { .. } => Retry::AfterRead,
            Self::BudgetExceeded { .. } | Self::RateLimited { .. } => Retry::AfterDelay,
            Self::Busy => Retry::Yes,
            Self::DeadlineExceeded => Retry::YesForReads,
            Self::Internal => Retry::Maybe,
            _ => Retry::No,
        }
    }

    /// The one fixed hint sentence for this code. Contains no document text.
    pub fn hint(&self) -> &'static str {
        match self {
            Self::Unauthorized => "Credential missing, expired or revoked; sign in again.",
            Self::Forbidden => "No access to this file or scope.",
            Self::PolicyDenied { .. } => "Project policy does not allow this call.",
            Self::NotFound { .. } => "File, proposal or block not visible.",
            Self::BadRequest { .. } => "Fix the named field and retry.",
            Self::UnsupportedOp => "This server version does not know that op.",
            Self::UnknownRef { .. } => "Ref does not resolve at this rev; call outline.",
            Self::AmbiguousRef { .. } => "Ref matches several blocks; use a listed candidate.",
            Self::StaleBase { .. } => "The block changed after you read it; read it again.",
            Self::MatchCount { .. } => "Match count differs from expect; adjust find or expect.",
            Self::NoMatch => "The find text is not in the block.",
            Self::NoChange => "The result equals the current text.",
            Self::UnsupportedStructure => "This operation is not available on this node type.",
            Self::RichBlock => "The block holds inline objects this op would destroy.",
            Self::RedactedBlock { .. } => "A data-loss rule withholds this block.",
            Self::Protected => "The sheet is protected.",
            Self::LimitExceeded { .. } => "A size or count limit was exceeded.",
            Self::IdempotencyConflict => "This request id was used with different content.",
            Self::ProposalClosed => "The proposal is merged, declined or withdrawn.",
            Self::Conflict { .. } => "The edit overlaps a newer change; see status.",
            Self::BudgetExceeded { .. } => "Budget used up; retry after the delay.",
            Self::RateLimited { .. } => "Too many requests; retry after the delay.",
            Self::Busy => "Server busy; retry.",
            Self::DeadlineExceeded => "Deadline exceeded; retry reads, retry writes with the same request id.",
            Self::StorageLimit => "Shared history budget is full.",
            Self::Internal => "Internal error; detail is logged, not returned.",
        }
    }

    /// `dd/1 error code=<agent_code> ...` (one line, no body).
    pub fn to_dct(&self) -> String {
        let mut h = HeaderBuilder::new(Tool::Error).kv("code", self.agent_code());
        let join = |refs: &[Ref]| {
            refs.iter()
                .map(Ref::as_str)
                .collect::<Vec<_>>()
                .join(",")
        };
        match self {
            Self::PolicyDenied { policy } => h = h.kv("policy", policy),
            Self::BadRequest { field } => h = h.kv("field", field),
            Self::NotFound { refs }
            | Self::UnknownRef { refs }
            | Self::AmbiguousRef { refs }
            | Self::RedactedBlock { refs }
            | Self::Conflict { refs } => {
                if !refs.is_empty() {
                    h = h.kv("refs", join(refs));
                }
            }
            Self::StaleBase { target, current } => {
                h = h.kv("ref", target.as_str()).kv("current", current.as_str());
            }
            Self::MatchCount {
                found,
                expected,
                refs,
            } => {
                h = h.kv("found", found).kv("expected", expected);
                if !refs.is_empty() {
                    h = h.kv("refs", join(refs));
                }
            }
            Self::LimitExceeded {
                field,
                limit,
                value,
            } => h = h.kv("field", field).kv("limit", limit).kv("value", value),
            Self::BudgetExceeded { retry_after } | Self::RateLimited { retry_after } => {
                h = h.kv("retry_after", retry_after);
            }
            _ => {}
        }
        h.kv_quoted("hint", self.hint()).finish_line()
    }
}

impl std::fmt::Display for AgentError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.agent_code(), self.hint())
    }
}

impl std::error::Error for AgentError {}
