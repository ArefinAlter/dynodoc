//! `dynodoc.merge-policy.v1`: which strategy decides which field, and how strictly.
//!
//! A policy is data: JSON text, loaded strictly (unknown keys, duplicate keys,
//! `null`, unknown strategies, unknown parameters and every other deviation are
//! errors with a path), normalized, and hashed over its canonical form. Nothing in
//! it carries document content.
//!
//! The canonical form is the validated policy with every schema-level default
//! written out, object keys sorted and each rule's `match.type` list sorted (it is
//! a set). Per-strategy parameters are kept exactly as written: a default a strategy
//! has for a parameter is not expanded, so a strategy's defaults are frozen with its
//! versioned id.

use super::{
    registry::Registry,
    strategy::{ParamType, Strategy},
    Aspect, Family, Gate,
};
use serde::{de::Error as _, Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;
use std::{collections::BTreeMap, fmt, marker::PhantomData};

/// The only schema version this module reads.
pub const VERSION: u32 = 1;
/// Input bytes accepted before parsing.
pub const MAX_INPUT_BYTES: usize = 65_536;
/// A commit embeds the canonical policy (design 6.3), so it has a hard bound.
pub const MAX_CANONICAL_BYTES: usize = 16_384;
pub const MAX_RULES: usize = 64;
/// `SHA-256(HASH_DOMAIN || canonical JSON)` is the policy hash.
pub const HASH_DOMAIN: &[u8] = b"dynodoc.merge-policy\0v1\0";
const MAX_TEXT: usize = 256;
const MAX_TYPES: usize = 16;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Unknown {
    /// No rule matched: leave the difference to a human (a conflict).
    Manual,
    /// No rule matched: the difference is a conflict no automatic choice may settle.
    Blocked,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Loser {
    None,
    Suggestion,
    Comment,
}

/// What the post-merge invariant checks do with a violation the merge introduced.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum InvariantMode {
    /// Report it as evidence; the plan is unchanged.
    Flag,
    /// Report it as evidence and as a conflict in the plan.
    Conflict,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Role {
    Owner,
    Manager,
    Editor,
    Contributor,
    Reviewer,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Defaults {
    #[serde(default = "Defaults::unknown")]
    pub unknown: Unknown,
    #[serde(default = "Defaults::loser")]
    pub loser: Loser,
    /// Highest authority first.
    #[serde(default = "Defaults::roles_rank")]
    pub roles_rank: Vec<Role>,
    #[serde(default = "Defaults::invariants")]
    pub invariants: InvariantMode,
}

impl Defaults {
    fn unknown() -> Unknown {
        Unknown::Manual
    }
    fn loser() -> Loser {
        Loser::Suggestion
    }
    fn roles_rank() -> Vec<Role> {
        vec![
            Role::Owner,
            Role::Manager,
            Role::Editor,
            Role::Contributor,
            Role::Reviewer,
        ]
    }
    fn invariants() -> InvariantMode {
        InvariantMode::Flag
    }
}

impl Default for Defaults {
    fn default() -> Self {
        Self {
            unknown: Self::unknown(),
            loser: Self::loser(),
            roles_rank: Self::roles_rank(),
            invariants: Self::invariants(),
        }
    }
}

/// A key that is present must not be `null`: the canonical form has one spelling.
fn present<'de, D: Deserializer<'de>, T: Deserialize<'de>>(d: D) -> Result<Option<T>, D::Error> {
    T::deserialize(d).map(Some)
}

/// A JSON object whose duplicate keys are an error rather than "last one wins".
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StrictMap<V>(pub BTreeMap<String, V>);

impl<V> Default for StrictMap<V> {
    fn default() -> Self {
        Self(BTreeMap::new())
    }
}

impl<V: Serialize> Serialize for StrictMap<V> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(s)
    }
}

impl<'de, V: Deserialize<'de>> Deserialize<'de> for StrictMap<V> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct Visit<V>(PhantomData<V>);
        impl<'de, V: Deserialize<'de>> serde::de::Visitor<'de> for Visit<V> {
            type Value = StrictMap<V>;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("an object")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                mut map: A,
            ) -> Result<Self::Value, A::Error> {
                let mut out = BTreeMap::new();
                while let Some((key, value)) = map.next_entry::<String, V>()? {
                    if out.contains_key(&key) {
                        return Err(A::Error::custom(format!("duplicate key `{key}`")));
                    }
                    out.insert(key, value);
                }
                Ok(StrictMap(out))
            }
        }
        d.deserialize_map(Visit(PhantomData))
    }
}

/// Which decisions a rule applies to. Every key present must hold; an empty `match`
/// holds everywhere a rule of the same kind (field or collection) can apply.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Match {
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub family: Option<Family>,
    /// Node types (`item`, `section`, ...) or, for the `content` field, the block
    /// type of the rich-text value (`paragraph`, `bulletList`, ...). Any one matches.
    #[serde(
        default,
        rename = "type",
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub types: Option<Vec<String>>,
    /// A field name, or a prefix ending in `*` such as `cell_*`.
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub field: Option<String>,
    /// Set for rules about a document-level collection; a rule without it decides
    /// node fields.
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub aspect: Option<Aspect>,
}

/// What a decision looks like to the rules.
#[derive(Clone, Copy, Debug)]
pub struct Probe<'a> {
    pub family: Family,
    pub node_type: &'a str,
    /// The block type of the rich-text value, when the field is `content`.
    pub block_type: Option<&'a str>,
    pub field: &'a str,
    pub aspect: Option<Aspect>,
}

impl Match {
    fn is_catch_all(&self) -> bool {
        self == &Self::default()
    }
    fn sorted(&self) -> Self {
        let mut m = self.clone();
        if let Some(types) = &mut m.types {
            types.sort();
        }
        m
    }
    pub fn matches(&self, probe: &Probe<'_>) -> bool {
        self.aspect == probe.aspect
            && self.family.is_none_or(|f| f == probe.family)
            && self.types.as_ref().is_none_or(|types| {
                types
                    .iter()
                    .any(|t| t == probe.node_type || Some(t.as_str()) == probe.block_type)
            })
            && self
                .field
                .as_ref()
                .is_none_or(|pattern| match pattern.strip_suffix('*') {
                    Some(prefix) => probe.field.starts_with(prefix),
                    None => probe.field == pattern,
                })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rule {
    pub id: String,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub description: Option<String>,
    #[serde(rename = "match")]
    pub matcher: Match,
    pub strategy: String,
    #[serde(default)]
    pub params: StrictMap<Value>,
    pub gate: Gate,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Floors {
    /// Strategy id to the weakest gate a rule using it may have.
    #[serde(default)]
    pub gate_at_least: StrictMap<Gate>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MergePolicy {
    pub version: u32,
    pub id: String,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub description: Option<String>,
    #[serde(default)]
    pub defaults: Defaults,
    /// Evaluated top to bottom; the first match wins.
    pub rules: Vec<Rule>,
    #[serde(default)]
    pub floors: Floors,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErrorKind {
    InputTooLarge,
    Parse,
    Version,
    Id,
    Text,
    Roles,
    TooManyRules,
    DuplicateRuleId,
    MatchType,
    MatchField,
    UnknownStrategy,
    AspectMismatch,
    UnknownParameter,
    ParameterType,
    ParameterRange,
    Shadowed,
    WeakensFloor,
    UnknownFloorStrategy,
    CanonicalTooLarge,
}

/// A rejected policy: what rule was broken and where in the document.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PolicyError {
    pub kind: ErrorKind,
    /// A path such as `rules[2].match.type`; empty for the whole document.
    pub path: String,
    pub message: String,
}

impl fmt::Display for PolicyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.path.is_empty() {
            f.write_str(&self.message)
        } else {
            write!(f, "{}: {}", self.path, self.message)
        }
    }
}

impl std::error::Error for PolicyError {}

fn err(kind: ErrorKind, path: impl Into<String>, message: impl Into<String>) -> PolicyError {
    PolicyError {
        kind,
        path: path.into(),
        message: message.into(),
    }
}

/// Lowercase ASCII, digits, `.`, `_` and `-`; starts with a letter or digit.
fn check_id(value: &str, path: &str) -> Result<(), PolicyError> {
    let ok = !value.is_empty()
        && value.len() <= 64
        && value.starts_with(|c: char| c.is_ascii_lowercase() || c.is_ascii_digit())
        && value
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '.' | '_' | '-'));
    if ok {
        Ok(())
    } else {
        Err(err(
            ErrorKind::Id,
            path,
            format!("`{value}` is not an identifier: 1-64 characters from a-z, 0-9, `.`, `_`, `-`, starting with a letter or digit"),
        ))
    }
}

fn check_text(value: &Option<String>, path: &str) -> Result<(), PolicyError> {
    match value {
        Some(text) if text.chars().count() > MAX_TEXT || text.chars().any(char::is_control) => {
            Err(err(
                ErrorKind::Text,
                path,
                format!(
                    "description must be at most {MAX_TEXT} characters without control characters"
                ),
            ))
        }
        _ => Ok(()),
    }
}

fn check_type_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 32
        && name.starts_with(|c: char| c.is_ascii_alphabetic())
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn check_field(pattern: &str) -> bool {
    let body = pattern.strip_suffix('*').unwrap_or(pattern);
    !pattern.is_empty()
        && pattern.len() <= 64
        && body
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '$' | ':' | '.' | '-'))
}

impl MergePolicy {
    /// Parse and validate. This is the only way text becomes a policy; the
    /// registry decides which strategies and parameters exist.
    pub fn from_json(bytes: &[u8], registry: &Registry) -> Result<Self, PolicyError> {
        if bytes.len() > MAX_INPUT_BYTES {
            return Err(err(
                ErrorKind::InputTooLarge,
                "",
                format!(
                    "policy is {} bytes; the limit is {MAX_INPUT_BYTES}",
                    bytes.len()
                ),
            ));
        }
        let policy: Self = serde_json::from_slice(bytes)
            .map_err(|e| err(ErrorKind::Parse, "", format!("not a valid policy: {e}")))?;
        policy.validate(registry)?;
        Ok(policy)
    }

    /// Every rule of the schema, in a fixed order, so the first error is stable.
    pub fn validate(&self, registry: &Registry) -> Result<(), PolicyError> {
        if self.version != VERSION {
            return Err(err(
                ErrorKind::Version,
                "version",
                format!(
                    "version {} is not supported; only {VERSION} is",
                    self.version
                ),
            ));
        }
        check_id(&self.id, "id")?;
        check_text(&self.description, "description")?;
        let roles = &self.defaults.roles_rank;
        let mut seen = std::collections::BTreeSet::new();
        let roles_ok = !roles.is_empty() && roles.iter().all(|r| seen.insert(format!("{r:?}")));
        if !roles_ok {
            return Err(err(
                ErrorKind::Roles,
                "defaults.roles_rank",
                "must list each role at most once and at least one role",
            ));
        }
        if self.rules.len() > MAX_RULES {
            return Err(err(
                ErrorKind::TooManyRules,
                "rules",
                format!("{} rules; the limit is {MAX_RULES}", self.rules.len()),
            ));
        }
        let mut ids = std::collections::BTreeSet::new();
        for (i, rule) in self.rules.iter().enumerate() {
            let at = format!("rules[{i}]");
            check_id(&rule.id, &format!("{at}.id"))?;
            if !ids.insert(rule.id.as_str()) {
                return Err(err(
                    ErrorKind::DuplicateRuleId,
                    format!("{at}.id"),
                    format!("rule id `{}` is used twice", rule.id),
                ));
            }
            check_text(&rule.description, &format!("{at}.description"))?;
            self.validate_match(&rule.matcher, &at)?;
            self.validate_strategy(rule, &at, registry)?;
            if let Some(earlier) = self.rules[..i]
                .iter()
                .find(|e| e.matcher.is_catch_all() || e.matcher.sorted() == rule.matcher.sorted())
            {
                return Err(err(
                    ErrorKind::Shadowed,
                    format!("{at}.match"),
                    format!(
                        "rule `{}` always matches first, so this rule never runs",
                        earlier.id
                    ),
                ));
            }
        }
        for (strategy, _) in &self.floors.gate_at_least.0 {
            if registry.get(strategy).is_none() {
                return Err(err(
                    ErrorKind::UnknownFloorStrategy,
                    format!("floors.gate_at_least.{strategy}"),
                    format!("unknown strategy `{strategy}`"),
                ));
            }
        }
        let size = self.canonical_bytes().len();
        if size > MAX_CANONICAL_BYTES {
            return Err(err(
                ErrorKind::CanonicalTooLarge,
                "",
                format!("canonical form is {size} bytes; the limit is {MAX_CANONICAL_BYTES}"),
            ));
        }
        Ok(())
    }

    fn validate_match(&self, m: &Match, at: &str) -> Result<(), PolicyError> {
        if let Some(types) = &m.types {
            let path = format!("{at}.match.type");
            let mut seen = std::collections::BTreeSet::new();
            if types.is_empty() || types.len() > MAX_TYPES {
                return Err(err(
                    ErrorKind::MatchType,
                    path,
                    format!("must list between 1 and {MAX_TYPES} types"),
                ));
            }
            for t in types {
                if !check_type_name(t) {
                    return Err(err(
                        ErrorKind::MatchType,
                        path,
                        format!("`{t}` is not a type name (a letter, then letters, digits or `_`; at most 32)"),
                    ));
                }
                if !seen.insert(t) {
                    return Err(err(
                        ErrorKind::MatchType,
                        path,
                        format!("`{t}` is listed twice"),
                    ));
                }
            }
        }
        if let Some(field) = &m.field {
            if !check_field(field) {
                return Err(err(
                    ErrorKind::MatchField,
                    format!("{at}.match.field"),
                    format!("`{field}` is not a field name or a prefix ending in `*`"),
                ));
            }
        }
        Ok(())
    }

    fn validate_strategy(
        &self,
        rule: &Rule,
        at: &str,
        registry: &Registry,
    ) -> Result<(), PolicyError> {
        let strategy: &dyn Strategy = registry.get(&rule.strategy).ok_or_else(|| {
            err(
                ErrorKind::UnknownStrategy,
                format!("{at}.strategy"),
                format!("unknown strategy `{}`", rule.strategy),
            )
        })?;
        let aspects = strategy.aspects();
        let compatible = match rule.matcher.aspect {
            Some(aspect) => aspects.contains(&aspect),
            None => aspects.is_empty(),
        };
        if !compatible {
            let what = match rule.matcher.aspect {
                Some(a) => format!("the `{a:?}` collection").to_lowercase(),
                None => "node fields".to_string(),
            };
            return Err(err(
                ErrorKind::AspectMismatch,
                format!("{at}.strategy"),
                format!("strategy `{}` does not decide {what}", rule.strategy),
            ));
        }
        for (name, value) in &rule.params.0 {
            let path = format!("{at}.params.{name}");
            let spec = strategy
                .params()
                .iter()
                .find(|s| s.name == name)
                .ok_or_else(|| {
                    err(
                        ErrorKind::UnknownParameter,
                        path.clone(),
                        format!("strategy `{}` has no parameter `{name}`", rule.strategy),
                    )
                })?;
            match spec.ty {
                ParamType::Bool if !value.is_boolean() => {
                    return Err(err(ErrorKind::ParameterType, path, "must be true or false"));
                }
                ParamType::Int { min, max } => match value.as_i64() {
                    None => {
                        return Err(err(ErrorKind::ParameterType, path, "must be an integer"));
                    }
                    Some(n) if n < min || n > max => {
                        return Err(err(
                            ErrorKind::ParameterRange,
                            path,
                            format!("{n} is outside {min}..={max}"),
                        ));
                    }
                    Some(_) => {}
                },
                ParamType::OneOf(options) => match value.as_str() {
                    None => return Err(err(ErrorKind::ParameterType, path, "must be text")),
                    Some(s) if !options.contains(&s) => {
                        return Err(err(
                            ErrorKind::ParameterRange,
                            path,
                            format!("`{s}` is not one of {}", options.join(", ")),
                        ));
                    }
                    Some(_) => {}
                },
                ParamType::Bool => {}
            }
        }
        // The gate is the strictest of the rule and the floor (design 6.2): a rule may
        // not be more permissive than the floor for its strategy.
        if let Some(floor) = self.floors.gate_at_least.0.get(&rule.strategy) {
            if rule.gate < *floor {
                return Err(err(
                    ErrorKind::WeakensFloor,
                    format!("{at}.gate"),
                    format!(
                        "gate `{}` is weaker than the floor `{}` for `{}`",
                        rule.gate.name(),
                        floor.name(),
                        rule.strategy
                    ),
                ));
            }
        }
        Ok(())
    }

    /// The first rule that matches, or `None` for the implicit `unknown` rule.
    pub fn find_rule(&self, probe: &Probe<'_>) -> Option<&Rule> {
        self.rules.iter().find(|r| r.matcher.matches(probe))
    }

    /// The gate a decision gets: the strictest of the rule and the floor.
    pub fn effective_gate(&self, rule: &Rule) -> Gate {
        match self.floors.gate_at_least.0.get(&rule.strategy) {
            Some(floor) => rule.gate.max(*floor),
            None => rule.gate,
        }
    }

    /// The canonical JSON value: see the module documentation.
    pub fn canonical_value(&self) -> Value {
        let mut policy = self.clone();
        for rule in &mut policy.rules {
            rule.matcher = rule.matcher.sorted();
        }
        serde_json::to_value(&policy).expect("a policy is plain JSON data")
    }

    /// The bytes that are hashed and embedded in a commit.
    pub fn canonical_bytes(&self) -> Vec<u8> {
        crate::log::canonical_json(&self.canonical_value())
    }

    /// `SHA-256(HASH_DOMAIN || canonical bytes)`.
    pub fn hash(&self) -> [u8; 32] {
        let mut context = ring::digest::Context::new(&ring::digest::SHA256);
        context.update(HASH_DOMAIN);
        context.update(&self.canonical_bytes());
        let mut out = [0u8; 32];
        out.copy_from_slice(context.finish().as_ref());
        out
    }

    pub fn hash_hex(&self) -> String {
        super::hex(&self.hash())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::merge_policy::{
        strategy::{Context, Inputs, ParamSpec, Resolved},
        Registry,
    };
    use serde_json::json;

    /// Declares one parameter of each type, so the loader's parameter rules can be
    /// exercised without inventing parameters on a real strategy.
    struct ParamProbe;
    impl Strategy for ParamProbe {
        fn id(&self) -> &'static str {
            "test.params.v1"
        }
        fn params(&self) -> &'static [ParamSpec] {
            &[
                ParamSpec {
                    name: "flag",
                    ty: ParamType::Bool,
                },
                ParamSpec {
                    name: "count",
                    ty: ParamType::Int { min: 1, max: 200 },
                },
                ParamSpec {
                    name: "mode",
                    ty: ParamType::OneOf(&["dedupe", "keep"]),
                },
            ]
        }
        fn resolve(&self, _: &Inputs<'_>, _: &Context<'_>) -> Resolved {
            unreachable!("the loader never runs a strategy")
        }
    }

    fn registry() -> Registry {
        let mut r = Registry::builtin();
        r.register(Box::new(ParamProbe)).unwrap();
        r
    }

    fn load(text: &str) -> Result<MergePolicy, PolicyError> {
        MergePolicy::from_json(text.as_bytes(), &registry())
    }

    fn rule(extra: &str) -> String {
        format!(
            r#"{{"id":"r","match":{{}},"strategy":"dynodoc.default.whole-value.v1","gate":"auto"{extra}}}"#
        )
    }

    fn policy(rules: &str) -> String {
        format!(r#"{{"version":1,"id":"p","rules":[{rules}]}}"#)
    }

    fn rejected(text: &str) -> PolicyError {
        load(text).expect_err("policy must be rejected")
    }

    #[test]
    fn a_minimal_policy_loads_with_every_schema_default_filled_in() {
        let p = load(&policy("")).unwrap();
        assert_eq!(p.defaults, Defaults::default());
        assert_eq!(p.defaults.unknown, Unknown::Manual);
        assert_eq!(p.defaults.loser, Loser::Suggestion);
        assert_eq!(p.defaults.invariants, InvariantMode::Flag);
        let canonical = String::from_utf8(p.canonical_bytes()).unwrap();
        assert_eq!(
            canonical,
            r#"{"defaults":{"invariants":"flag","loser":"suggestion","roles_rank":["owner","manager","editor","contributor","reviewer"],"unknown":"manual"},"floors":{"gate_at_least":{}},"id":"p","rules":[],"version":1}"#
        );
    }

    #[test]
    fn spelling_and_order_do_not_change_the_hash_but_content_does() {
        let a = load(
            r#"{"version":1,"id":"p","rules":[{"id":"r","match":{"type":["item","section"],"field":"x"},"strategy":"test.params.v1","params":{"count":5,"flag":true},"gate":"manual"}]}"#,
        )
        .unwrap();
        let b = load(
            r#"{ "rules":[{"gate":"manual","params":{"flag":true,"count":5},"strategy":"test.params.v1","match":{"field":"x","type":["section","item"]},"id":"r"}], "id":"p", "version":1,
                 "defaults":{"unknown":"manual","loser":"suggestion","invariants":"flag","roles_rank":["owner","manager","editor","contributor","reviewer"]},
                 "floors":{"gate_at_least":{}} }"#,
        )
        .unwrap();
        assert_eq!(a.canonical_bytes(), b.canonical_bytes());
        assert_eq!(a.hash(), b.hash());
        assert_eq!(a.hash_hex().len(), 64);
        let c = load(&policy(&rule(""))).unwrap();
        assert_ne!(a.hash(), c.hash());
        let mut gate = a.clone();
        gate.rules[0].gate = Gate::Blocked;
        assert_ne!(a.hash(), gate.hash());
    }

    #[test]
    fn the_hash_is_over_the_domain_and_the_canonical_bytes() {
        let p = load(&policy("")).unwrap();
        let mut input = HASH_DOMAIN.to_vec();
        input.extend(p.canonical_bytes());
        let expected = ring::digest::digest(&ring::digest::SHA256, &input);
        assert_eq!(p.hash().as_slice(), expected.as_ref());
    }

    #[test]
    fn syntax_and_shape_errors() {
        for text in [
            "",
            "not json",
            "[]",
            r#"{"version":1,"id":"p","rules":[]} x"#,
            r#"{"id":"p","rules":[]}"#,                       // no version
            r#"{"version":1,"rules":[]}"#,                    // no id
            r#"{"version":1,"id":"p"}"#,                      // no rules
            r#"{"version":1,"id":"p","rules":[],"extra":1}"#, // unknown key
            r#"{"version":1,"id":"p","rules":[],"rules":[]}"#, // duplicate key
            r#"{"version":1,"id":"p","rules":[],"description":null}"#, // null
            r#"{"version":"1","id":"p","rules":[]}"#,
            r#"{"version":1,"id":"p","rules":[],"defaults":{"unknown":"auto"}}"#,
            r#"{"version":1,"id":"p","rules":[],"defaults":{"loser":"x"}}"#,
            r#"{"version":1,"id":"p","rules":[],"defaults":{"extra":1}}"#,
            r#"{"version":1,"id":"p","rules":[],"floors":{"extra":1}}"#,
            r#"{"version":1,"id":"p","rules":[],"floors":{"gate_at_least":{"a":"auto","a":"manual"}}}"#,
        ] {
            let e = rejected(text);
            assert_eq!(e.kind, ErrorKind::Parse, "{text}: {e}");
        }
        assert!(rejected(&policy("{}")).message.contains("missing field"));
        assert!(rejected(r#"{"version":1,"id":"p","rules":[],"rules":[]}"#)
            .message
            .contains("duplicate field"));
        let dup = rejected(&policy(&rule(r#","params":{"count":1,"count":2}"#)));
        assert!(dup.message.contains("duplicate key `count`"), "{dup}");
    }

    #[test]
    fn rule_syntax_errors() {
        let base = |m: &str| policy(&rule("").replace(r#""match":{}"#, &format!(r#""match":{m}"#)));
        for m in [
            r#"{"family":"slideshow"}"#,
            r#"{"family":null}"#,
            r#"{"aspect":"tables"}"#,
            r#"{"type":"item"}"#,
            r#"{"unknown":1}"#,
        ] {
            assert_eq!(rejected(&base(m)).kind, ErrorKind::Parse, "{m}");
        }
        // A missing gate or strategy is not defaulted.
        for missing in ["gate", "strategy", "match", "id"] {
            let mut r: Value = serde_json::from_str(&rule("")).unwrap();
            r.as_object_mut().unwrap().remove(missing);
            assert_eq!(
                rejected(&policy(&r.to_string())).kind,
                ErrorKind::Parse,
                "{missing}"
            );
        }
    }

    #[test]
    fn version_must_be_one() {
        for v in ["0", "2", "4294967295"] {
            let e = rejected(&format!(r#"{{"version":{v},"id":"p","rules":[]}}"#));
            assert_eq!((e.kind, e.path.as_str()), (ErrorKind::Version, "version"));
        }
        assert_eq!(
            rejected(r#"{"version":-1,"id":"p","rules":[]}"#).kind,
            ErrorKind::Parse
        );
    }

    #[test]
    fn identifiers_are_closed() {
        for id in ["", "P", "a b", "-a", ".a", "a/b", &"a".repeat(65), "é"] {
            let e = rejected(&format!(r#"{{"version":1,"id":{},"rules":[]}}"#, json!(id)));
            assert_eq!((e.kind, e.path.as_str()), (ErrorKind::Id, "id"), "{id:?}");
        }
        assert!(load(&format!(
            r#"{{"version":1,"id":"{}","rules":[]}}"#,
            "a".repeat(64)
        ))
        .is_ok());
        assert!(load(r#"{"version":1,"id":"dynodoc.default.v1","rules":[]}"#).is_ok());
        let e = rejected(&policy(&rule("").replace(r#""id":"r""#, r#""id":"R!""#)));
        assert_eq!((e.kind, e.path.as_str()), (ErrorKind::Id, "rules[0].id"));
    }

    #[test]
    fn descriptions_are_short_and_plain() {
        for text in ["x".repeat(257), "line\nbreak".to_string()] {
            let doc = format!(
                r#"{{"version":1,"id":"p","description":{},"rules":[]}}"#,
                json!(text)
            );
            assert_eq!(rejected(&doc).kind, ErrorKind::Text);
        }
        assert!(load(&format!(
            r#"{{"version":1,"id":"p","description":"{}","rules":[]}}"#,
            "x".repeat(256)
        ))
        .is_ok());
        let on_rule = policy(&rule(&format!(r#","description":"{}""#, "x".repeat(257))));
        let e = rejected(&on_rule);
        assert_eq!(
            (e.kind, e.path.as_str()),
            (ErrorKind::Text, "rules[0].description")
        );
    }

    #[test]
    fn roles_must_be_a_nonempty_set() {
        for roles in ["[]", r#"["owner","owner"]"#] {
            let e = rejected(&format!(
                r#"{{"version":1,"id":"p","rules":[],"defaults":{{"roles_rank":{roles}}}}}"#
            ));
            assert_eq!(
                (e.kind, e.path.as_str()),
                (ErrorKind::Roles, "defaults.roles_rank")
            );
        }
        let e =
            rejected(r#"{"version":1,"id":"p","rules":[],"defaults":{"roles_rank":["admin"]}}"#);
        assert_eq!(e.kind, ErrorKind::Parse);
        let p = load(
            r#"{"version":1,"id":"p","rules":[],"defaults":{"roles_rank":["reviewer","owner"]}}"#,
        )
        .unwrap();
        // The order is the rank, so it is kept.
        assert_eq!(p.defaults.roles_rank, vec![Role::Reviewer, Role::Owner]);
    }

    #[test]
    fn at_most_sixty_four_rules_with_unique_ids() {
        let many = |n: usize| {
            (0..n)
                .map(|i| {
                    format!(
                        r#"{{"id":"r{i}","match":{{"field":"f{i}"}},"strategy":"dynodoc.default.whole-value.v1","gate":"auto"}}"#
                    )
                })
                .collect::<Vec<_>>()
                .join(",")
        };
        assert!(load(&policy(&many(64))).is_ok());
        assert_eq!(rejected(&policy(&many(65))).kind, ErrorKind::TooManyRules);
        let twice = format!(
            "{},{}",
            rule(r#""#),
            rule("").replace(r#""match":{}"#, r#""match":{"field":"x"}"#)
        );
        let e = rejected(&policy(&twice));
        assert_eq!(
            (e.kind, e.path.as_str()),
            (ErrorKind::DuplicateRuleId, "rules[1].id")
        );
    }

    #[test]
    fn match_types_and_fields_are_checked() {
        let with = |m: &str| policy(&rule("").replace(r#""match":{}"#, &format!(r#""match":{m}"#)));
        for m in [
            r#"{"type":[]}"#,
            r#"{"type":["item","item"]}"#,
            r#"{"type":["1x"]}"#,
            r#"{"type":["a-b"]}"#,
            r#"{"type":[""]}"#,
        ] {
            let e = rejected(&with(m));
            assert_eq!(
                (e.kind, e.path.as_str()),
                (ErrorKind::MatchType, "rules[0].match.type"),
                "{m}"
            );
        }
        let long: Vec<String> = (0..17).map(|i| format!("\"t{i}\"")).collect();
        assert_eq!(
            rejected(&with(&format!(r#"{{"type":[{}]}}"#, long.join(",")))).kind,
            ErrorKind::MatchType
        );
        for m in [
            r#"{"field":""}"#,
            r#"{"field":"a*b"}"#,
            r#"{"field":"a b"}"#,
            r#"{"field":"**"}"#,
        ] {
            let e = rejected(&with(m));
            assert_eq!(
                (e.kind, e.path.as_str()),
                (ErrorKind::MatchField, "rules[0].match.field"),
                "{m}"
            );
        }
        for m in [
            r#"{"field":"cell_*"}"#,
            r#"{"field":"$location"}"#,
            r#"{"field":"*"}"#,
            r#"{"type":["codeBlock"]}"#,
        ] {
            assert!(load(&with(m)).is_ok(), "{m}");
        }
    }

    #[test]
    fn strategies_must_exist_and_fit_what_the_rule_decides() {
        let e = rejected(&policy(
            &rule("").replace("dynodoc.default.whole-value.v1", "nope.v1"),
        ));
        assert_eq!(
            (e.kind, e.path.as_str()),
            (ErrorKind::UnknownStrategy, "rules[0].strategy")
        );
        // The grapheme text strategy is designed (N2) but not built.
        let e = rejected(&policy(&rule("").replace(
            "dynodoc.default.whole-value.v1",
            "dynodoc.text.diff3-grapheme.v1",
        )));
        assert_eq!(e.kind, ErrorKind::UnknownStrategy);
        // A field strategy cannot decide a collection and the reverse.
        let on_collection = rule("").replace(r#""match":{}"#, r#""match":{"aspect":"comments"}"#);
        let e = rejected(&policy(&on_collection));
        assert_eq!(
            (e.kind, e.path.as_str()),
            (ErrorKind::AspectMismatch, "rules[0].strategy")
        );
        let elementwise = rule("").replace(
            "dynodoc.default.whole-value.v1",
            "dynodoc.collection.element-wise.v1",
        );
        let e = rejected(&policy(&elementwise));
        assert_eq!(e.kind, ErrorKind::AspectMismatch);
        let wrong_aspect = elementwise.replace(r#""match":{}"#, r#""match":{"aspect":"marks"}"#);
        assert_eq!(
            rejected(&policy(&wrong_aspect)).kind,
            ErrorKind::AspectMismatch
        );
        let good = elementwise.replace(r#""match":{}"#, r#""match":{"aspect":"comments"}"#);
        assert!(load(&policy(&good)).is_ok());
    }

    #[test]
    fn parameters_are_closed_and_typed() {
        let with = |params: &str| {
            policy(
                &rule(&format!(r#","params":{params}"#))
                    .replace("dynodoc.default.whole-value.v1", "test.params.v1"),
            )
        };
        assert!(load(&with(r#"{"flag":false,"count":200,"mode":"keep"}"#)).is_ok());
        assert!(load(&with("{}")).is_ok());
        let e = rejected(&with(r#"{"nope":1}"#));
        assert_eq!(
            (e.kind, e.path.as_str()),
            (ErrorKind::UnknownParameter, "rules[0].params.nope")
        );
        for (params, kind) in [
            (r#"{"flag":"yes"}"#, ErrorKind::ParameterType),
            (r#"{"flag":1}"#, ErrorKind::ParameterType),
            (r#"{"count":"5"}"#, ErrorKind::ParameterType),
            (r#"{"count":1.5}"#, ErrorKind::ParameterType),
            (r#"{"count":1e2}"#, ErrorKind::ParameterType),
            (r#"{"count":true}"#, ErrorKind::ParameterType),
            (r#"{"count":0}"#, ErrorKind::ParameterRange),
            (r#"{"count":201}"#, ErrorKind::ParameterRange),
            (
                r#"{"count":99999999999999999999}"#,
                ErrorKind::ParameterType,
            ),
            (r#"{"mode":5}"#, ErrorKind::ParameterType),
            (r#"{"mode":"other"}"#, ErrorKind::ParameterRange),
            (r#"{"mode":{"a":1}}"#, ErrorKind::ParameterType),
        ] {
            assert_eq!(rejected(&with(params)).kind, kind, "{params}");
        }
        // Built-in strategies take no parameters at all.
        let e = rejected(&policy(&rule(r#","params":{"x":1}"#)));
        assert_eq!(e.kind, ErrorKind::UnknownParameter);
    }

    #[test]
    fn a_rule_that_can_never_run_is_rejected() {
        let a = rule("");
        let same = a.replace(r#""id":"r""#, r#""id":"s""#);
        // An identical match, whatever the type order.
        let e = rejected(&policy(&format!("{a},{same}")));
        assert_eq!(
            (e.kind, e.path.as_str()),
            (ErrorKind::Shadowed, "rules[1].match")
        );
        let t1 = rule("").replace(r#""match":{}"#, r#""match":{"type":["item","section"]}"#);
        let t2 = t1
            .replace(r#""id":"r""#, r#""id":"s""#)
            .replace(r#"["item","section"]"#, r#"["section","item"]"#);
        assert_eq!(
            rejected(&policy(&format!("{t1},{t2}"))).kind,
            ErrorKind::Shadowed
        );
        // Nothing runs after a rule that matches everything.
        let narrow = rule("")
            .replace(r#""id":"r""#, r#""id":"s""#)
            .replace(r#""match":{}"#, r#""match":{"field":"x"}"#);
        assert_eq!(
            rejected(&policy(&format!("{a},{narrow}"))).kind,
            ErrorKind::Shadowed
        );
        // The catch-all last is fine.
        assert!(load(&policy(&format!("{narrow},{a}"))).is_ok());
    }

    #[test]
    fn a_rule_cannot_be_weaker_than_the_floor() {
        let doc = |gate: &str| {
            format!(
                r#"{{"version":1,"id":"p","rules":[{}],"floors":{{"gate_at_least":{{"dynodoc.default.whole-value.v1":"manual"}}}}}}"#,
                rule("").replace(r#""auto""#, &format!("\"{gate}\""))
            )
        };
        for gate in ["auto", "auto-with-review"] {
            let e = rejected(&doc(gate));
            assert_eq!(
                (e.kind, e.path.as_str()),
                (ErrorKind::WeakensFloor, "rules[0].gate"),
                "{gate}"
            );
        }
        for gate in ["manual", "blocked"] {
            assert!(load(&doc(gate)).is_ok(), "{gate}");
        }
        let e = rejected(
            r#"{"version":1,"id":"p","rules":[],"floors":{"gate_at_least":{"nope.v1":"manual"}}}"#,
        );
        assert_eq!(e.kind, ErrorKind::UnknownFloorStrategy);
        // The effective gate is the strictest of rule and floor.
        let p = load(&doc("blocked")).unwrap();
        assert_eq!(p.effective_gate(&p.rules[0]), Gate::Blocked);
        let mut p = load(&policy(&rule(""))).unwrap();
        p.floors
            .gate_at_least
            .0
            .insert("dynodoc.default.whole-value.v1".into(), Gate::Manual);
        assert_eq!(p.effective_gate(&p.rules[0]), Gate::Manual);
    }

    #[test]
    fn size_limits() {
        assert_eq!(
            MergePolicy::from_json(&vec![b' '; MAX_INPUT_BYTES + 1], &registry())
                .unwrap_err()
                .kind,
            ErrorKind::InputTooLarge
        );
        // Under the input limit but over the canonical limit: long descriptions.
        let rules: Vec<String> = (0..64)
            .map(|i| {
                format!(
                    r#"{{"id":"r{i}","description":"{}","match":{{"field":"f{i}"}},"strategy":"dynodoc.default.whole-value.v1","gate":"auto"}}"#,
                    "d".repeat(256)
                )
            })
            .collect();
        let doc = policy(&rules.join(","));
        assert!(doc.len() < MAX_INPUT_BYTES);
        assert_eq!(rejected(&doc).kind, ErrorKind::CanonicalTooLarge);
    }

    #[test]
    fn rules_are_matched_top_to_bottom_on_the_decision() {
        let p = load(&policy(&format!(
            "{},{},{}",
            rule("").replace(
                r#""match":{}"#,
                r#""match":{"field":"content","type":["codeBlock"]}"#
            ),
            rule("").replace(r#""id":"r""#, r#""id":"s""#).replace(
                r#""match":{}"#,
                r#""match":{"field":"cell_*","family":"spreadsheet"}"#
            ),
            rule("").replace(r#""id":"r""#, r#""id":"t""#),
        )))
        .unwrap();
        let probe = |family, node_type, block_type, field, aspect| super::Probe {
            family,
            node_type,
            block_type,
            field,
            aspect,
        };
        let first = |pr: super::Probe| p.find_rule(&pr).map(|r| r.id.as_str());
        let d = Family::Document;
        assert_eq!(
            first(probe(d, "item", Some("codeBlock"), "content", None)),
            Some("r")
        );
        assert_eq!(
            first(probe(d, "item", Some("paragraph"), "content", None)),
            Some("t")
        );
        assert_eq!(
            first(probe(d, "codeBlock", None, "content", None)),
            Some("r")
        );
        assert_eq!(
            first(probe(Family::Spreadsheet, "item", None, "cell_12", None)),
            Some("s")
        );
        assert_eq!(first(probe(d, "item", None, "cell_12", None)), Some("t"));
        // A rule without an aspect never decides a collection.
        assert_eq!(
            first(probe(d, "", None, "$comments", Some(Aspect::Comments))),
            None
        );
    }
}
