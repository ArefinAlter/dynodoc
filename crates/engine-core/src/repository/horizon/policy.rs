//! Retention policy `dynodoc.retention-policy.v1` (design section 4).
//!
//! A policy is data: a closed JSON vocabulary with no expressions. The loader
//! reports every problem it finds with a stable code and a JSON pointer, so an
//! operator can fix a file in one pass. Loading allocates nothing outside this
//! value and touches no repository.
//!
//! The policy hash is `SHA-256("dynodoc.retention-policy\0v1\0" || canonical JSON)`
//! of the *normalized* policy: every defaulted field written out, optional fields
//! absent when unset, lists sorted and unique. Two files that mean the same policy
//! therefore have the same hash. YAML is not accepted (no YAML parser is a
//! dependency); the design's YAML example converts to this JSON one-to-one.

use super::{canonical_bytes, sha256, valid_policy_id, Tier, MAX_SIGNATURES};
use crate::{repository::valid_branch, shared_checkpoint::Address};
use serde::de::{self, Deserializer, MapAccess, SeqAccess, Visitor};
use serde::Deserialize;
use serde_json::{json, Map, Value};
use std::collections::BTreeSet;

pub const SCHEMA: &str = "dynodoc.retention-policy.v1";
pub const MAX_POLICY_BYTES: usize = 64 * 1024;
const HASH_DOMAIN: &[u8] = b"dynodoc.retention-policy\0v1\0";
pub const MAX_PINNED_COMMITS: usize = 4096;
pub const MAX_BRANCH_PATTERNS: usize = 64;
pub const MAX_DAYS: u64 = 36_500;
pub const MAX_COMMIT_WINDOW: u64 = 1_000_000;
pub const MAX_QUARANTINE_DAYS: u64 = 365;
pub const MAX_DESCRIPTION_BYTES: usize = 1000;
pub const MAX_DESTINATION_BYTES: usize = 1024;

/// Stable error codes. The first six are the design's table (section 4.3).
pub mod code {
    pub const UNKNOWN_FIELD: &str = "policy.unknown_field";
    pub const HEADS_UNPROTECTED: &str = "policy.heads_unprotected";
    pub const WINDOW_EMPTY: &str = "policy.window_empty";
    pub const SIGNATURES_BELOW_FLOOR: &str = "policy.signatures_below_floor";
    pub const TOPOLOGY_DROP_UNACKNOWLEDGED: &str = "policy.topology_drop_unacknowledged";
    /// Reserved: the planner is read-only and cannot test a destination for
    /// writability; the archive writer (design P3) will report this at plan time.
    pub const ARCHIVE_UNREACHABLE: &str = "policy.archive_unreachable";
    // Added by the loader.
    pub const MALFORMED_JSON: &str = "policy.malformed_json";
    pub const TOO_LARGE: &str = "policy.too_large";
    pub const DUPLICATE_FIELD: &str = "policy.duplicate_field";
    pub const UNSUPPORTED_VERSION: &str = "policy.unsupported_version";
    pub const MISSING_FIELD: &str = "policy.missing_field";
    pub const INVALID_VALUE: &str = "policy.invalid_value";
    pub const PROTECTION_NOT_OPTIONAL: &str = "policy.protection_not_optional";
    pub const ARCHIVE_DESTINATION_MISSING: &str = "policy.archive_destination_missing";
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct PolicyIssue {
    /// JSON pointer to the offending value ("" for the document).
    pub path: String,
    pub code: &'static str,
    pub message: String,
}

impl std::fmt::Display for PolicyIssue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} at {:?}: {}", self.code, self.path, self.message)
    }
}

/// Every problem found, sorted by path then code.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PolicyErrors(pub Vec<PolicyIssue>);

impl PolicyErrors {
    pub fn has(&self, code: &str) -> bool {
        self.0.iter().any(|issue| issue.code == code)
    }
}

impl std::fmt::Display for PolicyErrors {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (index, issue) in self.0.iter().enumerate() {
            if index > 0 {
                writeln!(f)?;
            }
            write!(f, "{issue}")?;
        }
        Ok(())
    }
}

impl std::error::Error for PolicyErrors {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LoadOptions {
    /// The smallest `signatures.required` the context accepts: 1 locally, the
    /// service's configured minimum on the shared service (design CP-Q2).
    pub min_signatures: u32,
}

impl Default for LoadOptions {
    fn default() -> Self {
        Self { min_signatures: 1 }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Topology {
    /// Every released commit leaves a skeleton entry.
    Keep,
    /// No skeleton: ancestry across the horizon is unanswerable (needs acknowledgement).
    Drop,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReviewedCandidates {
    None,
    Body,
    BodyStates,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MergeInputs {
    None,
    Bodies,
    States,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Window {
    pub keep_within_days: Option<u32>,
    pub keep_last_commits: Option<u32>,
    pub min_commits_kept: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MergeBases {
    pub against: String,
    /// Exact branch names or a prefix ending in `*` (`*` alone matches every branch).
    pub branches: Vec<String>,
}

impl MergeBases {
    pub fn matches(&self, branch: &str) -> bool {
        self.branches
            .iter()
            .any(|pattern| match pattern.strip_suffix('*') {
                Some(prefix) => branch.starts_with(prefix),
                None => pattern == branch,
            })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Protect {
    /// Always true in a loaded policy.
    pub branch_heads: bool,
    pub named_versions: bool,
    pub merge_bases: Option<MergeBases>,
    pub open_review_candidates: bool,
    pub reviewed_candidates: ReviewedCandidates,
    /// Always true in a loaded policy: the verifiability closure is hard-coded.
    pub recovery_sources: bool,
    pub merge_inputs: MergeInputs,
    /// Always true in a loaded policy.
    pub holds: bool,
    pub commits: Vec<Address>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ArchivePolicy {
    pub required: bool,
    pub destination: Option<String>,
}

/// A validated, normalized policy.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Policy {
    pub id: String,
    pub description: String,
    pub tier: Tier,
    pub window: Window,
    pub protect: Protect,
    pub topology: Topology,
    pub acknowledge_unmergeable_history: bool,
    pub archive: ArchivePolicy,
    pub signatures_required: u32,
    pub quarantine_days: u32,
}

// ---- strict JSON ------------------------------------------------------------

/// `serde_json::Value`, except that a repeated object key is an error: the last
/// value silently winning would let a duplicated `keep_last_commits` change a
/// reviewed policy.
struct Strict(Value);

struct StrictVisitor;

impl<'de> Visitor<'de> for StrictVisitor {
    type Value = Strict;
    fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("a JSON value")
    }
    fn visit_bool<E>(self, v: bool) -> Result<Strict, E> {
        Ok(Strict(Value::Bool(v)))
    }
    fn visit_i64<E>(self, v: i64) -> Result<Strict, E> {
        Ok(Strict(Value::from(v)))
    }
    fn visit_u64<E>(self, v: u64) -> Result<Strict, E> {
        Ok(Strict(Value::from(v)))
    }
    fn visit_f64<E>(self, v: f64) -> Result<Strict, E> {
        Ok(Strict(Value::from(v)))
    }
    fn visit_str<E>(self, v: &str) -> Result<Strict, E> {
        Ok(Strict(Value::String(v.to_owned())))
    }
    fn visit_unit<E>(self) -> Result<Strict, E> {
        Ok(Strict(Value::Null))
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Strict, A::Error> {
        let mut items = Vec::new();
        while let Some(Strict(item)) = seq.next_element()? {
            items.push(item);
        }
        Ok(Strict(Value::Array(items)))
    }
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Strict, A::Error> {
        let mut object = Map::new();
        while let Some((key, Strict(value))) = map.next_entry::<String, Strict>()? {
            if object.contains_key(&key) {
                return Err(de::Error::custom(format!("duplicate field `{key}`")));
            }
            object.insert(key, value);
        }
        Ok(Strict(Value::Object(object)))
    }
}

impl<'de> Deserialize<'de> for Strict {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(StrictVisitor)
    }
}

// ---- field readers ------------------------------------------------------------

struct Issues(BTreeSet<PolicyIssue>);

impl Issues {
    fn add(&mut self, path: &str, code: &'static str, message: impl Into<String>) {
        self.0.insert(PolicyIssue {
            path: path.to_owned(),
            code,
            message: message.into(),
        });
    }

    /// `value` as an object whose keys are all in `allowed`.
    fn object<'a>(
        &mut self,
        value: &'a Value,
        path: &str,
        allowed: &[&str],
    ) -> Option<&'a Map<String, Value>> {
        let Some(map) = value.as_object() else {
            self.add(path, code::INVALID_VALUE, "expected an object");
            return None;
        };
        for key in map.keys().filter(|k| !allowed.contains(&k.as_str())) {
            self.add(
                &format!("{path}/{key}"),
                code::UNKNOWN_FIELD,
                format!("`{key}` is not part of {SCHEMA}"),
            );
        }
        Some(map)
    }

    fn flag(&mut self, map: &Map<String, Value>, path: &str, key: &str) -> Option<bool> {
        let value = map.get(key)?;
        let flag = value.as_bool();
        if flag.is_none() {
            self.add(
                &format!("{path}/{key}"),
                code::INVALID_VALUE,
                "expected true or false",
            );
        }
        flag
    }

    fn number(
        &mut self,
        map: &Map<String, Value>,
        path: &str,
        key: &str,
        range: std::ops::RangeInclusive<u64>,
    ) -> Option<u32> {
        let value = map.get(key)?;
        match value.as_u64().filter(|n| range.contains(n)) {
            Some(n) => Some(n as u32),
            None => {
                self.add(
                    &format!("{path}/{key}"),
                    code::INVALID_VALUE,
                    format!(
                        "expected an integer from {} to {}",
                        range.start(),
                        range.end()
                    ),
                );
                None
            }
        }
    }

    fn choice<'a>(
        &mut self,
        map: &'a Map<String, Value>,
        path: &str,
        key: &str,
        words: &[&str],
    ) -> Option<&'a str> {
        let value = map.get(key)?;
        match value.as_str().filter(|s| words.contains(s)) {
            Some(word) => Some(word),
            None => {
                self.add(
                    &format!("{path}/{key}"),
                    code::INVALID_VALUE,
                    format!("expected one of {}", words.join(", ")),
                );
                None
            }
        }
    }

    fn text<'a>(&mut self, map: &'a Map<String, Value>, path: &str, key: &str) -> Option<&'a str> {
        let value = map.get(key)?;
        let text = value.as_str();
        if text.is_none() {
            self.add(
                &format!("{path}/{key}"),
                code::INVALID_VALUE,
                "expected a string",
            );
        }
        text
    }

    fn strings(
        &mut self,
        map: &Map<String, Value>,
        path: &str,
        key: &str,
        limit: usize,
    ) -> Option<Vec<String>> {
        let value = map.get(key)?;
        let at = format!("{path}/{key}");
        let Some(items) = value.as_array() else {
            self.add(&at, code::INVALID_VALUE, "expected a list of strings");
            return None;
        };
        if items.len() > limit {
            self.add(&at, code::INVALID_VALUE, format!("at most {limit} entries"));
            return None;
        }
        let mut out = Vec::new();
        for (index, item) in items.iter().enumerate() {
            match item.as_str() {
                Some(text) => out.push(text.to_owned()),
                None => self.add(
                    &format!("{at}/{index}"),
                    code::INVALID_VALUE,
                    "expected a string",
                ),
            }
        }
        (out.len() == items.len()).then_some(out)
    }
}

fn valid_pattern(pattern: &str) -> bool {
    match pattern.strip_suffix('*') {
        Some(prefix) => {
            pattern.len() <= 200
                && !prefix.contains('*')
                && !prefix.starts_with('/')
                && !prefix.contains("..")
                && prefix
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"-_/.".contains(&b))
        }
        None => valid_branch(pattern),
    }
}

/// `file://` URL with only the `{project}` and `{sequence}` placeholders.
fn valid_destination(text: &str) -> bool {
    if !text.starts_with("file://")
        || text.len() > MAX_DESTINATION_BYTES
        || text.chars().any(char::is_control)
    {
        return false;
    }
    let mut rest = text;
    while let Some(open) = rest.find(['{', '}']) {
        if !rest[open..].starts_with('{') {
            return false;
        }
        let Some(close) = rest[open..].find('}') else {
            return false;
        };
        if !["{project}", "{sequence}"].contains(&&rest[open..open + close + 1]) {
            return false;
        }
        rest = &rest[open + close + 1..];
    }
    true
}

impl Policy {
    /// Parse, validate and normalize a policy file.
    pub fn load(bytes: &[u8], options: LoadOptions) -> Result<Self, PolicyErrors> {
        let mut issues = Issues(BTreeSet::new());
        let policy = Self::read(bytes, options, &mut issues);
        match policy {
            Some(policy) if issues.0.is_empty() => Ok(policy),
            _ => Err(PolicyErrors(issues.0.into_iter().collect())),
        }
    }

    fn read(bytes: &[u8], options: LoadOptions, issues: &mut Issues) -> Option<Self> {
        if bytes.len() > MAX_POLICY_BYTES {
            issues.add("", code::TOO_LARGE, "a policy is at most 64 KiB");
            return None;
        }
        let root = match serde_json::from_slice::<Strict>(bytes) {
            Ok(Strict(value)) => value,
            Err(error) => {
                let duplicate = error.to_string().starts_with("duplicate field");
                issues.add(
                    "",
                    if duplicate {
                        code::DUPLICATE_FIELD
                    } else {
                        code::MALFORMED_JSON
                    },
                    error.to_string(),
                );
                return None;
            }
        };
        let top = issues.object(
            &root,
            "",
            &[
                "version",
                "id",
                "description",
                "tier",
                "window",
                "protect",
                "topology",
                "acknowledge_unmergeable_history",
                "archive",
                "signatures",
                "quarantine_days",
            ],
        )?;
        match top.get("version") {
            None => issues.add("/version", code::MISSING_FIELD, "version is required"),
            Some(version) if version.as_u64() == Some(1) => {}
            Some(_) => issues.add(
                "/version",
                code::UNSUPPORTED_VERSION,
                "only version 1 is defined",
            ),
        }
        let id = match issues.text(top, "", "id") {
            Some(id) if valid_policy_id(id) => Some(id.to_owned()),
            Some(_) => {
                issues.add(
                    "/id",
                    code::INVALID_VALUE,
                    "1-64 of a-z, 0-9, '.', '_', '-'; not starting with a symbol",
                );
                None
            }
            None => {
                if !top.contains_key("id") {
                    issues.add("/id", code::MISSING_FIELD, "id is required");
                }
                None
            }
        };
        let description = match issues.text(top, "", "description") {
            Some(text)
                if text.len() > MAX_DESCRIPTION_BYTES
                    || text.chars().any(|c| c.is_control() && c != '\n') =>
            {
                issues.add(
                    "/description",
                    code::INVALID_VALUE,
                    "at most 1000 bytes, no control characters",
                );
                None
            }
            other => Some(other.unwrap_or("").to_owned()),
        };
        let tier = match issues.choice(top, "", "tier", &["content", "history"]) {
            Some("content") => Some(Tier::Content),
            Some(_) => Some(Tier::History),
            None => {
                if !top.contains_key("tier") {
                    issues.add("/tier", code::MISSING_FIELD, "tier is required");
                }
                None
            }
        };

        // window
        let mut window = Window::default();
        match top.get("window") {
            Some(value) => {
                if let Some(map) = issues.object(
                    value,
                    "/window",
                    &["keep_within_days", "keep_last_commits", "min_commits_kept"],
                ) {
                    window.keep_within_days =
                        issues.number(map, "/window", "keep_within_days", 1..=MAX_DAYS);
                    window.keep_last_commits =
                        issues.number(map, "/window", "keep_last_commits", 1..=MAX_COMMIT_WINDOW);
                    window.min_commits_kept =
                        issues.number(map, "/window", "min_commits_kept", 1..=MAX_COMMIT_WINDOW);
                    if !["keep_within_days", "keep_last_commits", "min_commits_kept"]
                        .iter()
                        .any(|key| map.contains_key(*key))
                    {
                        issues.add(
                            "/window",
                            code::WINDOW_EMPTY,
                            "a keep rule or min_commits_kept is required",
                        );
                    }
                }
            }
            None => issues.add(
                "/window",
                code::WINDOW_EMPTY,
                "a keep rule or min_commits_kept is required",
            ),
        }

        // protect
        let mut protect = Protect {
            branch_heads: true,
            named_versions: true,
            merge_bases: None,
            open_review_candidates: true,
            reviewed_candidates: ReviewedCandidates::Body,
            recovery_sources: true,
            merge_inputs: MergeInputs::States,
            holds: true,
            commits: Vec::new(),
        };
        let mut heads_declared = None;
        if let Some(value) = top.get("protect") {
            const KEYS: &[&str] = &[
                "branch_heads",
                "named_versions",
                "merge_bases",
                "open_review_candidates",
                "reviewed_candidates",
                "recovery_sources",
                "merge_inputs",
                "holds",
                "commits",
            ];
            if let Some(map) = issues.object(value, "/protect", KEYS) {
                heads_declared = issues.flag(map, "/protect", "branch_heads");
                if heads_declared == Some(false) {
                    issues.add(
                        "/protect/branch_heads",
                        code::HEADS_UNPROTECTED,
                        "branch heads are never released; this cannot be false",
                    );
                }
                for key in ["recovery_sources", "holds"] {
                    if issues.flag(map, "/protect", key) == Some(false) {
                        issues.add(
                            &format!("/protect/{key}"),
                            code::PROTECTION_NOT_OPTIONAL,
                            "this protection is hard-coded and cannot be switched off",
                        );
                    }
                }
                protect.named_versions = issues
                    .flag(map, "/protect", "named_versions")
                    .unwrap_or(true);
                protect.open_review_candidates = issues
                    .flag(map, "/protect", "open_review_candidates")
                    .unwrap_or(true);
                protect.reviewed_candidates = match issues.choice(
                    map,
                    "/protect",
                    "reviewed_candidates",
                    &["none", "body", "body+states"],
                ) {
                    Some("none") => ReviewedCandidates::None,
                    Some("body+states") => ReviewedCandidates::BodyStates,
                    _ => ReviewedCandidates::Body,
                };
                protect.merge_inputs = match issues.choice(
                    map,
                    "/protect",
                    "merge_inputs",
                    &["none", "bodies", "states"],
                ) {
                    Some("none") => MergeInputs::None,
                    Some("bodies") => MergeInputs::Bodies,
                    _ => MergeInputs::States,
                };
                if let Some(value) = map.get("merge_bases") {
                    if let Some(bases) =
                        issues.object(value, "/protect/merge_bases", &["against", "branches"])
                    {
                        let against = match issues.text(bases, "/protect/merge_bases", "against") {
                            Some(name) if valid_branch(name) => Some(name.to_owned()),
                            Some(_) => {
                                issues.add(
                                    "/protect/merge_bases/against",
                                    code::INVALID_VALUE,
                                    "not a valid branch name",
                                );
                                None
                            }
                            None => {
                                if !bases.contains_key("against") {
                                    issues.add(
                                        "/protect/merge_bases/against",
                                        code::MISSING_FIELD,
                                        "against is required",
                                    );
                                }
                                None
                            }
                        };
                        let mut branches = match bases.contains_key("branches") {
                            true => issues.strings(
                                bases,
                                "/protect/merge_bases",
                                "branches",
                                MAX_BRANCH_PATTERNS,
                            ),
                            false => Some(vec!["*".to_owned()]),
                        };
                        if let Some(list) = &mut branches {
                            if list.is_empty() {
                                issues.add(
                                    "/protect/merge_bases/branches",
                                    code::INVALID_VALUE,
                                    "at least one pattern",
                                );
                            }
                            for (index, pattern) in list.iter().enumerate() {
                                if !valid_pattern(pattern) {
                                    issues.add(
                                        &format!("/protect/merge_bases/branches/{index}"),
                                        code::INVALID_VALUE,
                                        "a branch name or a prefix ending in `*`",
                                    );
                                }
                            }
                            list.sort();
                            list.dedup();
                        }
                        if let (Some(against), Some(branches)) = (against, branches) {
                            protect.merge_bases = Some(MergeBases { against, branches });
                        }
                    }
                }
                if let Some(list) = issues.strings(map, "/protect", "commits", MAX_PINNED_COMMITS) {
                    let mut commits = BTreeSet::new();
                    for (index, text) in list.into_iter().enumerate() {
                        match Address::try_from(text) {
                            Ok(address) => {
                                commits.insert(address);
                            }
                            Err(_) => issues.add(
                                &format!("/protect/commits/{index}"),
                                code::INVALID_VALUE,
                                "expected 64 lowercase hexadecimal digits",
                            ),
                        }
                    }
                    protect.commits = commits.into_iter().collect();
                }
            }
        }
        if tier == Some(Tier::History) && heads_declared.is_none() {
            issues.add(
                "/protect/branch_heads",
                code::HEADS_UNPROTECTED,
                "a history policy must state `branch_heads: true`",
            );
        }

        // topology
        let topology = match issues.choice(top, "", "topology", &["keep", "drop"]) {
            Some("drop") => Topology::Drop,
            _ => Topology::Keep,
        };
        let acknowledged = issues
            .flag(top, "", "acknowledge_unmergeable_history")
            .unwrap_or(false);
        if topology == Topology::Drop {
            if !acknowledged {
                issues.add(
                    "/topology",
                    code::TOPOLOGY_DROP_UNACKNOWLEDGED,
                    "drop needs `acknowledge_unmergeable_history: true`",
                );
            }
            if tier == Some(Tier::Content) {
                issues.add(
                    "/topology",
                    code::INVALID_VALUE,
                    "a content policy releases no commit, so it has no topology to drop",
                );
            }
        }

        // archive
        let mut archive = ArchivePolicy::default();
        if let Some(value) = top.get("archive") {
            if let Some(map) = issues.object(value, "/archive", &["required", "destination"]) {
                archive.required = issues.flag(map, "/archive", "required").unwrap_or(false);
                if let Some(text) = issues.text(map, "/archive", "destination") {
                    if valid_destination(text) {
                        archive.destination = Some(text.to_owned());
                    } else {
                        issues.add(
                            "/archive/destination",
                            code::INVALID_VALUE,
                            "a file:// URL using only {project} and {sequence} placeholders",
                        );
                    }
                }
                if archive.required && !map.contains_key("destination") {
                    issues.add(
                        "/archive/destination",
                        code::ARCHIVE_DESTINATION_MISSING,
                        "a required archive needs a destination",
                    );
                }
            }
        }

        // signatures and quarantine
        let mut signatures_required = 1;
        if let Some(value) = top.get("signatures") {
            if let Some(map) = issues.object(value, "/signatures", &["required"]) {
                match issues.number(map, "/signatures", "required", 0..=MAX_SIGNATURES as u64) {
                    Some(n) if n >= options.min_signatures.max(1) => signatures_required = n,
                    Some(_) => issues.add(
                        "/signatures/required",
                        code::SIGNATURES_BELOW_FLOOR,
                        format!(
                            "at least {} distinct signers are required here",
                            options.min_signatures.max(1)
                        ),
                    ),
                    None => {}
                }
            }
        } else if options.min_signatures > 1 {
            issues.add(
                "/signatures/required",
                code::SIGNATURES_BELOW_FLOOR,
                format!(
                    "at least {} distinct signers are required here",
                    options.min_signatures
                ),
            );
        }
        let quarantine_days = issues
            .number(top, "", "quarantine_days", 0..=MAX_QUARANTINE_DAYS)
            .unwrap_or(0);

        Some(Policy {
            id: id?,
            description: description?,
            tier: tier?,
            window,
            protect,
            topology,
            acknowledge_unmergeable_history: acknowledged,
            archive,
            signatures_required,
            quarantine_days,
        })
    }

    /// The normalized form that is hashed (see the module documentation).
    pub fn to_value(&self) -> Value {
        let mut window = Map::new();
        for (key, value) in [
            ("keep_within_days", self.window.keep_within_days),
            ("keep_last_commits", self.window.keep_last_commits),
            ("min_commits_kept", self.window.min_commits_kept),
        ] {
            if let Some(value) = value {
                window.insert(key.into(), json!(value));
            }
        }
        let protect = &self.protect;
        let mut shield = json!({
            "branch_heads": protect.branch_heads,
            "named_versions": protect.named_versions,
            "open_review_candidates": protect.open_review_candidates,
            "reviewed_candidates": match protect.reviewed_candidates {
                ReviewedCandidates::None => "none",
                ReviewedCandidates::Body => "body",
                ReviewedCandidates::BodyStates => "body+states",
            },
            "recovery_sources": protect.recovery_sources,
            "merge_inputs": match protect.merge_inputs {
                MergeInputs::None => "none",
                MergeInputs::Bodies => "bodies",
                MergeInputs::States => "states",
            },
            "holds": protect.holds,
            "commits": protect.commits.iter().map(|a| a.as_str()).collect::<Vec<_>>(),
        });
        if let Some(bases) = &protect.merge_bases {
            shield["merge_bases"] = json!({"against": bases.against, "branches": bases.branches});
        }
        let mut archive = json!({"required": self.archive.required});
        if let Some(destination) = &self.archive.destination {
            archive["destination"] = json!(destination);
        }
        json!({
            "version": 1,
            "id": self.id,
            "description": self.description,
            "tier": match self.tier { Tier::Content => "content", Tier::History => "history" },
            "window": window,
            "protect": shield,
            "topology": match self.topology { Topology::Keep => "keep", Topology::Drop => "drop" },
            "acknowledge_unmergeable_history": self.acknowledge_unmergeable_history,
            "archive": archive,
            "signatures": {"required": self.signatures_required},
            "quarantine_days": self.quarantine_days,
        })
    }

    /// Canonical bytes of the normalized policy.
    pub fn canonical_bytes(&self) -> Vec<u8> {
        canonical_bytes(&self.to_value()).expect("a policy value always encodes")
    }

    pub fn hash(&self) -> Address {
        super::address_of(&sha256(&[HASH_DOMAIN, &self.canonical_bytes()]))
    }

    pub fn policy_ref(&self) -> super::PolicyRef {
        super::PolicyRef {
            id: self.id.clone(),
            hash: self.hash(),
        }
    }
}
