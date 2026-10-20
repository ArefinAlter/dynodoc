//! A real process on real pipes: `Server::serve_stdio` with a backend that panics, every log
//! level on, and hostile lines in the input. The panic message must reach standard error and
//! never standard output; standard output must hold protocol messages only.
//!
//! This test has no harness (`harness = false`): the libtest harness prints to standard
//! output, which would hide exactly what is being checked. The test binary runs itself as
//! the server child when `ENGINE_MCP_STDIO_CHILD` is set.

use std::{
    io::Write,
    process::{Command, Stdio},
};

use engine_mcp::{Backend, CallContext, Profile, Server, ToolCall, ToolOutput};
use serde_json::{json, Value};

const CHILD: &str = "ENGINE_MCP_STDIO_CHILD";
const PANIC_TEXT: &str = "backend exploded: recognisable-panic-text";

struct Explodes;

impl Backend for Explodes {
    async fn call(&self, _ctx: CallContext, _call: ToolCall) -> ToolOutput {
        panic!("{PANIC_TEXT}");
    }
}

fn child() {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::TRACE)
        .with_writer(std::io::stderr)
        .init();
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            Server::new(Explodes, Profile::read_only())
                .serve_stdio()
                .await
                .unwrap();
        });
}

fn parent() {
    let mut process = Command::new(std::env::current_exe().unwrap())
        .env(CHILD, "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = process.stdin.take().unwrap();
    let requests = [
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25"}}).to_string(),
        "WARN something that looks like a log line".to_owned(),
        json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"status","arguments":{}}}).to_string(),
        "{".to_owned(),
        json!({"jsonrpc":"2.0","id":3,"method":"ping"}).to_string(),
        json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"files","arguments":{}}}).to_string(),
        json!({"jsonrpc":"2.0","id":5,"method":"tools/list"}).to_string(),
    ];
    for line in &requests {
        writeln!(stdin, "{line}").unwrap();
    }
    drop(stdin);
    let output = process.wait_with_output().unwrap();
    let stdout = String::from_utf8(output.stdout).expect("stdout is UTF-8");
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();

    assert!(output.status.success(), "the child died: {stderr}");
    let messages: Vec<Value> = stdout
        .lines()
        .map(|l| serde_json::from_str(l).unwrap_or_else(|_| panic!("stdout holds {l:?}")))
        .collect();
    for message in &messages {
        assert_eq!(message["jsonrpc"], "2.0");
        assert!(message.get("id").is_some());
        assert!(message.get("result").is_some() != message.get("error").is_some());
    }
    let ids: Vec<&Value> = messages.iter().map(|m| &m["id"]).collect();
    assert_eq!(
        ids,
        [
            &json!(1),
            &Value::Null,
            &json!(2),
            &Value::Null,
            &json!(3),
            &json!(4),
            &json!(5)
        ]
    );
    // Both panicking calls became tool errors, and the connection went on.
    for index in [2, 5] {
        assert_eq!(messages[index]["result"]["isError"], true);
        let text = messages[index]["result"]["content"][0]["text"]
            .as_str()
            .unwrap();
        assert!(text.starts_with("dd/1 error code=internal "), "{text}");
    }
    assert_eq!(messages[6]["result"]["tools"].as_array().unwrap().len(), 6);
    // The panic is reported where a person can see it, and only there.
    assert!(
        stderr.contains("recognisable-panic-text"),
        "the panic is not on stderr:\n{stderr}"
    );
    assert!(
        stderr.contains("tool call failed"),
        "no log line on stderr:\n{stderr}"
    );
    assert!(!stdout.contains("recognisable-panic-text") && !stdout.contains("tool call failed"));
    assert!(!stdout.contains("WARN"), "input was echoed");
    eprintln!(
        "stdio_process: ok ({} protocol messages, panic text on stderr only)",
        messages.len()
    );
}

fn main() {
    if std::env::var_os(CHILD).is_some() {
        child();
    } else {
        parent();
    }
}
