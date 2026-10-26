//! The places the client can work against, named by an `env` string that the page passes with each
//! request: `server` (the one configured server and the web view's own session, as in 0.0.1a),
//! `local` (the workspace on this computer, no account) and `acct:<id>` (a server the person
//! signed in to from the app, with the session kept in the credential store).
//!
//! Every request, whichever place it is for, must be one of the fixed route shapes in `remote.rs`:
//! the page cannot name an address, a header or a port.

use crate::remote::{self, Method, Reply, Target};
#[cfg(desktop)]
use crate::AppState;
#[cfg(desktop)]
use tauri::Manager;
use serde_json::Value;
use tauri::AppHandle;

/// The largest request the client sends to the local engine (an imported file's operations).
pub const LOCAL_BODY_LIMIT: usize = 24 * 1024 * 1024;

/// Which place a request is for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Env {
    /// The configured server through the web view's session (0.0.1a).
    Server,
    Local,
    Account(String),
}

impl Env {
    pub fn parse(text: Option<&str>) -> Result<Self, String> {
        match text {
            None | Some("server") | Some("") => Ok(Self::Server),
            Some("local") => Ok(Self::Local),
            Some(t) => match t.strip_prefix("acct:") {
                Some(id) if crate::accounts::id_ok(id) => Ok(Self::Account(id.to_string())),
                _ => Err("That place is not available from the client.".to_string()),
            },
        }
    }
}

const NOT_AVAILABLE: &str = "That request is not available from the client.";

/// The size limit of a request body for a route. Uploads and batches of operations carry whole
/// files' worth of content; everything else is small.
pub fn body_limit(env: &Env, path: &str) -> usize {
    let big = path.ends_with("/batch")
        || path.ends_with("/uploads")
        || path.contains("/drafts/") && !path.ends_with("/submit") && !path.ends_with("/sync") && !path.ends_with("/withdraw");
    match (env, big) {
        (Env::Local, true) => LOCAL_BODY_LIMIT,
        (_, true) => 8 * 1024 * 1024,
        _ => remote::MAX_BODY_BYTES,
    }
}

/// One request for a place other than the configured server.
pub async fn call(
    app: &AppHandle,
    env: &Env,
    target: Target,
    method: Method,
    path: &str,
    body: Option<Value>,
    if_match: Option<&str>,
) -> Result<Reply, String> {
    remote::allowed(target, method, path).map_err(str::to_string)?;
    if let Some(value) = &body {
        if serde_json::to_vec(value).map_or(true, |b| b.len() > body_limit(env, path)) {
            return Err("That change is too large to send at once.".to_string());
        }
    }
    match env {
        Env::Server => Err(NOT_AVAILABLE.to_string()),
        #[cfg(desktop)]
        Env::Local => {
            let state = app.state::<AppState>();
            let local = &state.local;
            if !local.is_ready() {
                return Err("The local workspace is still starting.".to_string());
            }
            match target {
                Target::Api => local
                    .api(method, path, body.as_ref(), if_match)
                    .await
                    .map_err(|e| e.to_string()),
                Target::Svc => Ok(local.svc(method, path, body.as_ref()).await),
            }
        }
        #[cfg(not(desktop))]
        Env::Local => Err("The local workspace is not available on phones.".to_string()),
        Env::Account(id) => crate::accounts::call(app, id, target, method, path, body, if_match).await,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn places_are_named_from_a_short_list() {
        assert_eq!(Env::parse(None), Ok(Env::Server));
        assert_eq!(Env::parse(Some("server")), Ok(Env::Server));
        assert_eq!(Env::parse(Some("local")), Ok(Env::Local));
        assert_eq!(
            Env::parse(Some("acct:srv_0123abcd")),
            Ok(Env::Account("srv_0123abcd".to_string()))
        );
        for bad in ["acct:", "acct:../x", "http://evil.example", "LOCAL", "acct:a b"] {
            assert!(Env::parse(Some(bad)).is_err(), "{bad}");
        }
    }

    #[test]
    fn only_batches_and_uploads_may_be_large() {
        assert_eq!(body_limit(&Env::Local, "documents/x/batch"), LOCAL_BODY_LIMIT);
        assert_eq!(body_limit(&Env::Local, "documents/x/uploads"), LOCAL_BODY_LIMIT);
        assert_eq!(body_limit(&Env::Local, "projects"), remote::MAX_BODY_BYTES);
        assert!(body_limit(&Env::Account("a".into()), "documents/x/batch") < LOCAL_BODY_LIMIT);
    }
}
