//! The MCP server: newline-delimited JSON-RPC 2.0 over a reader and a writer.
//!
//! Hand-written for one reason: a tool server needs `initialize`, `ping`,
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
//! * `tools/call` validates the arguments against the published schema. An unknown tool, or a
//!   tool the connection's profile does not offer is a
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

use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

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

/// A trusted host must show the exact tool and validated arguments to a person.
/// The sink must also bind each decision to its backend project. The agent's own request
/// or an untrusted MCP client must never implement this decision.
/// The server asks again for each write and fails closed on denial or timeout.
pub trait HumanConfirmation: Send + Sync {
    fn confirm<'a>(
        &'a self,
        tool: Tool,
        arguments: &'a Map<String, Value>,
    ) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>>;
}

struct Inner<B> {
    backend: B,
    profile: Profile,
    confirmation: Option<Arc<dyn HumanConfirmation>>,
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
    /// Without a trusted confirmation sink, the connection is always read-only.
    pub fn new(backend: B, _profile: Profile) -> Self {
        Self::build(backend, Profile::read_only(), None)
    }

    /// Only trusted hosts with a real human confirmation surface may expose writes.
    pub fn with_human_confirmation(
        backend: B,
        profile: Profile,
        confirmation: Arc<dyn HumanConfirmation>,
    ) -> Self {
        Self::build(backend, profile, Some(confirmation))
    }

    fn build(
        backend: B,
        profile: Profile,
        confirmation: Option<Arc<dyn HumanConfirmation>>,
    ) -> Self {
        Self {
            inner: Arc::new(Inner {
                backend,
                profile,
                confirmation,
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
            "resources/list" => Ok(self.list_resources(&params)),
            "resources/read" => self.read_resource(params).await,
            "prompts/list" => Ok(self.list_prompts(&params)),
            "prompts/get" => self.get_prompt(params).await,
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
        json!({
            "tools": { "listChanged": false },
            "resources": { "subscribe": false, "listChanged": false },
            "prompts": { "listChanged": false }
        })
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
        if let Err(error) = self.inner.profile.validate(tool, &args) {
            return Ok(tool_result(ToolOutput::error(&error)));
        }
        if tool.is_write() {
            let approved = match &self.inner.confirmation {
                Some(sink) => {
                    tokio::time::timeout(Duration::from_secs(120), sink.confirm(tool, &args))
                        .await
                        .unwrap_or(false)
                }
                None => false,
            };
            if !approved {
                return Ok(tool_result(ToolOutput::error(&ToolError::new(
                    "policy_denied",
                ))));
            }
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

    fn list_resources(&self, _params: &Value) -> Value {
        json!({
            "resources": [
                {
                    "uri": "dynodoc://schema/document-ir",
                    "name": "Document Event & Block IR Schema",
                    "description": "JSON Schema describing Dynodoc's tamper-evident SHA-256 hash-chained revisions, structured paragraph/cell blocks, and event records.",
                    "mimeType": "application/schema+json"
                },
                {
                    "uri": "dynodoc://schema/change-request",
                    "name": "Change Request & Review Workflow Schema",
                    "description": "JSON Schema for draft reviews, change comments, approval gates, and human-in-the-loop merge requirements.",
                    "mimeType": "application/schema+json"
                },
                {
                    "uri": "dynodoc://projects",
                    "name": "Active Repository Catalog",
                    "description": "Active repository metadata, branch state, and document outline catalog.",
                    "mimeType": "application/json"
                }
            ]
        })
    }

    async fn read_resource(&self, params: Value) -> Result<Value, Failure> {
        let Value::Object(params) = params else {
            return Err(failure(rpc::INVALID_PARAMS, "params must be an object"));
        };
        let uri = params
            .get("uri")
            .and_then(Value::as_str)
            .ok_or_else(|| failure(rpc::INVALID_PARAMS, "uri is required"))?;

        let contents = match uri {
            "dynodoc://schema/document-ir" => json!({
                "type": "object",
                "title": "Dynodoc Document Event IR",
                "$schema": "https://json-schema.org/draft/2020-12/schema",
                "description": "Tamper-evident SHA-256 hash-chained document revision events.",
                "properties": {
                    "document_id": { "type": "string", "format": "uuid" },
                    "sequence": { "type": "integer", "minimum": 1 },
                    "chain_hash": { "type": "string", "pattern": "^[0-9a-f]{64}$" },
                    "prev_hash": { "type": "string", "pattern": "^[0-9a-f]{64}$" },
                    "blocks": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "id": { "type": "string" },
                                "type": { "type": "string", "enum": ["paragraph", "heading", "cell", "slide", "table"] },
                                "text": { "type": "string" },
                                "hash": { "type": "string" }
                            },
                            "required": ["id", "type", "text"]
                        }
                    }
                },
                "required": ["document_id", "sequence", "chain_hash", "blocks"]
            }),
            "dynodoc://schema/change-request" => json!({
                "type": "object",
                "title": "Dynodoc Change Request Workflow",
                "$schema": "https://json-schema.org/draft/2020-12/schema",
                "description": "Draft reviews, approval gates, and conflict choices.",
                "properties": {
                    "request_number": { "type": "integer" },
                    "state": { "type": "string", "enum": ["open", "merged", "closed"] },
                    "source_branch": { "type": "string" },
                    "target_branch": { "type": "string" },
                    "author": { "type": "string" },
                    "reviews": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "reviewer": { "type": "string" },
                                "status": { "type": "string", "enum": ["approved", "changes_requested", "commented"] },
                                "comments": { "type": "array" }
                            }
                        }
                    }
                },
                "required": ["request_number", "state", "source_branch", "target_branch"]
            }),
            "dynodoc://projects" => json!({
                "protocol": PROTOCOL,
                "profile": self.inner.profile.name(),
                "server": SERVER_NAME,
                "access": if self.inner.profile.offers(Tool::Propose) { "propose" } else { "read" },
                "available_tools": self.inner.profile.tools().iter().map(|t| t.name()).collect::<Vec<_>>(),
                "supported_formats": ["docx", "xlsx", "pptx", "markdown", "ir"],
                "status": "ready"
            }),
            _ => {
                return Err(failure(rpc::INVALID_PARAMS, "Resource not found"));
            }
        };

        Ok(json!({
            "contents": [
                {
                    "uri": uri,
                    "mimeType": "application/json",
                    "text": serde_json::to_string_pretty(&contents).unwrap_or_default()
                }
            ]
        }))
    }

    fn list_prompts(&self, _params: &Value) -> Value {
        json!({
            "prompts": [
                {
                    "name": "audit_document_history",
                    "description": "Inspect tamper-evident SHA-256 event chains, author signatures, and commit records to audit document provenance.",
                    "arguments": [
                        { "name": "file", "description": "File name (e.g. Handbook.docx) or UUID to audit", "required": true },
                        { "name": "branch", "description": "Branch or draft name to inspect (defaults to main)", "required": false }
                    ]
                },
                {
                    "name": "review_change_request",
                    "description": "Guided review prompt template to analyze a change request diff, check conflicts, and draft constructive review comments.",
                    "arguments": [
                        { "name": "request_number", "description": "Change request number (e.g. 14)", "required": true }
                    ]
                },
                {
                    "name": "summarize_document_diff",
                    "description": "Structured prompt template to compare revisions across branches and draft concise release notes.",
                    "arguments": [
                        { "name": "file", "description": "File name or UUID", "required": true },
                        { "name": "from", "description": "Starting revision number", "required": false },
                        { "name": "to", "description": "Ending revision number", "required": false }
                    ]
                }
            ]
        })
    }

    async fn get_prompt(&self, params: Value) -> Result<Value, Failure> {
        let Value::Object(params) = params else {
            return Err(failure(rpc::INVALID_PARAMS, "params must be an object"));
        };
        let name = params
            .get("name")
            .and_then(Value::as_str)
            .ok_or_else(|| failure(rpc::INVALID_PARAMS, "name is required"))?;
        let args = params.get("arguments").and_then(Value::as_object);

        let (description, text) = match name {
            "audit_document_history" => {
                let file = args
                    .and_then(|a| a.get("file"))
                    .and_then(Value::as_str)
                    .unwrap_or("<file>");
                let branch = args
                    .and_then(|a| a.get("branch"))
                    .and_then(Value::as_str)
                    .unwrap_or("main");
                (
                    "Inspect tamper-evident SHA-256 event chains, author signatures, and commit records.",
                    format!(
                        "Perform a comprehensive audit of the event history and hash chain for document '{file}' on branch '{branch}'.\n\
                         1. Call `outline` or `read` to inspect document structure and verified state.\n\
                         2. Inspect the sequence, author signatures, and chain hash continuity.\n\
                         3. Report any gaps or verify that the chain hash tip matches the settled revisions."
                    )
                )
            }
            "review_change_request" => {
                let req_num = args
                    .and_then(|a| a.get("request_number"))
                    .and_then(|v| {
                        v.as_i64()
                            .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
                    })
                    .unwrap_or(1);
                (
                    "Guided review prompt template to analyze a change request diff and check conflicts.",
                    format!(
                        "Analyze change request #{req_num}.\n\
                         1. Inspect the change request summary and review state.\n\
                         2. Call `diff` to inspect structural paragraph and cell changes.\n\
                         3. Check for conflicting blocks or rules violations.\n\
                         4. Keep comments concise, specific, and kind. Note: agents propose changes or comments; human reviewers approve and merge."
                    )
                )
            }
            "summarize_document_diff" => {
                let file = args
                    .and_then(|a| a.get("file"))
                    .and_then(Value::as_str)
                    .unwrap_or("<file>");
                let from = args
                    .and_then(|a| a.get("from"))
                    .and_then(|v| {
                        v.as_i64()
                            .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
                    })
                    .unwrap_or(0);
                let to = args
                    .and_then(|a| a.get("to"))
                    .and_then(|v| {
                        v.as_i64()
                            .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
                    })
                    .unwrap_or(0);
                (
                    "Structured prompt template to compare revisions across branches.",
                    format!(
                        "Summarize the changes in '{file}' between revisions {from} and {to}.\n\
                         1. Call `diff` to compute paragraph, cell, and slide differences.\n\
                         2. Provide an executive summary of key modifications.\n\
                         3. Group changes by document headings or sheet sections."
                    ),
                )
            }
            _ => return Err(failure(rpc::INVALID_PARAMS, "Unknown prompt")),
        };

        Ok(json!({
            "description": description,
            "messages": [
                {
                    "role": "user",
                    "content": {
                        "type": "text",
                        "text": text
                    }
                }
            ]
        }))
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
