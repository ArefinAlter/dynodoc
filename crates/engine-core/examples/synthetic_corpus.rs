//! Writes the synthetic benchmark corpus for other tools: `<shape>-events.jsonl`
//! (one operation per line) and `<shape>-state.json` (the replayed state) for a
//! report, a spreadsheet and a questionnaire.
//! cargo run --release -p engine-core --example synthetic_corpus -- OUT_DIR
#[path = "support/corpus.rs"]
mod corpus;

use std::{fs, io::Write, path::PathBuf};

fn main() -> anyhow::Result<()> {
    let out = PathBuf::from(
        std::env::args()
            .nth(1)
            .ok_or_else(|| anyhow::anyhow!("usage: synthetic_corpus OUT_DIR"))?,
    );
    fs::create_dir_all(&out)?;
    for (name, corpus) in [
        ("report", corpus::report(11, 8000).0),
        ("spreadsheet", corpus::sheet(12, 3000)),
        ("questionnaire", corpus::questionnaire(13, 500)),
    ] {
        let mut events = fs::File::create(out.join(format!("{name}-events.jsonl")))?;
        for op in &corpus.events {
            writeln!(events, "{}", serde_json::to_string(op)?)?;
        }
        fs::write(
            out.join(format!("{name}-state.json")),
            serde_json::to_vec(&corpus.state())?,
        )?;
        println!("{name}: {} operations", corpus.events.len());
    }
    Ok(())
}
