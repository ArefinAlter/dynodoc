//! Shared by the contract tests: the specification text and its tool definitions.
#![allow(dead_code)]

use std::collections::BTreeMap;

use engine_agent::error::AgentError;
use serde_json::{Map, Value};

/// The specification text, or `None` where it is not shipped: the standalone engine
/// repository does not carry the private design documents, and these tests then skip.
pub fn spec() -> Option<String> {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/AI-AGENT-PROTOCOL-SPEC.md"
    );
    match std::fs::read_to_string(path) {
        Ok(text) => Some(text.replace("\r\n", "\n")),
        Err(_) => {
            eprintln!("skipped: docs/AI-AGENT-PROTOCOL-SPEC.md is not in this repository");
            None
        }
    }
}

/// The first JSON block under each `### 7.N \`tool\`` heading: the specification's tool
/// definitions, by tool name.
pub fn spec_definitions(text: &str) -> BTreeMap<String, Value> {
    let mut found = BTreeMap::new();
    let mut tool: Option<String> = None;
    let mut block: Option<String> = None;
    for line in text.lines() {
        if let Some(open) = &mut block {
            if line == "```" {
                if let (Some(name), Ok(value)) = (&tool, serde_json::from_str::<Value>(open)) {
                    if value.get("inputSchema").is_some() {
                        found.entry(name.clone()).or_insert(value);
                    }
                }
                block = None;
            } else {
                open.push_str(line);
                open.push('\n');
            }
        } else if line.starts_with("```json") {
            block = Some(String::new());
        } else if let Some(heading) = line.strip_prefix("### ") {
            tool = heading
                .strip_prefix("7.")
                .and_then(|h| h.split('`').nth(1))
                .map(str::to_owned);
        } else if line.starts_with("## ") {
            tool = None;
        }
    }
    found
}

/// The steps of the transcript under `### 16.1` whose arguments are printed whole on their
/// line, with those arguments (the propose step is continued over lines and is not one).
pub fn transcript_16_1(text: &str) -> Vec<(String, Map<String, Value>)> {
    let section = text.split("### 16.1").nth(1).expect("section 16.1");
    let block = section.split("```").nth(1).expect("its first code block");
    block
        .lines()
        .filter_map(|line| {
            let mut words = line.split_whitespace();
            words.next()?.parse::<u32>().ok()?;
            let tool = words.next()?.to_owned();
            let from = line.find('{')?;
            let to = line.find(" ->")?;
            let args: Value = serde_json::from_str(line[from..to].trim()).ok()?;
            Some((tool, args.as_object()?.clone()))
        })
        .collect()
}

/// `details` as `engine-api/src/agent/error.rs` writes it for a protocol error.
pub fn details(error: &AgentError) -> Value {
    let mut map = Map::new();
    map.insert("agent_code".into(), error.agent_code().into());
    let refs = |refs: &[engine_agent::refs::Ref]| -> Value {
        refs.iter().map(|r| r.as_str()).collect::<Vec<_>>().into()
    };
    match error {
        AgentError::PolicyDenied { policy } => {
            map.insert("policy".into(), (*policy).into());
        }
        AgentError::BadRequest { field } => {
            map.insert("field".into(), (*field).into());
        }
        AgentError::NotFound { refs: r }
        | AgentError::UnknownRef { refs: r }
        | AgentError::AmbiguousRef { refs: r }
        | AgentError::RedactedBlock { refs: r }
        | AgentError::Conflict { refs: r } => {
            if !r.is_empty() {
                map.insert("refs".into(), refs(r));
            }
        }
        AgentError::StaleBase { target, current } => {
            map.insert("ref".into(), target.as_str().into());
            map.insert("current".into(), current.as_str().into());
        }
        AgentError::MatchCount {
            found,
            expected,
            refs: r,
        } => {
            map.insert("found".into(), (*found).into());
            map.insert("expected".into(), (*expected).into());
            if !r.is_empty() {
                map.insert("refs".into(), refs(r));
            }
        }
        AgentError::LimitExceeded {
            field,
            limit,
            value,
        } => {
            map.insert("field".into(), (*field).into());
            map.insert("limit".into(), (*limit).into());
            map.insert("value".into(), (*value).into());
        }
        AgentError::BudgetExceeded { retry_after } | AgentError::RateLimited { retry_after } => {
            map.insert("retry_after".into(), (*retry_after).into());
        }
        _ => {}
    }
    Value::Object(map)
}
