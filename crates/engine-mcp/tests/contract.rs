//! The contract between this crate and the protocol specification
//! (docs/AI-AGENT-PROTOCOL-SPEC.md): the tool definitions against section 7, the request
//! mapping against the transcripts of section 16, the error lines against the `engine-agent`
//! error table, and the compatibility rules of the vendor document, section 4.
//!
//! The spec text is read from the repository, so a change to either side fails here.

use std::collections::BTreeMap;

use engine_agent::{dct::parse_header, error::AgentError};
use engine_mcp::{
    backend::header_pairs,
    error::{hint, CODES},
    request::{plan, Method},
    Profile, Tool, ToolError,
};
use serde_json::{json, Map, Value};
use uuid::Uuid;

const PROJECT: Uuid = Uuid::from_u128(0x1111_1111_2222_4333_8444_5555_5555_5555);

/// The specification text, or `None` where it is not shipped: the standalone engine
/// repository does not carry the private design documents, and these tests then skip.
fn spec() -> Option<String> {
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
fn spec_definitions(text: &str) -> BTreeMap<String, Value> {
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

fn mine(tool: Tool) -> &'static Value {
    tool.definition()
}

// --- section 7: the definitions ----------------------------------------------------------------

/// The deviations from the specification's schemas, each stated once (crate documentation,
/// "Decisions and deviations", items 3 to 5). Applying them to the spec must give exactly
/// our schema: any other difference is drift.
fn adjusted(tool: Tool, spec: &Value) -> Value {
    let mut schema = spec["inputSchema"].clone();
    let properties = schema["properties"].as_object_mut().unwrap();
    for (name, property) in properties.iter_mut() {
        let object = property.as_object_mut().unwrap();
        // 5: a type on enum properties.
        if object.contains_key("enum") && !object.contains_key("type") {
            let kind = if object["enum"][0].is_number() {
                "integer"
            } else {
                "string"
            };
            object.insert("type".into(), json!(kind));
        }
        // 3: `file` is a UUID or its 8-character prefix.
        if name == "file" {
            assert_eq!(*property, json!({"type": "string", "format": "uuid"}));
            *property = mine(tool)["inputSchema"]["properties"]["file"].clone();
        }
    }
    // 4: a cursor to continue a truncated answer.
    if matches!(tool, Tool::Find | Tool::Diff) {
        assert!(
            !properties.contains_key("cursor"),
            "the spec now has a cursor; drop deviation 4"
        );
        properties.insert("cursor".into(), json!({"type": "string"}));
    }
    schema
}

#[test]
fn the_definitions_are_the_specifications_apart_from_the_stated_deviations() {
    let Some(text) = spec() else { return };
    let specified = spec_definitions(&text);
    for tool in Tool::READ_ONLY {
        let ours = mine(tool);
        let spec = specified
            .get(tool.name())
            .unwrap_or_else(|| panic!("no definition of {} in the spec", tool.name()));
        if tool == Tool::Status {
            continue; // narrowed, below
        }
        assert_eq!(ours["description"], spec["description"], "{}", tool.name());
        assert_eq!(ours["annotations"], spec["annotations"], "{}", tool.name());
        assert_eq!(ours["inputSchema"], adjusted(tool, spec), "{}", tool.name());
        // Added to the spec: a title (spec 13.3, the directory requires one).
        assert!(ours["title"].as_str().is_some_and(|t| !t.is_empty()));
    }
    // 2: `status` offers `access` only. Everything else about it is the spec's.
    let status = &specified["status"];
    assert_eq!(mine(Tool::Status)["annotations"], status["annotations"]);
    assert_eq!(
        status["inputSchema"]["properties"]["what"]["enum"],
        json!(["proposal", "proposals", "access"])
    );
    assert_eq!(
        mine(Tool::Status)["inputSchema"]["properties"]["what"]["enum"],
        json!(["access"])
    );
    // The write tools exist in the spec and are not offered.
    assert!(specified.contains_key("propose") && specified.contains_key("submit"));
    let names: Vec<&str> = Profile::read_only()
        .tools()
        .iter()
        .map(|t| t.name())
        .collect();
    assert_eq!(
        names,
        ["files", "outline", "read", "find", "diff", "status"]
    );
}

#[test]
fn the_definitions_stay_small() {
    let sizes: Vec<(&str, usize)> = Tool::READ_ONLY
        .iter()
        .map(|t| {
            let d = mine(*t);
            // The spec's measure: name, description, inputSchema and annotations.
            let counted = json!({
                "name": d["name"], "description": d["description"],
                "inputSchema": d["inputSchema"], "annotations": d["annotations"]
            });
            (t.name(), serde_json::to_string(&counted).unwrap().len())
        })
        .collect();
    let total: usize = sizes.iter().map(|(_, n)| n).sum();
    println!("definition bytes (spec measure): {sizes:?} total {total}");
    let published: usize = Profile::read_only()
        .definitions()
        .iter()
        .map(|d| serde_json::to_string(d).unwrap().len())
        .sum();
    println!("definition bytes as published (with titles): {published}");
    // 4,845 and 4,983 when written. Spec 13.3 says 4,472 for a read-only grant and the
    // token-efficiency document 4,616; the difference is the stated deviations.
    assert!(total <= 4_900, "definitions grew to {total} bytes");
    assert!(
        published <= 5_050,
        "published definitions grew to {published} bytes"
    );
}

#[test]
fn the_definitions_obey_the_vendor_compatibility_rules() {
    // docs/AI-AGENT-VENDOR-INTEGRATIONS.md section 4.
    fn walk(value: &Value, path: &str) {
        match value {
            Value::Object(map) => {
                for forbidden in [
                    "oneOf",
                    "anyOf",
                    "allOf",
                    "$ref",
                    "exclusiveMinimum",
                    "exclusiveMaximum",
                ] {
                    assert!(!map.contains_key(forbidden), "{forbidden} at {path}");
                }
                if let Some(kind) = map.get("type") {
                    assert!(kind.is_string(), "type array at {path}");
                }
                for (key, child) in map {
                    walk(child, &format!("{path}/{key}"));
                }
            }
            Value::Array(items) => items.iter().for_each(|v| walk(v, path)),
            _ => {}
        }
    }
    assert!(!"dynodoc".contains(['-', '_']));
    for tool in Tool::READ_ONLY {
        let d = mine(tool);
        let name = d["name"].as_str().unwrap();
        assert!(name.len() <= 63 && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'));
        assert!(
            d["title"].is_string(),
            "{name}: the directory requires a title"
        );
        for hint in ["readOnlyHint", "destructiveHint", "openWorldHint"] {
            assert!(
                d["annotations"][hint].is_boolean(),
                "{name}: {hint} must be explicit"
            );
        }
        assert_eq!(d["annotations"]["readOnlyHint"], true);
        assert_eq!(d["annotations"]["destructiveHint"], false);
        // Every property has a type, including the enums.
        for (property, schema) in d["inputSchema"]["properties"].as_object().unwrap() {
            assert!(schema["type"].is_string(), "{name}.{property} has no type");
        }
        walk(&d["inputSchema"], name);
        // No sentences addressed to other tools or the model beyond the tool's own use.
        let description = d["description"].as_str().unwrap().to_lowercase();
        for banned in [
            "ignore",
            "you must",
            "before using any other",
            "always call",
        ] {
            assert!(!description.contains(banned), "{name}: {description}");
        }
    }
    let instructions = Profile::read_only().instructions();
    assert!(instructions.chars().count() <= 500);
    assert!(instructions.starts_with("dynodoc.agent/1, read-only. Start with `outline`."));
    assert!(
        !instructions.contains("propose"),
        "the profile has no such tool"
    );
}

// --- section 16: the transcripts -----------------------------------------------------------------

/// The read-only steps of the transcript under `### 16.1`, with the arguments it prints.
fn transcript_16_1(text: &str) -> Vec<(String, Map<String, Value>)> {
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

fn url(plan: &engine_mcp::request::Plan) -> String {
    let query: Vec<String> = plan.query.iter().map(|(k, v)| format!("{k}={v}")).collect();
    let method = if plan.method == Method::Get {
        "GET"
    } else {
        "POST"
    };
    let path = plan.path.replace(&PROJECT.to_string(), "<P>");
    if query.is_empty() {
        format!("{method} {path}")
    } else {
        format!("{method} {path}?{}", query.join("&"))
    }
}

fn request(tool: &str, args: Value) -> Result<String, ToolError> {
    let tool = Tool::parse(tool).unwrap();
    let args = args.as_object().unwrap().clone();
    tool.validate(&args)?;
    plan(tool, &args, PROJECT).map(|p| url(&p))
}

#[test]
fn the_read_only_steps_of_transcript_16_1_validate_and_map_to_the_routes() {
    let Some(text) = spec() else { return };
    let steps = transcript_16_1(&text);
    // `propose` and `submit` are not parsed (the propose line is continued) or not offered.
    let read_only: Vec<_> = steps
        .iter()
        .filter(|(tool, _)| Tool::parse(tool).is_some())
        .collect();
    assert_eq!(
        read_only
            .iter()
            .map(|(t, _)| t.as_str())
            .collect::<Vec<_>>(),
        ["files", "outline", "find"],
        "the transcript changed; update this test with it"
    );
    let routes: Vec<String> = read_only
        .iter()
        .map(|(tool, args)| request(tool, Value::Object(args.clone())).unwrap())
        .collect();
    assert_eq!(
        routes,
        [
            "GET projects/<P>/agent/files?q=Q3 Policy",
            "GET projects/<P>/agent/files/7034d450/outline?depth=0",
            "GET projects/<P>/agent/files/7034d450/find?q=clause&word=true",
        ]
    );
    // The only other step with parseable arguments is `submit`, which is not offered.
    let others: Vec<&str> = steps
        .iter()
        .map(|(t, _)| t.as_str())
        .filter(|t| Tool::parse(t).is_none())
        .collect();
    assert_eq!(others, ["submit"]);
}

#[test]
fn transcripts_16_2_and_16_3_map_to_the_routes() {
    // 16.2: outline (profile), then read range A1:L41. The edit step is not offered.
    assert_eq!(
        request(
            "outline",
            json!({"file": "5c1d0f4e", "depth": 1, "under": "Budget"})
        )
        .unwrap(),
        "GET projects/<P>/agent/files/5c1d0f4e/outline?depth=1&under=Budget"
    );
    assert_eq!(
        request(
            "read",
            json!({"file": "5c1d0f4e", "range": "Budget!A1:L41"})
        )
        .unwrap(),
        "GET projects/<P>/agent/files/5c1d0f4e/cells?range=Budget!A1:L41"
    );
    // 16.3: read two blocks, one held by hash.
    assert_eq!(
        request(
            "read",
            json!({"file": "7034d450", "refs": ["7C02", "7D91"], "have": {"7D91": "00ab34"}})
        )
        .unwrap(),
        "POST projects/<P>/agent/files/7034d450/blocks:read"
    );
    // The `status` of a proposal is not offered in this profile.
    let error = request("status", json!({"proposal": "4d0e9b1c"})).unwrap_err();
    assert_eq!(error, ToolError::bad_request("proposal"));
    // What `status` can still answer.
    assert_eq!(
        request("status", json!({"what": "access"})).unwrap(),
        "GET projects/<P>/agent/access"
    );
}

#[test]
fn every_route_of_the_read_half_is_reachable_from_a_tool() {
    // Spec 12.1 routes 1 to 5, 7 to 9 and 14 (route 6 is the outline route with `under`).
    let cases = [
        (
            json!({"q": "x"}),
            "files",
            "GET projects/<P>/agent/files?q=x",
        ),
        (
            json!({"file": "7034d450"}),
            "outline",
            "GET projects/<P>/agent/files/7034d450/outline",
        ),
        (
            json!({"file": "7034d450", "refs": ["A"]}),
            "read",
            "GET projects/<P>/agent/files/7034d450/blocks?refs=A",
        ),
        (
            json!({"file": "7034d450", "refs": ["A"], "have": {"A": "000000"}}),
            "read",
            "POST projects/<P>/agent/files/7034d450/blocks:read",
        ),
        (
            json!({"file": "5c1d0f4e", "range": "S!A1:B2"}),
            "read",
            "GET projects/<P>/agent/files/5c1d0f4e/cells?range=S!A1:B2",
        ),
        (
            json!({"file": "9aa2c3d1", "slides": ["s2"]}),
            "read",
            "GET projects/<P>/agent/files/9aa2c3d1/slides?ids=s2",
        ),
        (
            json!({"file": "7034d450", "q": "x"}),
            "find",
            "GET projects/<P>/agent/files/7034d450/find?q=x",
        ),
        (
            json!({"file": "7034d450", "from": 410, "to": 412, "detail": "text"}),
            "diff",
            "GET projects/<P>/agent/files/7034d450/diff?from=410&to=412&detail=text",
        ),
        (json!({}), "status", "GET projects/<P>/agent/access"),
    ];
    for (args, tool, expected) in cases {
        assert_eq!(
            request(tool, args.clone()).unwrap(),
            expected,
            "{tool} {args}"
        );
    }
}

#[test]
fn a_full_uuid_file_id_is_written_canonically() {
    let line = request(
        "outline",
        json!({"file": "7034D450-1111-4222-8333-444455556666"}),
    )
    .unwrap();
    assert_eq!(
        line,
        "GET projects/<P>/agent/files/7034d450-1111-4222-8333-444455556666/outline"
    );
}

// --- section 10: errors -------------------------------------------------------------------------------

/// `details` as `engine-api/src/agent/error.rs` writes it for a protocol error.
fn details(error: &AgentError) -> Value {
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

#[test]
fn every_error_of_the_table_survives_the_http_round_trip_byte_for_byte() {
    let table = engine_agent::corpus::error_table();
    assert_eq!(table.len(), 26);
    for error in table {
        let body = json!({"error": error.hint(), "code": error.code(), "details": details(&error)});
        let rebuilt = ToolError::from_http(error.http_status(), body.to_string().as_bytes(), None);
        assert_eq!(rebuilt.to_dct(), error.to_dct(), "{}", error.agent_code());
        assert_eq!(rebuilt.code(), error.agent_code());
        // The line is DCT: the strict parser reads it.
        let (header, _) = engine_agent::dct::split_response(&rebuilt.to_dct()).unwrap();
        assert_eq!(header.get("code"), Some(error.agent_code()));
    }
}

#[test]
fn the_code_list_and_the_hints_are_the_error_tables() {
    assert_eq!(CODES, engine_agent::error::ALL_CODES);
    for error in engine_agent::corpus::error_table() {
        assert_eq!(
            hint(error.agent_code()),
            error.hint(),
            "{}",
            error.agent_code()
        );
    }
}

#[test]
fn the_header_reader_agrees_with_the_strict_parser_on_every_golden_answer() {
    let mut checked = 0;
    for call in engine_agent::corpus::golden_calls() {
        let first = call.text.lines().next().unwrap();
        let Ok(strict) = parse_header(first) else {
            continue; // the multi-line error table
        };
        assert_eq!(header_pairs(first), strict.pairs, "{}", call.name);
        checked += 1;
    }
    assert!(checked > 40, "only {checked} golden answers were compared");
}
