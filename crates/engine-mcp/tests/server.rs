//! The protocol layer, driven through its reader and writer with a scripted backend:
//! handshakes, `tools/list`, `tools/call`, error codes, and that the writer carries
//! nothing but protocol messages whatever the reader sends (including a panicking tool).

use std::sync::{Arc, Mutex};

use engine_mcp::{
    server::{MAX_LINE_BYTES, SUPPORTED_VERSIONS},
    Backend, CallContext, Profile, Server, Tool, ToolCall, ToolError, ToolOutput,
};
use serde_json::{json, Value};

/// Records what it is asked and answers from the arguments.
#[derive(Clone, Default)]
struct Scripted {
    seen: Arc<Mutex<Vec<(CallContext, ToolCall)>>>,
}

impl Backend for Scripted {
    async fn call(&self, ctx: CallContext, call: ToolCall) -> ToolOutput {
        let query = call.args.get("q").and_then(Value::as_str).unwrap_or("");
        let output = match query {
            "panic" => panic!("scripted failure with a recognisable message"),
            "fail" => ToolOutput::error(&ToolError::new("not_found")),
            _ => ToolOutput {
                text: format!("dd/1 {} rev=7 chain=0123abcd\nbody\n", call.tool.name()),
                is_error: false,
                meta: Some(json!({"dynodoc/v1": {"rev": 7}})),
            },
        };
        self.seen.lock().unwrap().push((ctx, call));
        output
    }
}

fn server() -> (Server<Scripted>, Scripted) {
    let backend = Scripted::default();
    (Server::new(backend.clone(), Profile::read_only()), backend)
}

/// Feed `input` and return every line the server wrote.
async fn run(server: &Server<Scripted>, input: &str) -> Vec<String> {
    let mut out = Vec::new();
    server.serve(input.as_bytes(), &mut out).await.unwrap();
    String::from_utf8(out)
        .expect("stdout is UTF-8")
        .lines()
        .map(str::to_owned)
        .collect()
}

async fn one(server: &Server<Scripted>, request: Value) -> Value {
    let lines = run(server, &format!("{request}\n")).await;
    assert_eq!(lines.len(), 1, "{lines:?}");
    serde_json::from_str(&lines[0]).unwrap()
}

fn request(id: u64, method: &str, params: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params})
}

fn call(id: u64, name: &str, arguments: Value) -> Value {
    request(
        id,
        "tools/call",
        json!({"name": name, "arguments": arguments}),
    )
}

/// A line is a protocol message: a JSON-RPC 2.0 response with an id and a result or error.
fn assert_protocol_line(line: &str) {
    let value: Value = serde_json::from_str(line).unwrap_or_else(|_| panic!("not JSON: {line}"));
    assert_eq!(value["jsonrpc"], "2.0", "{line}");
    assert!(value.get("id").is_some(), "{line}");
    assert!(
        value.get("result").is_some() != value.get("error").is_some(),
        "{line}"
    );
}

#[tokio::test]
async fn initialize_agrees_a_revision_and_describes_the_server() {
    let (server, _) = server();
    for requested in SUPPORTED_VERSIONS {
        let reply = one(
            &server,
            request(
                1,
                "initialize",
                json!({"protocolVersion": requested, "capabilities": {},
                       "clientInfo": {"name": "claude-code", "version": "2.1"}}),
            ),
        )
        .await;
        let result = &reply["result"];
        assert_eq!(result["protocolVersion"], requested);
        assert_eq!(result["serverInfo"]["name"], "dynodoc");
        assert_eq!(result["capabilities"]["tools"]["listChanged"], false);
        assert!(result["capabilities"].get("resources").is_none());
        let instructions = result["instructions"].as_str().unwrap();
        assert!(instructions.chars().count() <= 500);
        assert!(instructions.starts_with("dynodoc.agent/1"));
        assert_eq!(result["_meta"]["dynodoc/v1"]["protocol"], "dynodoc.agent/1");
    }
    // An unknown revision gets the newest one that has the handshake.
    let reply = one(
        &server,
        request(2, "initialize", json!({"protocolVersion": "2099-01-01"})),
    )
    .await;
    assert_eq!(reply["result"]["protocolVersion"], "2025-11-25");
    // The revision is required.
    let reply = one(&server, request(3, "initialize", json!({}))).await;
    assert_eq!(reply["error"]["code"], -32602);
    assert_eq!(reply["id"], 3);
}

#[tokio::test]
async fn the_client_name_reaches_the_backend_as_a_claim() {
    let (server, backend) = server();
    let input = format!(
        "{}\n{}\n",
        request(
            1,
            "initialize",
            json!({"protocolVersion": "2025-11-25", "clientInfo": {"name": "claude-code", "version": "2.1"}})
        ),
        call(2, "status", json!({}))
    );
    run(&server, &input).await;
    assert_eq!(backend.seen.lock().unwrap()[0].0.client, "claude-code/2.1");
}

#[tokio::test]
async fn discovery_and_ping_answer_without_a_handshake() {
    let (server, _) = server();
    let reply = one(&server, request(1, "server/discover", json!({}))).await;
    assert_eq!(reply["result"]["supportedVersions"][0], "2026-07-28");
    assert_eq!(reply["result"]["serverInfo"]["name"], "dynodoc");
    let reply = one(&server, request(2, "ping", Value::Null)).await;
    assert_eq!(reply["result"], json!({}));
}

#[tokio::test]
async fn notifications_and_unrelated_responses_get_no_reply() {
    let (server, _) = server();
    let input = [
        json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
        json!({"jsonrpc": "2.0", "method": "notifications/cancelled", "params": {"requestId": 5}}),
        json!({"jsonrpc": "2.0", "method": "notifications/unheard-of", "params": [1, 2]}),
        json!({"jsonrpc": "2.0", "id": 9, "result": {}}),
        json!({"jsonrpc": "2.0", "id": 10, "error": {"code": -1, "message": "x"}}),
    ]
    .map(|m| format!("{m}\n"))
    .concat();
    assert!(run(&server, &input).await.is_empty());
}

#[tokio::test]
async fn unknown_methods_are_method_not_found_and_keep_the_id() {
    let (server, _) = server();
    for method in [
        "resources/list",
        "prompts/list",
        "logging/setLevel",
        "tools/run",
        "",
    ] {
        let reply = one(&server, request(41, method, json!({}))).await;
        assert_eq!(reply["error"]["code"], -32601, "{method}");
        assert_eq!(reply["id"], 41);
    }
    let reply = one(
        &server,
        json!({"jsonrpc": "2.0", "id": "abc", "method": "nope"}),
    )
    .await;
    assert_eq!(reply["id"], "abc");
}

#[tokio::test]
async fn tools_list_is_the_six_read_tools_in_order_and_is_deterministic() {
    let (server, _) = server();
    let first = one(&server, request(1, "tools/list", json!({}))).await;
    let again = one(&server, request(1, "tools/list", json!({}))).await;
    assert_eq!(first, again);
    assert_eq!(first.to_string(), again.to_string());
    let names: Vec<&str> = first["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        names,
        ["files", "outline", "read", "find", "diff", "status"]
    );
    for tool in first["result"]["tools"].as_array().unwrap() {
        let hints = &tool["annotations"];
        assert_eq!(hints["readOnlyHint"], true);
        assert_eq!(hints["destructiveHint"], false);
        assert_eq!(hints["openWorldHint"], false);
        assert!(tool["title"].as_str().is_some_and(|t| !t.is_empty()));
        assert_eq!(tool["inputSchema"]["additionalProperties"], false);
    }
    // No write tool, and no paging fields before the stateless revision.
    assert!(!names.contains(&"propose") && !names.contains(&"submit"));
    assert!(first["result"].get("ttlMs").is_none());
    assert!(first["result"].get("nextCursor").is_none());
}

#[tokio::test]
async fn the_stateless_revision_gets_cache_fields() {
    let (server, _) = server();
    // Named by the request itself.
    let stateless = one(
        &server,
        request(
            1,
            "tools/list",
            json!({"_meta": {"io.modelcontextprotocol/protocolVersion": "2026-07-28"}}),
        ),
    )
    .await;
    assert!(stateless["result"]["ttlMs"].as_u64().unwrap() > 0);
    assert!(stateless["result"]["cacheScope"].is_string());
    // Named by an older revision in the request: none.
    let older = one(
        &server,
        request(
            2,
            "tools/list",
            json!({"_meta": {"io.modelcontextprotocol/protocolVersion": "2025-11-25"}}),
        ),
    )
    .await;
    assert!(older["result"].get("ttlMs").is_none());
    // Or negotiated by `initialize` on the same connection.
    let input = format!(
        "{}\n{}\n",
        request(1, "initialize", json!({"protocolVersion": "2026-07-28"})),
        request(2, "tools/list", json!({}))
    );
    let lines = run(&server, &input).await;
    let list: Value = serde_json::from_str(&lines[1]).unwrap();
    assert!(list["result"].get("ttlMs").is_some());
    // The tool definitions themselves do not change with the revision.
    assert_eq!(stateless["result"]["tools"], older["result"]["tools"]);
}

#[tokio::test]
async fn a_call_returns_one_text_block_and_meta() {
    let (server, backend) = server();
    let reply = one(
        &server,
        call(5, "find", json!({"file": "7034d450", "q": "clause"})),
    )
    .await;
    let result = &reply["result"];
    assert_eq!(result["content"].as_array().unwrap().len(), 1);
    assert_eq!(result["content"][0]["type"], "text");
    assert_eq!(
        result["content"][0]["text"],
        "dd/1 find rev=7 chain=0123abcd\nbody\n"
    );
    assert!(result.get("isError").is_none());
    assert_eq!(result["_meta"]["dynodoc/v1"]["rev"], 7);
    assert!(result.get("structuredContent").is_none());
    let seen = backend.seen.lock().unwrap();
    assert_eq!(seen[0].1.tool, Tool::Find);
    assert_eq!(seen[0].1.args["q"], "clause");
}

#[tokio::test]
async fn tool_failures_are_results_with_is_error() {
    let (server, _) = server();
    let reply = one(
        &server,
        call(1, "find", json!({"file": "7034d450", "q": "fail"})),
    )
    .await;
    assert_eq!(reply["result"]["isError"], true);
    assert!(reply["result"]["content"][0]["text"]
        .as_str()
        .unwrap()
        .starts_with("dd/1 error code=not_found "));
    assert!(reply.get("error").is_none());
}

#[tokio::test]
async fn bad_arguments_are_tool_errors_naming_the_field() {
    let (server, backend) = server();
    for (name, arguments, field) in [
        ("outline", json!({}), "file"),
        ("outline", json!({"file": "7034d450", "depth": 2}), "depth"),
        (
            "outline",
            json!({"file": "7034d450", "depth": "1"}),
            "depth",
        ),
        (
            "outline",
            json!({"file": "7034d450", "limit": 1001}),
            "limit",
        ),
        ("outline", json!({"file": "x"}), "file"),
        ("outline", json!({"file": "7034d450", "extra": 1}), "extra"),
        (
            "read",
            json!({"file": "7034d450", "max_bytes": 100}),
            "max_bytes",
        ),
        ("read", json!({"file": "7034d450", "refs": "7C02"}), "refs"),
        (
            "read",
            json!({"file": "7034d450", "have": {"7C02": 5}}),
            "have",
        ),
        ("find", json!({"file": "7034d450"}), "q"),
        ("find", json!({"file": "7034d450", "q": ""}), "q"),
        (
            "find",
            json!({"file": "7034d450", "q": "x", "in": "everything"}),
            "in",
        ),
        ("files", json!({"kind": "folder"}), "kind"),
        ("files", json!({"project": "nope"}), "project"),
        ("status", json!({"what": "proposal"}), "what"),
    ] {
        let reply = one(&server, call(1, name, arguments.clone())).await;
        assert_eq!(reply["result"]["isError"], true, "{name} {arguments}");
        let text = reply["result"]["content"][0]["text"].as_str().unwrap();
        assert!(
            text.starts_with(&format!("dd/1 error code=bad_request field={field} ")),
            "{name} {arguments}: {text}"
        );
    }
    assert!(
        backend.seen.lock().unwrap().is_empty(),
        "nothing invalid reached the backend"
    );
}

#[tokio::test]
async fn malformed_calls_are_protocol_errors() {
    let (server, _) = server();
    for (params, label) in [
        (json!({"name": "propose", "arguments": {}}), "write tool"),
        (json!({"name": "nope"}), "unknown tool"),
        (json!({"arguments": {}}), "no name"),
        (json!({"name": 5}), "name not a string"),
        (
            json!({"name": "status", "arguments": []}),
            "arguments not an object",
        ),
        (
            json!({"name": "status", "arguments": "{}"}),
            "arguments a string",
        ),
        (json!([]), "params an array"),
        (Value::Null, "params missing"),
    ] {
        let reply = one(&server, request(8, "tools/call", params)).await;
        assert_eq!(reply["error"]["code"], -32602, "{label}");
        assert_eq!(reply["id"], 8, "{label}");
    }
    // Arguments may be omitted or null when nothing is required.
    for arguments in [json!(null), json!({})] {
        let reply = one(&server, call(1, "status", arguments)).await;
        assert!(reply.get("error").is_none());
    }
    let reply = one(&server, request(2, "tools/call", json!({"name": "status"}))).await;
    assert!(reply.get("error").is_none());
}

#[tokio::test]
async fn hostile_input_never_puts_anything_but_protocol_messages_on_the_writer() {
    let (server, _) = server();
    let oversized = "x".repeat(MAX_LINE_BYTES + 10);
    let deep = "[".repeat(200_000);
    let deep_object = "{\"a\":".repeat(5_000);
    let lines: Vec<String> = vec![
        "not json".into(),
        "{".into(),
        "}".into(),
        "null".into(),
        "true".into(),
        "42".into(),
        "\"text\"".into(),
        "[]".into(),
        "[{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"ping\"}]".into(),
        "{}".into(),
        "{\"jsonrpc\":\"2.0\"}".into(),
        "{\"jsonrpc\":\"2.0\",\"id\":1}".into(),
        "{\"jsonrpc\":\"2.0\",\"id\":null,\"method\":\"ping\"}".into(),
        "{\"jsonrpc\":\"2.0\",\"id\":[1],\"method\":\"ping\"}".into(),
        "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":null}".into(),
        "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"tools/call\",\"params\":7}".into(),
        "\u{feff}{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"ping\"}".into(),
        deep,
        deep_object,
        oversized,
        // Printable text that looks like a log line or a prompt.
        "INFO engine_mcp: starting".into(),
        "Enter passphrase: ".into(),
        "{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"ping\"}".into(),
    ];
    let mut input = lines.join("\n");
    input.push('\n');
    // Invalid UTF-8, a CRLF line, blank lines and a final line without a newline.
    let mut bytes = input.into_bytes();
    bytes.extend_from_slice(b"\xff\xfe\xfd\n\n  \n");
    bytes.extend_from_slice(b"{\"jsonrpc\":\"2.0\",\"id\":3,\"method\":\"ping\"}\r\n");
    bytes.extend_from_slice(b"{\"jsonrpc\":\"2.0\",\"id\":4,\"method\":\"ping\"}");
    let mut out = Vec::new();
    server.serve(&bytes[..], &mut out).await.unwrap();
    let text = String::from_utf8(out).expect("UTF-8 only");
    assert!(text.ends_with('\n'));
    let written: Vec<&str> = text.lines().collect();
    for line in &written {
        assert_protocol_line(line);
    }
    // Every ping that was a valid message was answered, after all the garbage.
    let answered: Vec<Value> = written
        .iter()
        .map(|l| serde_json::from_str::<Value>(l).unwrap())
        .filter(|v| v.get("result").is_some())
        .collect();
    let ids: Vec<&Value> = answered.iter().map(|v| &v["id"]).collect();
    assert_eq!(ids, [&json!(2), &json!(3), &json!(4)]);
    // None of the garbage is echoed back.
    assert!(!text.contains("passphrase") && !text.contains("starting"));
}

#[tokio::test]
async fn a_panicking_tool_costs_one_call_and_nothing_reaches_the_writer() {
    let (server, _) = server();
    let input = format!(
        "{}\n{}\n{}\n",
        call(1, "find", json!({"file": "7034d450", "q": "panic"})),
        request(2, "ping", json!({})),
        call(3, "find", json!({"file": "7034d450", "q": "fine"})),
    );
    let lines = run(&server, &input).await;
    assert_eq!(lines.len(), 3, "{lines:?}");
    for line in &lines {
        assert_protocol_line(line);
        assert!(!line.contains("recognisable"), "{line}");
    }
    let first: Value = serde_json::from_str(&lines[0]).unwrap();
    assert_eq!(first["result"]["isError"], true);
    assert!(first["result"]["content"][0]["text"]
        .as_str()
        .unwrap()
        .starts_with("dd/1 error code=internal "));
    let last: Value = serde_json::from_str(&lines[2]).unwrap();
    assert!(last["result"].get("isError").is_none());
}

#[tokio::test]
async fn the_loop_ends_cleanly_at_end_of_input() {
    let (server, _) = server();
    assert!(run(&server, "").await.is_empty());
    assert!(run(&server, "\n\n").await.is_empty());
}
