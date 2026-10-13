//! V2 typed checkpoints. Stable IDs index immutable values; large strings use
//! content trees. All budgets apply across the complete graph, including reused
//! references. See docs/TYPED-CHECKPOINTS.md for the wire contract and limits.

use std::{
    collections::BTreeMap,
    pin::Pin,
    task::{Context, Poll},
};

use engine_shared::{DocumentId, NodeId};
use ring::digest::{Context as HashContext, SHA256};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::io::AsyncWrite;

use super::{
    content, Address, Entry, Error, Limits, ObjectStore, Reader, StateRoots, WriteStats, Writer,
};
use crate::materializer::{CommentState, DocumentState, MaterializedNode, SuggestionState};

pub(super) const MAGIC: &[u8] = b"DYNOTYP\0\x02";
const DOMAIN: &[u8] = b"dynodoc.typed-checkpoint\0v2\0";
pub const MAX_KEY_BYTES: usize = 1024;
pub const MAX_METADATA_BYTES: usize = 4096;
const MAX_TYPED_BYTES: usize = 32 * 1024;
pub const MAX_DEPTH: usize = 64;

/// Semantic budgets bound reconstruction, independently of encoded object size.
/// This is a finite materialization bound, not constant-space DocumentState.
#[derive(Clone, Copy, Debug)]
pub struct SemanticLimits {
    pub values: usize,
    pub string_bytes: usize,
    pub depth: usize,
}

impl Default for SemanticLimits {
    fn default() -> Self {
        Self {
            values: 1_000_000,
            string_bytes: 256 * 1024 * 1024,
            depth: MAX_DEPTH,
        }
    }
}

impl SemanticLimits {
    fn value(&mut self, depth: usize) -> Result<(), Error> {
        if depth > self.depth.min(MAX_DEPTH) {
            return Err(Error::Limit("semantic depth"));
        }
        self.values = self
            .values
            .checked_sub(1)
            .ok_or(Error::Limit("semantic values"))?;
        Ok(())
    }
    fn bytes(&mut self, n: usize) -> Result<(), Error> {
        self.string_bytes = self
            .string_bytes
            .checked_sub(n)
            .ok_or(Error::Limit("semantic string bytes"))?;
        Ok(())
    }
    fn metadata(&mut self, value: &str) -> Result<(), Error> {
        if value.len() > MAX_METADATA_BYTES {
            return Err(Error::Limit("metadata bytes"));
        }
        self.bytes(value.len())
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Text {
    root: Address,
    bytes: usize,
}

#[derive(Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
enum Object {
    Scalar(Value),
    Text(Text),
    Array {
        entries: Address,
        count: usize,
    },
    Dictionary {
        entries: Address,
        count: usize,
    },
    Node {
        id: NodeId,
        parent_id: Option<NodeId>,
        node_type: String,
        pos: String,
        var_name: Option<String>,
        deleted: bool,
        fields: Address,
    },
    Comment {
        node_id: Option<NodeId>,
        body: Text,
    },
    Suggestion {
        target_node_id: Option<NodeId>,
        detail: Address,
        accepted: bool,
        rejected_reason: Option<Text>,
    },
    RemovedChoice(NodeId),
    State(StateRoots),
    Asset {
        media_type: String,
        content: Text,
    },
}

fn address(bytes: &[u8]) -> Address {
    let mut hash = HashContext::new(&SHA256);
    hash.update(DOMAIN);
    hash.update(bytes);
    Address(super::hex(hash.finish().as_ref()))
}

fn encode(object: &Object) -> Result<(Address, Vec<u8>), Error> {
    let mut bytes = MAGIC.to_vec();
    let value = serde_json::to_value(object)?;
    serde_json::to_writer(&mut bytes, &super::canonical::CanonicalValue(&value))?;
    if bytes.len() > MAX_TYPED_BYTES {
        return Err(Error::Limit("typed object bytes"));
    }
    Ok((address(&bytes), bytes))
}

fn decode(expected: &Address, bytes: &[u8]) -> Result<Object, Error> {
    if bytes.len() > MAX_TYPED_BYTES {
        return Err(Error::Limit("typed object bytes"));
    }
    if !bytes.starts_with(MAGIC) {
        return Err(Error::Invalid("typed header/version"));
    }
    if address(bytes) != *expected {
        return Err(Error::HashMismatch);
    }
    let object: Object = serde_json::from_slice(&bytes[MAGIC.len()..])?;
    // Scalars cannot smuggle nested JSON past semantic traversal or allocate a
    // large output through one reference. Canonicalization rejects duplicate keys.
    if matches!(
        &object,
        Object::Scalar(Value::Array(_) | Value::Object(_) | Value::String(_))
    ) {
        return Err(Error::Invalid("expected primitive scalar"));
    }
    if encode(&object)?.1 != bytes {
        return Err(Error::Invalid("noncanonical typed object"));
    }
    Ok(object)
}

pub(super) fn validate(expected: &Address, bytes: &[u8]) -> Result<(), Error> {
    decode(expected, bytes).map(|_| ())
}

/// Direct child addresses of a verified typed object (see `super::references`).
pub(super) fn references(expected: &Address, bytes: &[u8]) -> Result<Vec<Address>, Error> {
    Ok(match decode(expected, bytes)? {
        Object::Scalar(_) | Object::RemovedChoice(_) => Vec::new(),
        Object::Text(text) => vec![text.root],
        Object::Array { entries, .. } | Object::Dictionary { entries, .. } => vec![entries],
        Object::Node { fields, .. } => vec![fields],
        Object::Comment { body, .. } => vec![body.root],
        Object::Suggestion {
            detail,
            rejected_reason,
            ..
        } => std::iter::once(detail)
            .chain(rejected_reason.map(|reason| reason.root))
            .collect(),
        Object::State(roots) => roots.addresses(),
        Object::Asset { content, .. } => vec![content.root],
    })
}

struct Encoder<'a, S> {
    writer: Writer<'a, S>,
    semantic: SemanticLimits,
}
impl<S: ObjectStore> Encoder<'_, S> {
    async fn object(&mut self, object: Object) -> Result<Address, Error> {
        let (address, bytes) = encode(&object)?;
        self.writer.encoded(address, bytes).await
    }
    async fn text(&mut self, text: &[u8]) -> Result<Text, Error> {
        self.semantic.bytes(text.len())?;
        let root = content::write_with_writer(&mut self.writer, &mut &*text).await?;
        Ok(Text {
            root,
            bytes: text.len(),
        })
    }
    async fn value(&mut self, value: &Value, depth: usize) -> Result<Address, Error> {
        self.semantic.value(depth)?;
        let object = match value {
            Value::String(text) => Object::Text(self.text(text.as_bytes()).await?),
            Value::Array(values) => {
                if values.len() > self.semantic.values {
                    return Err(Error::Limit("semantic values"));
                }
                let mut entries = Vec::new();
                for (i, value) in values.iter().enumerate() {
                    let key = format!("{i:016x}");
                    let address = Box::pin(self.value(value, depth + 1)).await?;
                    entries.push((super::route(&key), Entry { key, address }));
                }
                Object::Array {
                    entries: self.writer.entries(entries).await?,
                    count: values.len(),
                }
            }
            Value::Object(values) => {
                if values.len() > self.semantic.values {
                    return Err(Error::Limit("semantic values"));
                }
                let mut entries = Vec::new();
                // Sorting is bounded by the remaining value budget; preserve_order
                // feature unification must not alter the object address.
                let mut keys: Vec<_> = values.keys().collect();
                keys.sort_unstable();
                for key in keys {
                    if key.len() > MAX_KEY_BYTES {
                        return Err(Error::Limit("property key bytes"));
                    }
                    self.semantic.bytes(key.len())?;
                    let address = Box::pin(self.value(&values[key], depth + 1)).await?;
                    entries.push((
                        super::route(key),
                        Entry {
                            key: key.clone(),
                            address,
                        },
                    ));
                }
                Object::Dictionary {
                    entries: self.writer.entries(entries).await?,
                    count: values.len(),
                }
            }
            value => Object::Scalar(value.clone()),
        };
        self.object(object).await
    }
}

fn node_metadata(limits: &mut SemanticLimits, node: &MaterializedNode) -> Result<(), Error> {
    for text in [&node.id.0, &node.node_type, &node.pos]
        .into_iter()
        .chain(node.parent_id.as_ref().map(|v| &v.0))
        .chain(node.var_name.as_ref())
    {
        limits.metadata(text)?;
    }
    Ok(())
}

/// Write a typed state graph; the caller owns authorization and atomic publication.
pub async fn write_state<S: ObjectStore>(
    store: &mut S,
    document: DocumentId,
    state: &DocumentState,
    limits: Limits,
    semantic: SemanticLimits,
) -> Result<(Address, WriteStats), Error> {
    let mut encoder = Encoder {
        writer: Writer {
            store,
            document,
            budget: limits,
            stats: WriteStats::default(),
            pending: Vec::new(),
            pending_bytes: 0,
        },
        semantic,
    };
    let mut entries = Vec::new();
    for (id, node) in &state.nodes {
        encoder.semantic.value(0)?;
        if id != &node.id {
            return Err(Error::Invalid("node map key differs from logical id"));
        }
        node_metadata(&mut encoder.semantic, node)?;
        let fields = encoder.value(&node.current_fields, 0).await?;
        let address = encoder
            .object(Object::Node {
                id: id.clone(),
                parent_id: node.parent_id.clone(),
                node_type: node.node_type.clone(),
                pos: node.pos.clone(),
                var_name: node.var_name.clone(),
                deleted: node.deleted,
                fields,
            })
            .await?;
        entries.push((
            super::route(&id.0),
            Entry {
                key: id.0.clone(),
                address,
            },
        ));
    }
    let nodes = encoder.writer.entries(entries).await?;
    let mut entries = Vec::new();
    for (i, comment) in state.comments.iter().enumerate() {
        encoder.semantic.value(0)?;
        if let Some(id) = &comment.node_id {
            encoder.semantic.metadata(&id.0)?;
        }
        let body = encoder.text(comment.body.as_bytes()).await?;
        let address = encoder
            .object(Object::Comment {
                node_id: comment.node_id.clone(),
                body,
            })
            .await?;
        let key = format!("{i:016x}");
        entries.push((super::route(&key), Entry { key, address }));
    }
    let comments = encoder.writer.entries(entries).await?;
    let mut entries = Vec::new();
    for (key, suggestion) in &state.suggestions {
        encoder.semantic.value(0)?;
        encoder.semantic.metadata(key)?;
        if let Some(id) = &suggestion.target_node_id {
            encoder.semantic.metadata(&id.0)?;
        }
        let detail = encoder.value(&suggestion.detail, 0).await?;
        let rejected_reason = match &suggestion.rejected_reason {
            Some(reason) => Some(encoder.text(reason.as_bytes()).await?),
            None => None,
        };
        let address = encoder
            .object(Object::Suggestion {
                target_node_id: suggestion.target_node_id.clone(),
                detail,
                accepted: suggestion.accepted,
                rejected_reason,
            })
            .await?;
        entries.push((
            super::route(key),
            Entry {
                key: key.clone(),
                address,
            },
        ));
    }
    let suggestions = encoder.writer.entries(entries).await?;
    let mut entries = Vec::new();
    if state.removed_choices.len() > encoder.semantic.values {
        return Err(Error::Limit("semantic values"));
    }
    let mut removed: Vec<_> = state.removed_choices.iter().collect();
    removed.sort();
    for id in removed {
        encoder.semantic.value(0)?;
        encoder.semantic.metadata(&id.0)?;
        let address = encoder.object(Object::RemovedChoice(id.clone())).await?;
        entries.push((
            super::route(&id.0),
            Entry {
                key: id.0.clone(),
                address,
            },
        ));
    }
    let removed_choices = encoder.writer.entries(entries).await?;
    let root = encoder
        .object(Object::State(StateRoots {
            nodes,
            comments,
            suggestions,
            removed_choices,
        }))
        .await?;
    encoder.writer.flush().await?;
    Ok((root, encoder.writer.stats))
}

struct BoundedBytes {
    bytes: Vec<u8>,
    limit: usize,
}
impl AsyncWrite for BoundedBytes {
    fn poll_write(
        mut self: Pin<&mut Self>,
        _: &mut Context<'_>,
        bytes: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        if bytes.len() > self.limit - self.bytes.len() {
            return Poll::Ready(Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "content exceeds declared length",
            )));
        }
        self.bytes.extend_from_slice(bytes);
        Poll::Ready(Ok(bytes.len()))
    }
    fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Poll::Ready(Ok(()))
    }
    fn poll_shutdown(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}

struct Decoder<'a, S> {
    reader: Reader<'a, S>,
    semantic: SemanticLimits,
}
impl<S: ObjectStore> Decoder<'_, S> {
    async fn object(&mut self, root: &Address) -> Result<Object, Error> {
        if self.reader.budget.max_objects == 0 {
            return Err(Error::Limit("objects"));
        }
        let bytes = self.reader.store.get(self.reader.document, root).await?;
        self.reader.budget.consume(bytes.len())?;
        decode(root, &bytes)
    }
    async fn bytes(&mut self, text: Text) -> Result<Vec<u8>, Error> {
        self.semantic.bytes(text.bytes)?;
        let mut sink = BoundedBytes {
            bytes: Vec::new(),
            limit: text.bytes,
        };
        let length = content::read_with_budget(
            self.reader.store,
            self.reader.document,
            &text.root,
            &mut sink,
            &mut self.reader.budget,
        )
        .await?;
        if length != text.bytes as u64 {
            return Err(Error::Invalid("declared content length"));
        }
        Ok(sink.bytes)
    }
    async fn text(&mut self, text: Text) -> Result<String, Error> {
        String::from_utf8(self.bytes(text).await?).map_err(|_| Error::Invalid("text UTF-8"))
    }
    async fn entries(
        &mut self,
        root: &Address,
        count: usize,
        key_limit: usize,
    ) -> Result<BTreeMap<String, Address>, Error> {
        if count > self.semantic.values {
            return Err(Error::Limit("semantic values"));
        }
        let entries = self.reader.entries_limited(root, count, key_limit).await?;
        if entries.len() != count {
            return Err(Error::Invalid("container count"));
        }
        Ok(entries)
    }
    async fn value(&mut self, root: &Address, depth: usize) -> Result<Value, Error> {
        self.semantic.value(depth)?;
        match self.object(root).await? {
            Object::Scalar(value) => Ok(value),
            Object::Text(text) => Ok(Value::String(self.text(text).await?)),
            Object::Array { entries, count } => {
                let entries = self.entries(&entries, count, 16).await?;
                let mut values = Vec::new();
                for (i, (key, root)) in entries.into_iter().enumerate() {
                    if key != format!("{i:016x}") {
                        return Err(Error::Invalid("array ordinal"));
                    }
                    values.push(Box::pin(self.value(&root, depth + 1)).await?);
                }
                Ok(Value::Array(values))
            }
            Object::Dictionary { entries, count } => {
                let entries = self.entries(&entries, count, MAX_KEY_BYTES).await?;
                let mut values = serde_json::Map::new();
                for (key, root) in entries {
                    self.semantic.bytes(key.len())?;
                    values.insert(key, Box::pin(self.value(&root, depth + 1)).await?);
                }
                Ok(Value::Object(values))
            }
            _ => Err(Error::Invalid("expected typed value")),
        }
    }
    async fn records(&mut self, root: &Address) -> Result<BTreeMap<String, Address>, Error> {
        self.reader
            .entries_limited(root, self.semantic.values, MAX_METADATA_BYTES)
            .await
    }
}

/// Verify the entire typed graph and reconstruct state under semantic and I/O
/// budgets. Missing/corrupt/wrongly typed content fails closed, including tombstones.
pub async fn read_state<S: ObjectStore>(
    store: &mut S,
    document: DocumentId,
    root: &Address,
    limits: Limits,
    semantic: SemanticLimits,
) -> Result<DocumentState, Error> {
    let mut decoder = Decoder {
        reader: Reader {
            store,
            document,
            budget: limits,
        },
        semantic,
    };
    let Object::State(roots) = decoder.object(root).await? else {
        return Err(Error::Invalid("expected typed state"));
    };
    let mut state = DocumentState::default();
    for (key, root) in decoder.records(&roots.nodes).await? {
        decoder.semantic.value(0)?;
        let Object::Node {
            id,
            parent_id,
            node_type,
            pos,
            var_name,
            deleted,
            fields,
        } = decoder.object(&root).await?
        else {
            return Err(Error::Invalid("expected typed node"));
        };
        if id.0 != key {
            return Err(Error::Invalid("node key/id"));
        }
        let mut node = MaterializedNode {
            id,
            parent_id,
            node_type,
            pos,
            var_name,
            deleted,
            current_fields: Value::Null,
        };
        node_metadata(&mut decoder.semantic, &node)?;
        node.current_fields = decoder.value(&fields, 0).await?;
        state.nodes.insert(node.id.clone(), node);
    }
    for (key, root) in decoder.records(&roots.comments).await? {
        decoder.semantic.value(0)?;
        if key != format!("{:016x}", state.comments.len()) {
            return Err(Error::Invalid("comment ordinal"));
        }
        let Object::Comment { node_id, body } = decoder.object(&root).await? else {
            return Err(Error::Invalid("expected typed comment"));
        };
        if let Some(id) = &node_id {
            decoder.semantic.metadata(&id.0)?;
        }
        state.comments.push(CommentState {
            node_id,
            body: decoder.text(body).await?,
        });
    }
    for (key, root) in decoder.records(&roots.suggestions).await? {
        decoder.semantic.value(0)?;
        decoder.semantic.metadata(&key)?;
        let Object::Suggestion {
            target_node_id,
            detail,
            accepted,
            rejected_reason,
        } = decoder.object(&root).await?
        else {
            return Err(Error::Invalid("expected typed suggestion"));
        };
        if let Some(id) = &target_node_id {
            decoder.semantic.metadata(&id.0)?;
        }
        let rejected_reason = match rejected_reason {
            Some(reason) => Some(decoder.text(reason).await?),
            None => None,
        };
        let detail = decoder.value(&detail, 0).await?;
        state.suggestions.insert(
            key,
            SuggestionState {
                target_node_id,
                detail,
                accepted,
                rejected_reason,
            },
        );
    }
    for (key, root) in decoder.records(&roots.removed_choices).await? {
        decoder.semantic.value(0)?;
        decoder.semantic.metadata(&key)?;
        match decoder.object(&root).await? {
            Object::RemovedChoice(id) if id.0 == key => {
                state.removed_choices.insert(id);
            }
            _ => return Err(Error::Invalid("removed choice key/id")),
        }
    }
    Ok(state)
}

/// Store an opaque asset manifest over preexisting content, verifying the full
/// byte tree without buffering it. Native adapters must explicitly reference this
/// manifest; it is not silently added to DocumentState or a checkpoint.
pub async fn write_asset<S: ObjectStore>(
    store: &mut S,
    document: DocumentId,
    root: &Address,
    media_type: &str,
    limits: Limits,
) -> Result<Address, Error> {
    if media_type.is_empty() || media_type.len() > 255 || !media_type.is_ascii() {
        return Err(Error::Invalid("asset media type"));
    }
    let mut budget = limits;
    let bytes =
        content::read_with_budget(store, document, root, &mut tokio::io::sink(), &mut budget)
            .await?;
    let bytes = usize::try_from(bytes).map_err(|_| Error::Limit("asset length"))?;
    let (address, encoded) = encode(&Object::Asset {
        media_type: media_type.into(),
        content: Text {
            root: root.clone(),
            bytes,
        },
    })?;
    budget.consume(encoded.len())?;
    store.put(document, &address, &encoded).await?;
    Ok(address)
}

/// Verify and stream an explicitly referenced native asset to a staged sink.
pub async fn read_asset<S: ObjectStore, W: AsyncWrite + Unpin + Send>(
    store: &mut S,
    document: DocumentId,
    root: &Address,
    output: &mut W,
    mut limits: Limits,
) -> Result<(String, u64), Error> {
    if limits.max_objects == 0 {
        return Err(Error::Limit("objects"));
    }
    let bytes = store.get(document, root).await?;
    limits.consume(bytes.len())?;
    let Object::Asset {
        media_type,
        content,
    } = decode(root, &bytes)?
    else {
        return Err(Error::Invalid("expected asset"));
    };
    if media_type.is_empty() || media_type.len() > 255 || !media_type.is_ascii() {
        return Err(Error::Invalid("asset media type"));
    }
    let length =
        content::read_with_budget(store, document, &content.root, output, &mut limits).await?;
    if length != content.bytes as u64 {
        return Err(Error::Invalid("asset length"));
    }
    Ok((media_type, length))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared_checkpoint::{MemoryStore, MAX_OBJECT_BYTES};
    use serde_json::json;

    fn doc() -> DocumentId {
        DocumentId(uuid::Uuid::new_v4())
    }
    async fn write(
        store: &mut MemoryStore,
        document: DocumentId,
        state: &DocumentState,
    ) -> (Address, WriteStats) {
        write_state(
            store,
            document,
            state,
            Limits::default(),
            SemanticLimits::default(),
        )
        .await
        .unwrap()
    }
    async fn read(
        store: &mut MemoryStore,
        document: DocumentId,
        root: &Address,
    ) -> Result<DocumentState, Error> {
        read_state(
            store,
            document,
            root,
            Limits::default(),
            SemanticLimits::default(),
        )
        .await
    }
    fn fixture() -> DocumentState {
        let mut state = super::super::tests::fixture(75);
        state.nodes.values_mut().next().unwrap().current_fields = json!({
            "text": "বাংলা 🦀".repeat(150_000), "array": [null, true, -0.0, 1.25, 18446744073709551615u64],
            "nested": {"runs": [{"text":"Hello", "bold":true}]}
        });
        state.comments[0].body = "Comment".repeat(160_000);
        state
            .suggestions
            .values_mut()
            .next()
            .unwrap()
            .rejected_reason = Some("Reason".repeat(180_000));
        state
    }

    #[tokio::test]
    async fn large_typed_state_roundtrips_reuses_and_preserves_v1() {
        let mut store = MemoryStore::default();
        let document = doc();
        let state = fixture();
        assert!(matches!(
            super::super::write_state(&mut store, document, &state, Limits::default()).await,
            Err(Error::Limit(_))
        ));
        let (root, first) = write(&mut store, document, &state).await;
        assert_eq!(read(&mut store, document, &root).await.unwrap(), state);
        let (same, again) = write(&mut store, document, &state).await;
        assert_eq!(same, root);
        assert_eq!(again.new_objects, 0);
        assert_eq!(again.new_bytes, 0);
        let mut edited = state.clone();
        edited.nodes.values_mut().nth(20).unwrap().current_fields["text"] = json!("Changed");
        let (changed, delta) = write(&mut store, document, &edited).await;
        assert_ne!(changed, root);
        assert!(delta.new_bytes < first.new_bytes / 5);
        assert_eq!(read(&mut store, document, &changed).await.unwrap(), edited);
        assert_eq!(read(&mut store, document, &root).await.unwrap(), state);
        assert!(matches!(
            read(&mut store, doc(), &root).await,
            Err(Error::Missing(_))
        ));
        assert!(
            super::super::read_state(&mut store, document, &root, Limits::default())
                .await
                .is_err()
        );
        let small = super::super::tests::fixture(2);
        let (v1, _) = super::super::write_state(&mut store, document, &small, Limits::default())
            .await
            .unwrap();
        assert_eq!(
            super::super::read_state(&mut store, document, &v1, Limits::default())
                .await
                .unwrap(),
            small
        );
        assert!(read(&mut store, document, &v1).await.is_err());
    }

    #[tokio::test]
    async fn reference_walk_reaches_exactly_the_written_typed_graph() {
        let mut store = MemoryStore::default();
        let document = doc();
        let (root, _) = write(&mut store, document, &fixture()).await;
        // Includes v2 values, v1 map nodes and multi-level content trees.
        assert!(store.object_count() > 100);
        let reached = super::super::tests::reachable(&mut store, document, &root).await;
        assert_eq!(reached.len(), store.object_count());
        let body = "asset body ".repeat(20_000);
        let (content, _) = content::write(
            &mut store,
            document,
            &mut body.as_bytes(),
            Limits::default(),
        )
        .await
        .unwrap();
        let asset = write_asset(
            &mut store,
            document,
            &content,
            "text/plain",
            Limits::default(),
        )
        .await
        .unwrap();
        let reached = super::super::tests::reachable(&mut store, document, &asset).await;
        assert!(reached.len() >= 3);
        assert!(!reached.contains(&root));
    }

    #[tokio::test]
    async fn global_budgets_cover_repeated_values_strings_and_nested_graphs() {
        let mut store = MemoryStore::default();
        let document = doc();
        let mut state = super::super::tests::fixture(1);
        state.nodes.values_mut().next().unwrap().current_fields =
            json!(vec!["x".repeat(MAX_OBJECT_BYTES); 3]);
        let (root, stats) = write(&mut store, document, &state).await;
        for limits in [
            Limits {
                max_bytes: stats.encoded_bytes - 1,
                max_objects: usize::MAX,
            },
            Limits {
                max_bytes: usize::MAX,
                max_objects: stats.visited_objects - 1,
            },
        ] {
            assert!(matches!(
                write_state(
                    &mut store,
                    document,
                    &state,
                    limits,
                    SemanticLimits::default()
                )
                .await,
                Err(Error::Limit(_))
            ));
            assert!(matches!(
                read_state(
                    &mut store,
                    document,
                    &root,
                    limits,
                    SemanticLimits::default()
                )
                .await,
                Err(Error::Limit(_))
            ));
        }
        for semantic in [
            SemanticLimits {
                values: 3,
                ..Default::default()
            },
            SemanticLimits {
                string_bytes: MAX_OBJECT_BYTES * 2,
                ..Default::default()
            },
            SemanticLimits {
                depth: 0,
                ..Default::default()
            },
        ] {
            assert!(matches!(
                write_state(&mut store, document, &state, Limits::default(), semantic).await,
                Err(Error::Limit(_))
            ));
            assert!(matches!(
                read_state(&mut store, document, &root, Limits::default(), semantic).await,
                Err(Error::Limit(_))
            ));
        }
        let exact = Limits {
            max_bytes: stats.encoded_bytes,
            max_objects: stats.visited_objects,
        };
        assert_eq!(
            read_state(
                &mut store,
                document,
                &root,
                exact,
                SemanticLimits::default()
            )
            .await
            .unwrap(),
            state
        );
        let mut nested = Value::Null;
        for _ in 0..MAX_DEPTH + 1 {
            nested = json!([nested]);
        }
        state.nodes.values_mut().next().unwrap().current_fields = nested;
        assert!(matches!(
            write_state(
                &mut store,
                document,
                &state,
                Limits::default(),
                SemanticLimits::default()
            )
            .await,
            Err(Error::Limit("semantic depth"))
        ));
    }

    #[tokio::test]
    async fn missing_corrupt_and_mistyped_values_fail_closed() {
        let mut store = MemoryStore::default();
        let document = doc();
        let state = super::super::tests::fixture(2);
        let (root, _) = write(&mut store, document, &state).await;
        let key = store
            .objects
            .iter()
            .find(|(_, bytes)| bytes.starts_with(content::MAGIC) && bytes[9] == 1)
            .unwrap()
            .0
            .clone();
        let original = store.objects.remove(&key).unwrap();
        assert!(matches!(
            read(&mut store, document, &root).await,
            Err(Error::Missing(_))
        ));
        let mut broken = original.clone();
        broken[10] ^= 1;
        store.objects.insert(key.clone(), broken);
        assert!(matches!(
            read(&mut store, document, &root).await,
            Err(Error::HashMismatch)
        ));
        store.objects.insert(key, original);
        assert_eq!(read(&mut store, document, &root).await.unwrap(), state);
        let (address, bytes) = encode(&Object::Scalar(json!(7))).unwrap();
        store.put(document, &address, &bytes).await.unwrap();
        assert!(matches!(
            read(&mut store, document, &address).await,
            Err(Error::Invalid("expected typed state"))
        ));
    }

    #[tokio::test]
    async fn forged_lengths_counts_utf8_and_metadata_are_rejected() {
        let document = doc();
        let mut store = MemoryStore::default();
        let (root, _) = content::write(&mut store, document, &mut &b"abc"[..], Limits::default())
            .await
            .unwrap();
        let mut decoder = Decoder {
            reader: Reader {
                store: &mut store,
                document,
                budget: Limits::default(),
            },
            semantic: SemanticLimits::default(),
        };
        assert!(decoder
            .text(Text {
                root: root.clone(),
                bytes: 2
            })
            .await
            .is_err());
        assert!(decoder.text(Text { root, bytes: 4 }).await.is_err());
        let (root, _) = content::write(&mut store, document, &mut &b"\xff"[..], Limits::default())
            .await
            .unwrap();
        let mut decoder = Decoder {
            reader: Reader {
                store: &mut store,
                document,
                budget: Limits::default(),
            },
            semantic: SemanticLimits::default(),
        };
        assert!(matches!(
            decoder.text(Text { root, bytes: 1 }).await,
            Err(Error::Invalid("text UTF-8"))
        ));
        let (map, bytes) = super::super::encode(super::super::Object::MapLeaf(vec![])).unwrap();
        store.put(document, &map, &bytes).await.unwrap();
        let (root, bytes) = encode(&Object::Array {
            entries: map,
            count: usize::MAX,
        })
        .unwrap();
        store.put(document, &root, &bytes).await.unwrap();
        let mut decoder = Decoder {
            reader: Reader {
                store: &mut store,
                document,
                budget: Limits::default(),
            },
            semantic: SemanticLimits::default(),
        };
        assert!(matches!(
            decoder.value(&root, 0).await,
            Err(Error::Limit("semantic values"))
        ));
        let mut state = super::super::tests::fixture(1);
        state.nodes.values_mut().next().unwrap().pos = "x".repeat(MAX_METADATA_BYTES + 1);
        assert!(write_state(
            &mut store,
            document,
            &state,
            Limits::default(),
            SemanticLimits::default()
        )
        .await
        .is_err());
        state.nodes.values_mut().next().unwrap().pos = "ok".into();
        state.nodes.values_mut().next().unwrap().current_fields =
            json!({"x".repeat(MAX_KEY_BYTES + 1): null});
        assert!(write_state(
            &mut store,
            document,
            &state,
            Limits::default(),
            SemanticLimits::default()
        )
        .await
        .is_err());
    }

    #[tokio::test]
    async fn asset_manifest_verifies_scoped_bytes_without_interpreting_them() {
        let mut store = MemoryStore::default();
        let document = doc();
        let data = vec![0xff; 2 * MAX_OBJECT_BYTES];
        let (content, _) = content::write(
            &mut store,
            document,
            &mut data.as_slice(),
            Limits::default(),
        )
        .await
        .unwrap();
        let root = write_asset(
            &mut store,
            document,
            &content,
            "image/png",
            Limits::default(),
        )
        .await
        .unwrap();
        let mut output = Vec::new();
        let (media_type, length) =
            read_asset(&mut store, document, &root, &mut output, Limits::default())
                .await
                .unwrap();
        assert_eq!(media_type, "image/png");
        assert_eq!(length, data.len() as u64);
        assert_eq!(output, data);
        assert!(read(&mut store, document, &root).await.is_err());
        assert!(
            write_asset(&mut store, doc(), &content, "image/png", Limits::default())
                .await
                .is_err()
        );
        assert!(
            write_asset(&mut store, document, &root, "image/png", Limits::default())
                .await
                .is_err()
        );
    }

    #[test]
    fn typed_canonical_vectors_and_noncanonical_rejection() {
        // SHA-256(domain || magic || canonical JSON), independently computed with Python hashlib.
        let (root, bytes) = encode(&Object::Scalar(Value::Null)).unwrap();
        assert_eq!(&bytes[MAGIC.len()..], br#"{"kind":"scalar","value":null}"#);
        assert_eq!(
            root.as_str(),
            "614afa1a326802d2432a51bb917cbff36c01c440f925efdd479eebec01ce6af0"
        );
        assert!(decode(&root, &bytes).is_ok());
        for tail in [
            br#"{"kind":"scalar","value":null,"extra":true}"#.as_slice(),
            br#"{"kind":"scalar","value":[],"value":null}"#,
            br#"{"kind":"scalar","value":{}}"#,
            br#"{ "kind":"scalar","value":null}"#,
        ] {
            let mut bad = MAGIC.to_vec();
            bad.extend_from_slice(tail);
            assert!(decode(&address(&bad), &bad).is_err());
        }
    }
}
