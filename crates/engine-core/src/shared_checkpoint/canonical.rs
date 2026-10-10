//! Borrowed v1 serialization. Field order below is the existing canonical byte
//! contract, not the declaration order of the materializer's structs.
use std::io::{self, Write};

use engine_shared::NodeId;
use serde::ser::{SerializeMap, SerializeSeq};
use serde::{Serialize, Serializer};
use serde_json::Value;

use super::{Address, Entry, Error, Object, StateRoots, MAX_OBJECT_BYTES};
use crate::materializer::{CommentState, MaterializedNode, SuggestionState};

pub(super) enum ObjectRef<'a> {
    Node(&'a MaterializedNode),
    Comment(&'a CommentState),
    Suggestion(&'a SuggestionState),
    RemovedChoice(&'a NodeId),
    MapLeaf(&'a [Entry]),
    MapBranch(&'a std::collections::BTreeMap<String, Address>),
    State(&'a StateRoots),
}

impl<'a> From<&'a Object> for ObjectRef<'a> {
    fn from(value: &'a Object) -> Self {
        match value {
            Object::Node(v) => Self::Node(v),
            Object::Comment(v) => Self::Comment(v),
            Object::Suggestion(v) => Self::Suggestion(v),
            Object::RemovedChoice(v) => Self::RemovedChoice(v),
            Object::MapLeaf(v) => Self::MapLeaf(v),
            Object::MapBranch(v) => Self::MapBranch(v),
            Object::State(v) => Self::State(v),
        }
    }
}

pub(crate) struct CanonicalValue<'a>(pub(crate) &'a Value);
impl Serialize for CanonicalValue<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self.0 {
            Value::Object(values) => {
                // Also correct if another dependency enables preserve_order.
                // Only key references are sorted; values and keys are not cloned.
                let mut keys: Vec<_> = values.keys().collect();
                keys.sort_unstable();
                let mut map = serializer.serialize_map(Some(keys.len()))?;
                for key in keys {
                    map.serialize_entry(key, &CanonicalValue(&values[key]))?;
                }
                map.end()
            }
            Value::Array(values) => {
                let mut seq = serializer.serialize_seq(Some(values.len()))?;
                for value in values {
                    seq.serialize_element(&CanonicalValue(value))?;
                }
                seq.end()
            }
            value => value.serialize(serializer),
        }
    }
}

#[derive(Serialize)]
struct NodeView<'a> {
    current_fields: CanonicalValue<'a>,
    deleted: bool,
    id: &'a NodeId,
    parent_id: &'a Option<NodeId>,
    pos: &'a str,
    #[serde(rename = "type")]
    node_type: &'a str,
    var_name: &'a Option<String>,
}
#[derive(Serialize)]
struct CommentView<'a> {
    body: &'a str,
    node_id: &'a Option<NodeId>,
}
#[derive(Serialize)]
struct SuggestionView<'a> {
    accepted: bool,
    detail: CanonicalValue<'a>,
    rejected_reason: &'a Option<String>,
    target_node_id: &'a Option<NodeId>,
}
#[derive(Serialize)]
struct StateView<'a> {
    comments: &'a Address,
    nodes: &'a Address,
    removed_choices: &'a Address,
    suggestions: &'a Address,
}
struct EntriesView<'a>(&'a [Entry]);
impl Serialize for EntriesView<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        #[derive(Serialize)]
        struct EntryView<'a> {
            address: &'a Address,
            key: &'a str,
        }
        let mut seq = serializer.serialize_seq(Some(self.0.len()))?;
        for entry in self.0 {
            seq.serialize_element(&EntryView {
                address: &entry.address,
                key: &entry.key,
            })?;
        }
        seq.end()
    }
}

impl Serialize for ObjectRef<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(2))?;
        match self {
            Self::Node(v) => {
                map.serialize_entry("kind", "node")?;
                map.serialize_entry(
                    "value",
                    &NodeView {
                        current_fields: CanonicalValue(&v.current_fields),
                        deleted: v.deleted,
                        id: &v.id,
                        parent_id: &v.parent_id,
                        pos: &v.pos,
                        node_type: &v.node_type,
                        var_name: &v.var_name,
                    },
                )?;
            }
            Self::Comment(v) => {
                map.serialize_entry("kind", "comment")?;
                map.serialize_entry(
                    "value",
                    &CommentView {
                        body: &v.body,
                        node_id: &v.node_id,
                    },
                )?;
            }
            Self::Suggestion(v) => {
                map.serialize_entry("kind", "suggestion")?;
                map.serialize_entry(
                    "value",
                    &SuggestionView {
                        accepted: v.accepted,
                        detail: CanonicalValue(&v.detail),
                        rejected_reason: &v.rejected_reason,
                        target_node_id: &v.target_node_id,
                    },
                )?;
            }
            Self::RemovedChoice(v) => {
                map.serialize_entry("kind", "removed_choice")?;
                map.serialize_entry("value", v)?;
            }
            Self::MapLeaf(v) => {
                map.serialize_entry("kind", "map_leaf")?;
                map.serialize_entry("value", &EntriesView(v))?;
            }
            Self::MapBranch(v) => {
                map.serialize_entry("kind", "map_branch")?;
                map.serialize_entry("value", v)?;
            }
            Self::State(v) => {
                map.serialize_entry("kind", "state")?;
                map.serialize_entry(
                    "value",
                    &StateView {
                        comments: &v.comments,
                        nodes: &v.nodes,
                        removed_choices: &v.removed_choices,
                        suggestions: &v.suggestions,
                    },
                )?;
            }
        }
        map.end()
    }
}

#[derive(Serialize)]
struct EnvelopeView<'a> {
    object: ObjectRef<'a>,
    version: u8,
}

struct CappedBuffer {
    bytes: Vec<u8>,
    limit: usize,
    exceeded: bool,
}
impl Write for CappedBuffer {
    fn write(&mut self, input: &[u8]) -> io::Result<usize> {
        if input.len() > self.limit - self.bytes.len() {
            self.exceeded = true;
            return Err(io::Error::other("checkpoint encoding capacity"));
        }
        let needed = self.bytes.len() + input.len();
        if needed > self.bytes.capacity() {
            let capacity = needed
                .max(self.bytes.capacity().saturating_mul(2))
                .min(self.limit);
            self.bytes.reserve_exact(capacity - self.bytes.len());
        }
        self.bytes.extend_from_slice(input);
        Ok(input.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

pub(super) fn encode(
    object: ObjectRef<'_>,
    remaining_bytes: usize,
) -> Result<(Address, Vec<u8>), Error> {
    let limit = remaining_bytes.min(MAX_OBJECT_BYTES);
    let mut output = CappedBuffer {
        bytes: Vec::with_capacity(limit.min(4096)),
        limit,
        exceeded: false,
    };
    let result = serde_json::to_writer(&mut output, &EnvelopeView { object, version: 1 });
    if output.exceeded {
        return Err(Error::Limit(if remaining_bytes < MAX_OBJECT_BYTES {
            "bytes"
        } else {
            "object bytes"
        }));
    }
    result?;
    Ok((super::address(&output.bytes), output.bytes))
}

/// Compare canonical output with stored bytes as it is emitted. This avoids a
/// second encoded buffer and a clone of the decoded object during validation.
pub(super) fn matches(object: ObjectRef<'_>, bytes: &[u8]) -> Result<(), Error> {
    struct Compare<'a>(&'a [u8]);
    impl Write for Compare<'_> {
        fn write(&mut self, input: &[u8]) -> io::Result<usize> {
            if !self.0.starts_with(input) {
                return Err(io::Error::other("noncanonical object"));
            }
            self.0 = &self.0[input.len()..];
            Ok(input.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let mut output = Compare(bytes);
    serde_json::to_writer(&mut output, &EnvelopeView { object, version: 1 })
        .map_err(|_| Error::Invalid("noncanonical object"))?;
    if !output.0.is_empty() {
        return Err(Error::Invalid("noncanonical object"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared_checkpoint::{decode, Envelope};
    use proptest::prelude::*;
    use serde_json::json;
    use std::collections::BTreeMap;

    fn reference(object: &Object) -> Vec<u8> {
        // The previous shipped algorithm, kept only as a test oracle. It clones
        // values and allocates the full intermediate tree and output buffer.
        crate::log::canonical_json(
            &serde_json::to_value(Envelope {
                version: 1,
                object: object.clone(),
            })
            .unwrap(),
        )
    }

    fn objects(fields: Value) -> Vec<Object> {
        let id = NodeId("00000000000000000000000001".into());
        let hash = super::super::address(b"fixture");
        vec![
            Object::Node(MaterializedNode {
                id: id.clone(),
                parent_id: Some(NodeId("parent".into())),
                node_type: "paragraph".into(),
                pos: "a\n\"".into(),
                current_fields: fields.clone(),
                var_name: Some("var".into()),
                deleted: true,
            }),
            Object::Comment(CommentState {
                node_id: Some(id.clone()),
                body: "বাংলা 🦀\n\t\"\\".into(),
            }),
            Object::Suggestion(SuggestionState {
                target_node_id: Some(id.clone()),
                detail: fields,
                accepted: true,
                rejected_reason: Some("\0rejected".into()),
            }),
            Object::RemovedChoice(id),
            Object::MapLeaf(vec![Entry {
                key: "key\n".into(),
                address: hash.clone(),
            }]),
            Object::MapBranch(BTreeMap::from([
                ("f".into(), hash.clone()),
                ("0".into(), hash.clone()),
            ])),
            Object::State(StateRoots {
                nodes: hash.clone(),
                comments: hash.clone(),
                suggestions: hash.clone(),
                removed_choices: hash,
            }),
        ]
    }

    #[test]
    fn every_object_kind_retains_the_shipped_bytes_and_addresses() {
        for object in objects(
            json!({"z": [null, true, -0.0, 1.25, 1e30], "a": {"é": "a", "😀": "b", "b": "\0\r"}}),
        ) {
            let expected = reference(&object);
            let (address, actual) = encode((&object).into(), MAX_OBJECT_BYTES).unwrap();
            assert_eq!(actual, expected);
            assert_eq!(address, super::super::address(&expected));
            assert!(decode(&address, &actual).is_ok());
        }
    }

    #[test]
    fn exact_object_boundary_and_remaining_operation_budget_are_enforced() {
        let mut comment = CommentState {
            node_id: None,
            body: String::new(),
        };
        let overhead = encode(ObjectRef::Comment(&comment), MAX_OBJECT_BYTES)
            .unwrap()
            .1
            .len();
        comment.body = "a".repeat(MAX_OBJECT_BYTES - overhead);
        let (_, bytes) = encode(ObjectRef::Comment(&comment), MAX_OBJECT_BYTES).unwrap();
        assert_eq!(bytes.len(), MAX_OBJECT_BYTES);
        assert!(matches!(
            encode(ObjectRef::Comment(&comment), MAX_OBJECT_BYTES - 1),
            Err(Error::Limit("bytes"))
        ));
        comment.body.push('a');
        assert!(matches!(
            encode(ObjectRef::Comment(&comment), usize::MAX),
            Err(Error::Limit("object bytes"))
        ));
        // Encoded escaping, not source character count, consumes the capacity.
        comment.body = "\0".repeat(MAX_OBJECT_BYTES / 6);
        assert!(matches!(
            encode(ObjectRef::Comment(&comment), MAX_OBJECT_BYTES),
            Err(Error::Limit("object bytes"))
        ));
        for limit in [0, 1, 17, 4095] {
            assert!(matches!(
                encode(ObjectRef::Comment(&comment), limit),
                Err(Error::Limit("bytes"))
            ));
        }
    }

    #[test]
    fn rejected_input_never_grows_the_encoded_buffer_past_its_cap() {
        let input = vec![b'x'; 8 * MAX_OBJECT_BYTES];
        let mut output = CappedBuffer {
            bytes: Vec::new(),
            limit: 513,
            exceeded: false,
        };
        output.write_all(b"prefix").unwrap();
        let before = output.bytes.capacity();
        assert!(output.write_all(&input).is_err());
        assert_eq!(output.bytes, b"prefix");
        assert_eq!(output.bytes.capacity(), before);
        assert!(output.exceeded);
        // Many tiny escaped writes also stay bounded; no full input copy/tree.
        let comment = CommentState {
            node_id: None,
            body: "\0".repeat(8 * MAX_OBJECT_BYTES),
        };
        let mut output = CappedBuffer {
            bytes: Vec::new(),
            limit: 513,
            exceeded: false,
        };
        assert!(serde_json::to_writer(
            &mut output,
            &EnvelopeView {
                object: ObjectRef::Comment(&comment),
                version: 1
            }
        )
        .is_err());
        assert!(output.bytes.len() <= 513 && output.bytes.capacity() <= 513);
        assert!(output.exceeded);
    }

    #[test]
    fn streaming_comparison_rejects_noncanonical_and_duplicate_fields() {
        for bytes in [
            br#"{"object":{"kind":"comment","value":{"body":"\u0061","node_id":null}},"version":1}"#.as_slice(),
            br#"{"object":{"kind":"comment","value":{"body":"a","body":"a","node_id":null}},"version":1}"#,
            br#"{"version":1,"object":{"kind":"comment","value":{"body":"a","node_id":null}}}"#,
            br#"{"object":{"kind":"comment","value":{"body":"a","node_id":null}},"version":1} "#,
        ] {
            assert!(decode(&super::super::address(bytes), bytes).is_err());
        }
    }

    fn values() -> impl Strategy<Value = Value> {
        prop_oneof![
            Just(Value::Null),
            any::<bool>().prop_map(Value::Bool),
            any::<i64>().prop_map(|v| json!(v)),
            ".{0,50}".prop_map(Value::String)
        ]
        .prop_recursive(4, 128, 8, |inner| {
            prop_oneof![
                prop::collection::vec(inner.clone(), 0..8).prop_map(Value::Array),
                prop::collection::btree_map(".{0,20}", inner, 0..8)
                    .prop_map(|v| Value::Object(v.into_iter().collect())),
            ]
        })
    }
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(128))]
        #[test]
        fn nested_values_match_the_previous_encoder(value in values()) {
            for object in objects(value) {
                let expected = reference(&object);
                let (address, actual) = encode((&object).into(), MAX_OBJECT_BYTES).unwrap();
                prop_assert_eq!(actual.as_slice(), expected.as_slice());
                prop_assert_eq!(address.clone(), super::super::address(&expected));
                prop_assert!(decode(&address, &actual).is_ok());
            }
        }
    }
}
