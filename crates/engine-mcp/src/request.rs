//! Tool call to HTTP request (protocol spec section 12.1), as a pure function.
//!
//! [`plan`] decides the route, query and body of a validated tool call; the backend only
//! sends what it returns. Keeping the mapping free of I/O lets the contract tests pin every
//! request byte for byte.
//!
//! | Tool | Route (under `projects/{project}/agent`) |
//! |---|---|
//! | `files` | `GET files` |
//! | `outline` | `GET files/{file}/outline` |
//! | `read` refs or section | `GET files/{file}/blocks`, or `POST files/{file}/blocks:read` when `have` is given |
//! | `read` range | `GET files/{file}/cells` |
//! | `read` slides | `GET files/{file}/slides` |
//! | `find` | `GET files/{file}/find` |
//! | `diff` | `GET files/{file}/diff` |
//! | `propose` | `POST files/{file}/propose` (the service's name; the spec says `proposals`) |
//! | `submit` | `POST proposals/{proposal}/submit`, or `.../withdraw` for `action=withdraw` |
//! | `status` | `GET proposals/{proposal}` for a proposal, else `GET access` |
//!
//! The sheet-profile route (6) is not used: the outline route answers a sheet's profile
//! for `under=<sheet>`, which is what the `outline` tool's `under` argument carries.
//!
//! The planner is pure, so it cannot invent the `request_id` of a `propose`: the backend adds
//! one to the arguments when the model gave none, before planning, and keeps the plan for the
//! whole call (a renewal or a retry sends the same body).

use serde_json::{json, Map, Value};
use uuid::Uuid;

use crate::{error::ToolError, tools::Tool};

/// `ops` per `propose` (protocol spec 11; the service refuses more).
pub const MAX_OPS: usize = 200;
/// The request body of a `propose` (protocol spec 11: 1 MiB).
pub const MAX_BODY_BYTES: usize = 1 << 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    Get,
    Post,
}

/// One HTTP request, relative to the API base.
#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    pub method: Method,
    /// Without a leading slash: `projects/<project>/agent/...`.
    pub path: String,
    /// In the order the tool schema lists the arguments, so a URL is deterministic.
    pub query: Vec<(&'static str, String)>,
    /// JSON body of a `POST`.
    pub body: Option<Value>,
    /// Value for the `If-None-Match` header, in its quoted form: `"412:9f3a1c04"`.
    pub if_none_match: Option<String>,
    /// First 8 hex characters of the file, for answers the client builds itself.
    pub file: Option<String>,
    /// The `request_id` a `propose` carries in its body: the key under which a lost answer
    /// can be asked for again, named in the error of a call whose outcome is unknown.
    pub request_id: Option<String>,
}

type Args = Map<String, Value>;

fn text<'a>(args: &'a Args, key: &str) -> Option<&'a str> {
    args.get(key).and_then(Value::as_str)
}

fn strings(args: &Args, key: &str) -> Vec<String> {
    args.get(key)
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

/// Add `key=<value>` for each argument present, in the order given. Integers and booleans
/// are written as the server's query parser reads them.
fn pairs(args: &Args, keys: &[&'static str]) -> Vec<(&'static str, String)> {
    keys.iter()
        .filter_map(|key| {
            let rendered = match args.get(*key)? {
                Value::String(s) => s.clone(),
                Value::Number(n) => n.to_string(),
                Value::Bool(b) => b.to_string(),
                _ => return None,
            };
            Some((*key, rendered))
        })
        .collect()
}

/// A file id as a path segment: a full UUID (written canonically) or 8 hex characters.
/// Nothing else reaches the URL, so an argument cannot add path segments or a query.
fn file_segment(args: &Args) -> Result<String, ToolError> {
    let raw = text(args, "file").ok_or_else(|| ToolError::bad_request("file"))?;
    if raw.len() == 36 {
        return Uuid::parse_str(raw)
            .map(|u| u.hyphenated().to_string())
            .map_err(|_| ToolError::bad_request("file"));
    }
    if raw.len() == 8 && raw.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Ok(raw.to_ascii_lowercase());
    }
    Err(ToolError::bad_request("file"))
}

/// `"412:9f3a1c04"` from `412:9f3a1c04`, `"412:9f3a1c04"` or `W/"412:9f3a1c04"`; `None`
/// for anything else (an `oh=` hash, for example).
fn etag_of(value: &str) -> Option<String> {
    let bare = value
        .trim()
        .strip_prefix("W/")
        .unwrap_or_else(|| value.trim())
        .trim_matches('"');
    let (rev, chain) = bare.split_once(':')?;
    let valid = !rev.is_empty()
        && rev.len() <= 20
        && rev.bytes().all(|b| b.is_ascii_digit())
        && chain.len() == 8
        && chain
            .bytes()
            .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'));
    valid.then(|| format!("\"{bare}\""))
}

/// Build the request for a tool call whose arguments already satisfy the tool's schema.
/// `project` is the project this connection's session is for.
pub fn plan(tool: Tool, args: &Args, project: Uuid) -> Result<Plan, ToolError> {
    let root = format!("projects/{project}/agent");
    let get = |path: String, query: Vec<(&'static str, String)>| Plan {
        method: Method::Get,
        path,
        query,
        body: None,
        if_none_match: None,
        request_id: None,
        file: None,
    };
    match tool {
        Tool::Files => {
            // The session reaches one project; a request for another is not sent.
            if let Some(asked) = text(args, "project") {
                if Uuid::parse_str(asked) != Ok(project) {
                    return Err(ToolError::new("forbidden"));
                }
            }
            Ok(get(
                format!("{root}/files"),
                pairs(args, &["q", "kind", "limit", "cursor"]),
            ))
        }
        Tool::Status => status(args, &root),
        Tool::Propose => propose(args, &root),
        Tool::Submit => submit(args, &root),
        Tool::Find => {
            // The routes search one file; there is no project-wide search route.
            if text(args, "file").is_none() {
                return Err(ToolError::bad_request("file"));
            }
            if let Some(asked) = text(args, "project") {
                if Uuid::parse_str(asked) != Ok(project) {
                    return Err(ToolError::new("forbidden"));
                }
            }
            let file = file_segment(args)?;
            let mut plan = get(
                format!("{root}/files/{file}/find"),
                pairs(
                    args,
                    &[
                        "q", "regex", "case", "word", "in", "context", "limit", "cursor",
                    ],
                ),
            );
            plan.file = Some(file[..8].to_owned());
            Ok(plan)
        }
        Tool::Diff => {
            let file = file_segment(args)?;
            let mut plan = get(
                format!("{root}/files/{file}/diff"),
                pairs(
                    args,
                    &["from", "to", "proposal", "detail", "max_bytes", "cursor"],
                ),
            );
            plan.file = Some(file[..8].to_owned());
            Ok(plan)
        }
        Tool::Outline => {
            let file = file_segment(args)?;
            let mut query = pairs(args, &["depth", "preview", "under", "limit", "cursor"]);
            // An ETag from `_meta` is a header; an `oh=` hash is the library's own
            // parameter. The parameter keeps its place in the schema order.
            let mut if_none_match = None;
            if let Some(given) = text(args, "if_none_match") {
                match etag_of(given) {
                    Some(etag) => if_none_match = Some(etag),
                    None => {
                        let at = query
                            .iter()
                            .position(|(k, _)| matches!(*k, "limit" | "cursor"))
                            .unwrap_or(query.len());
                        query.insert(at, ("if_none_match", given.to_owned()));
                    }
                }
            }
            let mut plan = get(format!("{root}/files/{file}/outline"), query);
            plan.if_none_match = if_none_match;
            plan.file = Some(file[..8].to_owned());
            Ok(plan)
        }
        Tool::Read => read(args, &root),
    }
}

fn read(args: &Args, root: &str) -> Result<Plan, ToolError> {
    let file = file_segment(args)?;
    let refs = strings(args, "refs");
    let slides = strings(args, "slides");
    let section = text(args, "section");
    let range = text(args, "range");
    let selectors = [
        !refs.is_empty(),
        section.is_some(),
        range.is_some(),
        !slides.is_empty(),
    ];
    if selectors.iter().filter(|s| **s).count() != 1 {
        return Err(ToolError::bad_request("selector"));
    }
    let have = args
        .get("have")
        .and_then(Value::as_object)
        .filter(|h| !h.is_empty());
    if have.is_some() && (range.is_some() || !slides.is_empty()) {
        return Err(ToolError::bad_request("have"));
    }
    for item in refs.iter().chain(&slides) {
        // The server splits these lists on commas.
        if item.is_empty() || item.contains(',') || item.chars().any(char::is_control) {
            let field = if refs.contains(item) {
                "refs"
            } else {
                "slides"
            };
            return Err(ToolError::bad_request(field));
        }
    }
    let tail = ["fmt", "rev", "max_bytes", "cursor"];
    let base = format!("{root}/files/{file}");
    let file8 = file[..8].to_owned();
    let mut plan = if let Some(have) = have {
        // `have` does not fit a query string (spec 12.1, route 4).
        let mut body = Map::new();
        if !refs.is_empty() {
            body.insert("refs".into(), json!(refs));
        }
        if let Some(section) = section {
            body.insert("section".into(), json!(section));
        }
        body.insert("have".into(), Value::Object(have.clone()));
        for key in tail {
            if let Some(value) = args.get(key) {
                body.insert(key.into(), value.clone());
            }
        }
        Plan {
            method: Method::Post,
            path: format!("{base}/blocks:read"),
            query: Vec::new(),
            body: Some(Value::Object(body)),
            if_none_match: None,
            request_id: None,
            file: None,
        }
    } else if let Some(range) = range {
        let mut q = vec![("range", range.to_owned())];
        q.extend(pairs(args, &tail));
        Plan {
            method: Method::Get,
            path: format!("{base}/cells"),
            query: q,
            body: None,
            if_none_match: None,
            request_id: None,
            file: None,
        }
    } else if !slides.is_empty() {
        let mut q = vec![("ids", slides.join(","))];
        q.extend(pairs(args, &tail));
        Plan {
            method: Method::Get,
            path: format!("{base}/slides"),
            query: q,
            body: None,
            if_none_match: None,
            request_id: None,
            file: None,
        }
    } else {
        let mut q = Vec::new();
        if !refs.is_empty() {
            q.push(("refs", refs.join(",")));
        }
        if let Some(section) = section {
            q.push(("section", section.to_owned()));
        }
        q.extend(pairs(args, &tail));
        Plan {
            method: Method::Get,
            path: format!("{base}/blocks"),
            query: q,
            body: None,
            if_none_match: None,
            request_id: None,
            file: None,
        }
    };
    plan.file = Some(file8);
    Ok(plan)
}

/// A full UUID argument, written canonically; `None` when the argument is absent.
fn uuid_arg(args: &Args, key: &'static str) -> Result<Option<Uuid>, ToolError> {
    match args.get(key) {
        None => Ok(None),
        Some(Value::String(raw)) if raw.len() == 36 => Uuid::parse_str(raw)
            .map(Some)
            .map_err(|_| ToolError::bad_request(key)),
        Some(_) => Err(ToolError::bad_request(key)),
    }
}

fn post(path: String, body: Map<String, Value>) -> Plan {
    Plan {
        method: Method::Post,
        path,
        query: Vec::new(),
        body: Some(Value::Object(body)),
        if_none_match: None,
        request_id: None,
        file: None,
    }
}

/// `propose`: the body is the service's, minus `file` (the path names it). The service wants
/// a `request_id`; planning refuses a call without one rather than make one up. A call over
/// the protocol's size limits is refused here, before anything is sent.
fn propose(args: &Args, root: &str) -> Result<Plan, ToolError> {
    let file = file_segment(args)?;
    let request_id = uuid_arg(args, "request_id")?
        .ok_or_else(|| ToolError::bad_request("request_id"))?
        .to_string();
    let rev = args
        .get("rev")
        .and_then(Value::as_u64)
        .ok_or_else(|| ToolError::bad_request("rev"))?;
    let ops = args
        .get("ops")
        .and_then(Value::as_array)
        .filter(|ops| !ops.is_empty())
        .ok_or_else(|| ToolError::bad_request("ops"))?;
    if ops.len() > MAX_OPS {
        return Err(ToolError::new("limit_exceeded")
            .with("field", "ops")
            .with_number("limit", MAX_OPS as u64)
            .with_number("value", ops.len() as u64));
    }
    let mut body = Map::new();
    body.insert("rev".into(), json!(rev));
    body.insert("request_id".into(), json!(request_id));
    if let Some(proposal) = uuid_arg(args, "proposal")? {
        body.insert("proposal".into(), json!(proposal.to_string()));
    }
    for key in ["name", "ops", "dry_run", "strict", "return"] {
        if let Some(value) = args.get(key) {
            body.insert(key.into(), value.clone());
        }
    }
    let size = serde_json::to_vec(&body).map_or(usize::MAX, |bytes| bytes.len());
    if size > MAX_BODY_BYTES {
        return Err(ToolError::new("limit_exceeded")
            .with("field", "body")
            .with_number("limit", MAX_BODY_BYTES as u64)
            .with_number("value", size as u64));
    }
    let mut plan = post(format!("{root}/files/{file}/propose"), body);
    plan.file = Some(file[..8].to_owned());
    plan.request_id = Some(request_id);
    Ok(plan)
}

/// `submit` and `withdraw` are two routes of one tool; both take an optional note.
fn submit(args: &Args, root: &str) -> Result<Plan, ToolError> {
    let proposal = uuid_arg(args, "proposal")?.ok_or_else(|| ToolError::bad_request("proposal"))?;
    let action = match text(args, "action") {
        Some(action @ ("submit" | "withdraw")) => action,
        _ => return Err(ToolError::bad_request("action")),
    };
    let mut body = Map::new();
    if let Some(note) = text(args, "note") {
        body.insert("note".into(), json!(note));
    }
    Ok(post(format!("{root}/proposals/{proposal}/{action}"), body))
}

/// `status`: one proposal when one is named (or `what=proposal`), else the connection's own
/// access. `what=proposal` without a proposal, and `what=access` with one, are refused.
fn status(args: &Args, root: &str) -> Result<Plan, ToolError> {
    let proposal = uuid_arg(args, "proposal")?;
    let get = |path: String| Plan {
        method: Method::Get,
        path,
        query: Vec::new(),
        body: None,
        if_none_match: None,
        request_id: None,
        file: None,
    };
    match (text(args, "what"), proposal) {
        (None | Some("access"), None) => Ok(get(format!("{root}/access"))),
        (None | Some("proposal"), Some(proposal)) => {
            Ok(get(format!("{root}/proposals/{proposal}")))
        }
        (Some("proposal" | "access"), _) => Err(ToolError::bad_request("proposal")),
        (Some(_), _) => Err(ToolError::bad_request("what")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PROJECT: Uuid = Uuid::from_u128(0x1111_1111_2222_4333_8444_5555_5555_5555);

    fn args(value: Value) -> Args {
        value.as_object().unwrap().clone()
    }

    fn url(plan: &Plan) -> String {
        let query: Vec<String> = plan.query.iter().map(|(k, v)| format!("{k}={v}")).collect();
        if query.is_empty() {
            plan.path.clone()
        } else {
            format!("{}?{}", plan.path, query.join("&"))
        }
    }

    #[test]
    fn the_file_segment_admits_only_hex_ids() {
        for good in [
            "7034d450",
            "7034D450",
            "7034d450-1111-4222-8333-444455556666",
        ] {
            let plan = plan(Tool::Diff, &args(json!({"file": good, "from": 1})), PROJECT).unwrap();
            assert!(plan.path.contains("/files/7034d450"), "{}", plan.path);
        }
        for bad in [
            "../x",
            "7034d45",
            "7034d4500",
            "7034d45g",
            "7034d450/outline",
            "%2e%2e%2f%2e",
        ] {
            let error = plan(Tool::Outline, &args(json!({"file": bad})), PROJECT).unwrap_err();
            assert_eq!(error, ToolError::bad_request("file"), "{bad}");
        }
    }

    #[test]
    fn read_needs_exactly_one_selector() {
        let go = |v: Value| plan(Tool::Read, &args(v), PROJECT);
        let field = |v: Value| go(v).unwrap_err().to_dct();
        assert!(field(json!({"file": "7034d450"})).contains("field=selector"));
        assert!(field(json!({"file": "7034d450", "refs": []})).contains("field=selector"));
        assert!(
            field(json!({"file": "7034d450", "refs": ["A"], "section": "B"}))
                .contains("field=selector")
        );
        assert!(field(json!({"file": "7034d450", "refs": ["A,B"]})).contains("field=refs"));
        assert!(
            field(json!({"file": "7034d450", "range": "A!A1:B2", "have": {"A": "000000"}}))
                .contains("field=have")
        );
        assert_eq!(
            url(&go(json!({"file": "7034d450", "refs": ["7C02", "7D91"], "fmt": "md", "max_bytes": 2000})).unwrap()),
            format!("projects/{PROJECT}/agent/files/7034d450/blocks?refs=7C02,7D91&fmt=md&max_bytes=2000")
        );
    }

    #[test]
    fn have_moves_the_read_to_the_post_route() {
        let p = plan(
            Tool::Read,
            &args(json!({"file": "7034d450", "refs": ["7C02"], "have": {"7C02": "b7a210"}, "rev": 412})),
            PROJECT,
        )
        .unwrap();
        assert_eq!(p.method, Method::Post);
        assert!(p.path.ends_with("/files/7034d450/blocks:read"));
        assert_eq!(
            p.body.unwrap(),
            json!({"refs": ["7C02"], "have": {"7C02": "b7a210"}, "rev": 412})
        );
    }

    #[test]
    fn an_etag_is_a_header_and_an_oh_hash_is_a_parameter() {
        let go = |given: &str| {
            plan(
                Tool::Outline,
                &args(json!({"file": "7034d450", "depth": 0, "limit": 5, "if_none_match": given})),
                PROJECT,
            )
            .unwrap()
        };
        let etag = go("\"412:9f3a1c04\"");
        assert_eq!(etag.if_none_match.as_deref(), Some("\"412:9f3a1c04\""));
        assert_eq!(
            go("412:9f3a1c04").if_none_match.as_deref(),
            Some("\"412:9f3a1c04\"")
        );
        assert_eq!(
            go("W/\"412:9f3a1c04\"").if_none_match.as_deref(),
            Some("\"412:9f3a1c04\"")
        );
        assert!(url(&etag).ends_with("outline?depth=0&limit=5"));
        let oh = go("5be2a9");
        assert_eq!(oh.if_none_match, None);
        assert!(url(&oh).ends_with("outline?depth=0&if_none_match=5be2a9&limit=5"));
        // Upper-case hex or a wrong length is not an ETag.
        assert_eq!(go("412:9F3A1C04").if_none_match, None);
        assert_eq!(go("412:9f3a1c0").if_none_match, None);
    }

    #[test]
    fn another_projects_id_is_refused_before_anything_is_sent() {
        let other = "99999999-2222-4333-8444-555555555555";
        for (tool, a) in [
            (Tool::Files, json!({"project": other})),
            (
                Tool::Find,
                json!({"project": other, "file": "7034d450", "q": "x"}),
            ),
        ] {
            assert_eq!(
                plan(tool, &args(a), PROJECT).unwrap_err(),
                ToolError::new("forbidden")
            );
        }
        let own = PROJECT.to_string();
        let ok = plan(
            Tool::Files,
            &args(json!({"project": own, "q": "Q3"})),
            PROJECT,
        )
        .unwrap();
        assert_eq!(url(&ok), format!("projects/{PROJECT}/agent/files?q=Q3"));
    }
}
