//! Reproducible synthetic benchmark for copy detection. It times fingerprinting and
//! comparison in memory; database lookups and uploads are excluded.
use engine_core::similarity::{self, Fingerprint};
use serde_json::json;
use std::time::Instant;

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
/// Deterministic pseudo-random sentences over a 5,000-word vocabulary (xorshift64),
/// so shingles are about as distinct as in real prose.
fn paragraph(i: usize) -> String {
    const STEMS: [&str; 10] = [
        "house", "survey", "water", "school", "income", "clinic", "market", "harvest", "loan",
        "labour",
    ];
    let mut state = (i as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1;
    (0..24)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            let word = (state % 5000) as usize;
            format!("{}{}", STEMS[word % STEMS.len()], word / STEMS.len())
        })
        .collect::<Vec<_>>()
        .join(" ")
}
fn main() {
    let blocks: Vec<String> = (0..8000).map(paragraph).collect();
    let words: usize = blocks.iter().map(|b| b.split(' ').count()).sum();
    let fingerprint_ms = median(|| {
        std::hint::black_box(Fingerprint::from_blocks(blocks.iter().map(String::as_str)));
    });
    let original = Fingerprint::from_blocks(blocks.iter().map(String::as_str));
    // A renamed copy with every tenth paragraph rewritten and 5% new material.
    let mut edited = blocks.clone();
    for (i, block) in edited.iter_mut().enumerate().filter(|(i, _)| i % 10 == 0) {
        *block = paragraph(100_000 + i);
    }
    edited.extend((0..400).map(|i| paragraph(200_000 + i)));
    let copy = Fingerprint::from_blocks(edited.iter().map(String::as_str));
    let compare_ms = median(|| {
        std::hint::black_box(similarity::compare(&copy, &original, false));
    });
    let comparison = similarity::compare(&copy, &original, false);
    let unrelated = Fingerprint::from_blocks(
        (0..8000)
            .map(|i| paragraph(300_000 + i))
            .collect::<Vec<_>>()
            .iter()
            .map(String::as_str),
    );
    let shared_bands = copy
        .bands
        .iter()
        .zip(&original.bands)
        .filter(|(a, b)| a == b)
        .count();
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "fixture": "synthetic 8,000 paragraphs of 24 words; renamed copy with 10% rewritten and 5% added",
            "arch": std::env::consts::ARCH,
            "os": std::env::consts::OS,
            "debug_assertions": cfg!(debug_assertions),
            "samples": 9,
            "words": words,
            "shingles": original.shingle_count,
            "stored_bytes": { "signature": original.signature_bytes().len(), "bands": original.bands.len() * 8, "block_hashes": original.blocks.len() * 8 },
            "median_ms": { "fingerprint_8000_blocks": fingerprint_ms, "compare_pair": compare_ms },
            "copy": comparison,
            "copy_shared_bands": shared_bands,
            "unrelated": similarity::compare(&unrelated, &original, false),
            "excludes": ["file parsing", "upload/network", "database lookups", "notification writes"]
        }))
        .unwrap()
    );
}
