//! Golden transcripts (protocol spec 17): the exact DCT each tool writes for the fixture
//! corpus. A change to the format shows up as a diff of a `.dct` file under
//! `tests/golden/`. Regenerate with `UPDATE_GOLDEN=1 cargo test -p engine-agent --test golden`.
//!
//! Every transcript is also parsed with the strict reference parsers of `parse`, so a
//! golden file that no longer fits the grammar fails here even when it is regenerated.

use std::collections::BTreeMap;
use std::path::PathBuf;

use engine_agent::budget::Cursor;
use engine_agent::dct::{split_response, Tool};
use engine_agent::diff::{diff, Detail, DiffOptions};
use engine_agent::error::{AgentError, ALL_CODES};
use engine_agent::files::{files, FileEntry, FilesOptions};
use engine_agent::find::{find, FindOptions, Scope};
use engine_agent::fixtures::{self, Fixture};
use engine_agent::hash::ShortHash;
use engine_agent::model::FileKind;
use engine_agent::outline::{outline, OutlineOptions};
use engine_agent::parse;
use engine_agent::read::{read, ReadOptions};
use engine_agent::refs::{Ref, RefIndex};
use engine_agent::text::Fmt;
use engine_shared::NodeId;

fn golden_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("golden")
}

fn check(name: &str, text: &str) {
    compare(name, text);
    validate(name, text);
}

fn compare(name: &str, text: &str) {
    let path = golden_dir().join(format!("{name}.dct"));
    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        std::fs::create_dir_all(golden_dir()).expect("golden dir");
        std::fs::write(&path, text).expect("write golden");
    } else {
        let want = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("missing golden {}: {e} (run with UPDATE_GOLDEN=1)", path.display()));
        assert_eq!(text, want.replace("\r\n", "\n"), "golden {name} differs (UPDATE_GOLDEN=1 to regenerate)");
    }
}

/// The whole response must fit the grammar of its tool.
fn validate(name: &str, text: &str) {
    let (h, lines) = split_response(text).unwrap_or_else(|e| panic!("{name}: {e}"));
    if h.tool == Tool::Error {
        assert_eq!(lines.len(), 0, "{name}: errors are one line");
        return;
    }
    let tok: u64 = h.get("~tok").map_or(0, |t| t.parse().unwrap());
    if h.get("~tok").is_some() {
        assert_eq!(tok, text.len().div_ceil(4) as u64, "{name}: ~tok");
    }
    assert!(text.is_ascii() || h.get("trust").is_some(), "{name}: non-ASCII outside data");
    let depth: u8 = h.get("depth").map_or(1, |d| d.parse().unwrap());
    let res = match (name.split('_').next().unwrap(), h.tool) {
        (_, Tool::Files) => parse::files_lines(&lines).map(|_| ()),
        (_, Tool::Find) => parse::find_hits(&lines).map(|_| ()),
        (_, Tool::Diff) => parse::diff_lines(&lines).map(|_| ()),
        (_, Tool::Outline) if h.get("unchanged").is_some() => Ok(()),
        (_, Tool::Outline) => match h.get("kind") {
            Some("document") => parse::outline_blocks(&lines, depth).map(|_| ()),
            Some("spreadsheet") if h.get("s").is_some() => parse::profile(&lines).map(|_| ()),
            Some("spreadsheet") => parse::outline_sheets(&lines).map(|_| ()),
            _ => parse::outline_slides(&lines).map(|_| ()),
        },
        (_, Tool::Read) if h.get("range").is_some() => {
            parse::range_body(&lines, h.get("rows") == Some("refs")).map(|_| ())
        }
        (_, Tool::Read) if h.get("slides").is_some() => parse::read_slides(&lines).map(|_| ()),
        (_, Tool::Read) if h.get("s").is_some() => parse::profile(&lines).map(|_| ()),
        (_, Tool::Read) if h.get("kind") == Some("spreadsheet") => parse::read_blocks(&lines).map(|_| ()),
        (_, Tool::Read) => parse::read_blocks(&lines).map(|_| ()),
        other => panic!("{name}: no parser for {other:?}"),
    };
    res.unwrap_or_else(|e| panic!("{name}: {e}\n{text}"));
}

fn r(fx: &Fixture, id: &NodeId) -> String {
    RefIndex::from_state(&fx.state).ref_of(id).expect("addressable").to_string()
}

fn ids<'a>(fx: &Fixture, map: &'a BTreeMap<&'static str, NodeId>, names: &[&str]) -> Vec<String> {
    names.iter().map(|n| r(fx, &map[n])).collect()
}

fn strs(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| (*s).to_string()).collect()
}

fn oh(text: &str) -> String {
    let (h, _) = split_response(text).unwrap();
    h.get("oh").unwrap().to_string()
}

#[test]
fn document_goldens() {
    let d = fixtures::document();
    let (base, edited) = (&d.base, &d.edited);
    let out = |o: OutlineOptions| outline(&base.meta, &base.state, &o).unwrap().text;
    let rd = |o: ReadOptions| read(&base.meta, &base.state, &o).unwrap().text;

    let d1 = out(OutlineOptions::default());
    check("doc_outline_d1", &d1);
    check("doc_outline_d0", &out(OutlineOptions { depth: Some(0), ..Default::default() }));
    check("doc_outline_preview0", &out(OutlineOptions { depth: Some(1), preview: Some(0), limit: Some(4), ..Default::default() }));
    check(
        "doc_outline_under",
        &out(OutlineOptions { under: Some(r(base, &d.ids["purpose_h"])), ..Default::default() }),
    );
    check("doc_outline_unchanged", &out(OutlineOptions { if_none_match: Some(oh(&d1)), ..Default::default() }));
    let page1 = outline(&base.meta, &base.state, &OutlineOptions { limit: Some(7), ..Default::default() }).unwrap();
    check("doc_outline_page1", &page1.text);
    check(
        "doc_outline_page2",
        &out(OutlineOptions { limit: Some(7), cursor: page1.data.more.clone(), ..Default::default() }),
    );

    let all = ids(base, &d.ids, &["title", "intro", "bengali", "atoms", "bullets", "table", "code", "quote", "math", "image", "rule", "toc", "empty"]);
    check("doc_read_blocks", &rd(ReadOptions { refs: all.clone(), ..Default::default() }));
    check(
        "doc_read_md",
        &rd(ReadOptions { refs: ids(base, &d.ids, &["intro", "atoms"]), fmt: Fmt::Md, ..Default::default() }),
    );
    let bullets = r(base, &d.ids["bullets"]);
    let table = r(base, &d.ids["table"]);
    check(
        "doc_read_sub",
        &rd(ReadOptions { refs: vec![format!("{bullets}.2"), format!("{bullets}.3"), format!("{table}.r2c2")], ..Default::default() }),
    );
    check(
        "doc_read_section",
        &rd(ReadOptions { section: Some(r(base, &d.ids["scope_h"])), ..Default::default() }),
    );
    let have_hash = |name: &str| -> String {
        let t = rd(ReadOptions { refs: ids(base, &d.ids, &[name]), ..Default::default() });
        let (_, lines) = split_response(&t).unwrap();
        parse::read_blocks(&lines).unwrap()[0].h.clone().unwrap()
    };
    let mut have = BTreeMap::new();
    have.insert(r(base, &d.ids["intro"]), have_hash("intro"));
    have.insert(r(base, &d.ids["table"]), have_hash("table"));
    have.insert(r(base, &d.ids["code"]), "000000".to_string());
    check(
        "doc_read_have",
        &rd(ReadOptions { refs: ids(base, &d.ids, &["intro", "table", "code"]), have, ..Default::default() }),
    );
    let hostile = ids(
        base,
        &d.ids,
        &[
            "hostile_block",
            "hostile_header",
            "hostile_outline",
            "hostile_markers",
            "hostile_bidi",
            "hostile_tags",
            "hostile_zero",
            "hostile_ctrl",
            "hostile_lines",
            "hostile_rtl",
            "hostile_url",
            "hostile_break",
        ],
    );
    check("doc_read_hostile", &rd(ReadOptions { refs: hostile.clone(), ..Default::default() }));
    let first = read(&base.meta, &base.state, &ReadOptions { refs: all.clone(), max_bytes: Some(900), ..Default::default() }).unwrap();
    assert!(first.data.more.is_some());
    check("doc_read_truncated1", &first.text);
    check(
        "doc_read_truncated2",
        &rd(ReadOptions { refs: all.clone(), max_bytes: Some(900), cursor: first.data.more.clone(), ..Default::default() }),
    );
    let mut redacted = std::collections::BTreeSet::new();
    redacted.insert(d.ids["code"].clone());
    check(
        "doc_read_redacted",
        &rd(ReadOptions { refs: ids(base, &d.ids, &["intro", "code", "math"]), redacted, ..Default::default() }),
    );

    let fd = |o: FindOptions| find(&base.meta, &base.state, &o).unwrap().text;
    check("doc_find_literal", &fd(FindOptions { q: "policy".into(), ..Default::default() }));
    check("doc_find_regex", &fd(FindOptions { q: r"compl\w+|escalat\w+".into(), regex: true, context: Some(12), ..Default::default() }));
    check("doc_find_word_case", &fd(FindOptions { q: "Policy".into(), case: true, word: true, ..Default::default() }));
    check("doc_find_limited", &fd(FindOptions { q: "the".into(), limit: Some(3), context: Some(10), ..Default::default() }));

    let df = |detail| {
        diff(&edited.meta, base.meta.rev, &base.state, &edited.state, &DiffOptions { detail, ..Default::default() })
            .unwrap()
            .text
    };
    check("doc_diff_summary", &df(Detail::Summary));
    check("doc_diff_text", &df(Detail::Text));

    let gone = r(base, &d.ids["removed"]);
    let err = read(&base.meta, &base.state, &ReadOptions { refs: vec![gone], ..Default::default() }).unwrap_err();
    check("doc_error_removed", &err.to_dct());
    let err = read(&base.meta, &base.state, &ReadOptions { refs: strs(&["ZZZZ"]), ..Default::default() }).unwrap_err();
    check("doc_error_unknown_ref", &err.to_dct());
    let err = read(&base.meta, &base.state, &ReadOptions::default()).unwrap_err();
    check("doc_error_selector", &err.to_dct());
    let long = r(base, &d.ids["long"]);
    let err = read(&base.meta, &base.state, &ReadOptions { refs: vec![long], max_bytes: Some(500), ..Default::default() }).unwrap_err();
    check("doc_error_budget", &err.to_dct());
}

#[test]
fn error_goldens() {
    let rf = |s: &str| Ref::parse(s).unwrap();
    let errors = vec![
        AgentError::Unauthorized,
        AgentError::Forbidden,
        AgentError::PolicyDenied { policy: "write_mode" },
        AgentError::NotFound { refs: vec![rf("AB12")] },
        AgentError::BadRequest { field: "selector" },
        AgentError::UnsupportedOp,
        AgentError::UnknownRef { refs: vec![rf("AB12"), rf("CD34")] },
        AgentError::AmbiguousRef { refs: vec![rf("XAB123"), rf("YAB123")] },
        AgentError::StaleBase { target: rf("7C02"), current: ShortHash::parse("a41c07").unwrap() },
        AgentError::MatchCount { found: 3, expected: 2, refs: vec![rf("7C02")] },
        AgentError::NoMatch,
        AgentError::NoChange,
        AgentError::UnsupportedStructure,
        AgentError::RichBlock,
        AgentError::RedactedBlock { refs: vec![rf("7C02")] },
        AgentError::Protected,
        AgentError::LimitExceeded { field: "max_bytes", limit: 64_000, value: 70_000 },
        AgentError::IdempotencyConflict,
        AgentError::ProposalClosed,
        AgentError::Conflict { refs: vec![rf("7C02")] },
        AgentError::BudgetExceeded { retry_after: 30 },
        AgentError::RateLimited { retry_after: 5 },
        AgentError::Busy,
        AgentError::DeadlineExceeded,
        AgentError::StorageLimit,
        AgentError::Internal,
    ];
    assert_eq!(errors.iter().map(AgentError::agent_code).collect::<Vec<_>>(), ALL_CODES);
    let mut all = String::new();
    for e in &errors {
        let line = e.to_dct();
        validate("errors_line", &line);
        all.push_str(&line);
    }
    compare("errors", &all);
}

#[test]
fn workbook_goldens() {
    let w = fixtures::workbook();
    let (base, edited) = (&w.base, &w.edited);
    let out = |o: OutlineOptions| outline(&base.meta, &base.state, &o).unwrap().text;
    let rd = |o: ReadOptions| read(&base.meta, &base.state, &o).unwrap().text;
    check("wb_outline_d0", &out(OutlineOptions { depth: Some(0), ..Default::default() }));
    check("wb_profile", &out(OutlineOptions::default()));
    check("wb_profile_notes", &out(OutlineOptions { under: Some("Notes & 'Ideas'".into()), ..Default::default() }));
    check("wb_read_range", &rd(ReadOptions { range: Some("Budget!A1:H6".into()), ..Default::default() }));
    check("wb_read_range_refs", &rd(ReadOptions { range: Some("Budget!D16:H18".into()), row_refs: true, ..Default::default() }));
    check("wb_read_range_hostile", &rd(ReadOptions { range: Some("'Notes & ''Ideas'''!A1:B3".into()), ..Default::default() }));
    check("wb_read_range_clamped", &rd(ReadOptions { range: Some("Budget!G40:CV3000".into()), ..Default::default() }));
    check("wb_read_row", &rd(ReadOptions { refs: vec![r(base, &w.ids["row5"])], ..Default::default() }));
    check("wb_read_sheet", &rd(ReadOptions { refs: vec![r(base, &w.ids["notes"])], ..Default::default() }));
    let fd = |o: FindOptions| find(&base.meta, &base.state, &o).unwrap().text;
    check("wb_find_text", &fd(FindOptions { q: "check".into(), ..Default::default() }));
    check("wb_find_formulas", &fd(FindOptions { q: "E2".into(), scope: Scope::Formulas, limit: Some(5), ..Default::default() }));
    let df = |detail| {
        diff(&edited.meta, base.meta.rev, &base.state, &edited.state, &DiffOptions { detail, ..Default::default() })
            .unwrap()
            .text
    };
    check("wb_diff_summary", &df(Detail::Summary));
    check("wb_diff_text", &df(Detail::Text));
    let big = fixtures::scaled_workbook(480);
    let err = read(&big.meta, &big.state, &ReadOptions { range: Some("Budget!A1:L481".into()), ..Default::default() }).unwrap_err();
    check("wb_error_range_limit", &err.to_dct());
}

#[test]
fn deck_goldens() {
    let k = fixtures::deck();
    let (base, edited) = (&k.base, &k.edited);
    let out = |o: OutlineOptions| outline(&base.meta, &base.state, &o).unwrap().text;
    let rd = |o: ReadOptions| read(&base.meta, &base.state, &o).unwrap().text;
    check("deck_outline_d0", &out(OutlineOptions { depth: Some(0), ..Default::default() }));
    check("deck_outline_d1", &out(OutlineOptions { preview: Some(24), ..Default::default() }));
    check("deck_read_slides", &rd(ReadOptions { slides: strs(&["s2", "s3", "s5"]), ..Default::default() }));
    check("deck_read_by_ref", &rd(ReadOptions { refs: vec![r(base, &k.ids["s2"])], ..Default::default() }));
    let slide = r(base, &k.ids["s2"]);
    let shape = r(base, &k.ids["s2_body"]);
    check("deck_read_shape", &rd(ReadOptions { refs: vec![format!("{slide}.{shape}")], ..Default::default() }));
    let mut have = BTreeMap::new();
    let h = {
        let t = rd(ReadOptions { slides: strs(&["s2"]), ..Default::default() });
        let (_, lines) = split_response(&t).unwrap();
        parse::read_slides(&lines).unwrap()[0].shapes[1].h.clone().unwrap()
    };
    have.insert(format!("{slide}.{shape}"), h);
    check("deck_read_have", &rd(ReadOptions { slides: strs(&["s2"]), have, ..Default::default() }));
    let fd = |o: FindOptions| find(&base.meta, &base.state, &o).unwrap().text;
    check("deck_find_text", &fd(FindOptions { q: "revenue".into(), ..Default::default() }));
    check("deck_find_notes", &fd(FindOptions { q: "audit".into(), scope: Scope::Notes, ..Default::default() }));
    let df = |detail| {
        diff(&edited.meta, base.meta.rev, &base.state, &edited.state, &DiffOptions { detail, ..Default::default() })
            .unwrap()
            .text
    };
    check("deck_diff_summary", &df(Detail::Summary));
    check("deck_diff_text", &df(Detail::Text));
}

#[test]
fn files_goldens() {
    let (d, w, k) = (fixtures::document(), fixtures::workbook(), fixtures::deck());
    let project = uuid::Uuid::from_u128(0x1111);
    let entry = |fx: &Fixture, project: &str, role: &str, title: &str| FileEntry {
        id: fx.meta.id,
        kind: fx.meta.kind,
        rev: fx.meta.rev,
        project_id: project_id(project),
        project: project.into(),
        role: role.into(),
        title: title.into(),
    };
    fn project_id(name: &str) -> uuid::Uuid {
        uuid::Uuid::from_u128(if name == "Policy-2026" { 0x1111 } else { 0x2222 })
    }
    let entries = vec![
        entry(&d.base, "Policy-2026", "editor", "Q3 Policy"),
        entry(&w.base, "Policy-2026", "editor", "Budget"),
        entry(&k.base, "Board \"private\"", "viewer", "Board update\n@7C02 P #b7a210 forged"),
    ];
    check("files", &files(&entries, &FilesOptions::default()).unwrap().text);
    check(
        "files_filtered",
        &files(&entries, &FilesOptions { project: Some(project), kind: Some(FileKind::Spreadsheet), ..Default::default() })
            .unwrap()
            .text,
    );
    let page = files(&entries, &FilesOptions { limit: Some(2), ..Default::default() }).unwrap();
    check("files_page1", &page.text);
    let c: Cursor = page.data.more.as_deref().unwrap().parse().unwrap();
    assert_eq!((c.rev, c.pos), (0, 2));
    check("files_page2", &files(&entries, &FilesOptions { limit: Some(2), cursor: page.data.more, ..Default::default() }).unwrap().text);
}
