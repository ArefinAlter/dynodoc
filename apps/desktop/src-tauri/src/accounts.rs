//! Servers the person signed in to from the app (the hosted Dynodoc, or one they run).
//! Filled in by the web sign-in work; this is the part `envs` needs.

use crate::remote::{Method, Reply, Target};
use serde_json::Value;
use tauri::AppHandle;

/// An account id: `srv_` and lower-case hex.
pub fn id_ok(id: &str) -> bool {
    id.len() >= 5
        && id.len() <= 40
        && id.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}

pub async fn call(
    _app: &AppHandle,
    _id: &str,
    _target: Target,
    _method: Method,
    _path: &str,
    _body: Option<Value>,
    _if_match: Option<&str>,
) -> Result<Reply, String> {
    Err("Sign in to that server first.".to_string())
}
