//! Leaf encodings and blinded tags (design 4.1).
//!
//! A leaf's data is canonical JSON (sorted keys, no whitespace, UTF-8, 1 to 256
//! bytes). The log's entry is `"dynodoc.transparency-leaf\0v1\0" || data` and the
//! Merkle leaf hash is the RFC 6962 leaf hash of that entry, i.e.
//! `SHA-256(0x00 || domain || data)` as the design states. Storing the full entry
//! (domain included) in entry bundles lets generic tlog tooling recompute leaf
//! hashes from bundles.
//!
//! Tags are `HMAC-SHA256(key, message)`, hex encoded. The messages are the design's
//! `"ref\0" || branch || "\0" || generation_u64be`, `"rot"`,
//! `"hzn\0" || seq_u64be` and `"ver\0" || doc_uuid`. Where the design leaves the
//! encoding open, this module fixes it: `seq` is a big-endian `u64` like
//! `generation`, and `doc_uuid` is the 16 raw UUID bytes.
use ring::hmac;
use serde_json::{json, Map, Value};
use uuid::Uuid;

use super::encoding::{hex, unhex_array};
use super::merkle::{leaf_hash, Hash};
use super::note::Origin;
use super::Error;

/// Domain prefix of every leaf entry.
pub const LEAF_DOMAIN: &[u8] = b"dynodoc.transparency-leaf\0v1\0";
/// Longest allowed leaf data.
pub const MAX_LEAF_BYTES: usize = 256;

/// A keyed-hash tag that only holders of the key can link to a scope.
pub type Tag = [u8; 32];

/// `k_project` or `k_installation`: a 256-bit HMAC key. Never printed.
#[derive(Clone, PartialEq, Eq)]
pub struct TagKey([u8; 32]);

impl std::fmt::Debug for TagKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("TagKey(..)")
    }
}

impl TagKey {
    pub fn new(key: [u8; 32]) -> Self {
        Self(key)
    }

    fn mac(&self, parts: &[&[u8]]) -> Tag {
        let key = hmac::Key::new(hmac::HMAC_SHA256, &self.0);
        let mut context = hmac::Context::with_key(&key);
        for part in parts {
            context.update(part);
        }
        context.sign().as_ref().try_into().expect("32 bytes")
    }

    /// `ref-receipt` tag, keyed with `k_project`.
    pub fn ref_tag(&self, branch: &str, generation: u64) -> Tag {
        self.mac(&[
            b"ref\0",
            branch.as_bytes(),
            b"\0",
            &generation.to_be_bytes(),
        ])
    }

    /// `authority-rotation` tag, keyed with `k_installation`.
    pub fn rotation_tag(&self) -> Tag {
        self.mac(&[b"rot"])
    }

    /// `history-horizon` tag, keyed with `k_project`.
    pub fn horizon_tag(&self, seq: u64) -> Tag {
        self.mac(&[b"hzn\0", &seq.to_be_bytes()])
    }

    /// `named-version` tag, keyed with `k_project`.
    pub fn version_tag(&self, document: &Uuid) -> Tag {
        self.mac(&[b"ver\0", document.as_bytes()])
    }
}

/// One log entry's meaning. No variant holds a project, branch, document, actor,
/// commit or path identifier: only keyed tags, addresses and roots.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Leaf {
    /// `{"t":"ref","tag":T,"receipt":ADDR}`
    RefReceipt { tag: Tag, receipt: Hash },
    /// `{"t":"rot","tag":T,"rotation":ADDR}`
    AuthorityRotation { tag: Tag, rotation: Hash },
    /// `{"t":"hzn","tag":T,"horizon":ADDR,"seq":N}`
    HistoryHorizon { tag: Tag, horizon: Hash, seq: u64 },
    /// `{"t":"evh","root":R,"count":N,"interval":I}`
    EventHeads {
        root: Hash,
        count: u64,
        interval: u64,
    },
    /// `{"t":"ver","tag":T,"version":ADDR}`
    NamedVersion { tag: Tag, version: Hash },
    /// `{"t":"suc","prev_origin":O,"size":N,"root":R}`
    LogSuccessor {
        prev_origin: Origin,
        size: u64,
        root: Hash,
    },
}

impl Leaf {
    pub(crate) fn to_value(&self) -> Value {
        match self {
            Self::RefReceipt { tag, receipt } => {
                json!({"t": "ref", "tag": hex(tag), "receipt": hex(receipt)})
            }
            Self::AuthorityRotation { tag, rotation } => {
                json!({"t": "rot", "tag": hex(tag), "rotation": hex(rotation)})
            }
            Self::HistoryHorizon { tag, horizon, seq } => {
                json!({"t": "hzn", "tag": hex(tag), "horizon": hex(horizon), "seq": seq})
            }
            Self::EventHeads {
                root,
                count,
                interval,
            } => json!({"t": "evh", "root": hex(root), "count": count, "interval": interval}),
            Self::NamedVersion { tag, version } => {
                json!({"t": "ver", "tag": hex(tag), "version": hex(version)})
            }
            Self::LogSuccessor {
                prev_origin,
                size,
                root,
            } => json!({
                "t": "suc",
                "prev_origin": prev_origin.as_str(),
                "size": size,
                "root": hex(root),
            }),
        }
    }

    /// Canonical JSON data of the leaf.
    pub fn data(&self) -> Result<Vec<u8>, Error> {
        let data = crate::log::canonical_json(&self.to_value());
        if data.len() > MAX_LEAF_BYTES {
            return Err(Error::InvalidLeaf("data is longer than 256 bytes"));
        }
        Ok(data)
    }

    /// The log entry: domain prefix then data.
    pub fn entry(&self) -> Result<Vec<u8>, Error> {
        let mut entry = LEAF_DOMAIN.to_vec();
        entry.extend(self.data()?);
        Ok(entry)
    }

    /// The Merkle leaf hash `SHA-256(0x00 || domain || data)`.
    pub fn hash(&self) -> Result<Hash, Error> {
        Ok(leaf_hash(&self.entry()?))
    }

    /// Decode a log entry (domain prefix then data).
    pub fn from_entry(entry: &[u8]) -> Result<Self, Error> {
        let data = entry
            .strip_prefix(LEAF_DOMAIN)
            .ok_or(Error::InvalidLeaf("missing the leaf domain prefix"))?;
        Self::parse(data)
    }

    /// Decode canonical leaf data. Anything that is not exactly the canonical
    /// encoding of a known leaf is rejected: whitespace, unsorted or duplicate
    /// keys, unknown or missing fields, uppercase hex, non-integer numbers.
    pub fn parse(data: &[u8]) -> Result<Self, Error> {
        if data.is_empty() || data.len() > MAX_LEAF_BYTES {
            return Err(Error::InvalidLeaf("data must be 1 to 256 bytes"));
        }
        let value: Value =
            serde_json::from_slice(data).map_err(|_| Error::InvalidLeaf("not valid JSON"))?;
        let leaf = Self::from_value(&value)?;
        if crate::log::canonical_json(&leaf.to_value()) != data {
            return Err(Error::InvalidLeaf("not canonical JSON"));
        }
        Ok(leaf)
    }

    pub(crate) fn from_value(value: &Value) -> Result<Self, Error> {
        let object = value
            .as_object()
            .ok_or(Error::InvalidLeaf("a leaf is a JSON object"))?;
        let kind = object
            .get("t")
            .and_then(Value::as_str)
            .ok_or(Error::InvalidLeaf("missing leaf type"))?;
        let fields = |names: &[&str]| -> Result<(), Error> {
            if object.len() == names.len() && names.iter().all(|n| object.contains_key(*n)) {
                Ok(())
            } else {
                Err(Error::InvalidLeaf("unexpected or missing fields"))
            }
        };
        let digest = |name: &str| field_hex(object, name);
        let number = |name: &str| {
            object
                .get(name)
                .and_then(Value::as_u64)
                .ok_or(Error::InvalidLeaf("expected an unsigned integer"))
        };
        match kind {
            "ref" => {
                fields(&["t", "tag", "receipt"])?;
                Ok(Self::RefReceipt {
                    tag: digest("tag")?,
                    receipt: digest("receipt")?,
                })
            }
            "rot" => {
                fields(&["t", "tag", "rotation"])?;
                Ok(Self::AuthorityRotation {
                    tag: digest("tag")?,
                    rotation: digest("rotation")?,
                })
            }
            "hzn" => {
                fields(&["t", "tag", "horizon", "seq"])?;
                Ok(Self::HistoryHorizon {
                    tag: digest("tag")?,
                    horizon: digest("horizon")?,
                    seq: number("seq")?,
                })
            }
            "evh" => {
                fields(&["t", "root", "count", "interval"])?;
                Ok(Self::EventHeads {
                    root: digest("root")?,
                    count: number("count")?,
                    interval: number("interval")?,
                })
            }
            "ver" => {
                fields(&["t", "tag", "version"])?;
                Ok(Self::NamedVersion {
                    tag: digest("tag")?,
                    version: digest("version")?,
                })
            }
            "suc" => {
                fields(&["t", "prev_origin", "size", "root"])?;
                let origin = object
                    .get("prev_origin")
                    .and_then(Value::as_str)
                    .ok_or(Error::InvalidLeaf("prev_origin is a string"))?;
                Ok(Self::LogSuccessor {
                    prev_origin: Origin::new(origin)?,
                    size: number("size")?,
                    root: digest("root")?,
                })
            }
            _ => Err(Error::InvalidLeaf("unknown leaf type")),
        }
    }
}

fn field_hex(object: &Map<String, Value>, name: &str) -> Result<[u8; 32], Error> {
    let text = object
        .get(name)
        .and_then(Value::as_str)
        .ok_or(Error::InvalidLeaf("expected a hex string"))?;
    unhex_array(text).map_err(|_| Error::InvalidLeaf("expected 64 lowercase hex digits"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn h(byte: u8) -> Hash {
        [byte; 32]
    }

    fn samples() -> Vec<Leaf> {
        vec![
            Leaf::RefReceipt {
                tag: h(1),
                receipt: h(2),
            },
            Leaf::AuthorityRotation {
                tag: h(3),
                rotation: h(4),
            },
            Leaf::HistoryHorizon {
                tag: h(5),
                horizon: h(6),
                seq: 7,
            },
            Leaf::EventHeads {
                root: h(8),
                count: 1234,
                interval: u64::MAX,
            },
            Leaf::NamedVersion {
                tag: h(9),
                version: h(10),
            },
            Leaf::LogSuccessor {
                prev_origin: Origin::new("old.example/dynodoc/v1/transparency").unwrap(),
                size: 99,
                root: h(11),
            },
        ]
    }

    #[test]
    fn canonical_encoding_matches_the_design() {
        let leaf = Leaf::RefReceipt {
            tag: h(0xaa),
            receipt: h(0xbb),
        };
        assert_eq!(
            String::from_utf8(leaf.data().unwrap()).unwrap(),
            format!(
                r#"{{"receipt":"{}","t":"ref","tag":"{}"}}"#,
                "bb".repeat(32),
                "aa".repeat(32)
            )
        );
        let entry = leaf.entry().unwrap();
        assert!(entry.starts_with(b"dynodoc.transparency-leaf\0v1\0{"));
        assert_eq!(leaf.hash().unwrap(), leaf_hash(&entry));
    }

    #[test]
    fn every_leaf_type_round_trips_within_the_size_limit() {
        for leaf in samples() {
            let data = leaf.data().unwrap();
            assert!((1..=MAX_LEAF_BYTES).contains(&data.len()), "{leaf:?}");
            assert_eq!(Leaf::parse(&data).unwrap(), leaf);
            assert_eq!(Leaf::from_entry(&leaf.entry().unwrap()).unwrap(), leaf);
        }
    }

    #[test]
    fn leaves_hold_only_tags_addresses_and_roots() {
        // No identifier-like field: every string value is hex or a fixed type name
        // (or the previous origin of a successor), every other value an integer.
        for leaf in samples() {
            let value = leaf.to_value();
            for (key, v) in value.as_object().unwrap() {
                match v {
                    Value::String(s) if key == "t" => assert_eq!(s.len(), 3),
                    Value::String(s) if key == "prev_origin" => assert!(s.contains('/')),
                    Value::String(s) => assert!(unhex_array::<32>(s).is_ok(), "{key}"),
                    Value::Number(_) => {}
                    other => panic!("unexpected {other:?}"),
                }
            }
        }
    }

    #[test]
    fn non_canonical_or_malformed_data_is_rejected() {
        let good = samples()[0].data().unwrap();
        let text = String::from_utf8(good.clone()).unwrap();
        let tag_hex = "ab".repeat(32);
        let receipt_hex = "02".repeat(32);
        let bad: Vec<(Vec<u8>, &str)> = vec![
            (Vec::new(), "empty"),
            (vec![b' '; 257], "too long"),
            (text.replace(',', ", ").into_bytes(), "whitespace"),
            (text.replacen('{', "{ ", 1).into_bytes(), "leading whitespace"),
            (
                format!(r#"{{"t":"ref","tag":"{tag_hex}","receipt":"{receipt_hex}"}}"#).into_bytes(),
                "unsorted keys",
            ),
            (
                format!(r#"{{"receipt":"{receipt_hex}","t":"ref","tag":"{tag_hex}","x":1}}"#)
                    .into_bytes(),
                "extra field",
            ),
            (
                format!(r#"{{"receipt":"{receipt_hex}","t":"ref"}}"#).into_bytes(),
                "missing field",
            ),
            (
                format!(
                    r#"{{"receipt":"{receipt_hex}","t":"ref","tag":"{}"}}"#,
                    tag_hex.to_uppercase()
                )
                .into_bytes(),
                "uppercase hex",
            ),
            (
                format!(r#"{{"receipt":"{receipt_hex}","t":"ref","tag":"0102"}}"#).into_bytes(),
                "short hex",
            ),
            (
                format!(
                    r#"{{"receipt":"{receipt_hex}","receipt":"{receipt_hex}","t":"ref","tag":"{tag_hex}"}}"#
                )
                .into_bytes(),
                "duplicate key",
            ),
            (
                format!(r#"{{"receipt":"{receipt_hex}","t":"nope","tag":"{tag_hex}"}}"#).into_bytes(),
                "unknown type",
            ),
            (
                format!(r#"{{"count":1.0,"interval":1,"root":"{tag_hex}","t":"evh"}}"#).into_bytes(),
                "float",
            ),
            (
                format!(
                    r#"{{"count":-1,"interval":1,"root":"{tag_hex}","t":"evh"}}"#
                )
                .into_bytes(),
                "negative",
            ),
            (
                format!(
                    r#"{{"count":1,"interval":18446744073709551616,"root":"{tag_hex}","t":"evh"}}"#
                )
                .into_bytes(),
                "overflow",
            ),
            (b"[]".to_vec(), "not an object"),
            (b"null".to_vec(), "null"),
            (b"\xef\xbb\xbf{}".to_vec(), "byte order mark"),
        ];
        for (data, why) in bad {
            assert!(
                matches!(Leaf::parse(&data), Err(Error::InvalidLeaf(_))),
                "{why}: {:?}",
                Leaf::parse(&data)
            );
        }
        assert!(Leaf::parse(&good).is_ok());
        assert!(matches!(
            Leaf::from_entry(&good),
            Err(Error::InvalidLeaf("missing the leaf domain prefix"))
        ));
    }

    #[test]
    fn successor_origin_must_be_a_valid_origin() {
        let data = format!(
            r#"{{"prev_origin":"https://x/dynodoc/v1/transparency","root":"{}","size":1,"t":"suc"}}"#,
            "00".repeat(32)
        );
        assert!(matches!(
            Leaf::parse(data.as_bytes()),
            Err(Error::InvalidOrigin(_))
        ));
    }

    #[test]
    fn oversized_successor_is_refused_at_encoding() {
        let host = "h".repeat(140);
        let leaf = Leaf::LogSuccessor {
            prev_origin: Origin::new(&format!("{host}/dynodoc/v1/transparency")).unwrap(),
            size: 1,
            root: h(1),
        };
        assert!(matches!(leaf.data(), Err(Error::InvalidLeaf(_))));
    }

    #[test]
    fn tags_are_keyed_and_domain_separated() {
        let k_project = TagKey::new([0x11; 32]);
        let other = TagKey::new([0x12; 32]);
        let a = k_project.ref_tag("main", 1);
        assert_eq!(a, k_project.ref_tag("main", 1));
        assert_ne!(a, k_project.ref_tag("main", 2));
        assert_ne!(a, k_project.ref_tag("draft", 1));
        assert_ne!(a, other.ref_tag("main", 1));
        let doc = Uuid::from_u128(5);
        let all = [
            a,
            k_project.rotation_tag(),
            k_project.horizon_tag(1),
            k_project.version_tag(&doc),
        ];
        for (i, x) in all.iter().enumerate() {
            for y in &all[i + 1..] {
                assert_ne!(x, y);
            }
        }
        assert_eq!(format!("{k_project:?}"), "TagKey(..)");
    }

    #[test]
    fn tag_matches_a_direct_hmac() {
        let key = [0x42u8; 32];
        let mut message = b"ref\0main\0".to_vec();
        message.extend_from_slice(&7u64.to_be_bytes());
        let expected = hmac::sign(&hmac::Key::new(hmac::HMAC_SHA256, &key), &message);
        assert_eq!(TagKey::new(key).ref_tag("main", 7), expected.as_ref());
    }
}
