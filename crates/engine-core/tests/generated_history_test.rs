//! Generated histories: every event type appears, and folding a snapshot plus the
//! remaining events equals folding them all.
#[path = "support/gen.rs"]
mod gen;

use engine_core::materializer::Materializer;
use engine_shared::{DocumentId, Snapshot, SnapshotId};
use proptest::prelude::*;
use std::collections::BTreeSet;

#[test]
fn generated_histories_cover_every_event_type() {
    let kinds: BTreeSet<String> = gen::history(7, 400)
        .into_iter()
        .map(|e| e.event_type)
        .collect();
    assert_eq!(kinds.len(), gen::KINDS, "{kinds:?}");
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    #[test]
    fn fold_matches_snapshot_plus_tail(seed in 1..u64::MAX, count in 1usize..40) {
        let events = gen::history(seed, count);
        let full = Materializer::fold(&events).unwrap();
        for cut in 0..=events.len() {
            let base = Materializer::fold(&events[..cut]).unwrap();
            let snapshot = Snapshot {
                id: SnapshotId(uuid::Uuid::nil()),
                document_id: DocumentId(uuid::Uuid::nil()),
                through_seq: cut as i64,
                state: serde_json::to_value(&base).unwrap(),
                merkle_root: vec![0; 32],
                event_chain_hash: vec![0; 32],
                created_at: chrono::Utc::now(),
            };
            let from_snapshot = Materializer::from_snapshot(&snapshot, &events[cut..]).unwrap();
            prop_assert_eq!(from_snapshot, full.clone());
        }
    }
}
