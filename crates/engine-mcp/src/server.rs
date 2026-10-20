//! The MCP server: newline-delimited JSON-RPC 2.0 over a reader and a writer.
//!
//! Hand-written for one reason: a read-only tool server needs `initialize`, `ping`,
//! `tools/list` and `tools/call` and nothing else, and the official Rust SDK (`rmcp`) would
//! add a large dependency tree for them. The protocol layer is isolated here so replacing
//! it is local (protocol spec 21, "MCP specification churn").
//!
//! # Behaviour
//!
//! * Both handshakes work (spec 13.1). `initialize` negotiates a revision: the client's if
//!   it is one of [`SUPPORTED_VERSIONS`], else `2025-11-25`. `server/discover` answers the
//!   stateless revision. Requests are served without a prior `initialize`, so a stateless
//!   client works unchanged. Notifications, including `notifications/initialized` and
//!   `notifications/cancelled`, are accepted and ignored: requests are served one at a time
//!   and a running call is never aborted.
//! * `tools/list` is deterministic (same bytes every time). When the request (or the
//!   negotiated revision) is 2026-07-28 or later it carries `ttlMs` and `cacheScope`.
//! * `tools/call` validates the arguments against the published schema. An unknown tool is a
//!   JSON-RPC error `-32602`; invalid arguments and every backend failure are a normal
//!   result with `isError: true` and a `dd/1 error` line (spec 7.1, and the 2025-11-25
//!   clarification that argument errors are tool errors so a model can correct itself).
//! * Anything else is `-32601`, a malformed line `-32700` or `-32600`. JSON-RPC batches are
//!   refused (removed in 2025-06-18). A panic in a handler becomes `-32603` (or an
//!   `internal` tool error) and the loop carries on.
//!
//! # Stdout
//!
//! The writer receives protocol messages and nothing else, one per line. Logs go through
//! `tracing`, which the binary points at stderr. This module and [`Server::serve_stdio`] are
//! the only code in the crate that names stdout.
//!
//! # Not verified
//!
//! The 2026-07-28 revision post-dates the author's own knowledge of the protocol; what is
//! known comes from docs/AI-AGENT-VENDOR-INTEGRATIONS.md section 2 (read 14 October 2026).
//! The shape of the `server/discover` result, the metadata key
//! `io.modelcontextprotocol/protocolVersion` and the allowed values of `cacheScope` are
//! therefore unverified: check at build time. They are isolated in the constants below.

use std::sync::{Arc, Mutex, PoisonError};

use serde_json::{json, Map, Value};
use tokio::io::{AsyncBufRead, AsyncWrite, AsyncWriteExt};

use crate::{
    backend::{Backend, CallContext, ToolCall, ToolOutput},
    error::ToolError,
    rpc::{self, Incoming, Line},
    tools::{Profile, Tool, PROTOCOL, SERVER_NAME},
};

/// Revisions this server answers, newest first.
pub const SUPPORTED_VERSIONS: [&str; 4] = ["2026-07-28", "2025-11-25", "2025-06-18", "2025-03-26"];
/// The answer to an `initialize` that asks for a revision not in the list.
const INITIALIZE_FALLBACK: &str = "2025-11-25";
/// The first stateless revision: no `initialize`, `ttlMs` and `cacheScope` on list results.
const STATELESS_VERSION: &str = "2026-07-28";
/// Where a stateless request names its revision (vendor document, section 2). Unverified.
const VERSION_META_KEY: &str = "io.modelcontextprotocol/protocolVersion";
/// How long a client may cache `tools/list` (it never changes while the process runs).
const LIST_TTL_MS: u64 = 3_600_000;
/// The list depends on the credential's profile, so it is not shared. Unverified value.
const LIST_CACHE_SCOPE: &str = "private";

/// Longest accepted input line. A `tools/call` is a few hundred bytes; `have` can reach a
/// few kilobytes.
pub const MAX_LINE_BYTES: usize = 1 << 20;

struct Connection {
    /// The revision `initialize` agreed on, if there was one.
    version: Option<String>,
    /// `name/version` of the MCP client, for `X-Dynodoc-Client`.
    client: String,
}

struct Inner<B> {
    backend: B,
    profile: Profile,
    connection: Mutex<Connection>,
}

/// A JSON-RPC error to send.
struct Failure {
    code: i64,
    message: &'static str,
}

fn failure(code: i64, message: &'static str) -> Failure {
    Failure { code, message }
}

/// The server. Cheap to clone; clones share one connection state.
pub struct Server<B> {
    inner: Arc<Inner<B>>,
}

impl<B> Clone for Server<B> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl<B: Backend> Server<B> {
    pub fn new(backend: B, profile: Profile) -> Self {
        Self {
            inner: Arc::new(Inner {
                backend,
                profile,
                connection: Mutex::new(Connection {
                    version: None,
                    client: String::new(),
                }),
            }),
        }
    }

    /// Serve standard input and output until input ends.
    pub async fn serve_stdio(&self) -> std::io::Result<()> {
        self.serve(
            tokio::io::BufReader::new(tokio::io::stdin()),
            tokio::io::stdout(),
        )
        .await
    }

    /// Serve `reader` and `writer` until `reader` ends. Returns an error only when the
    /// transport fails; no input, however malformed, ends the loop.
    pub async fn serve<R, W>(&self, mut reader: R, mut writer: W) -> std::io::Result<()>
    where
        R: AsyncBufRead + Unpin,
        W: AsyncWrite + Unpin,
    {
        loop {
            let reply = match rpc::read_line(&mut reader, MAX_LINE_BYTES).await? {
                Line::Eof => return Ok(()),
                Line::Text(text) if text.trim().is_empty() => continue,
                Line::Text(text) => self.handle_line(&text).await,
                Line::TooLong => Some(rpc::failure(
                    &Value::Null,
                    rpc::INVALID_REQUEST,
                    "Message too large",
                )),
                Line::NotUtf8 => Some(rpc::failure(&Value::Null, rpc::PARSE_ERROR, "Parse error")),
            };
            if let Some(reply) = reply {
                writer.write_all(reply.as_bytes()).await?;
                writer.write_all(b"\n").await?;
                writer.flush().await?;
            }
        }
    }

    /// Answer one input line: the response without its line ending, or `None` when the
    /// message needs none (a notification, or a response to nothing).
    pub async fn handle_line(&self, line: &str) -> Option<String> {
        match rpc::parse(line) {
            Incoming::Invalid { id, code, message } => Some(rpc::failure(&id, code, message)),
            Incoming::Response => None,
            Incoming::Notification { .. } => None,
            Incoming::Request { id, method, params } => {
                let server = self.clone();
                // A handler that panics must cost one request, not the connection.
                let outcome =
                    tokio::spawn(async move { server.request(&method, params).await }).await;
                Some(match outcome {
                    Ok(Ok(result)) => rpc::success(&id, result),
                    Ok(Err(f)) => rpc::failure(&id, f.code, f.message),
                    Err(_) => {
                        tracing::error!("request handler failed");
                        rpc::failure(&id, rpc::INTERNAL_ERROR, "Internal error")
                    }
                })
            }
        }
    }

    async fn request(&self, method: &str, params: Value) -> Result<Value, Failure> {
        match method {
            "initialize" => self.initialize(&params),
            "server/discover" => Ok(self.discover()),
            "ping" => Ok(json!({})),
            "tools/list" => Ok(self.list_tools(&params)),
            "tools/call" => self.call_tool(params).await,
            _ => Err(failure(rpc::METHOD_NOT_FOUND, "Method not found")),
        }
    }

    fn connection(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.inner
            .connection
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    fn server_info(&self) -> Value {
        json!({
            "name": SERVER_NAME,
            "title": "Dynodoc",
            "version": env!("CARGO_PKG_VERSION")
        })
    }

    fn capabilities(&self) -> Value {
        json!({ "tools": { "listChanged": false } })
    }

    fn initialize(&self, params: &Value) -> Result<Value, Failure> {
        let requested = params
            .get("protocolVersion")
            .and_then(Value::as_str)
            .ok_or_else(|| failure(rpc::INVALID_PARAMS, "protocolVersion is required"))?;
        let agreed = SUPPORTED_VERSIONS
            .iter()
            .copied()
            .find(|v| *v == requested)
            .unwrap_or(INITIALIZE_FALLBACK);
        let claim = params.get("clientInfo");
        let part = |key: &str| claim.and_then(|c| c.get(key)).and_then(Value::as_str);
        let client = match (part("name"), part("version")) {
            (Some(name), Some(version)) => format!("{name}/{version}"),
            (Some(name), None) => name.to_owned(),
            _ => String::new(),
        };
        {
            let mut connection = self.connection();
            connection.version = Some(agreed.to_owned());
            connection.client = client;
        }
        Ok(json!({
            "protocolVersion": agreed,
            "capabilities": self.capabilities(),
            "serverInfo": self.server_info(),
            "instructions": self.inner.profile.instructions(),
            "_meta": { "dynodoc/v1": { "protocol": PROTOCOL, "profile": self.inner.profile.name() } }
        }))
    }

    /// The stateless revision's discovery call. Shape unverified; see the module notes.
    fn discover(&self) -> Value {
        json!({
            "supportedVersions": SUPPORTED_VERSIONS,
            "capabilities": self.capabilities(),
            "serverInfo": self.server_info(),
            "instructions": self.inner.profile.instructions(),
            "_meta": { "dynodoc/v1": { "protocol": PROTOCOL, "profile": self.inner.profile.name() } }
        })
    }

    /// The revision a request is governed by: the one it names, else the negotiated one.
    fn revision(&self, params: &Value) -> Option<String> {
        params
            .get("_meta")
            .and_then(|m| m.get(VERSION_META_KEY))
            .and_then(Value::as_str)
            .map(str::to_owned)
            .or_else(|| self.connection().version.clone())
    }

    fn list_tools(&self, params: &Value) -> Value {
        let mut result = json!({ "tools": self.inner.profile.definitions() });
        if self
            .revision(params)
            .is_some_and(|v| v.as_str() >= STATELESS_VERSION)
        {
            result["ttlMs"] = json!(LIST_TTL_MS);
            result["cacheScope"] = json!(LIST_CACHE_SCOPE);
        }
        result
    }

    async fn call_tool(&self, params: Value) -> Result<Value, Failure> {
        let Value::Object(mut params) = params else {
            return Err(failure(rpc::INVALID_PARAMS, "params must be an object"));
        };
        let tool = params
            .get("name")
            .and_then(Value::as_str)
            .and_then(Tool::parse)
            .filter(|t| self.inner.profile.offers(*t))
            .ok_or_else(|| failure(rpc::INVALID_PARAMS, "Unknown tool"))?;
        let args = match params.remove("arguments") {
            None | Some(Value::Null) => Map::new(),
            Some(Value::Object(args)) => args,
            Some(_) => {
                return Err(failure(rpc::INVALID_PARAMS, "arguments must be an object"));
            }
        };
        if let Err(error) = tool.validate(&args) {
            return Ok(tool_result(ToolOutput::error(&error)));
        }
        let ctx = CallContext {
            client: self.connection().client.clone(),
        };
        let inner = self.inner.clone();
        let call = ToolCall { tool, args };
        let output = match tokio::spawn(async move { inner.backend.call(ctx, call).await }).await {
            Ok(output) => output,
            Err(error) => {
                tracing::error!(
                    tool = tool.name(),
                    panic = error.is_panic(),
                    "tool call failed"
                );
                ToolOutput::error(&ToolError::new("internal"))
            }
        };
        Ok(tool_result(output))
    }
}

/// The `tools/call` result: one text block, `isError` only when true, `_meta` when there
/// are machine fields.
fn tool_result(output: ToolOutput) -> Value {
    let mut result = json!({ "content": [{ "type": "text", "text": output.text }] });
    if output.is_error {
        result["isError"] = json!(true);
    }
    if let Some(meta) = output.meta {
        result["_meta"] = meta;
    }
    result
}
