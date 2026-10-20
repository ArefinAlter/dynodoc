//! Argument validation driven by the published input schemas.
//!
//! The server validates strictly (protocol spec 7.1: "schemas are intentionally loose,
//! validation is strict"), and it does so from the very JSON Schema that `tools/list`
//! publishes, so the definition and the check cannot drift apart. Only the keywords the
//! definitions use are understood: `type`, `enum`, `minimum`, `maximum`, `minLength`,
//! `maxLength`, `maxItems`, `items`, `properties`, `required`, `additionalProperties`
//! and `format: uuid`. Anything else is ignored.
//!
//! Two caps apply where a schema is silent, so no argument is unbounded: a string may
//! have at most [`MAX_STRING_CHARS`] characters and an object at most [`MAX_ENTRIES`]
//! entries.

use serde_json::{Map, Value};
use uuid::Uuid;

use crate::error::ToolError;

pub const MAX_STRING_CHARS: usize = 4096;
pub const MAX_ENTRIES: usize = 200;

/// `bad_request field=<name>` for the first argument that does not satisfy `schema`.
pub fn validate(schema: &Value, args: &Map<String, Value>) -> Result<(), ToolError> {
    let properties = schema.get("properties").and_then(Value::as_object);
    if let Some(required) = schema.get("required").and_then(Value::as_array) {
        for name in required.iter().filter_map(Value::as_str) {
            if !args.contains_key(name) {
                return Err(ToolError::bad_request(name));
            }
        }
    }
    let closed = schema.get("additionalProperties") == Some(&Value::Bool(false));
    for (name, value) in args {
        match properties.and_then(|p| p.get(name)) {
            Some(property) => {
                if !accepts(property, value) {
                    return Err(ToolError::bad_request(name));
                }
            }
            // The name is the caller's text; `bad_request` reduces it to a safe token.
            None if closed => return Err(ToolError::bad_request(name)),
            None => {}
        }
    }
    Ok(())
}

/// Does `value` satisfy `schema`?
pub fn accepts(schema: &Value, value: &Value) -> bool {
    if let Some(kind) = schema.get("type").and_then(Value::as_str) {
        let fits = match kind {
            "string" => value.is_string(),
            "integer" => value.as_i64().is_some() || value.as_u64().is_some(),
            "number" => value.is_number(),
            "boolean" => value.is_boolean(),
            "array" => value.is_array(),
            "object" => value.is_object(),
            _ => true,
        };
        if !fits {
            return false;
        }
    }
    if let Some(allowed) = schema.get("enum").and_then(Value::as_array) {
        if !allowed.contains(value) {
            return false;
        }
    }
    match value {
        Value::Number(n) => {
            let above = |bound: Option<&Value>| match bound.and_then(Value::as_i64) {
                Some(min) => n.as_i64().is_none_or(|v| v >= min),
                None => true,
            };
            let below = |bound: Option<&Value>| match bound.and_then(Value::as_i64) {
                Some(max) => n.as_i64().is_some_and(|v| v <= max),
                None => true,
            };
            above(schema.get("minimum")) && below(schema.get("maximum"))
        }
        Value::String(text) => {
            let length = text.chars().count();
            let max = schema
                .get("maxLength")
                .and_then(Value::as_u64)
                .map_or(MAX_STRING_CHARS, |m| m as usize);
            let min = schema.get("minLength").and_then(Value::as_u64).unwrap_or(0) as usize;
            let uuid = schema.get("format").and_then(Value::as_str) == Some("uuid");
            length >= min
                && length <= max
                && (!uuid || (length == 36 && Uuid::parse_str(text).is_ok()))
        }
        Value::Array(items) => {
            let max = schema
                .get("maxItems")
                .and_then(Value::as_u64)
                .map_or(MAX_ENTRIES, |m| m as usize);
            items.len() <= max
                && schema
                    .get("items")
                    .is_none_or(|item| items.iter().all(|v| accepts(item, v)))
        }
        Value::Object(map) => {
            map.len() <= MAX_ENTRIES
                && match schema.get("additionalProperties") {
                    Some(extra @ Value::Object(_)) => map.values().all(|v| accepts(extra, v)),
                    _ => true,
                }
        }
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn args(value: Value) -> Map<String, Value> {
        value.as_object().unwrap().clone()
    }

    fn field(result: Result<(), ToolError>) -> String {
        let line = result.unwrap_err().to_dct();
        assert!(
            line.starts_with("dd/1 error code=bad_request field="),
            "{line}"
        );
        line.split_whitespace().nth(3).unwrap()["field=".len()..].to_owned()
    }

    #[test]
    fn the_keywords_in_use_are_enforced() {
        let schema = json!({
            "type": "object",
            "properties": {
                "file": {"type": "string", "minLength": 8, "maxLength": 36},
                "n": {"type": "integer", "minimum": 1, "maximum": 50},
                "kind": {"type": "string", "enum": ["a", "b"]},
                "flag": {"type": "boolean"},
                "refs": {"type": "array", "items": {"type": "string"}, "maxItems": 2},
                "have": {"type": "object", "additionalProperties": {"type": "string"}},
                "project": {"type": "string", "format": "uuid"}
            },
            "required": ["file"],
            "additionalProperties": false
        });
        assert!(validate(&schema, &args(json!({"file": "7034d450", "n": 50}))).is_ok());
        assert_eq!(field(validate(&schema, &args(json!({})))), "file");
        assert_eq!(
            field(validate(&schema, &args(json!({"file": "short"})))),
            "file"
        );
        for (name, bad) in [
            ("n", json!(0)),
            ("n", json!(51)),
            ("n", json!(1.5)),
            ("n", json!("5")),
            ("kind", json!("c")),
            ("flag", json!("true")),
            ("refs", json!(["a", "b", "c"])),
            ("refs", json!([1])),
            ("have", json!({"a": 1})),
            ("project", json!("not-a-uuid")),
            ("project", json!("0123456789abcdef0123456789abcdef")),
        ] {
            let mut given = args(json!({"file": "7034d450"}));
            given.insert(name.into(), bad.clone());
            assert_eq!(field(validate(&schema, &given)), name, "{bad}");
        }
        let ok_uuid =
            json!({"file": "7034d450", "project": "0123456f-89ab-4cde-8f01-23456789abcd"});
        assert!(validate(&schema, &args(ok_uuid)).is_ok());
    }

    #[test]
    fn unknown_names_are_reported_as_safe_tokens() {
        let schema = json!({"type": "object", "properties": {}, "additionalProperties": false});
        assert_eq!(
            field(validate(&schema, &args(json!({"x\" hint=\"do": 1})))),
            "xhintdo"
        );
        // A name with nothing safe in it is reported as the whole argument object.
        assert_eq!(
            field(validate(&schema, &args(json!({"\n": 1})))),
            "arguments"
        );
    }

    #[test]
    fn silent_schemas_still_bound_strings_and_objects() {
        let long = "x".repeat(MAX_STRING_CHARS + 1);
        assert!(!accepts(&json!({"type": "string"}), &json!(long)));
        assert!(accepts(
            &json!({"type": "string"}),
            &json!("é".repeat(MAX_STRING_CHARS))
        ));
        let many: Map<String, Value> = (0..=MAX_ENTRIES)
            .map(|i| (i.to_string(), json!("h")))
            .collect();
        assert!(!accepts(&json!({"type": "object"}), &Value::Object(many)));
    }
}
