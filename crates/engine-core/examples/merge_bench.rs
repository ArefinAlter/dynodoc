//! Reproducible synthetic benchmark for merging a draft: an 8,000-paragraph report in
//! which the team and the draft each reword 1% of the blocks (different blocks). It
//! times `workspace_merge::merge`, including the diff that produces the operations;
//! storage and the API are excluded.
//! cargo run --release -p engine-core --example merge_bench
#[path = "support/corpus.rs"]
mod corpus;

use engine_core::workspace_merge;
use serde_json::json;
use std::{collections::BTreeMap, time::Instant};

fn median(mut f: impl FnMut()) -> f64 {
    f();
    let mut runs = Vec::new();
    for _ in 0..9 {
        let start = Instant::now();
        f();
        runs.push(start.elapsed().as_secs_f64() * 1000.0);
    }
    runs.sort_by(f64::total_cmp);
    runs[4]
}

fn main() {
    let blocks = 8000;
    let (corpus, ids) = corpus::report(11, blocks);
    let base = corpus.state();
    let (mut team, mut draft) = (base.clone(), base.clone());
    for (side, offset) in [(&mut team, 0), (&mut draft, 50)] {
        for i in 0..blocks / 100 {
            let fields = &mut side
                .nodes
                .get_mut(&ids[i * 100 + offset])
                .unwrap()
                .current_fields;
            let text = fields["label"]
                .as_str()
                .unwrap()
                .replacen(' ', " revised ", 1);
            fields["content"] = corpus::paragraph(&text);
            fields["label"] = json!(text);
        }
    }
    let plan = workspace_merge::merge(&base, &team, &draft, &BTreeMap::new());
    assert!(plan.conflicts.is_empty());
    let merge_ms = median(|| {
        std::hint::black_box(workspace_merge::merge(
            &base,
            &team,
            &draft,
            &BTreeMap::new(),
        ));
    });
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "date": chrono::Utc::now().date_naive().to_string(),
            "platform": format!("{} {}; release build; one thread", std::env::consts::OS, std::env::consts::ARCH),
            "measurement": "Median of 9 in-memory workspace_merge::merge calls (after one warm-up), including the diff to operations",
            "limitations": "Synthetic text; no storage, API, concurrency or memory measurement.",
            "results": {"blocks": blocks, "edited_blocks_per_side": blocks / 100, "operations": plan.ops.len(), "merge_ms": merge_ms},
        }))
        .unwrap()
    );
}
