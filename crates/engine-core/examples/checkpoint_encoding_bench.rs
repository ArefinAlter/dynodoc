//! Synthetic allocation probe for oversized checkpoint rejection, not total
//! document memory or service capacity. Run `... -- legacy` and `... -- bounded`.
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};
use std::time::Instant;

use engine_core::materializer::{DocumentState, MaterializedNode};
use engine_core::shared_checkpoint::{write_state, Error, Limits, MemoryStore, MAX_OBJECT_BYTES};
use engine_shared::{DocumentId, NodeId};
use serde::Serialize;
use serde_json::{json, Value};

struct Counted;
static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);
fn added(bytes: usize) {
    PEAK.fetch_max(LIVE.fetch_add(bytes, Relaxed) + bytes, Relaxed);
}
// Delegate allocation unchanged to System; count requested bytes only. Allocator
// metadata, stack, mappings and the caller's existing state are not measured.
unsafe impl GlobalAlloc for Counted {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let ptr = System.alloc(layout);
        if !ptr.is_null() {
            added(layout.size());
        }
        ptr
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let ptr = System.alloc_zeroed(layout);
        if !ptr.is_null() {
            added(layout.size());
        }
        ptr
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        System.dealloc(ptr, layout);
        LIVE.fetch_sub(layout.size(), Relaxed);
    }
    unsafe fn realloc(&self, ptr: *mut u8, old: Layout, size: usize) -> *mut u8 {
        let next = System.realloc(ptr, old, size);
        if !next.is_null() {
            if size >= old.size() {
                added(size - old.size());
            } else {
                LIVE.fetch_sub(old.size() - size, Relaxed);
            }
        }
        next
    }
}
#[global_allocator]
static ALLOCATOR: Counted = Counted;

// Frozen previous rejection path: clone node, serde Value tree, unbounded
// canonical bytes, then size check. Never used by the engine's writer.
fn legacy(node: &MaterializedNode) -> usize {
    #[derive(Serialize)]
    struct Envelope {
        version: u8,
        object: Object,
    }
    #[derive(Serialize)]
    #[serde(tag = "kind", content = "value", rename_all = "snake_case")]
    enum Object {
        Node(MaterializedNode),
    }
    fn canonical(value: &Value, out: &mut Vec<u8>) {
        match value {
            Value::Object(map) => {
                out.push(b'{');
                let mut keys: Vec<_> = map.keys().collect();
                keys.sort();
                for (i, key) in keys.into_iter().enumerate() {
                    if i > 0 {
                        out.push(b',');
                    }
                    serde_json::to_writer(&mut *out, &Value::String(key.clone())).unwrap();
                    out.push(b':');
                    canonical(&map[key], out);
                }
                out.push(b'}');
            }
            Value::Array(items) => {
                out.push(b'[');
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        out.push(b',');
                    }
                    canonical(item, out);
                }
                out.push(b']');
            }
            scalar => serde_json::to_writer(out, scalar).unwrap(),
        }
    }
    let value = serde_json::to_value(Envelope {
        version: 1,
        object: Object::Node(node.clone()),
    })
    .unwrap();
    let mut bytes = Vec::new();
    canonical(&value, &mut bytes);
    assert!(bytes.len() > MAX_OBJECT_BYTES);
    bytes.len()
}

fn main() {
    let mode = std::env::args().nth(1).expect("legacy or bounded");
    assert!(mode == "legacy" || mode == "bounded");
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();
    let document = DocumentId(uuid::Uuid::from_u128(1));
    let mut store = MemoryStore::default();
    let mut state = DocumentState::default();
    let id = NodeId("00000000000000000000000001".into());
    state.nodes.insert(
        id.clone(),
        MaterializedNode {
            id,
            parent_id: None,
            node_type: "paragraph".into(),
            pos: "a".into(),
            current_fields: json!({"text":"\0".repeat(8 * MAX_OBJECT_BYTES)}),
            var_name: None,
            deleted: false,
        },
    );
    let baseline = LIVE.load(Relaxed);
    PEAK.store(baseline, Relaxed);
    let start = Instant::now();
    let legacy_encoded = if mode == "legacy" {
        Some(legacy(state.nodes.values().next().unwrap()))
    } else {
        assert!(matches!(
            runtime.block_on(write_state(&mut store, document, &state, Limits::default())),
            Err(Error::Limit("object bytes"))
        ));
        assert_eq!(store.object_count(), 0);
        None
    };
    let elapsed = start.elapsed().as_micros();
    let additional = PEAK.load(Relaxed).saturating_sub(baseline);
    println!(
        "{}",
        json!({"mode":mode,"input_text_bytes":8 * MAX_OBJECT_BYTES,"peak_additional_requested_heap_bytes":additional,"elapsed_us":elapsed,"legacy_encoded_bytes_before_rejection":legacy_encoded})
    );
}
