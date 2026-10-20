//! The real `engine-cli mcp` process with the write tools, against a loopback service that
//! plays the device-session routes and the agent routes: what it asks for, what it lists,
//! what it sends, and that standard output carries protocol messages and nothing else even
//! when a write is refused. The service is a stand-in written for this test; nothing here ran
//! against `engine-api`, a real MCP client or a model.
use axum::{
    body::Body,
    extract::State,
    http::{header, Request},
    response::Response,
    Router,
};
use engine_core::repository::device_session::{SessionChallenge, SessionProof};
use serde_json::{json, Value};
use std::{
    io::Write,
    path::PathBuf,
    process::{Command, Output, Stdio},
    sync::{Arc, Mutex},
};
use uuid::Uuid;

const PROJECT: &str = "33333333-3333-4333-8333-333333333333";
const AUTHORITY: &str = "55555555-5555-4555-8555-555555555555";
const PROPOSAL: &str = "4d0e9b1c-5f2a-4b8e-9c31-0a7d6e2f8b14";
/// What the stand-in service issues; it must appear nowhere in the process's output.
const TOKEN: &str = "v4.local.MOCK-TOKEN-SECRET";
const DCT: &str = "text/vnd.dynodoc.dct; v=1; charset=utf-8";

fn exe() -> &'static str {
    env!("CARGO_BIN_EXE_engine-cli")
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

// --- the stand-in service ---------------------------------------------------------------------

#[derive(Default)]
struct Log {
    /// The scopes of each challenge asked for, as sent.
    challenges: Vec<Vec<String>>,
    /// Method, path and whether the bearer was the issued token, per agent request.
    requests: Vec<(String, String, bool)>,
    /// The JSON bodies of the propose and submit requests.
    bodies: Vec<Value>,
}

struct Shared {
    log: Mutex<Log>,
    /// Refuse every challenge that asks for a write scope, as for a key without the grant.
    refuse_writes: bool,
}

struct Service {
    base: String,
    shared: Arc<Shared>,
}

impl Service {
    fn log<T>(&self, read: impl FnOnce(&Log) -> T) -> T {
        read(&self.shared.log.lock().unwrap())
    }
}

fn answer(body: String) -> Response {
    Response::builder()
        .header(header::CONTENT_TYPE, DCT)
        .body(Body::from(body))
        .unwrap()
}

fn status(code: u16) -> Response {
    Response::builder()
        .status(code)
        .body(Body::empty())
        .unwrap()
}

async fn handle(State(shared): State<Arc<Shared>>, request: Request<Body>) -> Response {
    let (parts, body) = request.into_parts();
    let bytes = axum::body::to_bytes(body, 1 << 20).await.unwrap();
    let body: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    let path = parts.uri.path().to_owned();
    let method = parts.method.to_string();

    if path.ends_with("/device-sessions/challenge") {
        let asked: Vec<String> = serde_json::from_value(body["scopes"].clone()).unwrap();
        let writes = asked
            .iter()
            .any(|s| s == "content:propose" || s == "content:submit");
        shared.log.lock().unwrap().challenges.push(asked.clone());
        if writes && shared.refuse_writes {
            return status(403);
        }
        let mut sorted = asked;
        sorted.sort();
        let challenge = SessionChallenge {
            version: 1,
            id: Uuid::new_v4(),
            authority: Uuid::parse_str(AUTHORITY).unwrap(),
            public_key: body["public_key"].as_str().unwrap().to_owned(),
            project: Uuid::parse_str(PROJECT).unwrap(),
            scopes: sorted,
            nonce: "ab".repeat(32),
            issued_at_ms: 1_000_000,
            expires_at_ms: 1_100_000,
            token_ttl_seconds: 3600,
        };
        let issued = json!({"challenge": challenge, "mac": "cd".repeat(32)});
        return Response::builder()
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(issued.to_string()))
            .unwrap();
    }
    if path.ends_with("/device-sessions") {
        let proof: SessionProof = serde_json::from_value(body).unwrap();
        if proof.verify_signature().is_err() {
            return status(401);
        }
        let granted = json!({"access_token": TOKEN, "expires_in": 3600});
        return Response::builder()
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(granted.to_string()))
            .unwrap();
    }

    let bearer = parts
        .headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        == Some(&format!("Bearer {TOKEN}"));
    {
        let mut log = shared.log.lock().unwrap();
        log.requests.push((method, path.clone(), bearer));
        if path.ends_with("/propose") || path.ends_with("/submit") || path.ends_with("/withdraw") {
            log.bodies.push(body.clone());
        }
    }
    if !bearer {
        return status(401);
    }
    if path.ends_with("/propose") {
        return answer(if body["dry_run"] == true {
            "dd/1 propose file=7034d450 rev=412 applied=0 would_apply=1 expanded=1 stale=0 trust=data ~tok=30\n~7C02 P #b7a210>a41c07 -6 +9\n".to_owned()
        } else {
            format!("dd/1 propose file=7034d450 proposal={PROPOSAL} rev=412 applied=1 expanded=1 stale=0 state=draft revision=1 trust=data ~tok=40\n~7C02 P #b7a210>a41c07 -6 +9\n")
        });
    }
    if path.ends_with("/submit") {
        return answer(format!("dd/1 submit action=submit proposal={PROPOSAL} file=7034d450 state=awaiting_person changed=0 behind=0 trust=data\n"));
    }
    if path.contains("/proposals/") {
        return answer(format!("dd/1 status what=proposal proposal={PROPOSAL} file=7034d450 state=draft base=412 head=412 behind=0 revision=1 edits=1 approvals=0 required=0 satisfied=1 conflicts=0 trust=data\n"));
    }
    if path.ends_with("/access") {
        return answer("dd/1 status what=access project=33333333 role=editor scopes=content:propose,content:read,content:submit write_mode=draft calc=none\n".to_owned());
    }
    status(404)
}

fn start(refuse_writes: bool) -> Service {
    let shared = Arc::new(Shared {
        log: Mutex::new(Log::default()),
        refuse_writes,
    });
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let base = format!("http://{}/", listener.local_addr().unwrap());
    let app = Router::new().fallback(handle).with_state(shared.clone());
    std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async move {
            let listener = tokio::net::TcpListener::from_std(listener).unwrap();
            axum::serve(listener, app).await.unwrap();
        });
    });
    Service { base, shared }
}

// --- the process --------------------------------------------------------------------------------

fn scratch() -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let dir = std::env::temp_dir().join(format!(
        "dynodoc-mcp-write-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir(&dir).unwrap();
    dir
}

/// A key made by the binary itself and a remote file that points at the stand-in service.
fn setup(service: &Service) -> (PathBuf, PathBuf, PathBuf) {
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
    let remote = dir.join("remote.json");
    std::fs::write(
        &remote,
        json!({"version": 1, "url": service.base, "authority": AUTHORITY, "public_key": public_key})
            .to_string(),
    )
    .unwrap();
    (dir, key, remote)
}

/// Run the process with `extra` arguments and environment, send `messages`, close its input.
fn converse(
    service: &Service,
    extra: &[&str],
    environment: &[(&str, &str)],
    messages: &[Value],
) -> (Output, Vec<Value>) {
    let (dir, key, remote) = setup(service);
    let mut command = Command::new(exe());
    command
        .arg("mcp")
        .arg("--remote")
        .arg(&remote)
        .args([
            "--project",
            PROJECT,
            "--allow-loopback-http",
            "--device-key",
        ])
        .arg(&key)
        .args(extra)
        .env_remove("DYNODOC_KEY_PASSPHRASE_FILE")
        .env_remove("DYNODOC_WRITE")
        .env("RUST_LOG", "trace");
    for (name, value) in environment {
        command.env(name, value);
    }
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    for message in messages {
        stdin.write_all(message.to_string().as_bytes()).unwrap();
        stdin.write_all(b"\n").unwrap();
    }
    drop(stdin);
    let output = child.wait_with_output().unwrap();
    let stdout = text(&output.stdout);
    let replies: Vec<Value> = stdout
        .lines()
        .map(|l| serde_json::from_str(l).unwrap_or_else(|_| panic!("not JSON on stdout: {l:?}")))
        .collect();
    for reply in &replies {
        assert_eq!(reply["jsonrpc"], "2.0", "{reply}");
        assert!(
            reply.get("id").is_some()
                && (reply.get("result").is_some() != reply.get("error").is_some()),
            "{reply}"
        );
    }
    let _ = std::fs::remove_dir_all(dir);
    (output, replies)
}

fn call(id: u64, name: &str, arguments: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "method": "tools/call",
           "params": {"name": name, "arguments": arguments}})
}

fn list(id: u64) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "method": "tools/list"})
}

fn names(reply: &Value) -> Vec<&str> {
    reply["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect()
}

fn result_text(reply: &Value) -> &str {
    reply["result"]["content"][0]["text"].as_str().unwrap()
}

/// Neither the issued token nor an `Authorization` header appears anywhere in the output.
fn assert_no_secrets(output: &Output) {
    for stream in [text(&output.stdout), text(&output.stderr)] {
        assert!(
            !stream.contains("MOCK-TOKEN"),
            "the token leaked:\n{stream}"
        );
        assert!(!stream.contains("Bearer"));
    }
}

// --- tests -------------------------------------------------------------------------------------

#[test]
fn with_write_the_process_asks_for_the_write_scopes_and_sends_only_what_it_is_told_to() {
    let service = start(false);
    let ops = json!([{"op": "set_text", "ref": "7C02", "text": "New wording.", "base": "b7a210", "why": "clearer"}]);
    let (output, replies) = converse(
        &service,
        &["--write"],
        &[],
        &[
            json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
                "protocolVersion": "2025-11-25", "capabilities": {},
                "clientInfo": {"name": "test", "version": "0"}}}),
            list(2),
            call(
                3,
                "propose",
                json!({"file": "7034d450", "rev": 412, "ops": ops, "dry_run": true}),
            ),
            call(
                4,
                "propose",
                json!({"file": "7034d450", "rev": 412, "ops": ops, "name": "Wording"}),
            ),
            call(5, "status", json!({"proposal": PROPOSAL})),
            call(
                6,
                "submit",
                json!({"proposal": PROPOSAL, "action": "submit", "note": "Ready."}),
            ),
            call(7, "status", json!({})),
        ],
    );
    assert!(output.status.success(), "{}", text(&output.stderr));
    assert_eq!(replies.len(), 7);
    assert_eq!(
        names(&replies[1]),
        ["files", "outline", "read", "find", "diff", "propose", "submit", "status"]
    );
    assert!(replies[0]["result"]["instructions"]
        .as_str()
        .unwrap()
        .contains("never edits the team version"));

    // The dry run says nothing was stored; the propose says it is only a proposal.
    assert!(result_text(&replies[2]).contains("note \"dry run: nothing was stored"));
    let proposed = result_text(&replies[3]);
    assert!(
        proposed.contains(&format!("proposal={PROPOSAL}")),
        "{proposed}"
    );
    assert!(
        proposed.contains("note \"proposal only, still a private draft; nothing is live."),
        "{proposed}"
    );
    assert!(result_text(&replies[4]).contains("state=draft"));
    // The service said a person must submit: shown plainly, not as a failure.
    assert!(replies[5]["result"].get("isError").is_none());
    assert!(result_text(&replies[5])
        .contains("note \"waiting for a person to submit; nothing is live."));
    assert!(result_text(&replies[6]).contains("what=access"));

    // One session, with the three scopes; then exactly the requests that were asked for, in
    // order: the propose did not lead to a submit.
    assert_eq!(
        service.log(|l| l.challenges.clone()),
        [["content:read", "content:propose", "content:submit"]]
    );
    let requests = service.log(|l| l.requests.clone());
    let base = format!("/projects/{PROJECT}/agent");
    let expected: Vec<(String, String, bool)> = [
        ("POST", format!("{base}/files/7034d450/propose")),
        ("POST", format!("{base}/files/7034d450/propose")),
        ("GET", format!("{base}/proposals/{PROPOSAL}")),
        ("POST", format!("{base}/proposals/{PROPOSAL}/submit")),
        ("GET", format!("{base}/access")),
    ]
    .into_iter()
    .map(|(m, p)| (m.to_owned(), p, true))
    .collect();
    assert_eq!(requests, expected);
    let bodies = service.log(|l| l.bodies.clone());
    assert_eq!(bodies[0]["dry_run"], true);
    assert_ne!(
        bodies[0]["request_id"], bodies[1]["request_id"],
        "each call has its own request id"
    );
    assert_eq!(bodies[2], json!({"note": "Ready."}));
    assert_no_secrets(&output);
}

#[test]
fn a_refusal_of_the_write_scopes_leaves_a_working_read_session_and_plain_write_errors() {
    let service = start(true);
    let ops = json!([{"op": "delete", "ref": "7C02"}]);
    let (output, replies) = converse(
        &service,
        &["--write"],
        &[],
        &[
            call(
                1,
                "propose",
                json!({"file": "7034d450", "rev": 412, "ops": ops}),
            ),
            call(
                2,
                "submit",
                json!({"proposal": PROPOSAL, "action": "submit"}),
            ),
            call(3, "status", json!({})),
            list(4),
        ],
    );
    assert!(output.status.success(), "{}", text(&output.stderr));
    assert_eq!(replies.len(), 4);
    for reply in &replies[..2] {
        assert_eq!(reply["result"]["isError"], true, "{reply}");
        assert_eq!(
            result_text(reply),
            "dd/1 error code=forbidden policy=write_scope hint=\"No access to this file or scope.\"\n"
        );
    }
    // Reads work on the fallback session.
    assert!(replies[2]["result"].get("isError").is_none());
    assert!(result_text(&replies[2]).contains("what=access"));
    assert_eq!(
        names(&replies[3]).len(),
        8,
        "the profile is the one asked for"
    );
    // The write set was asked for first, then the read scope alone; no write request was sent.
    assert_eq!(
        service.log(|l| l.challenges.clone()),
        [
            vec!["content:read", "content:propose", "content:submit"],
            vec!["content:read"]
        ]
    );
    let paths: Vec<String> = service.log(|l| l.requests.iter().map(|r| r.1.clone()).collect());
    assert_eq!(paths, [format!("/projects/{PROJECT}/agent/access")]);
    assert!(text(&output.stderr).contains("no session with the write scopes"));
    assert_no_secrets(&output);
}

#[test]
fn without_the_flag_nothing_asks_for_a_write_scope_and_the_write_tools_do_not_exist() {
    let service = start(false);
    let (output, replies) = converse(
        &service,
        &[],
        &[],
        &[
            list(1),
            call(2, "status", json!({})),
            call(
                3,
                "propose",
                json!({"file": "7034d450", "rev": 1, "ops": [{"op": "delete", "ref": "A"}]}),
            ),
            call(
                4,
                "submit",
                json!({"proposal": PROPOSAL, "action": "submit"}),
            ),
        ],
    );
    assert!(output.status.success(), "{}", text(&output.stderr));
    assert_eq!(
        names(&replies[0]),
        ["files", "outline", "read", "find", "diff", "status"]
    );
    assert!(replies[1]["result"].get("isError").is_none());
    // A hidden tool is a protocol error and sends nothing anywhere.
    for reply in &replies[2..] {
        assert_eq!(reply["error"]["code"], -32602, "{reply}");
    }
    assert_eq!(service.log(|l| l.challenges.clone()), [["content:read"]]);
    assert_eq!(service.log(|l| l.requests.len()), 1);
    assert_no_secrets(&output);
}

#[test]
fn the_environment_variable_turns_writing_on_and_only_a_clear_yes_does() {
    let service = start(false);
    for (value, tools) in [
        ("on", 8),
        ("1", 8),
        ("off", 6),
        ("0", 6),
        ("", 6),
        ("maybe", 6),
    ] {
        let (output, replies) = converse(&service, &[], &[("DYNODOC_WRITE", value)], &[list(1)]);
        assert!(output.status.success(), "{}", text(&output.stderr));
        assert_eq!(names(&replies[0]).len(), tools, "DYNODOC_WRITE={value:?}");
    }
    // The flag wins over a variable that says no.
    let (_, replies) = converse(
        &service,
        &["--write"],
        &[("DYNODOC_WRITE", "off")],
        &[list(1)],
    );
    assert_eq!(names(&replies[0]).len(), 8);
    assert!(
        service.log(|l| l.challenges.is_empty()),
        "listing tools needs no session"
    );
}

#[test]
fn a_write_call_with_bad_arguments_is_refused_in_the_process_and_stdout_stays_clean() {
    let service = start(false);
    let many: Vec<Value> = (0..201)
        .map(|_| json!({"op": "delete", "ref": "7C02"}))
        .collect();
    let (output, replies) = converse(
        &service,
        &["--write"],
        &[],
        &[
            call(
                1,
                "propose",
                json!({"file": "7034d450", "rev": 1, "ops": many}),
            ),
            call(
                2,
                "propose",
                json!({"file": "7034d450", "rev": 1, "ops": [{"op": "delete", "ref": "A", "surprise": 1}]}),
            ),
            call(
                3,
                "submit",
                json!({"proposal": PROPOSAL, "action": "merge"}),
            ),
        ],
    );
    assert!(output.status.success(), "{}", text(&output.stderr));
    let lines: Vec<&str> = replies.iter().map(result_text).collect();
    assert!(
        lines[0].starts_with("dd/1 error code=bad_request field=ops "),
        "{}",
        lines[0]
    );
    assert!(
        lines[1].starts_with("dd/1 error code=bad_request field=surprise op=0 "),
        "{}",
        lines[1]
    );
    assert!(
        lines[2].starts_with("dd/1 error code=bad_request field=action "),
        "{}",
        lines[2]
    );
    assert!(
        service.log(|l| l.requests.is_empty() && l.challenges.is_empty()),
        "nothing invalid reached the network, not even a session request"
    );
    assert_no_secrets(&output);
}

#[test]
fn help_names_the_write_switch() {
    let help = Command::new(exe())
        .args(["mcp", "--help"])
        .output()
        .unwrap();
    let help = text(&help.stdout);
    assert!(help.contains("--write"), "{help}");
    assert!(help.contains("DYNODOC_WRITE"), "{help}");
}
