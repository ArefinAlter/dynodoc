//! The real `engine-cli mcp` process over its pipes: the handshake and tool list work,
//! standard output carries protocol messages and nothing else even with every log level
//! on and hostile lines on standard input, logs and failures go to standard error, and a
//! launch that cannot work fails before it serves. No network is used: the configured API
//! is a loopback port nothing listens on, so a tool call fails the way an unreachable
//! service does.
use serde_json::{json, Value};
use std::{
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
};

const PROJECT: &str = "33333333-3333-4333-8333-333333333333";
const AUTHORITY: &str = "55555555-5555-4555-8555-555555555555";

fn exe() -> &'static str {
    env!("CARGO_BIN_EXE_engine-cli")
}

fn scratch() -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let dir = std::env::temp_dir().join(format!(
        "dynodoc-mcp-stdio-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir(&dir).unwrap();
    dir
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// A key file made by the binary itself, and a remote file that points at a closed port.
struct Setup {
    dir: PathBuf,
    key: PathBuf,
    remote: PathBuf,
    public_key: String,
}

fn setup() -> Setup {
    let dir = scratch();
    let key = dir.join("agent.der");
    let output = Command::new(exe())
        .args(["device", "keygen", "--out"])
        .arg(&key)
        .stdin(Stdio::null())
        .env("RUST_LOG", "off")
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", text(&output.stderr));
    let public_key = text(&output.stdout)
        .split_whitespace()
        .find(|w| w.len() == 64 && w.bytes().all(|b| b.is_ascii_hexdigit()))
        .expect("keygen prints the public key")
        .to_owned();
    let closed = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/", closed.local_addr().unwrap());
    drop(closed);
    let remote = dir.join("remote.json");
    std::fs::write(
        &remote,
        json!({"version": 1, "url": url, "authority": AUTHORITY, "public_key": public_key})
            .to_string(),
    )
    .unwrap();
    Setup {
        dir,
        key,
        remote,
        public_key,
    }
}

fn mcp(s: &Setup, extra: &[&str]) -> Command {
    let mut command = Command::new(exe());
    command
        .arg("mcp")
        .arg("--remote")
        .arg(&s.remote)
        .args([
            "--project",
            PROJECT,
            "--allow-loopback-http",
            "--device-key",
        ])
        .arg(&s.key)
        .args(extra)
        .env_remove("DYNODOC_KEY_PASSPHRASE_FILE")
        .env("RUST_LOG", "trace");
    command
}

/// Start the process, send `input`, close its input and wait for it to stop.
fn converse(mut command: Command, input: &[u8]) -> Output {
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    stdin.write_all(input).unwrap();
    drop(stdin);
    child.wait_with_output().unwrap()
}

fn lines(messages: &[Value]) -> Vec<u8> {
    let mut bytes = Vec::new();
    for message in messages {
        bytes.extend_from_slice(message.to_string().as_bytes());
        bytes.push(b'\n');
    }
    bytes
}

#[test]
fn a_session_over_the_pipes_keeps_stdout_for_the_protocol() {
    let s = setup();
    let mut input = lines(&[
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{
            "protocolVersion":"2025-11-25","capabilities":{},
            "clientInfo":{"name":"test","version":"0"}}}),
        json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
        json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
    ]);
    // Lines that look like logs, prompts and broken JSON, in the middle of the stream.
    input.extend_from_slice(b"INFO engine_cli: hello\nEnter passphrase: \n{\n[]\n\xff\xfe\n");
    input.extend(lines(&[
        json!({"jsonrpc":"2.0","id":3,"method":"ping"}),
        json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"status","arguments":{}}}),
        json!({"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"propose","arguments":{}}}),
    ]));
    let output = converse(mcp(&s, &[]), &input);
    let stderr = text(&output.stderr);
    assert!(output.status.success(), "{stderr}");

    let stdout = text(&output.stdout);
    let messages: Vec<Value> = stdout
        .lines()
        .map(|l| serde_json::from_str(l).unwrap_or_else(|_| panic!("not JSON on stdout: {l:?}")))
        .collect();
    for m in &messages {
        assert_eq!(m["jsonrpc"], "2.0", "{m}");
        assert!(
            m.get("id").is_some() && (m.get("result").is_some() != m.get("error").is_some()),
            "{m}"
        );
    }
    let ids: Vec<&Value> = messages.iter().map(|m| &m["id"]).collect();
    // One answer per request, in order; five lines of garbage gave null-id errors.
    let null = Value::Null;
    assert_eq!(
        ids,
        [
            &json!(1),
            &json!(2),
            &null,
            &null,
            &null,
            &null,
            &null,
            &json!(3),
            &json!(4),
            &json!(5)
        ]
    );
    assert_eq!(messages[0]["result"]["serverInfo"]["name"], "dynodoc");
    let names: Vec<&str> = messages[1]["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        names,
        ["files", "outline", "read", "find", "diff", "status"]
    );
    // The status call could not reach the service: a tool error, not a protocol error.
    assert_eq!(messages[8]["result"]["isError"], true);
    let line = messages[8]["result"]["content"][0]["text"]
        .as_str()
        .unwrap();
    assert!(line.starts_with("dd/1 error code=busy "), "{line}");
    assert_eq!(
        messages[9]["error"]["code"], -32602,
        "a write tool does not exist"
    );

    // Logs went to stderr (RUST_LOG=trace), and nothing secret went anywhere.
    assert!(stderr.contains("ready on stdio"), "{stderr}");
    for stream in [&stdout, &stderr] {
        assert!(
            !stream.contains(&s.public_key),
            "the key's public half leaked"
        );
        assert!(!stream.contains("Bearer"));
    }
    assert!(!stdout.contains("INFO") && !stdout.contains("ready on stdio"));
    let _ = std::fs::remove_dir_all(&s.dir);
}

#[test]
fn a_launch_that_cannot_work_fails_before_serving_and_says_why_on_stderr() {
    let s = setup();
    let bad_remote = dir_file(&s.dir, "bad-remote.json", "{\"version\":1}");
    let cleartext = dir_file(
        &s.dir,
        "http-remote.json",
        &json!({"version":1,"url":"http://example.test/","authority":AUTHORITY,"public_key":s.public_key}).to_string(),
    );
    let launch = |remote: &Path, key: &Path| {
        let mut command = Command::new(exe());
        command
            .args([
                "mcp",
                "--project",
                PROJECT,
                "--allow-loopback-http",
                "--remote",
            ])
            .arg(remote)
            .arg("--device-key")
            .arg(key)
            .env("RUST_LOG", "trace");
        command
    };
    let missing_key = launch(&s.remote, &s.dir.join("missing.der"));
    let invalid_remote = launch(&bad_remote, &s.key);
    let cleartext_remote = launch(&cleartext, &s.key);
    for (label, command) in [
        ("missing key", missing_key),
        ("invalid remote", invalid_remote),
        ("cleartext remote", cleartext_remote),
    ] {
        let output = converse(
            command,
            b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"ping\"}\n",
        );
        assert!(!output.status.success(), "{label}");
        assert!(
            output.stdout.is_empty(),
            "{label}: stdout was {}",
            text(&output.stdout)
        );
        assert!(!output.stderr.is_empty(), "{label}: no explanation");
    }
    let _ = std::fs::remove_dir_all(&s.dir);
}

fn dir_file(dir: &Path, name: &str, content: &str) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, content).unwrap();
    path
}

#[test]
fn help_lists_the_options_and_the_whole_cli_still_starts() {
    // The extra subcommand must not push the generated parser past the main-thread stack.
    let top = Command::new(exe()).arg("--help").output().unwrap();
    assert!(top.status.success(), "{}", text(&top.stderr));
    assert!(text(&top.stdout).contains("mcp"));
    let help = Command::new(exe())
        .args(["mcp", "--help"])
        .output()
        .unwrap();
    assert!(help.status.success());
    let help = text(&help.stdout);
    for flag in [
        "--remote",
        "--project",
        "--device-key",
        "--allow-loopback-http",
    ] {
        assert!(help.contains(flag), "{flag} missing from:\n{help}");
    }
}
