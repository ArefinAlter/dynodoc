//! The three wire formats the providers speak, as plain functions: build the request, and read the
//! stream of events that comes back. No network and no key in here; `client.rs` sends what this builds.
//!
//! - Anthropic Messages (`/v1/messages`, `x-api-key`): content blocks, `tool_use` and `tool_result`.
//! - Google Gemini (`models/<model>:streamGenerateContent?alt=sse`, `x-goog-api-key`): `contents`
//!   with `parts`, `functionCall` and `functionResponse`.
//! - OpenAI chat completions (OpenAI, Mistral, DeepSeek, Qwen's compatible mode, Kimi, GLM, Ollama's
//!   `/v1`, and any compatible server): `messages`, `tool_calls` and `role: "tool"`.

use super::sse::SseEvent;
use super::types::{from_wire_name, wire_name, AgentError, Endpoint, Kind, Msg, Stop, Style, ToolCall, ToolDecl, Turn, Usage};
use serde_json::{json, Map, Value};

/// A request, without its key.
#[derive(Debug, Clone)]
pub struct Built {
    pub url: String,
    pub headers: Vec<(&'static str, String)>,
    /// The header that carries the key and what goes before it (`Authorization` / `Bearer `).
    pub key_header: Option<(&'static str, &'static str)>,
    pub body: Value,
}

const MAX_OUTPUT_TOKENS: u32 = 4096;

fn trimmed(base: &url::Url) -> String {
    base.as_str().trim_end_matches('/').to_string()
}

pub fn build(ep: &Endpoint, system: &str, msgs: &[Msg], tools: &[ToolDecl]) -> Built {
    match ep.kind.style() {
        Style::Anthropic => build_anthropic(ep, system, msgs, tools),
        Style::Gemini => build_gemini(ep, system, msgs, tools),
        Style::OpenAi => build_openai(ep, system, msgs, tools),
    }
}

// ---- Anthropic ----------------------------------------------------------------------------------------------

fn build_anthropic(ep: &Endpoint, system: &str, msgs: &[Msg], tools: &[ToolDecl]) -> Built {
    let base = trimmed(&ep.base);
    let url = if base.ends_with("/v1") { format!("{base}/messages") } else { format!("{base}/v1/messages") };
    let mut messages: Vec<Value> = Vec::new();
    let mut pending: Vec<Value> = Vec::new();
    let flush = |pending: &mut Vec<Value>, messages: &mut Vec<Value>| {
        if !pending.is_empty() {
            messages.push(json!({ "role": "user", "content": std::mem::take(pending) }));
        }
    };
    for m in msgs {
        match m {
            Msg::ToolResult { call_id, output, .. } => pending.push(json!({ "type": "tool_result", "tool_use_id": call_id, "content": output })),
            Msg::User(t) => {
                flush(&mut pending, &mut messages);
                messages.push(json!({ "role": "user", "content": t }));
            }
            Msg::Assistant { text, calls, .. } => {
                flush(&mut pending, &mut messages);
                let mut content: Vec<Value> = Vec::new();
                if !text.is_empty() {
                    content.push(json!({ "type": "text", "text": text }));
                }
                for c in calls {
                    content.push(json!({ "type": "tool_use", "id": c.id, "name": wire_name(&c.name), "input": c.args }));
                }
                if content.is_empty() {
                    content.push(json!({ "type": "text", "text": "…" }));
                }
                messages.push(json!({ "role": "assistant", "content": content }));
            }
        }
    }
    flush(&mut pending, &mut messages);
    let mut body = json!({ "model": ep.model, "max_tokens": MAX_OUTPUT_TOKENS, "stream": true, "messages": messages });
    if !system.is_empty() {
        body["system"] = json!(system);
    }
    if !tools.is_empty() {
        body["tools"] = Value::Array(tools.iter().map(|t| json!({ "name": wire_name(&t.name), "description": t.description, "input_schema": t.parameters })).collect());
    }
    Built {
        url,
        headers: vec![("content-type", "application/json".into()), ("anthropic-version", "2023-06-01".into()), ("accept", "text/event-stream".into())],
        key_header: Some(("x-api-key", "")),
        body,
    }
}

// ---- OpenAI chat completions ------------------------------------------------------------------------------------

fn openai_url(ep: &Endpoint) -> String {
    let base = trimmed(&ep.base);
    if base.ends_with("/chat/completions") {
        return base;
    }
    // Ollama's own address has no path: its compatible API lives under /v1.
    if ep.kind == Kind::Ollama && ep.base.path().trim_matches('/').is_empty() {
        return format!("{base}/v1/chat/completions");
    }
    format!("{base}/chat/completions")
}

fn build_openai(ep: &Endpoint, system: &str, msgs: &[Msg], tools: &[ToolDecl]) -> Built {
    let mut messages: Vec<Value> = Vec::new();
    if !system.is_empty() {
        messages.push(json!({ "role": "system", "content": system }));
    }
    for m in msgs {
        match m {
            Msg::User(t) => messages.push(json!({ "role": "user", "content": t })),
            Msg::Assistant { text, calls, .. } => {
                let mut o = Map::new();
                o.insert("role".into(), json!("assistant"));
                o.insert("content".into(), if text.is_empty() { Value::Null } else { json!(text) });
                if !calls.is_empty() {
                    o.insert(
                        "tool_calls".into(),
                        Value::Array(calls.iter().map(|c| json!({ "id": c.id, "type": "function", "function": { "name": wire_name(&c.name), "arguments": c.args.to_string() } })).collect()),
                    );
                }
                messages.push(Value::Object(o));
            }
            Msg::ToolResult { call_id, output, .. } => messages.push(json!({ "role": "tool", "tool_call_id": call_id, "content": output })),
        }
    }
    let mut body = json!({ "model": ep.model, "stream": true, "messages": messages });
    if ep.kind.asks_for_usage() {
        body["stream_options"] = json!({ "include_usage": true });
    }
    if !tools.is_empty() {
        body["tools"] = Value::Array(tools.iter().map(|t| json!({ "type": "function", "function": { "name": wire_name(&t.name), "description": t.description, "parameters": t.parameters } })).collect());
    }
    Built {
        url: openai_url(ep),
        headers: vec![("content-type", "application/json".into()), ("accept", "text/event-stream".into())],
        key_header: ep.kind.needs_key().then_some(("authorization", "Bearer ")),
        body,
    }
}

// ---- Gemini ----------------------------------------------------------------------------------------------------------

fn build_gemini(ep: &Endpoint, system: &str, msgs: &[Msg], tools: &[ToolDecl]) -> Built {
    let base = trimmed(&ep.base);
    let model = ep.model.trim_start_matches("models/");
    let url = format!("{base}/models/{model}:streamGenerateContent?alt=sse");
    let mut contents: Vec<Value> = Vec::new();
    let mut pending: Vec<Value> = Vec::new();
    let flush = |pending: &mut Vec<Value>, contents: &mut Vec<Value>| {
        if !pending.is_empty() {
            contents.push(json!({ "role": "user", "parts": std::mem::take(pending) }));
        }
    };
    for m in msgs {
        match m {
            Msg::ToolResult { name, output, .. } => pending.push(json!({ "functionResponse": { "name": wire_name(name), "response": { "output": output } } })),
            Msg::User(t) => {
                flush(&mut pending, &mut contents);
                contents.push(json!({ "role": "user", "parts": [{ "text": t }] }));
            }
            Msg::Assistant { text, calls, raw } => {
                flush(&mut pending, &mut contents);
                let parts = match raw.as_ref().and_then(|r| r.get("parts")).cloned() {
                    Some(p) => p,
                    None => {
                        let mut parts: Vec<Value> = Vec::new();
                        if !text.is_empty() {
                            parts.push(json!({ "text": text }));
                        }
                        for c in calls {
                            parts.push(json!({ "functionCall": { "name": wire_name(&c.name), "args": c.args } }));
                        }
                        if parts.is_empty() {
                            parts.push(json!({ "text": "…" }));
                        }
                        Value::Array(parts)
                    }
                };
                contents.push(json!({ "role": "model", "parts": parts }));
            }
        }
    }
    flush(&mut pending, &mut contents);
    let mut body = json!({ "contents": contents });
    if !system.is_empty() {
        body["systemInstruction"] = json!({ "parts": [{ "text": system }] });
    }
    if !tools.is_empty() {
        body["tools"] = json!([{ "functionDeclarations": tools.iter().map(|t| json!({ "name": wire_name(&t.name), "description": t.description, "parameters": t.parameters })).collect::<Vec<_>>() }]);
    }
    Built {
        url,
        headers: vec![("content-type", "application/json".into())],
        key_header: Some(("x-goog-api-key", "")),
        body,
    }
}

// ---- reading a stream ---------------------------------------------------------------------------------------------

pub trait Reader: Send {
    /// One event of the stream. `on_text` receives text as it arrives.
    fn feed(&mut self, ev: &SseEvent, on_text: &mut dyn FnMut(&str)) -> Result<(), AgentError>;
    /// The stream ended: the turn as a whole.
    fn finish(self: Box<Self>) -> Result<Turn, AgentError>;
}

pub fn reader(style: Style) -> Box<dyn Reader> {
    match style {
        Style::Anthropic => Box::<AnthropicReader>::default(),
        Style::Gemini => Box::<GeminiReader>::default(),
        Style::OpenAi => Box::<OpenAiReader>::default(),
    }
}

fn parse_args(text: &str) -> Value {
    if text.trim().is_empty() {
        return json!({});
    }
    serde_json::from_str(text).unwrap_or_else(|_| json!({ "_unparsed": text.chars().take(2000).collect::<String>() }))
}

fn bad(msg: &str) -> AgentError {
    AgentError::BadAnswer(msg.to_string())
}

// Anthropic

#[derive(Default)]
struct AnthropicReader {
    text: String,
    blocks: Vec<(usize, Option<(String, String)>, String)>,
    usage: Usage,
    stop: Option<Stop>,
    ended: bool,
}

impl Reader for AnthropicReader {
    fn feed(&mut self, ev: &SseEvent, on_text: &mut dyn FnMut(&str)) -> Result<(), AgentError> {
        let v: Value = serde_json::from_str(&ev.data).map_err(|_| bad("an event that is not JSON"))?;
        match v["type"].as_str().unwrap_or("") {
            "message_start" => {
                let u = &v["message"]["usage"];
                self.usage.input = ["input_tokens", "cache_creation_input_tokens", "cache_read_input_tokens"].iter().filter_map(|k| u[*k].as_u64()).sum();
                self.usage.output = u["output_tokens"].as_u64().unwrap_or(0);
            }
            "content_block_start" => {
                let i = v["index"].as_u64().unwrap_or(0) as usize;
                let b = &v["content_block"];
                let call = (b["type"] == "tool_use").then(|| (b["id"].as_str().unwrap_or("").to_string(), from_wire_name(b["name"].as_str().unwrap_or(""))));
                self.blocks.push((i, call, String::new()));
            }
            "content_block_delta" => {
                let i = v["index"].as_u64().unwrap_or(0) as usize;
                let d = &v["delta"];
                match d["type"].as_str().unwrap_or("") {
                    "text_delta" => {
                        let t = d["text"].as_str().unwrap_or("");
                        self.text.push_str(t);
                        on_text(t);
                    }
                    "input_json_delta" => {
                        if let Some(b) = self.blocks.iter_mut().rev().find(|b| b.0 == i) {
                            b.2.push_str(d["partial_json"].as_str().unwrap_or(""));
                        }
                    }
                    _ => {}
                }
            }
            "message_delta" => {
                if let Some(r) = v["delta"]["stop_reason"].as_str() {
                    self.stop = Some(match r {
                        "tool_use" => Stop::ToolCalls,
                        "end_turn" | "stop_sequence" => Stop::End,
                        "max_tokens" => Stop::Length,
                        _ => Stop::Other,
                    });
                }
                if let Some(o) = v["usage"]["output_tokens"].as_u64() {
                    self.usage.output = o;
                }
            }
            "message_stop" => self.ended = true,
            "error" => {
                let kind = v["error"]["type"].as_str().unwrap_or("error");
                let msg = v["error"]["message"].as_str().unwrap_or("");
                return Err(match kind {
                    "overloaded_error" | "rate_limit_error" => AgentError::RateLimited { retry_after_ms: None },
                    _ => AgentError::Provider { status: 0, message: format!("{kind}: {}", msg.chars().take(200).collect::<String>()) },
                });
            }
            _ => {}
        }
        Ok(())
    }

    fn finish(self: Box<Self>) -> Result<Turn, AgentError> {
        if !self.ended && self.stop.is_none() {
            return Err(bad("the stream ended early"));
        }
        let calls: Vec<ToolCall> = self.blocks.into_iter().filter_map(|(_, call, json)| call.map(|(id, name)| ToolCall { id, name, args: parse_args(&json) })).collect();
        let stop = self.stop.unwrap_or(if calls.is_empty() { Stop::End } else { Stop::ToolCalls });
        Ok(Turn { text: self.text, calls, usage: self.usage, stop, raw: None })
    }
}

// OpenAI chat completions

#[derive(Default)]
struct PartialCall {
    id: String,
    name: String,
    args: String,
}

#[derive(Default)]
struct OpenAiReader {
    text: String,
    calls: Vec<(Option<u64>, PartialCall)>,
    usage: Usage,
    finish: Option<String>,
    done: bool,
    seen: bool,
}

impl Reader for OpenAiReader {
    fn feed(&mut self, ev: &SseEvent, on_text: &mut dyn FnMut(&str)) -> Result<(), AgentError> {
        let data = ev.data.trim();
        if data == "[DONE]" {
            self.done = true;
            return Ok(());
        }
        let v: Value = serde_json::from_str(data).map_err(|_| bad("an event that is not JSON"))?;
        if let Some(err) = v.get("error").filter(|e| !e.is_null()) {
            let msg = err["message"].as_str().or_else(|| err.as_str()).unwrap_or("an error");
            return Err(AgentError::Provider { status: 0, message: msg.chars().take(300).collect() });
        }
        self.seen = true;
        let usage = if v["usage"].is_object() { &v["usage"] } else { &v["choices"][0]["usage"] };
        if usage.is_object() {
            self.usage.input = usage["prompt_tokens"].as_u64().unwrap_or(self.usage.input);
            self.usage.output = usage["completion_tokens"].as_u64().unwrap_or(self.usage.output);
        }
        let choice = &v["choices"][0];
        if let Some(r) = choice["finish_reason"].as_str() {
            self.finish = Some(r.to_string());
        }
        let delta = &choice["delta"];
        if let Some(t) = delta["content"].as_str() {
            if !t.is_empty() {
                self.text.push_str(t);
                on_text(t);
            }
        }
        if let Some(list) = delta["tool_calls"].as_array() {
            for (n, c) in list.iter().enumerate() {
                let index = c["index"].as_u64();
                let id = c["id"].as_str().unwrap_or("");
                let slot = match (index, self.calls.iter().position(|(i, p)| (index.is_some() && *i == index) || (!id.is_empty() && p.id == id))) {
                    (_, Some(p)) => p,
                    _ => {
                        self.calls.push((index.or(Some((self.calls.len() + n) as u64)), PartialCall::default()));
                        self.calls.len() - 1
                    }
                };
                let p = &mut self.calls[slot].1;
                if !id.is_empty() {
                    p.id = id.to_string();
                }
                if let Some(name) = c["function"]["name"].as_str() {
                    p.name.push_str(name);
                }
                match &c["function"]["arguments"] {
                    Value::String(s) => p.args.push_str(s),
                    Value::Object(_) => p.args = c["function"]["arguments"].to_string(),
                    _ => {}
                }
            }
        }
        Ok(())
    }

    fn finish(self: Box<Self>) -> Result<Turn, AgentError> {
        if !self.done && self.finish.is_none() && !self.seen {
            return Err(bad("the stream ended before any answer"));
        }
        let calls: Vec<ToolCall> = self
            .calls
            .into_iter()
            .enumerate()
            .filter(|(_, (_, p))| !p.name.is_empty())
            .map(|(n, (_, p))| ToolCall { id: if p.id.is_empty() { format!("call_{n}") } else { p.id }, name: from_wire_name(&p.name), args: parse_args(&p.args) })
            .collect();
        let stop = match self.finish.as_deref() {
            Some("tool_calls") | Some("function_call") => Stop::ToolCalls,
            Some("length") => Stop::Length,
            Some("stop") | None if calls.is_empty() => Stop::End,
            Some("stop") | None => Stop::ToolCalls,
            Some(_) if !calls.is_empty() => Stop::ToolCalls,
            Some(_) => Stop::Other,
        };
        Ok(Turn { text: self.text, calls, usage: self.usage, stop, raw: None })
    }
}

// Gemini

#[derive(Default)]
struct GeminiReader {
    text: String,
    parts: Vec<Value>,
    calls: Vec<ToolCall>,
    usage: Usage,
    finish: Option<String>,
    seen: bool,
}

impl Reader for GeminiReader {
    fn feed(&mut self, ev: &SseEvent, on_text: &mut dyn FnMut(&str)) -> Result<(), AgentError> {
        let v: Value = serde_json::from_str(&ev.data).map_err(|_| bad("an event that is not JSON"))?;
        if let Some(err) = v.get("error").filter(|e| !e.is_null()) {
            return Err(AgentError::Provider { status: err["code"].as_u64().unwrap_or(0) as u16, message: err["message"].as_str().unwrap_or("an error").chars().take(300).collect() });
        }
        self.seen = true;
        let u = &v["usageMetadata"];
        if u.is_object() {
            self.usage.input = u["promptTokenCount"].as_u64().unwrap_or(self.usage.input);
            self.usage.output = u["candidatesTokenCount"].as_u64().unwrap_or(0) + u["thoughtsTokenCount"].as_u64().unwrap_or(0);
        }
        let cand = &v["candidates"][0];
        if let Some(r) = cand["finishReason"].as_str() {
            self.finish = Some(r.to_string());
        }
        if let Some(parts) = cand["content"]["parts"].as_array() {
            for part in parts {
                self.parts.push(part.clone());
                if let Some(call) = part.get("functionCall") {
                    self.calls.push(ToolCall { id: format!("call_{}", self.calls.len()), name: from_wire_name(call["name"].as_str().unwrap_or("")), args: if call["args"].is_null() { json!({}) } else { call["args"].clone() } });
                } else if let Some(t) = part["text"].as_str() {
                    // A part marked as a thought is the model's own working, not an answer.
                    if part["thought"] != true && !t.is_empty() {
                        self.text.push_str(t);
                        on_text(t);
                    }
                }
            }
        }
        Ok(())
    }

    fn finish(self: Box<Self>) -> Result<Turn, AgentError> {
        if !self.seen {
            return Err(bad("the stream ended before any answer"));
        }
        let stop = match self.finish.as_deref() {
            _ if !self.calls.is_empty() => Stop::ToolCalls,
            Some("MAX_TOKENS") => Stop::Length,
            Some("STOP") | None => Stop::End,
            Some(_) => Stop::Other,
        };
        let raw = (!self.calls.is_empty()).then(|| json!({ "parts": self.parts }));
        Ok(Turn { text: self.text, calls: self.calls, usage: self.usage, stop, raw })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::sse::SseDecoder;

    fn run(style: Style, wire: &str) -> (Turn, String) {
        let mut d = SseDecoder::default();
        let mut r = reader(style);
        let mut seen = String::new();
        let mut events = d.push(wire.as_bytes()).unwrap();
        events.extend(d.finish());
        for e in events {
            r.feed(&e, &mut |t| seen.push_str(t)).unwrap();
        }
        (r.finish().unwrap(), seen)
    }

    fn ep(kind: Kind, base: &str, model: &str) -> Endpoint {
        Endpoint::parse(kind, base, model).unwrap()
    }

    fn tool() -> ToolDecl {
        ToolDecl { name: "history.read".into(), description: "Revisions".into(), parameters: json!({ "type": "object", "properties": { "document": { "type": "string" } }, "required": ["document"] }) }
    }

    fn conversation() -> Vec<Msg> {
        vec![
            Msg::User("What changed?".into()),
            Msg::Assistant { text: "Let me look.".into(), calls: vec![ToolCall { id: "c1".into(), name: "history.read".into(), args: json!({ "document": "Charter.docx" }) }], raw: None },
            Msg::ToolResult { call_id: "c1".into(), name: "history.read".into(), output: "r15 Merge #1".into() },
        ]
    }

    #[test]
    fn anthropic_requests_and_streams() {
        let b = build(&ep(Kind::Anthropic, "https://api.anthropic.com", "claude-sonnet-4-5"), "Be brief.", &conversation(), &[tool()]);
        assert_eq!(b.url, "https://api.anthropic.com/v1/messages");
        assert_eq!(b.key_header, Some(("x-api-key", "")));
        assert!(b.headers.iter().any(|(k, v)| *k == "anthropic-version" && v == "2023-06-01"));
        assert_eq!(b.body["system"], "Be brief.");
        assert_eq!(b.body["tools"][0]["name"], "history__read");
        assert_eq!(b.body["messages"][1]["content"][1]["type"], "tool_use");
        assert_eq!(b.body["messages"][2]["content"][0]["type"], "tool_result");
        assert_eq!(b.body["messages"][2]["content"][0]["tool_use_id"], "c1");
        let wire = "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"usage\":{\"input_tokens\":472,\"output_tokens\":2}}}\n\n\
event: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\n\
event: ping\ndata: {\"type\":\"ping\"}\n\n\
event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Looking\"}}\n\n\
event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\" now.\"}}\n\n\
event: content_block_stop\ndata: {\"type\":\"content_block_stop\",\"index\":0}\n\n\
event: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":1,\"content_block\":{\"type\":\"tool_use\",\"id\":\"toolu_1\",\"name\":\"history__read\",\"input\":{}}}\n\n\
event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":1,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"{\\\"document\\\":\"}}\n\n\
event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":1,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\" \\\"Charter.docx\\\"}\"}}\n\n\
event: content_block_stop\ndata: {\"type\":\"content_block_stop\",\"index\":1}\n\n\
event: message_delta\ndata: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"tool_use\"},\"usage\":{\"output_tokens\":57}}\n\n\
event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n";
        let (turn, seen) = run(Style::Anthropic, wire);
        assert_eq!(seen, "Looking now.");
        assert_eq!(turn.text, "Looking now.");
        assert_eq!(turn.stop, Stop::ToolCalls);
        assert_eq!(turn.usage, Usage { input: 472, output: 57 });
        assert_eq!(turn.calls, vec![ToolCall { id: "toolu_1".into(), name: "history.read".into(), args: json!({ "document": "Charter.docx" }) }]);
    }

    #[test]
    fn an_anthropic_error_event_names_what_went_wrong() {
        let mut r = reader(Style::Anthropic);
        let ev = SseEvent { event: Some("error".into()), data: "{\"type\":\"error\",\"error\":{\"type\":\"overloaded_error\",\"message\":\"Overloaded\"}}".into() };
        assert!(matches!(r.feed(&ev, &mut |_| {}), Err(AgentError::RateLimited { .. })));
        let ev = SseEvent { event: Some("error".into()), data: "{\"type\":\"error\",\"error\":{\"type\":\"invalid_request_error\",\"message\":\"bad\"}}".into() };
        assert!(matches!(r.feed(&ev, &mut |_| {}), Err(AgentError::Provider { .. })));
    }

    #[test]
    fn openai_style_requests_and_streams_with_split_tool_arguments() {
        for (kind, base, url) in [
            (Kind::Openai, "https://api.openai.com/v1", "https://api.openai.com/v1/chat/completions"),
            (Kind::Mistral, "https://api.mistral.ai/v1", "https://api.mistral.ai/v1/chat/completions"),
            (Kind::Deepseek, "https://api.deepseek.com", "https://api.deepseek.com/chat/completions"),
            (Kind::Qwen, "https://dashscope-intl.aliyuncs.com/compatible-mode/v1", "https://dashscope-intl.aliyuncs.com/compatible-mode/v1/chat/completions"),
            (Kind::Kimi, "https://api.moonshot.ai/v1", "https://api.moonshot.ai/v1/chat/completions"),
            (Kind::Glm, "https://open.bigmodel.cn/api/paas/v4", "https://open.bigmodel.cn/api/paas/v4/chat/completions"),
            (Kind::Ollama, "http://127.0.0.1:11434", "http://127.0.0.1:11434/v1/chat/completions"),
            (Kind::Compat, "https://llm.example.org/v1/", "https://llm.example.org/v1/chat/completions"),
        ] {
            let b = build(&ep(kind, base, "m"), "Be brief.", &conversation(), &[tool()]);
            assert_eq!(b.url, url, "{kind:?}");
            assert_eq!(b.key_header.is_some(), kind.needs_key(), "{kind:?}");
            assert_eq!(b.body["messages"][0]["role"], "system");
            assert_eq!(b.body["messages"][2]["tool_calls"][0]["function"]["name"], "history__read");
            assert_eq!(b.body["messages"][2]["tool_calls"][0]["function"]["arguments"], "{\"document\":\"Charter.docx\"}");
            assert_eq!(b.body["messages"][3]["role"], "tool");
            assert_eq!(b.body["stream_options"].is_object(), kind.asks_for_usage(), "{kind:?}");
        }
        let wire = "data: {\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":\"On it. \"},\"finish_reason\":null}]}\n\n\
data: {\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_9\",\"type\":\"function\",\"function\":{\"name\":\"history__read\",\"arguments\":\"\"}}]},\"finish_reason\":null}]}\n\n\
data: {\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"{\\\"document\\\"\"}}]},\"finish_reason\":null}]}\n\n\
data: {\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\": \\\"Budget.xlsx\\\"}\"}}]},\"finish_reason\":null}]}\n\n\
data: {\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"tool_calls\"}]}\n\n\
data: {\"choices\":[],\"usage\":{\"prompt_tokens\":120,\"completion_tokens\":31,\"total_tokens\":151}}\n\n\
data: [DONE]\n\n";
        let (turn, seen) = run(Style::OpenAi, wire);
        assert_eq!(seen, "On it. ");
        assert_eq!(turn.stop, Stop::ToolCalls);
        assert_eq!(turn.usage, Usage { input: 120, output: 31 });
        assert_eq!(turn.calls, vec![ToolCall { id: "call_9".into(), name: "history.read".into(), args: json!({ "document": "Budget.xlsx" }) }]);
    }

    #[test]
    fn openai_style_servers_that_put_usage_in_the_choice_or_send_whole_calls_still_read() {
        // Moonshot: usage inside the last choice. Ollama: the whole call in one chunk, finish "stop".
        let wire = "data: {\"choices\":[{\"delta\":{\"content\":\"Hi\"},\"finish_reason\":null}]}\n\ndata: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\",\"usage\":{\"prompt_tokens\":9,\"completion_tokens\":2}}]}\n\ndata: [DONE]\n\n";
        let (turn, _) = run(Style::OpenAi, wire);
        assert_eq!(turn.usage, Usage { input: 9, output: 2 });
        assert_eq!(turn.stop, Stop::End);
        let wire = "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"id\":\"c\",\"function\":{\"name\":\"requests__list\",\"arguments\":\"{\\\"state\\\":\\\"open\\\"}\"}}]},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n";
        let (turn, _) = run(Style::OpenAi, wire);
        assert_eq!(turn.stop, Stop::ToolCalls);
        assert_eq!(turn.calls[0].name, "requests.list");
        assert_eq!(turn.calls[0].args, json!({ "state": "open" }));
    }

    #[test]
    fn gemini_requests_and_streams_and_keeps_its_own_parts() {
        let b = build(&ep(Kind::Google, "https://generativelanguage.googleapis.com/v1beta", "gemini-2.5-pro"), "Be brief.", &conversation(), &[tool()]);
        assert_eq!(b.url, "https://generativelanguage.googleapis.com/v1beta/models/gemini-2.5-pro:streamGenerateContent?alt=sse");
        assert_eq!(b.key_header, Some(("x-goog-api-key", "")));
        assert!(!b.url.contains("key="), "the key never goes in the address");
        assert_eq!(b.body["systemInstruction"]["parts"][0]["text"], "Be brief.");
        assert_eq!(b.body["contents"][1]["role"], "model");
        assert_eq!(b.body["contents"][1]["parts"][1]["functionCall"]["name"], "history__read");
        assert_eq!(b.body["contents"][2]["parts"][0]["functionResponse"]["response"]["output"], "r15 Merge #1");
        assert_eq!(b.body["tools"][0]["functionDeclarations"][0]["name"], "history__read");
        let wire = "data: {\"candidates\":[{\"content\":{\"role\":\"model\",\"parts\":[{\"text\":\"Checking. \"}]}}]}\n\n\
data: {\"candidates\":[{\"content\":{\"role\":\"model\",\"parts\":[{\"functionCall\":{\"name\":\"requests__list\",\"args\":{\"state\":\"open\"}},\"thoughtSignature\":\"SIG\"}]},\"finishReason\":\"STOP\"}],\"usageMetadata\":{\"promptTokenCount\":88,\"candidatesTokenCount\":12,\"thoughtsTokenCount\":5}}\n\n";
        let (turn, seen) = run(Style::Gemini, wire);
        assert_eq!(seen, "Checking. ");
        assert_eq!(turn.stop, Stop::ToolCalls);
        assert_eq!(turn.usage, Usage { input: 88, output: 17 });
        assert_eq!(turn.calls[0].name, "requests.list");
        assert_eq!(turn.calls[0].args, json!({ "state": "open" }));
        // The signature goes back with the call on the next request.
        let history = vec![Msg::User("q".into()), Msg::Assistant { text: turn.text.clone(), calls: turn.calls.clone(), raw: turn.raw.clone() }, Msg::ToolResult { call_id: "call_0".into(), name: "requests.list".into(), output: "none".into() }];
        let again = build(&ep(Kind::Google, "https://generativelanguage.googleapis.com/v1beta", "gemini-2.5-pro"), "", &history, &[tool()]);
        assert_eq!(again.body["contents"][1]["parts"][1]["thoughtSignature"], "SIG");
    }

    #[test]
    fn a_stream_that_ends_without_an_answer_is_an_error() {
        for style in [Style::Anthropic, Style::Gemini, Style::OpenAi] {
            assert!(reader(style).finish().is_err(), "{style:?}");
        }
    }
}
