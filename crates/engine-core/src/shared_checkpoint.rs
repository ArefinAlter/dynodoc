//! Opt-in, document-scoped shared checkpoints. See docs/SHARED-CHECKPOINTS.md.
//!
//! This codec preserves all `DocumentState` fields. It does not change event
//! hashes, select a large-asset backend, or provide a portable commit protocol.

use std::collections::BTreeMap;
use std::future::Future;

use engine_shared::{DocumentId, NodeId};
use ring::digest::{digest, Context, SHA256};
use serde::{Deserialize, Serialize};

use crate::materializer::{CommentState, DocumentState, MaterializedNode, SuggestionState};

pub mod postgres;

pub const MAX_OBJECT_BYTES: usize = 1024 * 1024;
const LEAF_ENTRIES: usize = 32;
const DOMAIN: &[u8] = b"dynodoc.shared-checkpoint\0v1\0";

/// Canonical, lowercase hexadecimal SHA-256 address. Construction validates it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Address(String);

impl TryFrom<String> for Address {
    type Error = Error;
    fn try_from(value: String) -> Result<Self, Error> {
        if value.len() != 64
            || !value
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(Error::Invalid("invalid object address"));
        }
        Ok(Self(value))
    }
}

impl From<Address> for String {
    fn from(value: Address) -> String {
        value.0
    }
}

impl Address {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("shared checkpoint limit exceeded: {0}")]
    Limit(&'static str),
    #[error("invalid shared checkpoint: {0}")]
    Invalid(&'static str),
    #[error("missing checkpoint object {0}")]
    Missing(String),
    #[error("checkpoint object hash mismatch")]
    HashMismatch,
    #[error("checkpoint codec: {0}")]
    Json(#[from] serde_json::Error),
    #[error("checkpoint database: {0}")]
    Db(#[from] sqlx::Error),
    #[error("checkpoint history: {0}")]
    History(#[from] crate::snapshot::SnapshotError),
}

/// Per-operation budgets, including reused objects. No request is unbounded.
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    pub max_objects: usize,
    pub max_bytes: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_objects: 1_000_000,
            max_bytes: 512 * 1024 * 1024,
        }
    }
}

impl Limits {
    fn consume(&mut self, bytes: usize) -> Result<(), Error> {
        self.max_objects = self
            .max_objects
            .checked_sub(1)
            .ok_or(Error::Limit("objects"))?;
        self.max_bytes = self
            .max_bytes
            .checked_sub(bytes)
            .ok_or(Error::Limit("bytes"))?;
        Ok(())
    }
}

/// Internal storage interface, not an authorization interface. Every operation
/// must be scoped to a document already authorized by the caller. `put` must
/// validate bytes/address and return true only when inserting a new object.
pub trait ObjectStore {
    fn put(
        &mut self,
        document: DocumentId,
        address: &Address,
        bytes: &[u8],
    ) -> impl Future<Output = Result<bool, Error>> + Send;

    fn get(
        &mut self,
        document: DocumentId,
        address: &Address,
    ) -> impl Future<Output = Result<Vec<u8>, Error>> + Send;
}

/// Small local/test backend. Durability belongs to the PostgreSQL backend;
/// a durable on-disk local repository is a later increment.
#[derive(Default)]
pub struct MemoryStore {
    objects: BTreeMap<(uuid::Uuid, Address), Vec<u8>>,
}

impl MemoryStore {
    pub fn object_count(&self) -> usize {
        self.objects.len()
    }

    pub fn stored_bytes(&self) -> usize {
        self.objects.values().map(Vec::len).sum()
    }
}

impl ObjectStore for MemoryStore {
    async fn put(
        &mut self,
        document: DocumentId,
        address: &Address,
        bytes: &[u8],
    ) -> Result<bool, Error> {
        decode(address, bytes)?;
        let key = (document.0, address.clone());
        if let Some(existing) = self.objects.get(&key) {
            if existing != bytes {
                return Err(Error::HashMismatch);
            }
            return Ok(false);
        }
        self.objects.insert(key, bytes.to_vec());
        Ok(true)
    }

    async fn get(&mut self, document: DocumentId, address: &Address) -> Result<Vec<u8>, Error> {
        self.objects
            .get(&(document.0, address.clone()))
            .cloned()
            .ok_or_else(|| Error::Missing(address.0.clone()))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Entry {
    key: String,
    address: Address,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StateRoots {
    nodes: Address,
    comments: Address,
    suggestions: Address,
    removed_choices: Address,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
enum Object {
    Node(MaterializedNode),
    Comment(CommentState),
    Suggestion(SuggestionState),
    RemovedChoice(NodeId),
    MapLeaf(Vec<Entry>),
    MapBranch(BTreeMap<String, Address>),
    State(StateRoots),
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    version: u8,
    object: Object,
}

fn address(bytes: &[u8]) -> Address {
    let mut hash = Context::new(&SHA256);
    hash.update(DOMAIN);
    hash.update(bytes);
    Address(hex(hash.finish().as_ref()))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn route(key: &str) -> String {
    hex(digest(&SHA256, key.as_bytes()).as_ref())
}

fn encode(object: Object) -> Result<(Address, Vec<u8>), Error> {
    let value = serde_json::to_value(Envelope { version: 1, object })?;
    let bytes = crate::log::canonical_json(&value);
    if bytes.len() > MAX_OBJECT_BYTES {
        return Err(Error::Limit("object bytes"));
    }
    Ok((address(&bytes), bytes))
}

fn decode(expected: &Address, bytes: &[u8]) -> Result<Object, Error> {
    if bytes.len() > MAX_OBJECT_BYTES {
        return Err(Error::Limit("object bytes"));
    }
    if &address(bytes) != expected {
        return Err(Error::HashMismatch);
    }
    let envelope: Envelope = serde_json::from_slice(bytes)?;
    if envelope.version != 1 {
        return Err(Error::Invalid("unsupported object version"));
    }
    // Also rejects duplicate/unknown fields and noncanonical numeric/string forms.
    let (_, canonical) = encode(envelope.object.clone())?;
    if canonical != bytes {
        return Err(Error::Invalid("noncanonical object"));
    }
    match &envelope.object {
        Object::MapLeaf(entries)
            if entries.len() > LEAF_ENTRIES || entries.windows(2).any(|w| w[0].key >= w[1].key) =>
        {
            return Err(Error::Invalid("map leaf order or size"));
        }
        Object::MapBranch(children)
            if children.is_empty()
                || children.len() > 16
                || children
                    .keys()
                    .any(|k| k.len() != 1 || !b"0123456789abcdef".contains(&k.as_bytes()[0])) =>
        {
            return Err(Error::Invalid("map branch slots"));
        }
        _ => {}
    }
    Ok(envelope.object)
}

#[derive(Debug, Default, Clone, Copy)]
pub struct WriteStats {
    pub visited_objects: usize,
    pub encoded_bytes: usize,
    pub new_objects: usize,
    pub new_bytes: usize,
}

struct Writer<'a, S> {
    store: &'a mut S,
    document: DocumentId,
    budget: Limits,
    stats: WriteStats,
}

impl<S: ObjectStore + Send> Writer<'_, S> {
    async fn object(&mut self, object: Object) -> Result<Address, Error> {
        let (address, bytes) = encode(object)?;
        self.budget.consume(bytes.len())?;
        self.stats.visited_objects += 1;
        self.stats.encoded_bytes += bytes.len();
        if self.store.put(self.document, &address, &bytes).await? {
            self.stats.new_objects += 1;
            self.stats.new_bytes += bytes.len();
        }
        Ok(address)
    }

    // Entries are keyed by (hash route, original key), so splitting borrows slices
    // instead of copying all entries at every depth.
    async fn map(&mut self, entries: &[(String, Entry)], depth: usize) -> Result<Address, Error> {
        if entries.len() <= LEAF_ENTRIES {
            let mut leaf: Vec<_> = entries.iter().map(|(_, e)| e.clone()).collect();
            leaf.sort_by(|a, b| a.key.cmp(&b.key));
            return self.object(Object::MapLeaf(leaf)).await;
        }
        if depth == 64 {
            return Err(Error::Limit("radix depth"));
        }
        let mut children = BTreeMap::new();
        let mut start = 0;
        while start < entries.len() {
            let nibble = &entries[start].0[depth..depth + 1];
            let length = entries[start..].partition_point(|(r, _)| &r[depth..depth + 1] == nibble);
            let child = Box::pin(self.map(&entries[start..start + length], depth + 1)).await?;
            children.insert(nibble.to_owned(), child);
            start += length;
        }
        self.object(Object::MapBranch(children)).await
    }

    async fn entries(&mut self, mut entries: Vec<(String, Entry)>) -> Result<Address, Error> {
        entries.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.key.cmp(&b.1.key)));
        self.map(&entries, 0).await
    }
}

/// Write a deterministic shared state graph. The store's caller controls atomic
/// publication; use `postgres::take` for transactionally published checkpoints.
pub async fn write_state<S: ObjectStore + Send>(
    store: &mut S,
    document: DocumentId,
    state: &DocumentState,
    limits: Limits,
) -> Result<(Address, WriteStats), Error> {
    let mut writer = Writer {
        store,
        document,
        budget: limits,
        stats: WriteStats::default(),
    };
    let mut entries = Vec::new();
    for (id, node) in &state.nodes {
        if id != &node.id {
            return Err(Error::Invalid("node map key differs from logical id"));
        }
        let address = writer.object(Object::Node(node.clone())).await?;
        entries.push((
            route(&id.0),
            Entry {
                key: id.0.clone(),
                address,
            },
        ));
    }
    let nodes = writer.entries(entries).await?;
    let mut entries = Vec::new();
    for (index, comment) in state.comments.iter().enumerate() {
        let key = format!("{index:016x}");
        let address = writer.object(Object::Comment(comment.clone())).await?;
        entries.push((route(&key), Entry { key, address }));
    }
    let comments = writer.entries(entries).await?;
    let mut entries = Vec::new();
    for (key, suggestion) in &state.suggestions {
        let address = writer
            .object(Object::Suggestion(suggestion.clone()))
            .await?;
        entries.push((
            route(key),
            Entry {
                key: key.clone(),
                address,
            },
        ));
    }
    let suggestions = writer.entries(entries).await?;
    let mut entries = Vec::new();
    // Deterministic insertion order also prevents same-revision publishers from
    // locking shared object rows in opposite orders (HashSet order is randomized).
    let mut removed: Vec<_> = state.removed_choices.iter().collect();
    removed.sort();
    for id in removed {
        let address = writer.object(Object::RemovedChoice(id.clone())).await?;
        entries.push((
            route(&id.0),
            Entry {
                key: id.0.clone(),
                address,
            },
        ));
    }
    let removed_choices = writer.entries(entries).await?;
    let root = writer
        .object(Object::State(StateRoots {
            nodes,
            comments,
            suggestions,
            removed_choices,
        }))
        .await?;
    Ok((root, writer.stats))
}

struct Reader<'a, S> {
    store: &'a mut S,
    document: DocumentId,
    budget: Limits,
}

impl<S: ObjectStore + Send> Reader<'_, S> {
    async fn object(&mut self, address: &Address) -> Result<Object, Error> {
        // Check object budget before the backend read; bytes are capped by codec
        // and the PostgreSQL column constraint. Backends must bound their fetch.
        if self.budget.max_objects == 0 {
            return Err(Error::Limit("objects"));
        }
        let bytes = self.store.get(self.document, address).await?;
        self.budget.consume(bytes.len())?;
        decode(address, &bytes)
    }

    async fn map(
        &mut self,
        root: &Address,
        prefix: String,
        out: &mut BTreeMap<String, Address>,
    ) -> Result<usize, Error> {
        match self.object(root).await? {
            Object::MapLeaf(entries) => {
                let count = entries.len();
                for entry in entries {
                    if !route(&entry.key).starts_with(&prefix)
                        || out.insert(entry.key, entry.address).is_some()
                    {
                        return Err(Error::Invalid("map key route or duplicate"));
                    }
                }
                Ok(count)
            }
            Object::MapBranch(children) if prefix.len() < 64 => {
                let mut count = 0;
                for (slot, child) in children {
                    let n = Box::pin(self.map(&child, format!("{prefix}{slot}"), out)).await?;
                    if n == 0 {
                        return Err(Error::Invalid("empty branch child"));
                    }
                    count += n;
                }
                if count <= LEAF_ENTRIES {
                    return Err(Error::Invalid("unnecessary map branch"));
                }
                Ok(count)
            }
            _ => Err(Error::Invalid("expected bounded map")),
        }
    }

    async fn entries(&mut self, root: &Address) -> Result<BTreeMap<String, Address>, Error> {
        let mut entries = BTreeMap::new();
        self.map(root, String::new(), &mut entries).await?;
        Ok(entries)
    }
}

/// Verify every reachable object's bytes, type and map structure and reconstruct
/// the complete state. Missing/corrupt content never falls back to legacy data.
pub async fn read_state<S: ObjectStore + Send>(
    store: &mut S,
    document: DocumentId,
    root: &Address,
    limits: Limits,
) -> Result<DocumentState, Error> {
    let mut reader = Reader {
        store,
        document,
        budget: limits,
    };
    let Object::State(roots) = reader.object(root).await? else {
        return Err(Error::Invalid("expected state root"));
    };
    let mut state = DocumentState::default();
    for (key, address) in reader.entries(&roots.nodes).await? {
        match reader.object(&address).await? {
            Object::Node(node) if node.id.0 == key => {
                state.nodes.insert(node.id.clone(), node);
            }
            _ => return Err(Error::Invalid("node type or logical id")),
        }
    }
    for (key, address) in reader.entries(&roots.comments).await? {
        match reader.object(&address).await? {
            Object::Comment(comment) if key == format!("{:016x}", state.comments.len()) => {
                state.comments.push(comment)
            }
            _ => return Err(Error::Invalid("comment type or ordinal")),
        }
    }
    for (key, address) in reader.entries(&roots.suggestions).await? {
        match reader.object(&address).await? {
            Object::Suggestion(suggestion) => {
                state.suggestions.insert(key, suggestion);
            }
            _ => return Err(Error::Invalid("suggestion type")),
        }
    }
    for (key, address) in reader.entries(&roots.removed_choices).await? {
        match reader.object(&address).await? {
            Object::RemovedChoice(id) if id.0 == key => {
                state.removed_choices.insert(id);
            }
            _ => return Err(Error::Invalid("removed choice type or id")),
        }
    }
    Ok(state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use serde_json::json;

    fn fixture(count: usize) -> DocumentState {
        let mut state = DocumentState::default();
        for i in 0..count {
            let id = NodeId(format!("{i:026}"));
            state.nodes.insert(
                id.clone(),
                MaterializedNode {
                    id,
                    parent_id: None,
                    node_type: "paragraph".into(),
                    pos: format!("p{i:08}"),
                    current_fields: json!({"text": "বাংলা 🦀", "number": 1.25, "zero": -0.0}),
                    var_name: None,
                    deleted: i % 3 == 0,
                },
            );
        }
        state.comments = vec![
            CommentState {
                node_id: None,
                body: "First".into(),
            },
            CommentState {
                node_id: None,
                body: "Second".into(),
            },
        ];
        state.suggestions.insert(
            "suggestion".into(),
            SuggestionState {
                target_node_id: None,
                detail: json!({"a": 1}),
                accepted: false,
                rejected_reason: Some("No".into()),
            },
        );
        state
            .removed_choices
            .extend([NodeId("removed-a".into()), NodeId("removed-b".into())]);
        state
    }

    #[test]
    fn canonical_hash_vectors_and_version_rejection() {
        // Independently calculated using Python hashlib; freeze the byte contract.
        let (hash, bytes) = encode(Object::MapLeaf(vec![])).unwrap();
        assert_eq!(
            bytes,
            br#"{"object":{"kind":"map_leaf","value":[]},"version":1}"#
        );
        assert_eq!(
            hash.as_str(),
            "efaaafd1e1dec0e21809e614a073b33740339414045d8070c7820386f73e981b"
        );
        let (hash, bytes) = encode(Object::Comment(CommentState {
            node_id: None,
            body: "বাংলা 🦀\n".into(),
        }))
        .unwrap();
        assert_eq!(
            hash.as_str(),
            "d083d9ebaa357c47fd8875b5eaaaebaac345fcb14621fde6c42ea5d7b96eb5bd"
        );
        assert!(matches!(decode(&hash, &bytes), Ok(Object::Comment(_))));
        let future = br#"{"object":{"kind":"map_leaf","value":[]},"version":2}"#;
        assert!(matches!(
            decode(&address(future), future),
            Err(Error::Invalid("unsupported object version"))
        ));
        let noncanonical = b"{ \"object\":{\"kind\":\"map_leaf\",\"value\":[]},\"version\":1}";
        assert!(decode(&address(noncanonical), noncanonical).is_err());
        for bad in ["A".repeat(64), "f".repeat(63), "g".repeat(64)] {
            assert!(Address::try_from(bad).is_err());
        }
    }

    #[tokio::test]
    async fn full_state_round_trip_reuses_objects_and_is_document_scoped() {
        let doc = DocumentId(uuid::Uuid::new_v4());
        let mut store = MemoryStore::default();
        let mut state = fixture(1500);
        let (root, first) = write_state(&mut store, doc, &state, Limits::default())
            .await
            .unwrap();
        assert_eq!(
            read_state(&mut store, doc, &root, Limits::default())
                .await
                .unwrap(),
            state
        );
        let (again, retry) = write_state(&mut store, doc, &state, Limits::default())
            .await
            .unwrap();
        assert_eq!(root, again);
        assert_eq!(retry.new_bytes, 0);
        assert_eq!(retry.new_objects, 0);
        // One content edit changes one value and its radix path, not other values.
        state.nodes.values_mut().nth(700).unwrap().current_fields["text"] = json!("Edited");
        let (edited, delta) = write_state(&mut store, doc, &state, Limits::default())
            .await
            .unwrap();
        assert_ne!(root, edited);
        assert!(delta.new_objects <= 6, "{delta:?}");
        assert!(delta.new_bytes < first.new_bytes / 20, "{delta:?}");
        assert_eq!(
            read_state(&mut store, doc, &edited, Limits::default())
                .await
                .unwrap(),
            state
        );
        // Insertion also changes only a bounded path; no fixed-size array suffix rewrite.
        let mut node = state.nodes.values().next().unwrap().clone();
        node.id = NodeId("an-inserted-id".into());
        state.nodes.insert(node.id.clone(), node);
        let (_, insert) = write_state(&mut store, doc, &state, Limits::default())
            .await
            .unwrap();
        assert!(insert.new_objects <= 20, "{insert:?}");
        let other = DocumentId(uuid::Uuid::new_v4());
        assert!(matches!(
            read_state(&mut store, other, &root, Limits::default()).await,
            Err(Error::Missing(_))
        ));
        let (_, other_stats) = write_state(&mut store, other, &fixture(1500), Limits::default())
            .await
            .unwrap();
        assert_eq!(first.new_bytes, other_stats.new_bytes);
    }

    #[tokio::test]
    async fn every_state_component_changes_the_commitment() {
        let doc = DocumentId(uuid::Uuid::new_v4());
        let mut store = MemoryStore::default();
        let state = fixture(4);
        let (original, _) = write_state(&mut store, doc, &state, Limits::default())
            .await
            .unwrap();
        for change in 0..5 {
            let mut altered = state.clone();
            match change {
                0 => altered.comments.reverse(),
                1 => altered.suggestions.get_mut("suggestion").unwrap().accepted = true,
                2 => altered.removed_choices.clear(),
                3 => altered.nodes.values_mut().next().unwrap().deleted = false,
                _ => altered.nodes.values_mut().next().unwrap().pos = "moved".into(),
            }
            let (root, _) = write_state(&mut store, doc, &altered, Limits::default())
                .await
                .unwrap();
            assert_ne!(original, root);
            assert_eq!(
                read_state(&mut store, doc, &root, Limits::default())
                    .await
                    .unwrap(),
                altered
            );
        }
    }

    #[tokio::test]
    async fn missing_corrupt_and_wrongly_typed_references_fail_closed() {
        let doc = DocumentId(uuid::Uuid::new_v4());
        let mut store = MemoryStore::default();
        let state = fixture(3);
        let (root, _) = write_state(&mut store, doc, &state, Limits::default())
            .await
            .unwrap();
        let node_key = store
            .objects
            .iter()
            .find(|(_, bytes)| matches!(decode(&address(bytes), bytes), Ok(Object::Node(_))))
            .unwrap()
            .0
            .clone();
        store.objects.remove(&node_key).unwrap();
        assert!(matches!(
            read_state(&mut store, doc, &root, Limits::default()).await,
            Err(Error::Missing(_))
        ));
        store.objects.insert(node_key, b"corrupt".to_vec());
        assert!(matches!(
            read_state(&mut store, doc, &root, Limits::default()).await,
            Err(Error::HashMismatch)
        ));
        let (leaf, leaf_bytes) = encode(Object::MapLeaf(vec![])).unwrap();
        store.put(doc, &leaf, &leaf_bytes).await.unwrap();
        let (bad_root, bytes) = encode(Object::State(StateRoots {
            nodes: root.clone(),
            comments: leaf.clone(),
            suggestions: leaf.clone(),
            removed_choices: leaf,
        }))
        .unwrap();
        store.put(doc, &bad_root, &bytes).await.unwrap();
        assert!(matches!(
            read_state(&mut store, doc, &bad_root, Limits::default()).await,
            Err(Error::Invalid("expected bounded map"))
        ));
        assert!(store.put(doc, &root, &bytes).await.is_err());
        assert!(!bytes.is_empty());
    }

    #[tokio::test]
    async fn budgets_and_oversized_objects_are_rejected() {
        let doc = DocumentId(uuid::Uuid::new_v4());
        let mut store = MemoryStore::default();
        let mut state = fixture(1);
        let (root, _) = write_state(&mut store, doc, &state, Limits::default())
            .await
            .unwrap();
        for limit in [
            Limits {
                max_objects: 1,
                max_bytes: usize::MAX,
            },
            Limits {
                max_objects: usize::MAX,
                max_bytes: 1,
            },
        ] {
            assert!(matches!(
                write_state(&mut store, doc, &state, limit).await,
                Err(Error::Limit(_))
            ));
            assert!(matches!(
                read_state(&mut store, doc, &root, limit).await,
                Err(Error::Limit(_))
            ));
        }
        state.nodes.values_mut().next().unwrap().current_fields =
            json!({"text": "x".repeat(MAX_OBJECT_BYTES)});
        assert!(matches!(
            write_state(&mut store, doc, &state, Limits::default()).await,
            Err(Error::Limit("object bytes"))
        ));
    }

    #[tokio::test]
    async fn malformed_map_routes_and_comment_ordinals_are_rejected() {
        let doc = DocumentId(uuid::Uuid::new_v4());
        let mut store = MemoryStore::default();
        let (empty, bytes) = encode(Object::MapLeaf(vec![])).unwrap();
        store.put(doc, &empty, &bytes).await.unwrap();
        let (comment, bytes) = encode(Object::Comment(CommentState {
            node_id: None,
            body: "A".into(),
        }))
        .unwrap();
        store.put(doc, &comment, &bytes).await.unwrap();
        let key = "0000000000000001".to_owned(); // ordinal zero is missing
        let entry = Entry {
            key: key.clone(),
            address: comment,
        };
        let (leaf, bytes) = encode(Object::MapLeaf(vec![entry.clone()])).unwrap();
        store.put(doc, &leaf, &bytes).await.unwrap();
        let roots = |comments| {
            Object::State(StateRoots {
                nodes: empty.clone(),
                comments,
                suggestions: empty.clone(),
                removed_choices: empty.clone(),
            })
        };
        let (root, bytes) = encode(roots(leaf.clone())).unwrap();
        store.put(doc, &root, &bytes).await.unwrap();
        assert!(matches!(
            read_state(&mut store, doc, &root, Limits::default()).await,
            Err(Error::Invalid("comment type or ordinal"))
        ));
        let wrong_slot = if route(&key).starts_with('0') {
            "1"
        } else {
            "0"
        };
        let (branch, bytes) = encode(Object::MapBranch(BTreeMap::from([(
            wrong_slot.to_owned(),
            leaf,
        )])))
        .unwrap();
        store.put(doc, &branch, &bytes).await.unwrap();
        let (root, bytes) = encode(roots(branch)).unwrap();
        store.put(doc, &root, &bytes).await.unwrap();
        assert!(matches!(
            read_state(&mut store, doc, &root, Limits::default()).await,
            Err(Error::Invalid("map key route or duplicate"))
        ));
        let (duplicate, bytes) = encode(Object::MapLeaf(vec![entry.clone(), entry])).unwrap();
        assert!(matches!(
            store.put(doc, &duplicate, &bytes).await,
            Err(Error::Invalid("map leaf order or size"))
        ));
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(32))]
        #[test]
        fn arbitrary_text_and_unordered_removals_round_trip(text in ".{0,250}", count in 0usize..90) {
            let runtime = tokio::runtime::Runtime::new().unwrap();
            runtime.block_on(async {
                let doc = DocumentId(uuid::Uuid::new_v4());
                let mut store = MemoryStore::default();
                let mut state = fixture(count);
                state.comments[0].body = text;
                let (root, _) = write_state(&mut store, doc, &state, Limits::default()).await.unwrap();
                assert_eq!(read_state(&mut store, doc, &root, Limits::default()).await.unwrap(), state);
                state.removed_choices = state.removed_choices.iter().cloned().collect();
                let (again, _) = write_state(&mut store, doc, &state, Limits::default()).await.unwrap();
                assert_eq!(root, again);
            });
        }
    }
}
