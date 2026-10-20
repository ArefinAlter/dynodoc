//! Tool calls as HTTP requests to the agent routes (protocol spec section 12).
//!
//! [`HttpBackend`] turns a validated tool call into the request [`crate::request::plan`]
//! describes, adds the caller's bearer token, and turns the answer into an MCP tool result:
//! the DCT body verbatim for a success, one `dd/1 error ...` line (with `isError`) for every
//! failure. The API is the only authorisation point; nothing is cached or decided here.
//!
//! Safety properties, each pinned by a test:
//!
//! * The token is added to the request header (marked sensitive) and appears nowhere else:
//!   not in a URL, a log line, a tool result or an error.
//! * Redirects are not followed and proxies are not discovered, so the token goes to the
//!   configured origin only.
//! * A `401` renews the session at most once per call, then the call fails with
//!   `unauthorized`.
//! * Every wait is bounded: connect and per-request timeouts, one deadline for the whole
//!   call, and a byte cap on every response body.
//! * Nothing the server wrote is passed on except the DCT body of a `200` whose content type
//!   is `text/vnd.dynodoc.dct` and which starts with `dd/1 `. Errors are re-derived from
//!   typed fields ([`crate::error`]).

use std::{fmt, future::Future, net::IpAddr, time::Duration};

use reqwest::{
    header::{HeaderValue, ACCEPT, AUTHORIZATION, CONTENT_TYPE, ETAG, RETRY_AFTER},
    Client, StatusCode, Url,
};
use serde_json::{json, Map, Value};
use uuid::Uuid;

use crate::{
    error::ToolError,
    request::{self, Method, Plan},
    session::{Session, SessionError, Token},
    tools::Tool,
};

/// The media type the MCP server asks for (spec 12.2).
pub const DCT_ACCEPT: &str = "text/vnd.dynodoc.dct; v=1";

/// A validated tool call.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolCall {
    pub tool: Tool,
    pub args: Map<String, Value>,
}

/// What the backend may say about its caller.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallContext {
    /// Self-description of the MCP client, sent as `X-Dynodoc-Client`. A claim, never
    /// used for authorisation (spec 12.2).
    pub client: String,
}

/// A tool result: text for the model and machine fields for programmatic clients.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolOutput {
    pub text: String,
    pub is_error: bool,
    /// `{"dynodoc/v1": {rev, chain, file, cursor, etag}}` when the answer carries them.
    pub meta: Option<Value>,
}

impl ToolOutput {
    pub fn error(error: &ToolError) -> Self {
        Self {
            text: error.to_dct(),
            is_error: true,
            meta: None,
        }
    }
}

/// Where tool calls go.
pub trait Backend: Send + Sync + 'static {
    fn call(&self, ctx: CallContext, call: ToolCall) -> impl Future<Output = ToolOutput> + Send;
}

/// A configuration the backend refuses to run with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConfigError(&'static str);

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}

impl std::error::Error for ConfigError {}

/// The API base: HTTPS (plain HTTP only for a literal loopback address and only when
/// `allow_loopback_http`, for tests), no credentials, query or fragment, and a path that
/// ends with `/` so that relative routes join under it. The same rules `engine-cli sync`
/// applies to its remote.
pub fn parse_base(url: &str, allow_loopback_http: bool) -> Result<Url, ConfigError> {
    let base = Url::parse(url).map_err(|_| ConfigError("the remote URL is not a valid URL"))?;
    let loopback = base.host_str().is_some_and(|host| {
        host.trim_matches(['[', ']'])
            .parse::<IpAddr>()
            .is_ok_and(|ip| ip.is_loopback())
    });
    if !(base.scheme() == "https" || (allow_loopback_http && loopback && base.scheme() == "http")) {
        return Err(ConfigError(
            "HTTPS is required (test HTTP needs a literal loopback IP and --allow-loopback-http)",
        ));
    }
    if !base.username().is_empty()
        || base.password().is_some()
        || base.query().is_some()
        || base.fragment().is_some()
    {
        return Err(ConfigError(
            "the remote URL must not contain credentials, a query or a fragment",
        ));
    }
    if !base.path().ends_with('/') {
        return Err(ConfigError("the remote API base must end with /"));
    }
    Ok(base)
}

/// Settings of an [`HttpBackend`].
#[derive(Debug, Clone)]
pub struct BackendConfig {
    /// From [`parse_base`].
    pub base: Url,
    /// The project this connection's session is for.
    pub project: Uuid,
    pub connect_timeout: Duration,
    /// One HTTP request, body included.
    pub request_timeout: Duration,
    /// One tool call: obtaining a token, the request, a renewal and the retry.
    pub call_deadline: Duration,
    /// Largest response body accepted. The server's own cap is 64,000 bytes for a read.
    pub max_response_bytes: usize,
}

impl BackendConfig {
    pub fn new(base: Url, project: Uuid) -> Self {
        Self {
            base,
            project,
            connect_timeout: Duration::from_secs(10),
            request_timeout: Duration::from_secs(30),
            call_deadline: Duration::from_secs(75),
            max_response_bytes: 256 * 1024,
        }
    }
}

/// Largest error body read; an error envelope is a few hundred bytes.
const ERROR_BODY_BYTES: usize = 64 * 1024;

/// Calls the agent routes with a [`Session`]'s token.
pub struct HttpBackend<S> {
    http: Client,
    config: BackendConfig,
    session: S,
}

impl<S: Session> HttpBackend<S> {
    pub fn new(config: BackendConfig, session: S) -> Result<Self, ConfigError> {
        let http = Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .no_proxy()
            .connect_timeout(config.connect_timeout)
            .timeout(config.request_timeout)
            .user_agent(concat!("engine-mcp/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|_| ConfigError("the HTTP client could not be built"))?;
        Ok(Self {
            http,
            config,
            session,
        })
    }

    async fn execute(&self, ctx: &CallContext, tool: Tool, plan: &Plan) -> ToolOutput {
        let Ok(mut url) = self.config.base.join(&plan.path) else {
            return ToolOutput::error(&ToolError::new("internal"));
        };
        if !plan.query.is_empty() {
            url.query_pairs_mut()
                .extend_pairs(plan.query.iter().map(|(k, v)| (*k, v.as_str())));
        }
        let mut renewed = false;
        loop {
            let token = match self.session.token().await {
                Ok(token) => token,
                Err(error) => return session_failure(error),
            };
            let response = match self.send(ctx, plan, &url, &token).await {
                Ok(response) => response,
                Err(failure) => return failure,
            };
            if response.status() == StatusCode::UNAUTHORIZED && !renewed {
                renewed = true;
                match self.session.rejected(&token).await {
                    Ok(()) => continue,
                    Err(error) => return session_failure(error),
                }
            }
            return self.finish(tool, plan, response).await;
        }
    }

    async fn send(
        &self,
        ctx: &CallContext,
        plan: &Plan,
        url: &Url,
        token: &Token,
    ) -> Result<reqwest::Response, ToolOutput> {
        let internal = || ToolOutput::error(&ToolError::new("internal"));
        let mut authorization = HeaderValue::from_str(&format!("Bearer {}", token.expose()))
            .map_err(|_| {
                tracing::error!("the session token is not a valid header value");
                internal()
            })?;
        authorization.set_sensitive(true);
        let method = match plan.method {
            Method::Get => reqwest::Method::GET,
            Method::Post => reqwest::Method::POST,
        };
        let mut request = self
            .http
            .request(method, url.clone())
            .header(AUTHORIZATION, authorization)
            .header(ACCEPT, DCT_ACCEPT)
            .header("x-dynodoc-client", client_header(&ctx.client));
        if let Some(etag) = &plan.if_none_match {
            request = request.header(reqwest::header::IF_NONE_MATCH, etag);
        }
        if let Some(body) = &plan.body {
            let bytes = serde_json::to_vec(body).map_err(|_| internal())?;
            request = request.header(CONTENT_TYPE, "application/json").body(bytes);
        }
        request.send().await.map_err(|error| {
            // Never `{error}`: reqwest prints the URL, whose query holds the caller's text.
            let kind = if error.is_timeout() {
                "timeout"
            } else if error.is_connect() {
                "connect"
            } else {
                "request"
            };
            tracing::warn!(kind, "agent request failed");
            ToolOutput::error(&ToolError::new(if error.is_timeout() {
                "deadline_exceeded"
            } else {
                "internal"
            }))
        })
    }

    async fn finish(&self, tool: Tool, plan: &Plan, mut response: reqwest::Response) -> ToolOutput {
        let status = response.status();
        tracing::debug!(
            tool = tool.name(),
            status = status.as_u16(),
            "agent response"
        );
        let etag = header_text(&response, ETAG.as_str());
        if status == StatusCode::NOT_MODIFIED {
            return not_modified(tool, plan, etag.as_deref());
        }
        if !status.is_success() {
            let retry_after = header_text(&response, RETRY_AFTER.as_str());
            let cap = ERROR_BODY_BYTES.min(self.config.max_response_bytes);
            let body = read_body(&mut response, cap).await.unwrap_or_default();
            return ToolOutput::error(&ToolError::from_http(
                status.as_u16(),
                &body,
                retry_after.as_deref(),
            ));
        }
        let dct = header_text(&response, CONTENT_TYPE.as_str()).is_some_and(|t| {
            t.trim_start()
                .to_ascii_lowercase()
                .starts_with("text/vnd.dynodoc.dct")
        });
        let cap = self.config.max_response_bytes;
        let body = match read_body(&mut response, cap).await {
            Ok(body) => body,
            Err(BodyError::TooLarge(seen)) => {
                tracing::warn!(tool = tool.name(), cap, "agent response over the byte cap");
                return ToolOutput::error(
                    &ToolError::new("limit_exceeded")
                        .with("field", "response")
                        .with_number("limit", cap as u64)
                        .with_number("value", seen as u64),
                );
            }
            Err(BodyError::TimedOut) => {
                return ToolOutput::error(&ToolError::new("deadline_exceeded"));
            }
            Err(BodyError::Failed) => return ToolOutput::error(&ToolError::new("internal")),
        };
        match String::from_utf8(body) {
            Ok(text) if dct && text.starts_with("dd/1 ") => {
                let meta = meta_of(&text, etag.as_deref());
                ToolOutput {
                    text,
                    is_error: false,
                    meta,
                }
            }
            _ => {
                tracing::warn!(tool = tool.name(), "agent response is not DCT");
                ToolOutput::error(&ToolError::new("internal"))
            }
        }
    }
}

impl<S: Session> Backend for HttpBackend<S> {
    async fn call(&self, ctx: CallContext, call: ToolCall) -> ToolOutput {
        let plan = match request::plan(call.tool, &call.args, self.config.project) {
            Ok(plan) => plan,
            Err(error) => return ToolOutput::error(&error),
        };
        match tokio::time::timeout(
            self.config.call_deadline,
            self.execute(&ctx, call.tool, &plan),
        )
        .await
        {
            Ok(output) => output,
            Err(_) => {
                tracing::warn!(tool = call.tool.name(), "agent call deadline exceeded");
                ToolOutput::error(&ToolError::new("deadline_exceeded"))
            }
        }
    }
}

fn session_failure(error: SessionError) -> ToolOutput {
    tracing::warn!(%error, "no session token");
    ToolOutput::error(&ToolError::new(match error {
        SessionError::Refused => "unauthorized",
        SessionError::Unavailable => "busy",
    }))
}

/// `X-Dynodoc-Client`: `engine-mcp/<version> (<mcp client>)`, printable ASCII, at most 120
/// characters (the server truncates there).
fn client_header(client: &str) -> String {
    let mut value = concat!("engine-mcp/", env!("CARGO_PKG_VERSION")).to_owned();
    let client: String = client
        .chars()
        .filter(|c| c.is_ascii_graphic() || *c == ' ')
        .take(60)
        .collect();
    let client = client.trim();
    if !client.is_empty() {
        value.push_str(" (");
        value.push_str(client);
        value.push(')');
    }
    value
}

fn header_text(response: &reqwest::Response, name: &str) -> Option<String> {
    response
        .headers()
        .get(name)
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned)
}

enum BodyError {
    /// The body is, or is already known to be, larger than the cap; carries the bytes seen
    /// (or announced), which is more than the cap.
    TooLarge(usize),
    TimedOut,
    Failed,
}

/// The body, refusing more than `cap` bytes without buffering them.
async fn read_body(response: &mut reqwest::Response, cap: usize) -> Result<Vec<u8>, BodyError> {
    if let Some(announced) = response.content_length() {
        if announced > cap as u64 {
            return Err(BodyError::TooLarge(
                announced.min(usize::MAX as u64) as usize
            ));
        }
    }
    let mut body = Vec::new();
    loop {
        match response.chunk().await {
            Ok(Some(chunk)) => {
                if chunk.len() > cap - body.len() {
                    return Err(BodyError::TooLarge(body.len() + chunk.len()));
                }
                body.extend_from_slice(&chunk);
            }
            Ok(None) => return Ok(body),
            Err(error) if error.is_timeout() => return Err(BodyError::TimedOut),
            Err(_) => return Err(BodyError::Failed),
        }
    }
}

/// `304`: the caller sent the ETag of the revision it holds (`plan.if_none_match`), so the
/// answer is the one line the library writes for an unchanged outline.
fn not_modified(tool: Tool, plan: &Plan, etag: Option<&str>) -> ToolOutput {
    let known = etag.or(plan.if_none_match.as_deref());
    let mut line = format!("dd/1 {}", tool.name());
    if let Some(file) = &plan.file {
        line.push_str(&format!(" file={file}"));
    }
    if let Some((rev, chain)) = known
        .map(|e| e.trim().trim_start_matches("W/").trim_matches('"'))
        .and_then(|e| e.split_once(':'))
        .filter(|(rev, chain)| {
            !rev.is_empty()
                && rev.bytes().all(|b| b.is_ascii_digit())
                && chain.len() == 8
                && chain.bytes().all(|b| b.is_ascii_hexdigit())
        })
    {
        line.push_str(&format!(" rev={rev} chain={chain}"));
    }
    line.push_str(" unchanged=1\n");
    let meta = known.map(|e| json!({ "dynodoc/v1": { "etag": e } }));
    ToolOutput {
        text: line,
        is_error: false,
        meta,
    }
}

/// The key-value pairs of a DCT header line: `dd/1 <tool> key=value key="quoted" ...`.
/// Tolerant of anything else; unknown keys are the caller's to ignore.
pub fn header_pairs(line: &str) -> Vec<(String, String)> {
    let mut pairs = Vec::new();
    let mut chars = line.chars().peekable();
    while let Some(&c) = chars.peek() {
        if c == ' ' {
            chars.next();
            continue;
        }
        let mut word = String::new();
        let mut value = None::<String>;
        while let Some(&c) = chars.peek() {
            if c == ' ' {
                break;
            }
            chars.next();
            if c == '=' && value.is_none() {
                let mut v = String::new();
                if chars.peek() == Some(&'"') {
                    chars.next();
                    while let Some(c) = chars.next() {
                        match c {
                            '"' => break,
                            '\\' => match chars.next() {
                                Some('n') => v.push('\n'),
                                Some('t') => v.push('\t'),
                                Some(other) => v.push(other),
                                None => break,
                            },
                            other => v.push(other),
                        }
                    }
                } else {
                    while let Some(&c) = chars.peek() {
                        if c == ' ' {
                            break;
                        }
                        v.push(c);
                        chars.next();
                    }
                }
                value = Some(v);
                break;
            }
            word.push(c);
        }
        if let Some(value) = value {
            pairs.push((word, value));
        }
    }
    pairs
}

/// `_meta["dynodoc/v1"]` from the header line (spec 7.1): `rev`, `chain`, `file`, the
/// continuation `cursor` (`more` in the header), and the response's `etag`.
fn meta_of(text: &str, etag: Option<&str>) -> Option<Value> {
    let pairs = header_pairs(text.lines().next().unwrap_or(""));
    let get = |key: &str| {
        pairs
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    };
    let mut meta = Map::new();
    if let Some(rev) = get("rev").and_then(|r| r.parse::<u64>().ok()) {
        meta.insert("rev".into(), json!(rev));
    }
    for (header, key) in [("chain", "chain"), ("file", "file"), ("more", "cursor")] {
        if let Some(value) = get(header) {
            meta.insert(key.into(), json!(value));
        }
    }
    if let Some(etag) = etag {
        meta.insert("etag".into(), json!(etag));
    }
    (!meta.is_empty()).then(|| json!({ "dynodoc/v1": meta }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_api_base_must_be_https_and_clean() {
        let ok = |u: &str| parse_base(u, false);
        assert!(ok("https://api.example.test/").is_ok());
        assert!(ok("https://api.example.test/prefix/").is_ok());
        for bad in [
            "http://api.example.test/",
            "https://api.example.test/prefix",
            "https://user:pw@api.example.test/",
            "https://api.example.test/?q=1",
            "https://api.example.test/#f",
            "ftp://api.example.test/",
            "not a url",
        ] {
            assert!(ok(bad).is_err(), "{bad}");
        }
        assert!(parse_base("http://127.0.0.1:8080/", false).is_err());
        assert!(parse_base("http://127.0.0.1:8080/", true).is_ok());
        assert!(parse_base("http://[::1]:8080/", true).is_ok());
        assert!(parse_base("http://localhost:8080/", true).is_err());
        assert!(parse_base("http://192.168.1.5/", true).is_err());
    }

    #[test]
    fn header_pairs_read_bare_and_quoted_values() {
        let pairs = header_pairs(
            "dd/1 find file=7034d450 rev=412 q=\"a \\\"b\\\" c\" more=c_9f01 warn=truncated ~tok=370",
        );
        let get = |k: &str| {
            pairs
                .iter()
                .find(|(key, _)| key == k)
                .map(|(_, v)| v.as_str())
        };
        assert_eq!(get("file"), Some("7034d450"));
        assert_eq!(get("rev"), Some("412"));
        assert_eq!(get("q"), Some("a \"b\" c"));
        assert_eq!(get("more"), Some("c_9f01"));
        assert_eq!(get("~tok"), Some("370"));
        assert_eq!(get("dd/1"), None);
    }

    #[test]
    fn a_client_claim_is_printable_and_short() {
        assert_eq!(
            client_header("claude-code/2.1\r\nX-Evil: 1"),
            format!(
                "engine-mcp/{} (claude-code/2.1X-Evil: 1)",
                env!("CARGO_PKG_VERSION")
            )
        );
        assert!(client_header(&"a".repeat(500)).len() <= 120);
        assert_eq!(
            client_header(""),
            format!("engine-mcp/{}", env!("CARGO_PKG_VERSION"))
        );
    }
}
