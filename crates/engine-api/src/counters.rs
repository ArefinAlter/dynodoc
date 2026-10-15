//! Error responses counted since this API process started, for the admin overview.
//! Each process counts its own; these are not fleet-wide totals.
use crate::error::ErrorCode;
use axum::{
    extract::{Request, State},
    middleware::Next,
    response::Response,
};
use serde_json::{Map, Value};
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};

const CODES: [&str; 5] = [
    "internal",
    "replay_failed",
    "conflict",
    "rate_limited",
    "busy",
];

#[derive(Clone, Default)]
pub struct Counters(Arc<[AtomicU64; CODES.len()]>);

impl Counters {
    pub fn snapshot(&self) -> Value {
        let counts: Map<String, Value> = CODES
            .iter()
            .zip(self.0.iter())
            .map(|(code, n)| (code.to_string(), n.load(Ordering::Relaxed).into()))
            .collect();
        Value::Object(counts)
    }
}

pub async fn count(State(counters): State<Counters>, request: Request, next: Next) -> Response {
    let response = next.run(request).await;
    if let Some(ErrorCode(code)) = response.extensions().get::<ErrorCode>() {
        if let Some(i) = CODES.iter().position(|c| c == code) {
            counters.0[i].fetch_add(1, Ordering::Relaxed);
        }
    }
    response
}
