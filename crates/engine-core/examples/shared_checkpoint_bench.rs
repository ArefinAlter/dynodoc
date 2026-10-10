//! Synthetic storage growth, not a native-file or concurrent database benchmark.
//! cargo run --release -p engine-core --example shared_checkpoint_bench

use engine_core::{
    materializer::{DocumentState, MaterializedNode},
    shared_checkpoint::{read_state, write_state, Limits, MemoryStore},
};
use engine_shared::{DocumentId, NodeId};
use serde_json::json;
use std::time::Instant;

#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    let mut state = DocumentState::default();
    for index in 0..8000 {
        let id = NodeId(format!("{index:026}"));
        state.nodes.insert(id.clone(), MaterializedNode {
            id, parent_id: None, node_type: "paragraph".into(), pos: format!("p{index:08}"),
            current_fields: json!({"content":{"text":"Synthetic document text. ".repeat(24),"bold":false}}),
            var_name: None, deleted: false,
        });
    }
    let document = DocumentId(uuid::Uuid::nil());
    let mut store = MemoryStore::default();
    let baseline = serde_json::to_vec(&state)?.len();
    let start = Instant::now();
    let (mut root, initial) = write_state(&mut store, document, &state, Limits::default()).await?;
    let initial_ms = start.elapsed().as_secs_f64() * 1000.0;
    let mut legacy_total = baseline;
    let mut new_bytes = Vec::new();
    let mut write_ms = Vec::new();
    for index in 0..20 {
        let node = state.nodes.values_mut().nth((index * 397) % 8000).unwrap();
        node.current_fields["revision"] = json!(index + 1);
        legacy_total += serde_json::to_vec(&state)?.len();
        let start = Instant::now();
        let (next, stats) = write_state(&mut store, document, &state, Limits::default()).await?;
        write_ms.push(start.elapsed().as_secs_f64() * 1000.0);
        new_bytes.push(stats.new_bytes);
        root = next;
    }
    let start = Instant::now();
    let restored = read_state(&mut store, document, &root, Limits::default()).await?;
    let read_ms = start.elapsed().as_secs_f64() * 1000.0;
    assert_eq!(restored, state);
    write_ms.sort_by(f64::total_cmp);
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "blocks":8000, "checkpoints":21, "baseline_state_json_bytes":baseline,
            "legacy_full_state_bytes":legacy_total, "unique_object_bytes":store.stored_bytes(),
            "unique_objects":store.object_count(), "first_checkpoint_bytes":initial.new_bytes,
            "edit_new_bytes_min":new_bytes.iter().min(), "edit_new_bytes_max":new_bytes.iter().max(),
            "initial_write_ms":initial_ms, "edit_write_median_ms":write_ms[write_ms.len()/2], "final_read_ms":read_ms,
            "scope":"in-memory synthetic canonical bytes; excludes DB/WAL, native assets and event storage"
        }))?
    );
    Ok(())
}
