//! Tool responses: the DCT text, the `_meta["dynodoc/v1"]` object and the typed data.

use std::collections::BTreeMap;

use serde::Serialize;

use crate::dct::{estimate_tokens, HeaderBuilder};
use crate::escape::Dropped;
use crate::model::FileMeta;

/// `warn=` codes (spec 6.2, plus `ctrl` and `hidden` from 6.5 and AI-AGENT-SECURITY P3).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Warnings {
    pub stale: bool,
    pub low_budget: bool,
    pub truncated: bool,
    pub redacted: bool,
    pub dropped: Dropped,
}

impl Warnings {
    pub fn codes(&self) -> Vec<&'static str> {
        let mut out = Vec::new();
        if self.stale {
            out.push("stale");
        }
        if self.low_budget {
            out.push("low_budget");
        }
        if self.truncated {
            out.push("truncated");
        }
        if self.redacted {
            out.push("redacted");
        }
        if self.dropped.ctrl > 0 {
            out.push("ctrl");
        }
        if self.dropped.hidden > 0 {
            out.push("hidden");
        }
        out
    }

    /// Append `warn=a,b` to a header when there is anything to say.
    pub fn apply(&self, h: HeaderBuilder) -> HeaderBuilder {
        let codes = self.codes();
        if codes.is_empty() {
            h
        } else {
            h.kv("warn", codes.join(","))
        }
    }
}

/// `_meta["dynodoc/v1"]` for programmatic clients: what the text omits.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Meta {
    /// Full document UUID (`None` for `files`, which spans files).
    pub file: Option<String>,
    pub rev: Option<u64>,
    /// Full chain hash (hex) at `rev`.
    pub chain: Option<String>,
    pub cursor: Option<String>,
    pub budget: Budget,
    /// Ref to full node id, for every ref in the text.
    pub ids: BTreeMap<String, String>,
    pub warn: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Budget {
    pub max_bytes: usize,
    pub bytes: usize,
    /// `ceil(bytes / 4)`: an estimate, never a token count.
    pub est_tokens: u64,
}

impl Meta {
    pub fn new(
        file: &FileMeta,
        text_len: usize,
        max_bytes: usize,
        cursor: Option<String>,
        ids: BTreeMap<String, String>,
        warnings: &Warnings,
    ) -> Self {
        let mut m = Meta::list(text_len, max_bytes, cursor, ids, warnings);
        m.file = Some(file.id.0.to_string());
        m.rev = Some(file.rev);
        m.chain = Some(file.chain.clone());
        m
    }

    /// Metadata of a response that is not about one file.
    pub fn list(
        text_len: usize,
        max_bytes: usize,
        cursor: Option<String>,
        ids: BTreeMap<String, String>,
        warnings: &Warnings,
    ) -> Self {
        Meta {
            file: None,
            rev: None,
            chain: None,
            cursor,
            budget: Budget {
                max_bytes,
                bytes: text_len,
                est_tokens: estimate_tokens(text_len),
            },
            ids,
            warn: warnings.codes().into_iter().map(String::from).collect(),
        }
    }
}

/// A tool result: typed data, the DCT body that goes to the model and the metadata that
/// does not.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Response<T> {
    pub text: String,
    pub meta: Meta,
    pub data: T,
}

impl<T: Serialize> Response<T> {
    /// The JSON mirror of spec 12.3: the same information as the DCT text, as compact JSON,
    /// for programmatic callers and for the size comparison of the byte benchmark.
    ///
    /// `{"protocol":"dynodoc.agent/1","tool":..,"file":<uuid>,"rev":..,"chain":<8 hex>,
    /// "trust":"data", ..typed fields..}`. The typed fields use the short keys of the spec
    /// (`ref`, `t`, `h`, `w`, `p`).
    pub fn json_mirror(&self, tool: crate::dct::Tool) -> String {
        let mut map = serde_json::Map::new();
        map.insert("protocol".into(), "dynodoc.agent/1".into());
        map.insert("tool".into(), tool.as_str().into());
        if let Some(f) = &self.meta.file {
            map.insert("file".into(), f.clone().into());
        }
        if let Some(r) = self.meta.rev {
            map.insert("rev".into(), r.into());
        }
        if let Some(c) = &self.meta.chain {
            map.insert("chain".into(), c.chars().take(8).collect::<String>().into());
        }
        map.insert("trust".into(), "data".into());
        if let Ok(serde_json::Value::Object(fields)) = serde_json::to_value(&self.data) {
            map.extend(fields);
        }
        serde_json::to_string(&serde_json::Value::Object(map)).expect("JSON mirror serialises")
    }
}
