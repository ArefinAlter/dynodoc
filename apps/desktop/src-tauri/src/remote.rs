//! The client's way to the configured Dynodoc server: the only network code in the app.
//!
//! The launcher page has no network of its own (`connect-src ipc:`, no `fetch`). When the client
//! shows a real workspace, it asks the app, through the commands in `commands.rs`, to make one
//! request to the CONFIGURED server (the sealed address in config.rs; never an address the page
//! names), on a path from the fixed tables below:
//!
//! - `api`: the workspace's engine proxy, `<server>/api/engine/<path>` (the same route the web
//!   workspace uses, so the server's own access rules apply unchanged);
//! - `svc`: the pre-alpha service, `<server>/svc/v1/<path>` (deploy/pre-alpha in this repository:
//!   what the client shows and the server has no place for, and visitor sandboxes).
//!
//! The session is the web view's own session cookie for that server (`dynodoc_session`, set when
//! the person signs in in this window). It is read in Rust for each request and never reaches the
//! page; a renewed cookie the server sends back is written back to the web view. Every request
//! carries the app's User-Agent token (`signin::app_token`), never follows a redirect, has a time
//! limit, and the answer is size-limited and must be JSON. Nothing here logs a cookie, a token, a
//! body or a query string.

use crate::server::ServerOrigin;
use serde::Serialize;
use serde_json::Value;
use std::sync::OnceLock;
use std::time::Duration;
use url::Url;

/// The session cookie of the Dynodoc web workspace (frontend lib/server/session.ts).
pub const SESSION_COOKIE: &str = "dynodoc_session";
/// Request bodies the client may send (a large batch of edits stays well under this).
pub const MAX_BODY_BYTES: usize = 512 * 1024;
/// Answers larger than this are refused (a long event history is paged by the client).
pub const MAX_ANSWER_BYTES: usize = 8 * 1024 * 1024;
/// Client preferences on disk.
pub const MAX_PREFS_BYTES: usize = 256 * 1024;
pub const PREFS_FILE: &str = "client-settings.json";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    /// `<server>/api/engine/<path>`
    Api,
    /// `<server>/svc/v1/<path>`
    Svc,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    Get,
    Post,
    Put,
    Patch,
    Delete,
}

impl Method {
    pub fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "GET" => Self::Get,
            "POST" => Self::Post,
            "PUT" => Self::Put,
            "PATCH" => Self::Patch,
            "DELETE" => Self::Delete,
            _ => return None,
        })
    }
    fn as_reqwest(self) -> reqwest::Method {
        match self {
            Self::Get => reqwest::Method::GET,
            Self::Post => reqwest::Method::POST,
            Self::Put => reqwest::Method::PUT,
            Self::Patch => reqwest::Method::PATCH,
            Self::Delete => reqwest::Method::DELETE,
        }
    }
}

/// Segment classes of the route tables: `{u}` a UUID, `{n}` a decimal number, `{i}` an opaque id
/// of letters, digits, `-` and `_` (ULIDs, draft and link ids); anything else is literal.
fn segment_ok(pattern: &str, segment: &str) -> bool {
    match pattern {
        "{u}" => {
            segment.len() == 36
                && segment.char_indices().all(|(i, c)| {
                    if [8, 13, 18, 23].contains(&i) {
                        c == '-'
                    } else {
                        c.is_ascii_hexdigit() && !c.is_ascii_uppercase()
                    }
                })
        }
        "{n}" => {
            !segment.is_empty() && segment.len() <= 9 && segment.bytes().all(|b| b.is_ascii_digit())
        }
        "{i}" => {
            !segment.is_empty()
                && segment.len() <= 64
                && segment
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        }
        literal => literal == segment,
    }
}

/// What the client may read from the engine proxy.
const API_GET: &[&str] = &[
    "projects",
    "projects/{u}",
    "projects/{u}/files",
    "projects/{u}/change-requests",
    "projects/{u}/drafts",
    "projects/{u}/labels",
    "projects/{u}/issues",
    "projects/{u}/issues/{n}",
    "projects/{u}/activity",
    "projects/{u}/agent-policy",
    "projects/{u}/contributors",
    "spaces",
    "spaces/{u}/members",
    "documents/{u}",
    "documents/{u}/state",
    "documents/{u}/events",
    "documents/{u}/drafts",
    "documents/{u}/drafts/{u}",
    "documents/{u}/change-requests",
    "documents/{u}/change-requests/{u}",
    "documents/{u}/versions",
    "documents/{u}/history-points",
    "documents/{u}/discussions",
    "documents/{u}/public-links",
    "documents/{u}/people",
    "documents/{u}/connectors",
    "documents/{u}/copies",
    "documents/{u}/verify",
    "documents/{u}/uploads",
    "documents/{u}/conflicts",
    "documents/{u}/members",
    "documents/{u}/access",
    "notifications",
    "notifications/count",
    "profile",
    "devices",
    "workspace/index",
];

/// What the client may change through the engine proxy, by method.
const API_SEND: &[(Method, &str)] = &[
    (Method::Post, "projects"),
    (Method::Post, "projects/{u}/settings"),
    (Method::Post, "projects/{u}/watch"),
    (Method::Put, "projects/{u}/star"),
    (Method::Delete, "projects/{u}/star"),
    (Method::Put, "projects/{u}/agent-policy"),
    (Method::Post, "projects/{u}/agent-policy/kill"),
    (Method::Post, "projects/{u}/issues"),
    (Method::Patch, "projects/{u}/issues/{n}"),
    (Method::Post, "projects/{u}/issues/{n}/comments"),
    (Method::Post, "projects/{u}/labels"),
    (Method::Delete, "projects/{u}/labels/{u}"),
    (Method::Post, "spaces/{u}/members"),
    (Method::Post, "documents"),
    (Method::Post, "documents/{u}/metadata"),
    (Method::Post, "documents/{u}/uploads"),
    (Method::Post, "documents/{u}/location"),
    (Method::Post, "documents/{u}/change-requests"),
    (Method::Post, "documents/{u}/versions/{u}/restore"),
    (Method::Post, "documents/{u}/conflicts/{u}/resolve"),
    (Method::Post, "documents/{u}/batch"),
    (Method::Post, "documents/{u}/drafts"),
    (Method::Post, "documents/{u}/drafts/{u}"),
    (Method::Post, "documents/{u}/drafts/{u}/submit"),
    (Method::Post, "documents/{u}/drafts/{u}/sync"),
    (Method::Post, "documents/{u}/drafts/{u}/withdraw"),
    (Method::Post, "documents/{u}/change-requests/{u}/reviews"),
    (Method::Post, "documents/{u}/change-requests/{u}/merge"),
    (Method::Post, "documents/{u}/change-requests/{u}/decline"),
    (Method::Post, "documents/{u}/versions"),
    (Method::Post, "documents/{u}/discussions/{i}"),
    (Method::Post, "documents/{u}/ops/comment"),
    (Method::Post, "documents/{u}/public-links"),
    (Method::Post, "documents/{u}/public-links/{i}/revoke"),
    (Method::Post, "documents/{u}/connectors/{i}/revoke"),
    (Method::Post, "notifications/read"),
    (Method::Post, "profile"),
    (Method::Post, "devices/{i}/revoke"),
];

/// The pre-alpha service's routes (deploy/pre-alpha/service/svc.mjs).
const SVC_GET: &[&str] = &[
    "session",
    "state/working",
    "state/local",
    "state/threads",
    "state/client",
    "notes",
];
const SVC_SEND: &[(Method, &str)] = &[
    (Method::Put, "state/working"),
    (Method::Put, "state/local"),
    (Method::Put, "state/threads"),
    (Method::Put, "state/client"),
    (Method::Post, "notes"),
];

/// Query parameter names the client sends, with short plain values.
const QUERY_NAMES: &[&str] = &[
    "cursor",
    "limit",
    "offset",
    "from_seq",
    "to_seq",
    "through_seq",
    "before",
    "state",
    "unread",
    "status",
    "document",
];
const MAX_QUERY_PARAMS: usize = 60;

fn matches(pattern: &str, path: &str) -> bool {
    let p: Vec<&str> = pattern.split('/').collect();
    let s: Vec<&str> = path.split('/').collect();
    p.len() == s.len() && p.iter().zip(&s).all(|(p, s)| segment_ok(p, s))
}

fn query_ok(query: &str) -> bool {
    if query.is_empty() {
        return true;
    }
    let pairs: Vec<&str> = query.split('&').collect();
    pairs.len() <= MAX_QUERY_PARAMS
        && pairs.iter().all(|pair| {
            let Some((name, value)) = pair.split_once('=') else {
                return false;
            };
            QUERY_NAMES.contains(&name)
                && value.len() <= 64
                && value
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"-_.:".contains(&b))
        })
}

/// Whether the client may make this request: the path is one of the table's shapes (no `..`,
/// no encoded characters, no empty segment, nothing but the listed query parameters).
pub fn allowed(target: Target, method: Method, path_and_query: &str) -> Result<(), &'static str> {
    if path_and_query.len() > 2048 {
        return Err("That request is not available from the client.");
    }
    let (path, query) = path_and_query
        .split_once('?')
        .unwrap_or((path_and_query, ""));
    if !query_ok(query) || path_and_query.ends_with('?') {
        return Err("That request is not available from the client.");
    }
    let ok = match (target, method) {
        (Target::Api, Method::Get) => API_GET.iter().any(|p| matches(p, path)),
        (Target::Api, m) => API_SEND.iter().any(|(pm, p)| *pm == m && matches(p, path)),
        (Target::Svc, Method::Get) => SVC_GET.iter().any(|p| matches(p, path)),
        (Target::Svc, m) => SVC_SEND.iter().any(|(pm, p)| *pm == m && matches(p, path)),
    };
    if ok {
        Ok(())
    } else {
        Err("That request is not available from the client.")
    }
}

/// The address on the configured server. `allowed` must have passed.
pub fn address(server: &ServerOrigin, target: Target, path_and_query: &str) -> Option<Url> {
    let prefix = match target {
        Target::Api => "/api/engine/",
        Target::Svc => "/svc/v1/",
    };
    let url = server.join(&format!("{prefix}{path_and_query}"))?;
    // The join may not leave the server or the prefix (it cannot with the tables, checked again).
    (server.matches(&url) && url.path().starts_with(prefix)).then_some(url)
}

/// What the page gets back: the HTTP status and the JSON answer (or null).
#[derive(Debug, Serialize)]
pub struct Reply {
    pub status: u16,
    pub body: Value,
}

/// A transport failure, in the person's words (no address, no detail).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Failure {
    Unreachable,
    TooLarge,
    NotJson,
    Redirected,
}

impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Unreachable => {
                "The server could not be reached. Check the connection and try again."
            }
            Self::TooLarge => "The server's answer was too large.",
            Self::NotJson => "The server answered with something the client cannot read.",
            Self::Redirected => "The server asked to go elsewhere; the client does not follow.",
        })
    }
}

fn client() -> Option<&'static reqwest::Client> {
    static CLIENT: OnceLock<Option<reqwest::Client>> = OnceLock::new();
    CLIENT
        .get_or_init(|| {
            // One crypto provider for the whole process (ring); installing twice is harmless.
            let _ = rustls::crypto::ring::default_provider().install_default();
            reqwest::Client::builder()
                .user_agent(crate::signin::app_token())
                .redirect(reqwest::redirect::Policy::none())
                .connect_timeout(Duration::from_secs(10))
                .timeout(Duration::from_secs(30))
                .https_only(false)
                .build()
                .ok()
        })
        .as_ref()
}

/// Whether the engine proxy needs the editor header for this path (it refuses document routes
/// from pages that do not name the current history format; this client reads that format).
fn needs_editor_header(target: Target, path: &str) -> bool {
    target == Target::Api && path.starts_with("documents/")
}

/// One request. `cookie` is the session cookie's value (None: not signed in). Returns the reply
/// and, when the server renewed or cleared the session, the new `Set-Cookie` value for it.
pub async fn send(
    server: &ServerOrigin,
    url: Url,
    target: Target,
    method: Method,
    cookie: Option<&str>,
    body: Option<&Value>,
    if_match: Option<&str>,
) -> Result<(Reply, Option<String>), Failure> {
    let http = client().ok_or(Failure::Unreachable)?;
    let path = url.path().to_string();
    let mut request = http
        .request(method.as_reqwest(), url)
        .header("accept", "application/json");
    if let Some(value) = cookie {
        request = request.header("cookie", format!("{SESSION_COOKIE}={value}"));
    }
    if method != Method::Get {
        request = request.header("origin", server.as_text());
    }
    let relative = path
        .strip_prefix("/api/engine/")
        .or_else(|| path.strip_prefix("/svc/v1/"))
        .unwrap_or("");
    if needs_editor_header(target, relative) {
        request = request.header("x-dynodoc-editor", "office-v5");
    }
    if let Some(tag) = if_match {
        request = request.header("if-match", tag);
    }
    if let Some(value) = body {
        request = request
            .header("content-type", "application/json")
            .body(serde_json::to_vec(value).map_err(|_| Failure::NotJson)?);
    }
    let mut response = request.send().await.map_err(|_| Failure::Unreachable)?;
    let status = response.status();
    if status.is_redirection() {
        return Err(Failure::Redirected);
    }
    let renewed = session_set_cookie(response.headers());
    let mut bytes: Vec<u8> = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| Failure::Unreachable)? {
        if bytes.len() + chunk.len() > MAX_ANSWER_BYTES {
            return Err(Failure::TooLarge);
        }
        bytes.extend_from_slice(&chunk);
    }
    let body = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).map_err(|_| Failure::NotJson)?
    };
    Ok((
        Reply {
            status: status.as_u16(),
            body,
        },
        renewed,
    ))
}

/// The `Set-Cookie` header that sets (or clears) the session cookie, if there is one.
pub fn session_set_cookie(headers: &reqwest::header::HeaderMap) -> Option<String> {
    headers
        .get_all("set-cookie")
        .iter()
        .filter_map(|v| v.to_str().ok())
        .find(|v| {
            v.split(';')
                .next()
                .and_then(|pair| pair.split_once('='))
                .is_some_and(|(name, _)| name.trim() == SESSION_COOKIE)
        })
        .map(str::to_string)
}

/// The cookie to store in the web view for `Set-Cookie: dynodoc_session=...`, bound to the
/// configured server's host and the whole site, `Secure` on https and always `HttpOnly`.
/// Returns (cookie, cleared): a cleared session is deleted rather than stored.
pub fn web_view_cookie(
    server: &ServerOrigin,
    set_cookie: &str,
) -> Option<(tauri::webview::Cookie<'static>, bool)> {
    let parsed = tauri::webview::Cookie::parse(set_cookie.to_string()).ok()?;
    if parsed.name() != SESSION_COOKIE {
        return None;
    }
    let root = server.root();
    let host = root.host_str()?.to_string();
    let cleared =
        parsed.value().is_empty() || parsed.max_age().is_some_and(|age| age.whole_seconds() <= 0);
    let mut cookie = tauri::webview::Cookie::build((SESSION_COOKIE, parsed.value().to_string()))
        .domain(host)
        .path("/")
        .http_only(true)
        .secure(root.scheme() == "https")
        .same_site(tauri::webview::cookie::SameSite::Lax);
    if let Some(age) = parsed.max_age() {
        cookie = cookie.max_age(age);
    }
    Some((cookie.build(), cleared))
}

/// The session cookie's value among the web view's cookies for the server.
pub fn session_value(cookies: &[tauri::webview::Cookie<'static>]) -> Option<String> {
    cookies
        .iter()
        .find(|c| c.name() == SESSION_COOKIE && !c.value().is_empty())
        .map(|c| c.value().to_string())
}

/// Client preferences: a JSON object, bounded, in the app's config folder.
pub fn prefs_path(config_dir: &std::path::Path) -> std::path::PathBuf {
    config_dir.join(PREFS_FILE)
}

pub fn load_prefs(config_dir: &std::path::Path) -> Value {
    let path = prefs_path(config_dir);
    let Ok(meta) = std::fs::metadata(&path) else {
        return Value::Null;
    };
    if !meta.is_file() || meta.len() > MAX_PREFS_BYTES as u64 {
        return Value::Null;
    }
    std::fs::read(&path)
        .ok()
        .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
        .filter(Value::is_object)
        .unwrap_or(Value::Null)
}

pub fn save_prefs(config_dir: &std::path::Path, value: &Value) -> Result<(), &'static str> {
    if !value.is_object() {
        return Err("Preferences must be an object.");
    }
    let bytes = serde_json::to_vec_pretty(value).map_err(|_| "Preferences could not be saved.")?;
    if bytes.len() > MAX_PREFS_BYTES {
        return Err("Preferences are too large to save.");
    }
    std::fs::create_dir_all(config_dir).map_err(|_| "Preferences could not be saved.")?;
    let path = prefs_path(config_dir);
    let temporary = path.with_extension("json.tmp");
    std::fs::write(&temporary, &bytes).map_err(|_| "Preferences could not be saved.")?;
    std::fs::rename(&temporary, &path).map_err(|_| "Preferences could not be saved.")
}

/// A preference never holds a provider key: the client keeps those in memory only. A value
/// that looks like one is refused, so a mistake in the page cannot write a key to disk.
pub fn looks_like_a_key(value: &Value) -> bool {
    match value {
        Value::String(s) => {
            let t = s.trim();
            ["sk-", "sk_", "AIza", "xai-", "gsk_", "hf_"]
                .iter()
                .any(|p| t.starts_with(p) && t.len() >= 20)
                || t.contains("ZEUS-CANARY-")
                || t.contains("DYNODOC-CANARY-")
        }
        Value::Array(list) => list.iter().any(looks_like_a_key),
        Value::Object(map) => map.iter().any(|(k, v)| {
            let k = k.to_ascii_lowercase();
            (k == "apikey" || k == "api_key" || k == "secret" || k == "token" || k == "password")
                && !v.is_null()
                && v != &Value::String(String::new())
                || looks_like_a_key(v)
        }),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::parse_server_address;

    fn api(m: Method, p: &str) -> bool {
        allowed(Target::Api, m, p).is_ok()
    }

    const U: &str = "0190f5b2-1c2d-7000-8000-00000000abcd";

    #[test]
    fn the_tables_allow_the_client_routes_and_nothing_near_them() {
        assert!(api(Method::Get, "projects"));
        assert!(api(Method::Get, &format!("projects/{U}/files?offset=100")));
        assert!(api(
            Method::Get,
            &format!("documents/{U}/events?from_seq=1&limit=1000")
        ));
        assert!(api(Method::Get, &format!("documents/{U}/drafts/{U}")));
        assert!(api(Method::Post, &format!("documents/{U}/batch")));
        assert!(api(Method::Put, &format!("projects/{U}/star")));
        assert!(api(Method::Delete, &format!("projects/{U}/star")));
        assert!(api(Method::Patch, &format!("projects/{U}/issues/12")));
        assert!(api(
            Method::Post,
            &format!("documents/{U}/discussions/01JABCDEF0123456789ABCDEFG")
        ));
        for refused in [
            String::new(),
            "auth/me".into(),
            "auth/logout".into(),
            "admin/overview".into(),
            "admin/erase".into(),
            format!("projects/{U}/../admin"),
            format!("projects/{U}/files/"),
            format!("projects//{U}"),
            format!("projects/{U}%2Ffiles"),
            format!("projects/{}", U.to_uppercase()),
            "projects/not-a-uuid".into(),
            format!("documents/{U}/uploads/{U}"),
            format!("documents/{U}/stream"),
            format!("projects/{U}/files?token=abc"),
            format!("projects/{U}/files?offset=1#x"),
            format!("projects/{U}/files?offset=a/b"),
            format!("projects/{U}/files?offset"),
            "https://evil.example/projects".into(),
            "//evil.example/projects".into(),
            "projects?".into(),
        ] {
            assert!(!api(Method::Get, &refused), "GET {refused}");
        }
        // A read-only route cannot be written, and a write route needs its own method.
        assert!(!api(Method::Post, &format!("documents/{U}/events")));
        assert!(!api(Method::Delete, &format!("documents/{U}/batch")));
        assert!(!api(Method::Get, &format!("documents/{U}/batch")));
        assert!(!api(Method::Post, &format!("documents/{U}/trash")));
        assert!(!api(Method::Post, &format!("documents/{U}/owner")));
        assert!(!api(Method::Post, "devices"));
    }

    #[test]
    fn the_service_tables_are_separate_and_small() {
        assert!(allowed(Target::Svc, Method::Get, "session").is_ok());
        assert!(allowed(Target::Svc, Method::Put, "state/working").is_ok());
        assert!(allowed(
            Target::Svc,
            Method::Get,
            &format!("notes?document={U}&document={U}")
        )
        .is_ok());
        assert!(allowed(Target::Svc, Method::Post, "notes").is_ok());
        for refused in [
            "visitor",
            "state/secrets",
            "state",
            "../visitor/start",
            "healthz",
        ] {
            assert!(
                allowed(Target::Svc, Method::Get, refused).is_err(),
                "{refused}"
            );
            assert!(
                allowed(Target::Svc, Method::Post, refused).is_err(),
                "{refused}"
            );
        }
        // The engine's routes are not the service's, and the other way round.
        assert!(allowed(Target::Svc, Method::Get, "projects").is_err());
        assert!(allowed(Target::Api, Method::Get, "session").is_err());
    }

    #[test]
    fn addresses_stay_on_the_configured_server_under_the_prefix() {
        let server = parse_server_address("https://dynodoc.example.org").unwrap();
        let url = address(
            &server,
            Target::Api,
            &format!("projects/{U}/files?offset=0"),
        )
        .unwrap();
        assert_eq!(
            url.as_str(),
            format!("https://dynodoc.example.org/api/engine/projects/{U}/files?offset=0")
        );
        let svc = address(&server, Target::Svc, "state/working").unwrap();
        assert_eq!(
            svc.as_str(),
            "https://dynodoc.example.org/svc/v1/state/working"
        );
    }

    #[test]
    fn only_the_session_cookie_is_taken_and_it_is_bound_to_the_server() {
        let server = parse_server_address("https://dynodoc.example.org").unwrap();
        let (cookie, cleared) = web_view_cookie(
            &server,
            "dynodoc_session=abc.def; Path=/; Max-Age=2592000; HttpOnly; SameSite=Lax; Secure; Domain=evil.example",
        )
        .unwrap();
        assert!(!cleared);
        assert_eq!(cookie.domain(), Some("dynodoc.example.org"));
        assert_eq!(cookie.path(), Some("/"));
        assert_eq!(cookie.http_only(), Some(true));
        assert_eq!(cookie.secure(), Some(true));
        assert_eq!(cookie.value(), "abc.def");
        let (_, cleared) = web_view_cookie(&server, "dynodoc_session=; Path=/; Max-Age=0").unwrap();
        assert!(cleared);
        assert!(web_view_cookie(&server, "other=1; Path=/").is_none());
        let mut headers = reqwest::header::HeaderMap::new();
        headers.append("set-cookie", "a=1; Path=/".parse().unwrap());
        headers.append("set-cookie", "dynodoc_session=x; Path=/".parse().unwrap());
        assert_eq!(
            session_set_cookie(&headers).as_deref(),
            Some("dynodoc_session=x; Path=/")
        );
    }

    #[test]
    fn preferences_are_a_bounded_object_without_keys() {
        let dir = std::env::temp_dir().join(format!("dynodoc-prefs-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(load_prefs(&dir), Value::Null);
        let value = serde_json::json!({ "theme": "dark", "bindings": [] });
        save_prefs(&dir, &value).unwrap();
        assert_eq!(load_prefs(&dir), value);
        assert!(save_prefs(&dir, &serde_json::json!([1, 2])).is_err());
        let big = serde_json::json!({ "x": "y".repeat(MAX_PREFS_BYTES) });
        assert!(save_prefs(&dir, &big).is_err());
        std::fs::write(prefs_path(&dir), b"not json").unwrap();
        assert_eq!(load_prefs(&dir), Value::Null);
        let _ = std::fs::remove_dir_all(&dir);

        assert!(looks_like_a_key(
            &serde_json::json!({ "ladder": [{ "apiKey": "abc" }] })
        ));
        assert!(looks_like_a_key(
            &serde_json::json!({ "x": "sk-ant-0123456789abcdefghij" })
        ));
        assert!(looks_like_a_key(
            &serde_json::json!({ "x": "ZEUS-CANARY-1" })
        ));
        assert!(!looks_like_a_key(
            &serde_json::json!({ "keyHint": "4f2a", "theme": "dark", "apiKey": "" })
        ));
    }
}
