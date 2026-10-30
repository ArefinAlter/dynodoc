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
        // `q` of the read tools; the `name` of a proposal does the same for the write tools.
        let query = call
            .args
            .get("q")
            .or_else(|| call.args.get("name"))
            .and_then(Value::as_str)
            .unwrap_or("");
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

struct TestConfirmation(bool);
impl engine_mcp::server::HumanConfirmation for TestConfirmation {
    fn confirm<'a>(
        &'a self,
        _: Tool,
        _: &'a serde_json::Map<String, Value>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = bool> + Send + 'a>> {
        Box::pin(async move { self.0 })
    }
}

/// The profile of `engine-cli mcp --write`.
fn full_server() -> (Server<Scripted>, Scripted) {
    let backend = Scripted::default();
    (
        Server::with_human_confirmation(
            backend.clone(),
            Profile::full(),
            Arc::new(TestConfirmation(true)),
        ),
        backend,
    )
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
        assert_eq!(result["capabilities"]["resources"]["listChanged"], false);
        assert_eq!(result["capabilities"]["prompts"]["listChanged"], false);
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
        "logging/setLevel",
        "tools/run",
        "sampling/createMessage",
        "roots/list",
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
    let (server, backend) = server();
    for (params, label) in [
        (json!({"name": "propose", "arguments": {}}), "write tool"),
        (
            json!({"name": "submit", "arguments": {"proposal": "4d0e9b1c-5f2a-4b8e-9c31-0a7d6e2f8b14", "action": "submit"}}),
            "write tool with valid arguments",
        ),
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
    // A hidden tool is refused before anything else happens: the backend never saw one.
    assert!(
        backend.seen.lock().unwrap().is_empty(),
        "a write tool of the default profile reached the backend"
    );
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

#[tokio::test]
async fn write_tools_need_a_trusted_per_request_confirmation() {
    let backend = Scripted::default();
    let no_sink = Server::new(backend.clone(), Profile::full());
    let listed = one(&no_sink, request(1, "tools/list", json!({}))).await;
    assert!(!listed["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .any(|t| t["name"] == "propose"));
    let refused = one(
        &no_sink,
        request(
            2,
            "tools/call",
            json!({"name":"propose", "arguments": propose_call(json!([{ "op": "delete", "ref": "7C02" }]))}),
        ),
    )
    .await;
    assert_eq!(refused["error"]["code"], -32602);
    let deny = Server::with_human_confirmation(
        backend.clone(),
        Profile::full(),
        Arc::new(TestConfirmation(false)),
    );
    let refused = one(
        &deny,
        request(
            3,
            "tools/call",
            json!({"name":"propose", "arguments": propose_call(json!([{ "op": "delete", "ref": "7C02" }]))}),
        ),
    )
    .await;
    assert_eq!(refused["result"]["isError"], true);
    assert!(backend.seen.lock().unwrap().is_empty());
}

// --- the full profile (`--write`) ---------------------------------------------------------------

const PROPOSAL: &str = "4d0e9b1c-5f2a-4b8e-9c31-0a7d6e2f8b14";

fn propose_call(ops: Value) -> Value {
    json!({"file": "7034d450", "rev": 412, "ops": ops})
}

#[tokio::test]
async fn the_full_profile_lists_eight_tools_and_the_default_still_lists_six() {
    let (server, _) = full_server();
    let first = one(&server, request(1, "tools/list", json!({}))).await;
    let again = one(&server, request(1, "tools/list", json!({}))).await;
    assert_eq!(first.to_string(), again.to_string());
    let tools = first["result"]["tools"].as_array().unwrap();
    let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
    assert_eq!(
        names,
        ["files", "outline", "read", "find", "diff", "propose", "submit", "status"]
    );
    for tool in tools {
        let write = matches!(tool["name"].as_str().unwrap(), "propose" | "submit");
        let hints = &tool["annotations"];
        assert_eq!(hints["readOnlyHint"], !write, "{tool}");
        assert_eq!(hints["destructiveHint"], false);
        assert_eq!(hints["openWorldHint"], false);
        assert_eq!(hints["idempotentHint"] == true, write, "{tool}");
        assert_eq!(tool["inputSchema"]["additionalProperties"], false);
    }
    // The handshake says what this connection is.
    let reply = one(
        &server,
        request(2, "initialize", json!({"protocolVersion": "2025-11-25"})),
    )
    .await;
    let instructions = reply["result"]["instructions"].as_str().unwrap();
    assert!(instructions.chars().count() <= 500);
    assert!(instructions.contains("never edits the team version"));
    assert_eq!(reply["result"]["_meta"]["dynodoc/v1"]["profile"], "write");
    // The default profile, on its own server, is untouched.
    let (read_only, _) = server_with_read_only_profile();
    let list = one(&read_only, request(3, "tools/list", json!({}))).await;
    assert_eq!(list["result"]["tools"].as_array().unwrap().len(), 6);
}

fn server_with_read_only_profile() -> (Server<Scripted>, Scripted) {
    server()
}

#[tokio::test]
async fn nothing_is_called_unless_the_client_calls_it() {
    let (server, backend) = full_server();
    let input = [
        request(1, "initialize", json!({"protocolVersion": "2026-07-28"})),
        json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
        request(2, "tools/list", json!({})),
        request(3, "server/discover", json!({})),
        request(4, "ping", json!({})),
        json!({"jsonrpc": "2.0", "method": "notifications/cancelled", "params": {"requestId": 3}}),
    ]
    .map(|m| format!("{m}\n"))
    .concat();
    run(&server, &input).await;
    assert!(
        backend.seen.lock().unwrap().is_empty(),
        "the server called a tool by itself"
    );
    // One call from the client is one call to the backend, and a propose is not followed by
    // a submit.
    let ops = json!([{"op": "delete", "ref": "7C02"}]);
    let reply = one(&server, call(5, "propose", propose_call(ops))).await;
    assert!(reply["result"].get("isError").is_none(), "{reply}");
    let seen = backend.seen.lock().unwrap();
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].1.tool, Tool::Propose);
}

#[tokio::test]
async fn bad_write_arguments_are_tool_errors_naming_the_field_and_never_reach_the_backend() {
    let (server, backend) = full_server();
    let op = || json!({"op": "delete", "ref": "7C02"});
    let with = |key: &str, value: Value| {
        let mut a = propose_call(json!([op()]));
        a[key] = value;
        a
    };
    let many: Vec<Value> = (0..201).map(|_| op()).collect();
    let cases: Vec<(&str, Value, &str)> = vec![
        // Unknown fields, at the top and inside an op.
        ("propose", with("surprise", json!(1)), "field=surprise "),
        (
            "propose",
            propose_call(json!([op(), {"op": "delete", "ref": "7C02", "surprise": 1}])),
            "field=surprise op=1 ",
        ),
        // Missing or malformed.
        (
            "propose",
            json!({"file": "7034d450", "rev": 412}),
            "field=ops ",
        ),
        (
            "propose",
            json!({"file": "7034d450", "ops": [op()]}),
            "field=rev ",
        ),
        ("propose", propose_call(json!([])), "field=ops "),
        ("propose", propose_call(json!(many)), "field=ops "),
        (
            "propose",
            propose_call(json!([op(), {"op": "merge", "ref": "7C02"}])),
            "field=op op=1 ",
        ),
        (
            "propose",
            propose_call(json!([{"ref": "7C02"}])),
            "field=op op=0 ",
        ),
        (
            "propose",
            propose_call(json!([{"op": "replace_all", "find": "x", "with": "y", "nth": 0}])),
            "field=nth op=0 ",
        ),
        (
            "propose",
            propose_call(json!([{"op": "insert", "after": "7C02", "blocks": [{"t": "p"}]}])),
            "field=blocks op=0 ",
        ),
        (
            "propose",
            with("request_id", json!("b1f1c9f2")),
            "field=request_id ",
        ),
        (
            "propose",
            with("proposal", json!("4d0e9b1c")),
            "field=proposal ",
        ),
        ("propose", with("dry_run", json!("yes")), "field=dry_run "),
        (
            "propose",
            with("return", json!("everything")),
            "field=return ",
        ),
        (
            "propose",
            with("name", json!("n".repeat(201))),
            "field=name ",
        ),
        (
            "propose",
            propose_call(json!([{"op": "set_text", "ref": "7C02", "text": "x".repeat(100_001)}])),
            "field=text op=0 ",
        ),
        // The spec lists these ops; the service does not compile them, so they are not offered.
        (
            "propose",
            propose_call(json!([{"op": "macro", "name": "normalize_headings"}])),
            "field=op op=0 ",
        ),
        (
            "propose",
            propose_call(
                json!([{"op": "fill", "sheet": "S", "from": "A1", "to": "A9", "dir": "down"}]),
            ),
            "field=op op=0 ",
        ),
        // submit and status.
        ("submit", json!({"action": "submit"}), "field=proposal "),
        ("submit", json!({"proposal": PROPOSAL}), "field=action "),
        (
            "submit",
            json!({"proposal": PROPOSAL, "action": "approve"}),
            "field=action ",
        ),
        (
            "submit",
            json!({"proposal": PROPOSAL, "action": "merge"}),
            "field=action ",
        ),
        (
            "submit",
            json!({"proposal": "4d0e9b1c", "action": "submit"}),
            "field=proposal ",
        ),
        (
            "submit",
            json!({"proposal": PROPOSAL, "action": "submit", "note": "n".repeat(1001)}),
            "field=note ",
        ),
        (
            "submit",
            json!({"proposal": PROPOSAL, "action": "submit", "extra": 1}),
            "field=extra ",
        ),
        ("status", json!({"what": "proposals"}), "field=what "),
        ("status", json!({"proposal": "4d0e9b1c"}), "field=proposal "),
        ("status", json!({"file": "7034d450"}), "field=file "),
    ];
    for (name, arguments, expected) in cases {
        let reply = one(&server, call(1, name, arguments.clone())).await;
        assert_eq!(reply["result"]["isError"], true, "{name} {arguments}");
        let text = reply["result"]["content"][0]["text"].as_str().unwrap();
        assert!(
            text.starts_with(&format!("dd/1 error code=bad_request {expected}")),
            "{name} {arguments}: {text}"
        );
    }
    assert!(
        backend.seen.lock().unwrap().is_empty(),
        "nothing invalid reached the backend"
    );
}

#[tokio::test]
async fn good_write_arguments_reach_the_backend_as_they_are() {
    let (server, backend) = full_server();
    let ops = json!([
        {"op": "set_text", "ref": "7C02", "text": "New.", "base": "b7a210", "why": "clearer", "id": "a1"},
        {"op": "replace", "ref": "7D91", "find": "policy", "with": "procedure", "nth": 2},
        {"op": "replace_all", "find": "clause", "with": "article", "expect": 80, "args": {"word": true}},
        {"op": "insert", "after": "7C02", "blocks": [{"t": "p", "text": "Added."}]},
        {"op": "set_cells", "sheet": "Budget", "at": "J2", "values": [["review"], ["review"]]},
        {"op": "clear", "range": "Budget!A1:C3"},
        {"op": "set_shape", "slide": "s2", "shape": "W2SK", "text": "T"},
        {"op": "set_notes", "slide": "s2", "text": "N"},
        {"op": "move", "ref": "7C02", "before": "7D91"},
        {"op": "set_type", "ref": "7C02", "t": "h2"},
        {"op": "delete", "ref": "$1"}
    ]);
    let mut arguments = propose_call(ops);
    arguments["dry_run"] = json!(true);
    arguments["strict"] = json!(false);
    arguments["return"] = json!("diff");
    arguments["name"] = json!("Wording");
    arguments["request_id"] = json!("b1f1c9f2-3c4d-4e5f-8a6b-7c8d9e0f1a2b");
    arguments["proposal"] = json!(PROPOSAL);
    let reply = one(&server, call(1, "propose", arguments.clone())).await;
    assert!(reply["result"].get("isError").is_none(), "{reply}");
    for (name, arguments) in [
        (
            "submit",
            json!({"proposal": PROPOSAL, "action": "withdraw", "note": "Superseded."}),
        ),
        ("status", json!({"proposal": PROPOSAL})),
        ("status", json!({"what": "proposal", "proposal": PROPOSAL})),
        ("status", json!({"what": "access"})),
        ("status", json!({})),
    ] {
        let reply = one(&server, call(2, name, arguments.clone())).await;
        assert!(reply["result"].get("isError").is_none(), "{name} {reply}");
    }
    let seen = backend.seen.lock().unwrap();
    assert_eq!(seen.len(), 6);
    assert_eq!(Value::Object(seen[0].1.args.clone()), arguments);
}

#[tokio::test]
async fn a_failing_or_panicking_write_costs_one_call_and_nothing_reaches_the_writer() {
    let (server, _) = full_server();
    let ops = json!([{"op": "delete", "ref": "7C02"}]);
    let named = |name: &str| {
        let mut a = propose_call(ops.clone());
        a["name"] = json!(name);
        a
    };
    let input = format!(
        "{}\n{}\n{}\n{}\n",
        call(1, "propose", named("panic")),
        call(2, "propose", named("fail")),
        request(3, "ping", json!({})),
        call(4, "propose", named("fine")),
    );
    let lines = run(&server, &input).await;
    assert_eq!(lines.len(), 4, "{lines:?}");
    for line in &lines {
        assert_protocol_line(line);
        assert!(!line.contains("recognisable"), "{line}");
    }
    let results: Vec<Value> = lines
        .iter()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    let text = |i: usize| {
        results[i]["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .to_owned()
    };
    assert!(text(0).starts_with("dd/1 error code=internal "));
    assert!(text(1).starts_with("dd/1 error code=not_found "));
    assert_eq!(results[1]["result"]["isError"], true);
    assert!(results[3]["result"].get("isError").is_none());
}

#[tokio::test]
async fn test_resources_list_and_read() {
    let (server, _) = server();
    let reply = one(&server, request(1, "resources/list", json!({}))).await;
    let list = &reply["result"]["resources"];
    assert!(list.as_array().unwrap().len() >= 3);
    assert_eq!(list[0]["uri"], "dynodoc://schema/document-ir");
    assert_eq!(list[1]["uri"], "dynodoc://schema/change-request");
    assert_eq!(list[2]["uri"], "dynodoc://projects");

    let read_reply = one(
        &server,
        request(
            2,
            "resources/read",
            json!({"uri": "dynodoc://schema/document-ir"}),
        ),
    )
    .await;
    let contents = &read_reply["result"]["contents"];
    assert_eq!(contents[0]["uri"], "dynodoc://schema/document-ir");
    assert!(contents[0]["text"]
        .as_str()
        .unwrap()
        .contains("Dynodoc Document Event IR"));

    let proj_reply = one(
        &server,
        request(3, "resources/read", json!({"uri": "dynodoc://projects"})),
    )
    .await;
    assert!(proj_reply["result"]["contents"][0]["text"]
        .as_str()
        .unwrap()
        .contains("dynodoc"));
}

#[tokio::test]
async fn test_prompts_list_and_get() {
    let (server, _) = server();
    let reply = one(&server, request(1, "prompts/list", json!({}))).await;
    let list = &reply["result"]["prompts"];
    assert_eq!(list.as_array().unwrap().len(), 3);
    assert_eq!(list[0]["name"], "audit_document_history");
    assert_eq!(list[1]["name"], "review_change_request");
    assert_eq!(list[2]["name"], "summarize_document_diff");

    let get_reply = one(
        &server,
        request(
            2,
            "prompts/get",
            json!({
                "name": "audit_document_history",
                "arguments": {
                    "file": "Handbook.docx",
                    "branch": "main"
                }
            }),
        ),
    )
    .await;
    let msg = &get_reply["result"]["messages"][0];
    assert_eq!(msg["role"], "user");
    assert!(msg["content"]["text"]
        .as_str()
        .unwrap()
        .contains("Handbook.docx"));
}

#[tokio::test]
async fn test_snake_case_tool_aliases() {
    let (server, backend) = server();
    let reply = one(&server, call(1, "dynodoc_status", json!({}))).await;
    assert_eq!(reply["result"]["isError"].as_bool(), None);
    let seen = backend.seen.lock().unwrap();
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].1.tool, Tool::Status);
}
