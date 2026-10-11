//! Synthetic repeated-byte streaming probe, not native-file or service acceptance.
use engine_core::shared_checkpoint::{content, Limits, MemoryStore};
use engine_shared::DocumentId;
use tokio::io::AsyncReadExt;

#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    let mib: u64 = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "100".into())
        .parse()?;
    anyhow::ensure!((1..=500).contains(&mib), "select 1-500 MiB");
    let size = mib * 1024 * 1024;
    let mut input = tokio::io::repeat(0x5a).take(size);
    let mut store = MemoryStore::default();
    let document = DocumentId(uuid::Uuid::from_u128(1));
    let start = std::time::Instant::now();
    let (root, stats) = content::write(&mut store, document, &mut input, Limits::default()).await?;
    let write_ms = start.elapsed().as_secs_f64() * 1000.0;
    let start = std::time::Instant::now();
    let read_bytes = content::read_into(
        &mut store,
        document,
        &root,
        &mut tokio::io::sink(),
        Limits::default(),
    )
    .await?;
    anyhow::ensure!(size == read_bytes, "incorrect output length");
    println!(
        "{}",
        serde_json::json!({
            "workload": "repeated byte 0x5a; generated input; sink output; MemoryStore",
            "input_bytes": size, "read_bytes": read_bytes, "root": root.as_str(),
            "visited_objects": stats.visited_objects, "encoded_bytes": stats.encoded_bytes,
            "new_objects": stats.new_objects, "stored_bytes": store.stored_bytes(),
            "write_ms": write_ms, "read_ms": start.elapsed().as_secs_f64() * 1000.0,
        })
    );
    Ok(())
}
