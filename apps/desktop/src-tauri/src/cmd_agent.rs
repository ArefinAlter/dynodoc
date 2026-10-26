//! Commands for the agent: keys, consent, caps, a preview of what would be sent, and a run.
//! Same rules as `commands.rs`: only the bundled launcher page, and each command takes that second
//! lock first. A key goes in through `agent_key_set` and never comes out: the page gets back its last
//! four characters at most.

use crate::agent::config::{AgentConfig, Consent, Price};
use crate::agent::run::{self, Event, Io, RunInput, ToolReply};
use crate::agent::tools;
use crate::agent::types::{AgentError, ApiKey, Endpoint, Kind, Msg, ToolCall, ToolDecl, Usage};
use crate::agent::{client, wire, RunHandle};
use crate::commands::require_launcher;
use crate::AppState;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::time::Duration;
use tauri::ipc::Channel;
use tauri::{Manager, Webview};
use tokio::sync::{mpsc, watch};

#[cfg(test)]
pub const COMMANDS: [&str; 12] = [
    "agent_config",
    "agent_config_set",
    "agent_consent",
    "agent_key_set",
    "agent_key_clear",
    "agent_key_status",
    "agent_keys_unlock",
    "agent_preview",
    "agent_run",
    "agent_tool_result",
    "agent_cancel",
    "agent_usage",
];

fn state_of(webview: &Webview) -> tauri::State<'_, AppState> {
    webview.app_handle().state::<AppState>()
}

fn id_ok(id: &str) -> bool {
    crate::agent::keys::id_ok(id)
}

#[derive(Serialize)]
pub struct ConfigView {
    #[serde(flatten)]
    config: AgentConfig,
    keys_where: &'static str,
    keys_in_credential_store: bool,
    keys_locked: bool,
    keys_file_exists: bool,
    day_tokens: u64,
    day_cost_micros: u64,
}

async fn view(state: &AppState) -> ConfigView {
    let (_, day_tokens, day_cost) = state.local.ledger_totals("").await;
    ConfigView {
        config: state.agent.config(),
        keys_where: state.agent.keys.describe(),
        keys_in_credential_store: state.agent.keys.in_credential_store(),
        keys_locked: state.agent.keys.locked(),
        keys_file_exists: state.agent.keys.file_exists(),
        day_tokens,
        day_cost_micros: day_cost,
    }
}

/// The agent's settings, what holds the keys, and what the last 24 hours used.
#[tauri::command]
pub async fn agent_config(webview: Webview) -> Result<ConfigView, String> {
    require_launcher(webview.url())?;
    Ok(view(&state_of(&webview)).await)
}

/// Changes offline, the caps or the price table (any of them).
#[tauri::command]
pub async fn agent_config_set(webview: Webview, patch: Value) -> Result<ConfigView, String> {
    require_launcher(webview.url())?;
    let state = state_of(&webview);
    let obj = patch.as_object().ok_or_else(|| "Send an object of settings.".to_string())?;
    let mut prices: Option<BTreeMap<String, Price>> = None;
    if let Some(p) = obj.get("prices") {
        let map = p.as_object().ok_or_else(|| "Prices are an object of models.".to_string())?;
        if map.len() > 100 {
            return Err("Too many prices (at most 100).".to_string());
        }
        let mut out = BTreeMap::new();
        for (model, v) in map {
            let ok = !model.is_empty() && model.len() <= 200 && model.bytes().all(|b| b.is_ascii_alphanumeric() || b"._:/@+-".contains(&b));
            let (i, o) = (v["input"].as_u64(), v["output"].as_u64());
            let (Some(input), Some(output)) = (i, o) else { return Err("A price has an input and an output (millionths of a dollar per million tokens).".to_string()) };
            if !ok || input > 10_000_000_000 || output > 10_000_000_000 {
                return Err(format!("The price for {model} is not valid."));
            }
            out.insert(model.clone(), Price { input, output });
        }
        prices = Some(out);
    }
    let cap = |k: &str| -> Result<Option<u64>, String> {
        match obj.get(k) {
            None => Ok(None),
            Some(v) => v.as_u64().filter(|n| *n <= 1_000_000_000_000).map(Some).ok_or_else(|| "A cap is a whole number (0 means no model is called).".to_string()),
        }
    };
    let (t, d, c) = (cap("thread_token_cap")?, cap("day_token_cap")?, cap("day_cost_cap")?);
    let offline = match obj.get("offline") {
        None => None,
        Some(v) => Some(v.as_bool().ok_or_else(|| "Offline is on or off.".to_string())?),
    };
    state.agent.update(|cfg| {
        if let Some(v) = offline {
            cfg.offline = v;
        }
        if let Some(v) = t {
            cfg.thread_token_cap = v;
        }
        if let Some(v) = d {
            cfg.day_token_cap = v;
        }
        if let Some(v) = c {
            cfg.day_cost_cap = v;
        }
        if let Some(p) = prices {
            cfg.prices = p;
        }
    })?;
    Ok(view(&state).await)
}

/// Allows (or stops allowing) a provider entry to receive requests at one host.
#[tauri::command]
pub async fn agent_consent(webview: Webview, provider_id: String, host: Option<String>, allow: bool) -> Result<ConfigView, String> {
    require_launcher(webview.url())?;
    if !id_ok(&provider_id) {
        return Err("That provider entry has no valid name.".to_string());
    }
    let state = state_of(&webview);
    if allow {
        let host = host.filter(|h| !h.is_empty() && h.len() <= 260 && h.bytes().all(|b| b.is_ascii_alphanumeric() || b"-.:[]".contains(&b))).ok_or_else(|| "The provider's host is missing.".to_string())?;
        state.agent.update(|c| {
            c.consent.insert(provider_id.clone(), Consent { host, at: crate::local::appdb::now_ms() });
        })?;
    } else {
        state.agent.update(|c| {
            c.consent.remove(&provider_id);
        })?;
    }
    Ok(view(&state).await)
}

#[derive(Serialize)]
pub struct KeyHint {
    hint: String,
}

/// Stores a key for a provider entry. The key is not sent back: only its last four characters.
#[tauri::command]
pub fn agent_key_set(webview: Webview, provider_id: String, key: String) -> Result<KeyHint, String> {
    require_launcher(webview.url())?;
    let key = ApiKey::new(&key).ok_or_else(|| "That does not look like a key (8 to 512 visible characters, no spaces).".to_string())?;
    let state = state_of(&webview);
    state.agent.keys.set(&provider_id, &key).map_err(|e| e.to_string())?;
    Ok(KeyHint { hint: key.hint() })
}

#[tauri::command]
pub fn agent_key_clear(webview: Webview, provider_id: String) -> Result<(), String> {
    require_launcher(webview.url())?;
    state_of(&webview).agent.keys.delete(&provider_id).map_err(|e| e.to_string())
}

/// Which of the entries have a key, as their last four characters.
#[tauri::command]
pub fn agent_key_status(webview: Webview, ids: Vec<String>) -> Result<BTreeMap<String, Option<String>>, String> {
    require_launcher(webview.url())?;
    let state = state_of(&webview);
    let mut out = BTreeMap::new();
    for id in ids.into_iter().take(50) {
        out.insert(id.clone(), state.agent.keys.get(&id).map_err(|e| e.to_string())?.map(|k| k.hint()));
    }
    Ok(out)
}

/// Opens the encrypted key file with its passphrase (only where there is no credential store).
#[tauri::command]
pub async fn agent_keys_unlock(webview: Webview, passphrase: String) -> Result<ConfigView, String> {
    require_launcher(webview.url())?;
    let state = state_of(&webview);
    state.agent.keys.unlock(&passphrase).map_err(|e| e.to_string())?;
    Ok(view(&state).await)
}

#[derive(Deserialize)]
pub struct Turn {
    role: String,
    text: String,
}

#[derive(Deserialize)]
pub struct ToolIn {
    name: String,
    description: String,
    parameters: Value,
}

#[derive(Deserialize)]
pub struct RunRequest {
    run_id: String,
    thread: String,
    provider_id: String,
    kind: Kind,
    base: String,
    model: String,
    system: String,
    history: Vec<Turn>,
    tools: Vec<ToolIn>,
}

fn prepare(req: &RunRequest) -> Result<RunInput, String> {
    if !id_ok(&req.provider_id) || !id_ok(&req.thread.to_ascii_lowercase()) {
        return Err("The thread or the provider entry has no valid name.".to_string());
    }
    if req.history.is_empty() || req.history.len() > 60 || req.system.chars().count() > 8000 {
        return Err("The conversation is empty or too long.".to_string());
    }
    let endpoint = Endpoint::parse(req.kind, &req.base, &req.model).map_err(|e| e.to_string())?;
    let history = req
        .history
        .iter()
        .map(|t| {
            if t.text.chars().count() > 20_000 {
                return Err("One message is too long.".to_string());
            }
            match t.role.as_str() {
                "user" => Ok(Msg::User(t.text.clone())),
                "assistant" => Ok(Msg::Assistant { text: t.text.clone(), calls: vec![], raw: None }),
                _ => Err("A message has no valid role.".to_string()),
            }
        })
        .collect::<Result<Vec<_>, _>>()?;
    let decls: Vec<ToolDecl> = req.tools.iter().map(|t| ToolDecl { name: t.name.clone(), description: t.description.clone(), parameters: t.parameters.clone() }).collect();
    tools::validate(&decls)?;
    Ok(RunInput { thread: req.thread.clone(), provider_id: req.provider_id.clone(), endpoint, system: req.system.clone(), history, tools: decls })
}

#[derive(Serialize)]
pub struct Preview {
    host: String,
    url: String,
    /// The request body, as it would be sent (the key goes in a header, never in the body).
    body: String,
    key_header: Option<String>,
}

/// What a run would send to the provider's host, for the person to read before allowing it.
#[tauri::command]
pub fn agent_preview(webview: Webview, request: RunRequest) -> Result<Preview, String> {
    require_launcher(webview.url())?;
    let input = prepare(&request)?;
    let built = wire::build(&input.endpoint, &input.system, &input.history, &input.tools);
    let url = built.url.split('?').next().unwrap_or("").to_string();
    Ok(Preview {
        host: input.endpoint.host(),
        url,
        body: serde_json::to_string_pretty(&built.body).unwrap_or_default(),
        key_header: built.key_header.map(|(h, _)| h.to_string()),
    })
}

struct PageIo {
    channel: Channel<Value>,
    local: crate::local::Local,
    replies: mpsc::Receiver<(String, ToolReply)>,
}

impl Io for PageIo {
    fn emit(&mut self, event: Event) {
        let _ = self.channel.send(serde_json::to_value(event).unwrap_or(Value::Null));
    }

    async fn tool(&mut self, call: &ToolCall) -> ToolReply {
        let wait = async {
            while let Some((id, reply)) = self.replies.recv().await {
                if id == call.id {
                    return Some(reply);
                }
            }
            None
        };
        match tokio::time::timeout(Duration::from_secs(180), wait).await {
            Ok(Some(r)) => r,
            _ => ToolReply { output: String::new(), refused: Some("the tool did not answer in time".to_string()) },
        }
    }

    async fn totals(&mut self, thread: &str) -> (u64, u64, u64) {
        self.local.ledger_totals(thread).await
    }

    async fn record(&mut self, thread: &str, provider: &str, model: &str, usage: Usage, cost: Option<u64>) {
        self.local.ledger_add(thread, provider, model, usage.input, usage.output, cost).await;
    }
}

/// Runs the agent. Events (text as it streams, tool calls to run, usage, done or an error) come over
/// the channel; the page answers each tool call with `agent_tool_result`.
#[tauri::command]
pub async fn agent_run(webview: Webview, request: RunRequest, on_event: Channel<Value>) -> Result<(), String> {
    require_launcher(webview.url())?;
    let state = state_of(&webview);
    let fail = |kind: &str, message: String| {
        let _ = on_event.send(json!({ "type": "error", "kind": kind, "message": message }));
    };
    let run_id = request.run_id.clone();
    if !id_ok(&run_id.to_ascii_lowercase()) || run_id.len() < 6 {
        fail("setup", "The run has no valid name.".into());
        return Ok(());
    }
    let input = match prepare(&request) {
        Ok(i) => i,
        Err(m) => {
            fail("setup", m);
            return Ok(());
        }
    };
    let key = if input.endpoint.kind.needs_key() {
        match state.agent.keys.get(&input.provider_id) {
            Ok(k) => k,
            Err(e) => {
                fail("no_key", e.to_string());
                return Ok(());
            }
        }
    } else {
        None
    };
    let (cancel_tx, cancel_rx) = watch::channel(false);
    let (reply_tx, reply_rx) = mpsc::channel(16);
    if let Ok(mut runs) = state.agent.runs.lock() {
        if runs.len() >= 8 {
            fail("setup", "Too many runs at once.".into());
            return Ok(());
        }
        runs.insert(run_id.clone(), RunHandle { cancel: cancel_tx, replies: reply_tx });
    }
    let mut io = PageIo { channel: on_event.clone(), local: state.local.clone(), replies: reply_rx };
    let cfg = state.agent.config();
    let outcome = run::run(&mut io, &cfg, key, input, client::Limits::default(), cancel_rx).await;
    if let Ok(mut runs) = state.agent.runs.lock() {
        runs.remove(&run_id);
    }
    if let Err(AgentError::Cancelled) = outcome {
        // The page asked for it: say so plainly, as the end of the run.
    }
    Ok(())
}

/// The page's answer to a tool call of a run.
#[tauri::command]
pub async fn agent_tool_result(webview: Webview, run_id: String, call_id: String, output: String, refused: Option<String>) -> Result<(), String> {
    require_launcher(webview.url())?;
    let state = state_of(&webview);
    let sender = {
        let runs = state.agent.runs.lock().map_err(|_| "The run is busy.".to_string())?;
        runs.get(&run_id).map(|r| r.replies.clone())
    };
    let sender = sender.ok_or_else(|| "That run has ended.".to_string())?;
    sender.send((call_id, ToolReply { output, refused })).await.map_err(|_| "That run has ended.".to_string())
}

#[tauri::command]
pub fn agent_cancel(webview: Webview, run_id: String) -> Result<(), String> {
    require_launcher(webview.url())?;
    let state = state_of(&webview);
    let runs = state.agent.runs.lock().map_err(|_| "The run is busy.".to_string())?;
    if let Some(r) = runs.get(&run_id) {
        let _ = r.cancel.send(true);
    }
    Ok(())
}

#[derive(Serialize)]
pub struct Usage24 {
    thread_tokens: u64,
    day_tokens: u64,
    day_cost_micros: u64,
}

#[tauri::command]
pub async fn agent_usage(webview: Webview, thread: Option<String>) -> Result<Usage24, String> {
    require_launcher(webview.url())?;
    let (t, d, c) = state_of(&webview).local.ledger_totals(thread.as_deref().unwrap_or("")).await;
    Ok(Usage24 { thread_tokens: t, day_tokens: d, day_cost_micros: c })
}
