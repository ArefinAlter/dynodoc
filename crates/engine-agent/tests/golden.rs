//! Golden transcripts (protocol spec 17): the exact DCT each tool writes for the fixture
//! corpus. A change to the format shows up as a diff of a `.dct` file under
//! `tests/golden/`. Regenerate with `UPDATE_GOLDEN=1 cargo test -p engine-agent --test golden`
//! (this also removes golden files no call produces any more).
//!
//! The calls live in `engine_agent::corpus`, so the byte benchmark measures the same
//! outputs. Every transcript is parsed with the strict reference parsers of `parse`, so a
//! golden file that no longer fits the grammar fails here even when it is regenerated.

use std::collections::BTreeSet;
use std::path::PathBuf;

use engine_agent::corpus::{error_table, golden_calls};
use engine_agent::dct::{split_response, Tool};
use engine_agent::error::ALL_CODES;
use engine_agent::parse;

fn golden_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("golden")
}

fn updating() -> bool {
    std::env::var_os("UPDATE_GOLDEN").is_some()
}

fn compare(name: &str, text: &str) {
    let path = golden_dir().join(format!("{name}.dct"));
    if updating() {
        std::fs::create_dir_all(golden_dir()).expect("golden dir");
        std::fs::write(&path, text).expect("write golden");
    } else {
        let want = std::fs::read_to_string(&path).unwrap_or_else(|e| {
            panic!(
                "missing golden {}: {e} (run with UPDATE_GOLDEN=1)",
                path.display()
            )
        });
        assert_eq!(
            text,
            want.replace("\r\n", "\n"),
            "golden {name} differs (UPDATE_GOLDEN=1 to regenerate)"
        );
    }
}

/// The whole response must fit the grammar of its tool.
fn validate(name: &str, text: &str) {
    let (h, lines) = split_response(text).unwrap_or_else(|e| panic!("{name}: {e}"));
    if h.tool == Tool::Error {
        assert_eq!(lines.len(), 0, "{name}: errors are one line");
        return;
    }
    if let Some(tok) = h.get("~tok") {
        assert_eq!(
            tok.parse::<u64>().unwrap(),
            text.len().div_ceil(4) as u64,
            "{name}: ~tok"
        );
    }
    assert!(
        text.is_ascii() || h.get("trust") == Some("data"),
        "{name}: non-ASCII outside data"
    );
    let depth: u8 = h.get("depth").map_or(1, |d| d.parse().unwrap());
    let res = match h.tool {
        Tool::Files => parse::files_lines(&lines).map(|_| ()),
        Tool::Find => parse::find_hits(&lines).map(|_| ()),
        Tool::Diff => parse::diff_lines(&lines).map(|_| ()),
        Tool::Outline if h.get("unchanged").is_some() => Ok(()),
        Tool::Outline => match h.get("kind") {
            Some("document") => parse::outline_blocks(&lines, depth).map(|_| ()),
            Some("spreadsheet") if h.get("s").is_some() => parse::profile(&lines).map(|_| ()),
            Some("spreadsheet") => parse::outline_sheets(&lines).map(|_| ()),
            _ => parse::outline_slides(&lines).map(|_| ()),
        },
        Tool::Read if h.get("range").is_some() => {
            parse::range_body(&lines, h.get("rows") == Some("refs")).map(|_| ())
        }
        Tool::Read if h.get("slides").is_some() => parse::read_slides(&lines).map(|_| ()),
        Tool::Read if h.get("s").is_some() => parse::profile(&lines).map(|_| ()),
        Tool::Read => parse::read_blocks(&lines).map(|_| ()),
        other => panic!("{name}: no parser for {other:?}"),
    };
    res.unwrap_or_else(|e| panic!("{name}: {e}\n{text}"));
}

#[test]
fn golden_transcripts() {
    let calls = golden_calls();
    assert!(calls.len() >= 55, "{} calls", calls.len());
    let mut names = BTreeSet::new();
    for call in &calls {
        assert!(
            names.insert(call.name.clone()),
            "duplicate golden name {}",
            call.name
        );
        compare(&call.name, &call.text);
        if call.name == "errors" {
            for line in call.text.lines() {
                validate("errors", &format!("{line}\n"));
            }
        } else {
            validate(&call.name, &call.text);
        }
        if let Some(mirror) = &call.mirror {
            let v: serde_json::Value = serde_json::from_str(mirror).expect("mirror is JSON");
            assert_eq!(v["protocol"], "dynodoc.agent/1", "{}", call.name);
            assert_eq!(v["tool"], call.tool.as_str(), "{}", call.name);
            assert_eq!(v["trust"], "data");
        }
    }
    // No golden file without a call.
    for entry in std::fs::read_dir(golden_dir()).expect("golden dir") {
        let path = entry.expect("entry").path();
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or_default()
            .to_string();
        if path.extension().and_then(|e| e.to_str()) == Some("dct") && !names.contains(&stem) {
            if updating() {
                std::fs::remove_file(&path).expect("remove stale golden");
            } else {
                panic!("stale golden file {}", path.display());
            }
        }
    }
}

#[test]
fn every_error_code_has_a_transcript() {
    let table = error_table();
    assert_eq!(table.len(), ALL_CODES.len());
    let calls = golden_calls();
    let errors = calls
        .iter()
        .find(|c| c.name == "errors")
        .expect("errors transcript");
    let codes: Vec<String> = errors
        .text
        .lines()
        .map(|l| {
            split_response(&format!("{l}\n"))
                .unwrap()
                .0
                .get("code")
                .unwrap()
                .to_string()
        })
        .collect();
    assert_eq!(codes, ALL_CODES);
}
