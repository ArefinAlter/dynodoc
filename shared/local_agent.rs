//! Subscription-backed inference through installed, signed-in agent programs.
//!
//! This module never reads another program's credentials. Prompts go over stdin,
//! never a shell command line. The caller still owns consent, offline mode,
//! budgets and the closed product tool table. No API-key fallback occurs here.

use serde_json::{json, Value};
use std::{
    path::{Path, PathBuf},
    process::Stdio,
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const MAX_REQUEST: usize = 2 * 1024 * 1024;
const MAX_RESPONSE: usize = 2 * 1024 * 1024;

/// The reserved endpoint is intercepted locally; it is never requested over HTTP.
pub fn agent_for_url(url: &str) -> Option<&'static str> {
    ["codex", "claude", "cursor", "antigravity"]
        .into_iter()
        .find(|name| {
            let prefix = format!("https://{name}.local-agent.invalid/");
            url.starts_with(&prefix)
        })
}

/// An agent-specific setup failure, with no subprocess output or credential data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    /// The provider program is not installed.
    Missing,
    /// The provider refused or failed the request.
    Failed,
    /// The configured request deadline elapsed.
    Timeout,
    /// The input or output exceeded its byte limit.
    TooLarge,
    /// The provider did not finish its documented response protocol.
    Protocol,
    /// No adapter exists for this provider.
    Unsupported,
    /// A model identifier contains characters outside the supported set.
    InvalidModel,
    /// The installed program is signed in with a separately billed API account.
    NotSubscription,
    /// The installed agent's own policy could allow tools outside the product gate.
    ToolPolicy,
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::ToolPolicy => "Before using Antigravity here, set toolPermission to strict in its CLI settings (~/.gemini/antigravity-cli/settings.json). This app will not send a request while the CLI can approve tools independently.",
            Self::NotSubscription => "Sign in to the installed agent with your subscription account. Its current sign-in is not a verified subscription connection.",
            Self::Missing => "Install this provider's CLI and sign in to your subscription, then try again.",
            Self::Failed => "The installed agent did not complete the request. Check its sign-in, plan limits and selected model in its own app. This app did not fall back to another provider.",
            Self::Timeout => "The installed agent did not finish in time. The request was stopped.",
            Self::TooLarge => "The installed agent request or response exceeded the size limit.",
            Self::Protocol => "The installed agent returned an incomplete or unsupported response.",
            Self::Unsupported => "This subscription has no configured local agent connection. Use a supported installed agent or an API key.",
            Self::InvalidModel => "The selected model identifier is not valid.",
        })
    }
}
impl std::error::Error for Error {}

fn candidates(program: &str) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).collect())
        .unwrap_or_default();
    if let Some(home) = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")) {
        let home = PathBuf::from(home);
        dirs.extend([home.join(".local/bin"), home.join(".cargo/bin")]);
    }
    if let Some(appdata) = std::env::var_os("APPDATA") {
        dirs.push(PathBuf::from(appdata).join("npm"));
    }
    let names: Vec<String> = if cfg!(windows) {
        vec![format!("{program}.exe"), format!("{program}.cmd")]
    } else {
        vec![program.to_owned()]
    };
    dirs.into_iter()
        .flat_map(|d| names.iter().map(move |n| d.join(n)))
        .collect()
}
fn program(agent: &str) -> Result<PathBuf, Error> {
    let name = match agent {
        "codex" => "codex",
        "claude" => "claude",
        "cursor" => "agent",
        "antigravity" => "agy",
        _ => return Err(Error::Unsupported),
    };
    candidates(name)
        .into_iter()
        .find(|p| p.is_file())
        .ok_or(Error::Missing)
}

/// Availability is executable discovery, not an entitlement or authentication claim.
pub fn installed(agent: &str) -> bool {
    program(agent).is_ok()
}

struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Result<Self, Error> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| Error::Failed)?
            .as_nanos();
        for _ in 0..16 {
            let path = std::env::temp_dir().join(format!(
                "verne-agent-{}-{time}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            #[allow(unused_mut)]
            let mut builder = std::fs::DirBuilder::new();
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt;
                builder.mode(0o700);
            }
            match builder.create(&path) {
                Ok(()) => return Ok(Self(path)),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(_) => return Err(Error::Failed),
            }
        }
        Err(Error::Failed)
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn cursor_policy() -> Value {
    let mut deny: Vec<String> = [
        "Shell(*)",
        "WebFetch(*)",
        "Mcp(*:*)",
        "Read(**)",
        "Write(**)",
        "Read(/**)",
        "Write(/**)",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    // Absolute Windows drives and UNC roots are distinct from workspace globs.
    for drive in b'A'..=b'Z' {
        for letter in [drive as char, (drive as char).to_ascii_lowercase()] {
            for action in ["Read", "Write"] {
                deny.push(format!("{action}({letter}:/**)"));
            }
        }
    }
    deny.extend(["Read(//**)".into(), "Write(//**)".into()]);
    json!({"permissions":{"allow":[],"deny":deny}})
}
fn prepare_workspace(path: &Path, agent: &str) -> Result<(), Error> {
    if agent == "cursor" {
        let dir = path.join(".cursor");
        std::fs::create_dir(&dir).map_err(|_| Error::Failed)?;
        let bytes = serde_json::to_vec(&cursor_policy()).map_err(|_| Error::Failed)?;
        std::fs::write(dir.join("cli.json"), bytes).map_err(|_| Error::Failed)?;
    }
    Ok(())
}
fn antigravity_strict(bytes: &[u8]) -> bool {
    // Deserialize only the documented policy field, discarding all other settings.
    #[derive(serde::Deserialize)]
    struct Policy {
        #[serde(rename = "toolPermission")]
        tool_permission: Option<String>,
    }
    serde_json::from_slice::<Policy>(bytes)
        .ok()
        .is_some_and(|p| p.tool_permission.as_deref() == Some("strict"))
}
fn check_tool_policy(agent: &str) -> Result<(), Error> {
    if agent != "antigravity" {
        return Ok(());
    }
    let home = std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .ok_or(Error::ToolPolicy)?;
    let path = PathBuf::from(home).join(".gemini/antigravity-cli/settings.json");
    let file = std::fs::File::open(path).map_err(|_| Error::ToolPolicy)?;
    let mut bytes = zeroize::Zeroizing::new(Vec::new());
    std::io::Read::read_to_end(&mut std::io::Read::take(file, 65537), &mut bytes)
        .map_err(|_| Error::ToolPolicy)?;
    if bytes.len() > 65536 || !antigravity_strict(&bytes) {
        return Err(Error::ToolPolicy);
    }
    Ok(())
}

fn command_args(agent: &str, model: &str) -> Result<Vec<String>, Error> {
    if model.starts_with('-')
        || model.len() > 200
        || !model
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._:/@+-".contains(&b))
    {
        return Err(Error::InvalidModel);
    }
    let mut args: Vec<String> = match agent {
        #[cfg(test)]
        "stub" => vec![
            "--exact",
            "local_agent::tests::stub_child",
            "--ignored",
            "--nocapture",
        ],
        "claude" => vec![
            "--print",
            "--output-format",
            "json",
            "--tools",
            "",
            "--strict-mcp-config",
            "--mcp-config",
            "{\"mcpServers\":{}}",
            "--no-session-persistence",
            "--disable-slash-commands",
            "--setting-sources",
            "",
            "--settings",
            "{\"disableAllHooks\":true}",
        ],
        "codex" => vec![
            "exec",
            "--json",
            "--ephemeral",
            "--skip-git-repo-check",
            "--sandbox",
            "read-only",
            "-c",
            "approval_policy=\"never\"",
            "-c",
            "features.shell_tool=false",
            "-c",
            "features.unified_exec=false",
            "-c",
            "features.apply_patch_freeform=false",
            "-c",
            "web_search=\"disabled\"",
            "-c",
            "mcp_servers={}",
            "-c",
            "model_provider=\"openai\"",
        ],
        "cursor" => vec![
            "--print",
            "--mode",
            "ask",
            "--output-format",
            "json",
            "--trust",
        ],
        "antigravity" => vec![
            "--print",
            "--input-format",
            "stream-json",
            "--output-format",
            "stream-json",
            "--mode",
            "plan",
            "--disable-slash-commands",
            "--sandbox",
        ],
        _ => return Err(Error::Unsupported),
    }
    .into_iter()
    .map(str::to_owned)
    .collect();
    #[cfg(test)]
    if agent == "stub" {
        let module = module_path!()
            .split_once("::")
            .map(|(_, m)| m)
            .unwrap_or(module_path!());
        args[1] = format!("{module}::tests::stub_child");
    }
    if !model.is_empty() && model != "default" {
        args.extend(["--model".into(), model.into()]);
    }
    if agent == "codex" {
        args.push("-".into());
    }
    Ok(args)
}

fn prompt(body: &Value) -> Result<String, Error> {
    let request = serde_json::to_string(body).map_err(|_| Error::Protocol)?;
    if request.len() > MAX_REQUEST {
        return Err(Error::TooLarge);
    }
    Ok(format!(
        "You are providing one model completion for a Verne application. Follow the system instructions in the request below. Do not use your own filesystem, terminal, browser or MCP tools. The application's tools, when listed, are declarations only: request them in your answer; the application will validate them and obtain any required human approval. Do not perform those operations yourself. If tools are provided and a call is needed, return JSON {{\"tool_calls\":[{{\"id\":\"call_1\",\"type\":\"function\",\"function\":{{\"name\":\"the_listed_name\",\"arguments\":\"{{}}\"}}}}]}}. Otherwise answer normally, preserving any JSON response format required by the request.\n\nModel request:\n{request}"
    ))
}

fn completion(agent: &str, bytes: &[u8]) -> Result<(String, Value), Error> {
    let text = std::str::from_utf8(bytes).map_err(|_| Error::Protocol)?;
    let whole = serde_json::from_str::<Value>(text).ok();
    let events: Vec<Value> = if let Some(v) = whole {
        vec![v]
    } else {
        text.lines()
            .filter(|l| !l.trim().is_empty())
            .map(serde_json::from_str)
            .collect::<Result<_, _>>()
            .map_err(|_| Error::Protocol)?
    };
    let mut answer = None;
    let mut usage = Value::Null;
    let mut done = false;
    for event in events {
        if event["is_error"] == true
            || matches!(event["type"].as_str(), Some("error" | "turn.failed"))
        {
            return Err(Error::Failed);
        }
        match agent {
            "codex" => match event["type"].as_str() {
                Some("item.completed") if event["item"]["type"] == "agent_message" => {
                    answer = event["item"]["text"].as_str().map(str::to_owned);
                }
                Some("turn.completed") => {
                    usage = event["usage"].clone();
                    done = true;
                }
                _ => {}
            },
            "antigravity" => {
                let result = if event["event"] == "result" {
                    &event["result"]
                } else {
                    &event
                };
                if let Some(status) = result["status"].as_str() {
                    if status != "SUCCESS" {
                        return Err(Error::Failed);
                    }
                    answer = result["response"].as_str().map(str::to_owned);
                    usage = result["usage"].clone();
                    done = true;
                }
            }
            _ => {
                if let Some(result) = event["result"].as_str() {
                    answer = Some(result.into());
                    usage = event["usage"].clone();
                    if agent == "claude" {
                        let total = usage["input_tokens"]
                            .as_u64()
                            .unwrap_or(0)
                            .saturating_add(usage["cache_read_input_tokens"].as_u64().unwrap_or(0))
                            .saturating_add(
                                usage["cache_creation_input_tokens"].as_u64().unwrap_or(0),
                            );
                        usage["input_tokens"] = json!(total);
                    }
                    done = true;
                }
            }
        }
    }
    if !done {
        return Err(Error::Protocol);
    }
    Ok((
        answer
            .filter(|s| !s.trim().is_empty())
            .ok_or(Error::Protocol)?,
        usage,
    ))
}

fn envelope(text: String, usage: Value, body: &Value) -> Result<Value, Error> {
    let parsed = serde_json::from_str::<Value>(text.trim()).ok();
    let mut message = json!({"role":"assistant", "content":text});
    let mut reason = "stop";
    if let Some(calls) = parsed
        .as_ref()
        .and_then(|v| v.get("tool_calls"))
        .and_then(Value::as_array)
    {
        if calls.is_empty() || calls.len() > 16 {
            return Err(Error::Protocol);
        }
        let tools = body["tools"].as_array().ok_or(Error::Protocol)?;
        for call in calls {
            let name = call["function"]["name"].as_str().ok_or(Error::Protocol)?;
            if !tools.iter().any(|t| t["function"]["name"] == name)
                || call["function"]["arguments"]
                    .as_str()
                    .and_then(|s| serde_json::from_str::<Value>(s).ok())
                    .is_none()
            {
                return Err(Error::Protocol);
            }
        }
        message["content"] = parsed
            .as_ref()
            .and_then(|v| v.get("content"))
            .cloned()
            .unwrap_or(Value::Null);
        let mut calls = calls.clone();
        for (index, call) in calls.iter_mut().enumerate() {
            if call["id"].as_str().is_none_or(str::is_empty) {
                call["id"] = json!(format!("local_call_{index}"));
            }
            call["type"] = json!("function");
        }
        message["tool_calls"] = Value::Array(calls);
        reason = "tool_calls";
    }
    let input = usage
        .get("input_tokens")
        .or_else(|| usage.get("prompt_tokens"))
        .and_then(Value::as_u64);
    let output = usage
        .get("output_tokens")
        .or_else(|| usage.get("completion_tokens"))
        .and_then(Value::as_u64);
    let mut result = json!({"choices":[{"message":message,"finish_reason":reason}]});
    if let (Some(i), Some(o)) = (input, output) {
        result["usage"] = json!({"prompt_tokens":i,"completion_tokens":o});
    }
    Ok(result)
}

fn client_environment(cmd: &mut tokio::process::Command) {
    cmd.env_clear();
    // Forward OS paths and the owning CLI's configuration location, never
    // product credentials, inherited API keys or parent-agent markers.
    for key in [
        "PATH",
        "SystemRoot",
        "SYSTEMROOT",
        "WINDIR",
        "COMSPEC",
        "PATHEXT",
        "USERPROFILE",
        "HOME",
        "APPDATA",
        "LOCALAPPDATA",
        "TEMP",
        "TMP",
        "TMPDIR",
        "LANG",
        "LC_ALL",
        "DISPLAY",
        "WAYLAND_DISPLAY",
        "XDG_RUNTIME_DIR",
        "XDG_CONFIG_HOME",
        "XDG_STATE_HOME",
        "XDG_DATA_HOME",
        "DBUS_SESSION_BUS_ADDRESS",
        "CODEX_HOME",
        "CLAUDE_CONFIG_DIR",
        "HTTP_PROXY",
        "HTTPS_PROXY",
        "ALL_PROXY",
        "NO_PROXY",
        "http_proxy",
        "https_proxy",
        "all_proxy",
        "no_proxy",
    ] {
        if let Some(value) = std::env::var_os(key) {
            cmd.env(key, value);
        }
    }
}

#[cfg(windows)]
fn supervise_tree(child: &tokio::process::Child) -> Result<win32job::Job, Error> {
    let job = win32job::Job::create().map_err(|_| Error::Failed)?;
    let mut info = job.query_extended_limit_info().map_err(|_| Error::Failed)?;
    info.limit_kill_on_job_close();
    job.set_extended_limit_info(&info)
        .map_err(|_| Error::Failed)?;
    let handle = child.raw_handle().ok_or(Error::Failed)?;
    job.assign_process(handle as isize)
        .map_err(|_| Error::Failed)?;
    Ok(job)
}

async fn check_subscription(path: &Path, agent: &str) -> Result<(), Error> {
    let args: &[&str] = match agent {
        "claude" => &["auth", "status"],
        "codex" => &["login", "status"],
        _ => return Ok(()),
    };
    let mut cmd = tokio::process::Command::new(path);
    client_environment(&mut cmd);
    cmd.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(windows)]
    {
        cmd.creation_flags(0x08000000);
    }
    let mut child = cmd.spawn().map_err(|_| Error::Missing)?;
    #[cfg(windows)]
    let _job = supervise_tree(&child)?;
    let work = async {
        let mut out = Vec::new();
        let mut err = Vec::new();
        let mut stdout = child.stdout.take().ok_or(Error::Failed)?.take(65537);
        let mut stderr = child.stderr.take().ok_or(Error::Failed)?.take(65537);
        let (a, b) = tokio::join!(stdout.read_to_end(&mut out), stderr.read_to_end(&mut err));
        a.map_err(|_| Error::Failed)?;
        b.map_err(|_| Error::Failed)?;
        if out.len() > 65536 || err.len() > 65536 {
            return Err(Error::TooLarge);
        }
        if !child.wait().await.map_err(|_| Error::Failed)?.success() {
            return Err(Error::NotSubscription);
        }
        let ok = if agent == "claude" {
            serde_json::from_slice::<Value>(&out)
                .ok()
                .is_some_and(|v| v["authMethod"] == "claude.ai" && v["loggedIn"] != false)
        } else {
            out.extend(err);
            String::from_utf8_lossy(&out).contains("Logged in using ChatGPT")
        };
        if ok {
            Ok(())
        } else {
            Err(Error::NotSubscription)
        }
    };
    tokio::time::timeout(Duration::from_secs(20), work)
        .await
        .map_err(|_| Error::Timeout)?
}

async fn run_with(
    path: &Path,
    agent: &str,
    body: &Value,
    timeout: Duration,
) -> Result<Value, Error> {
    check_tool_policy(agent)?;
    check_subscription(path, agent).await?;
    let args = command_args(agent, body["model"].as_str().unwrap_or("default"))?;
    let prompt = prompt(body)?;
    let scratch = Scratch::new()?;
    prepare_workspace(&scratch.0, agent)?;
    let mut cmd = tokio::process::Command::new(path);
    cmd.args(args)
        .current_dir(&scratch.0)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    // Keep subscription selection explicit. These inherited keys must never turn
    // a subscription choice into a separately billed API request.
    client_environment(&mut cmd);
    #[cfg(windows)]
    {
        cmd.creation_flags(0x08000000);
    }
    let mut child = cmd.spawn().map_err(|_| Error::Missing)?;
    #[cfg(windows)]
    let _job = supervise_tree(&child)?;
    let work = async {
        let mut stdin = child.stdin.take().ok_or(Error::Failed)?;
        let input = if agent == "antigravity" {
            format!(
                "{}\n",
                json!({"event":"user", "message":{"role":"user","content":[{"type":"text","text":prompt}]}})
            )
        } else {
            prompt
        };
        stdin
            .write_all(input.as_bytes())
            .await
            .map_err(|_| Error::Failed)?;
        drop(stdin);
        let mut bytes = Vec::new();
        child
            .stdout
            .take()
            .ok_or(Error::Failed)?
            .take((MAX_RESPONSE + 1) as u64)
            .read_to_end(&mut bytes)
            .await
            .map_err(|_| Error::Failed)?;
        if bytes.len() > MAX_RESPONSE {
            return Err(Error::TooLarge);
        }
        if !child.wait().await.map_err(|_| Error::Failed)?.success() {
            return Err(Error::Failed);
        }
        let (text, usage) = completion(agent, &bytes)?;
        envelope(text, usage, body)
    };
    tokio::time::timeout(timeout, work)
        .await
        .map_err(|_| Error::Timeout)?
}

/// Complete one existing OpenAI-format request using the chosen installed agent.
/// Dropping this future (including cancellation by the caller) kills its child.
pub async fn complete(url: &str, body: &Value, timeout: Duration) -> Result<Value, Error> {
    let agent = agent_for_url(url).ok_or(Error::Unsupported)?;
    if !installed(agent) {
        return Err(Error::Missing);
    }
    run_with(&program(agent)?, agent, body, timeout).await
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_reserved_exact_origins_are_local() {
        assert_eq!(
            agent_for_url("https://codex.local-agent.invalid/v1/chat/completions"),
            Some("codex")
        );
        for url in [
            "http://codex.local-agent.invalid/",
            "https://codex.local-agent.invalid.evil/",
            "https://evil/codex.local-agent.invalid/",
            "https://codex.local-agent.invalid@evil/",
            "https://grok.local-agent.invalid/",
        ] {
            assert_eq!(agent_for_url(url), None);
        }
    }
    #[test]
    fn provider_tool_policies_cannot_independently_allow_product_operations() {
        let policy = cursor_policy();
        let deny = policy["permissions"]["deny"].as_array().unwrap();
        for rule in [
            "Shell(*)",
            "Mcp(*:*)",
            "WebFetch(*)",
            "Read(/**)",
            "Read(C:/**)",
            "Write(Z:/**)",
        ] {
            assert!(deny.iter().any(|v| v == rule));
        }
        assert_eq!(policy["permissions"]["allow"], json!([]));
        assert!(antigravity_strict(
            br#"{"toolPermission":"strict","unrelated":"ignored"}"#
        ));
        for bytes in [
            br#"{}"#.as_slice(),
            br#"{"toolPermission":"always-proceed"}"#,
            br#"{"toolPermission":"request-review"}"#,
            b"invalid",
        ] {
            assert!(!antigravity_strict(bytes));
        }
        let scratch = Scratch::new().unwrap();
        prepare_workspace(&scratch.0, "cursor").unwrap();
        let actual: Value =
            serde_json::from_slice(&std::fs::read(scratch.0.join(".cursor/cli.json")).unwrap())
                .unwrap();
        assert_eq!(actual, policy);
    }
    #[test]
    fn child_environment_drops_inherited_product_and_api_secrets() {
        let mut cmd = tokio::process::Command::new("unused-fixture");
        for key in [
            "OGUN_VPS_PW",
            "ANTHROPIC_API_KEY",
            "OPENAI_API_KEY",
            "VERNE_PARENT_SECRET",
        ] {
            cmd.env(key, "SECRET-CANARY");
        }
        client_environment(&mut cmd);
        assert!(!cmd.as_std().get_envs().any(|(key, _)| key == "OGUN_VPS_PW"
            || key == "ANTHROPIC_API_KEY"
            || key == "OPENAI_API_KEY"
            || key == "VERNE_PARENT_SECRET"));
    }
    #[test]
    fn prompts_are_data_and_models_cannot_inject_arguments() {
        assert!(command_args("claude", "x; echo secret").is_err());
        let a = command_args("claude", "default").unwrap();
        assert!(a.windows(2).any(|a| a == ["--tools", ""]));
        for agent in ["codex", "claude", "cursor", "antigravity"] {
            let a = command_args(agent, "default").unwrap();
            assert!(!a
                .iter()
                .any(|a| a == "--force" || a.contains("skip-permissions") || a == "--yolo"));
        }
    }
    #[test]
    fn codex_requires_a_completed_turn_and_never_uses_error_output() {
        let bytes=b"{\"type\":\"item.completed\",\"item\":{\"type\":\"agent_message\",\"text\":\"hello\"}}\n";
        assert_eq!(completion("codex", bytes).unwrap_err(), Error::Protocol);
        assert_eq!(
            completion("claude", br#"{"is_error":true,"result":"SECRET-CANARY"}"#).unwrap_err(),
            Error::Failed
        );
        assert!(!Error::Failed.to_string().contains("SECRET-CANARY"));
    }
    #[test]
    fn only_declared_tools_pass_back_to_the_product() {
        let body = json!({"tools":[{"function":{"name":"history_read"}}]});
        assert!(envelope(
            r#"{"tool_calls":[{"function":{"name":"shell","arguments":"{}"}}]}"#.into(),
            Value::Null,
            &body
        )
        .is_err());
        let value = envelope(
            r#"{"tool_calls":[{"function":{"name":"history_read","arguments":"{}"}}]}"#.into(),
            json!({"input_tokens":10,"output_tokens":2}),
            &body,
        )
        .unwrap();
        assert_eq!(value["choices"][0]["finish_reason"], "tool_calls");
        assert_eq!(
            value["choices"][0]["message"]["tool_calls"][0]["id"],
            "local_call_0"
        );
        assert_eq!(value["usage"]["prompt_tokens"], 10);
    }
    #[tokio::test]
    async fn deadline_stops_the_child_before_it_can_write_afterwards() {
        let scratch = Scratch::new().unwrap();
        let marker = scratch.0.join("orphan-marker");
        let body = json!({"model":"default","test_marker":marker.to_string_lossy()});
        let result = run_with(
            &std::env::current_exe().unwrap(),
            "stub",
            &body,
            Duration::from_millis(30),
        )
        .await;
        assert_eq!(result.unwrap_err(), Error::Timeout);
        tokio::time::sleep(Duration::from_millis(350)).await;
        assert!(
            !marker.exists(),
            "the delegate survived its request deadline"
        );
    }
    #[test]
    #[ignore = "subprocess fixture used by the deadline test"]
    fn stub_child() {
        use std::io::Read;
        let mut input = String::new();
        std::io::stdin().read_to_string(&mut input).unwrap();
        let body: Value =
            serde_json::from_str(input.split("Model request:\n").nth(1).unwrap()).unwrap();
        std::thread::sleep(Duration::from_millis(200));
        std::fs::write(body["test_marker"].as_str().unwrap(), b"orphan").unwrap();
    }
    #[test]
    fn claude_cache_tokens_count_toward_usage() {
        let (_,usage)=completion("claude",br#"{"result":"OK","usage":{"input_tokens":2,"cache_read_input_tokens":100,"cache_creation_input_tokens":50,"output_tokens":3}}"#).unwrap();
        assert_eq!(usage["input_tokens"], 152);
    }
    #[test]
    fn antigravity_requires_success() {
        assert!(completion(
            "antigravity",
            br#"{"event":"result","result":{"status":"WAITING","response":"partial"}}"#
        )
        .is_err());
        assert_eq!(
            completion(
                "antigravity",
                br#"{"event":"result","result":{"status":"SUCCESS","response":"hello"}}"#
            )
            .unwrap()
            .0,
            "hello"
        );
    }
}
