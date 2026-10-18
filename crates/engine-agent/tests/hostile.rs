//! Escaping and prompt-injection hygiene (protocol spec 6.5, 17 "Escape property";
//! AI-AGENT-SECURITY P1 to P3, AC12, AC13).
//!
//! Document text is hostile: whatever a document holds, every tool's output must parse back
//! with the strict reference parsers to exactly the structure that was written, and the
//! text must come back as the sanitised original. The corpus in `tests/corpus/injection.json`
//! holds known injection strings; the property tests add arbitrary text built from the
//! characters that matter (structure sigils, escapes, quotes, placeholders, bidi and tag
//! characters, controls, line separators).

use engine_agent::dct::{parse_header, split_response, HeaderBuilder, Tool};
use engine_agent::diff::{diff, Detail, DiffOptions};
use engine_agent::escape::{escape, escape_line, sanitize, unescape, Ctx};
use engine_agent::files::{files, FileEntry, FilesOptions};
use engine_agent::find::{find, FindOptions, Scope};
use engine_agent::fixtures::{self, Builder, Fixture};
use engine_agent::model::FileKind;
use engine_agent::outline::{outline, OutlineOptions};
use engine_agent::parse;
use engine_agent::read::{read, ReadOptions};
use engine_agent::refs::RefIndex;
use engine_shared::NodeType;
use proptest::prelude::*;
use proptest::test_runner::RngSeed;
use serde_json::json;

fn config() -> ProptestConfig {
    let cases = std::env::var("PROPTEST_CASES").ok().and_then(|v| v.parse().ok()).unwrap_or(300);
    let mut config = ProptestConfig::with_cases(cases);
    if std::env::var_os("PROPTEST_RNG_SEED").is_none() {
        config.rng_seed = RngSeed::Fixed(0x6473_7463_6f64_65);
    }
    config.failure_persistence = None;
    config
}

fn fragment() -> impl Strategy<Value = String> {
    let sigils = vec![
        "@", "#", "=", "-", " ", "\\", "\n", "\t", "\"", "⟦", "⟧", ".", "~", "+", ">", "[-", "-]", "{+", "+}", "\u{202e}",
        "\u{200b}", "\u{e0041}", "\u{1b}", "\u{2028}", "\r", "\u{0}", "dd/1 read ", "@7C02 P #b7a210", "# ref type hash words preview",
        "notes #000000: ", ".A1B2 title #7f3a21", "r1\t", "- ", "= ", "⟦fn:X⟧", "\\n", "\\@", "é", "ক্ষ", "👨\u{200d}👩", "[x](http://a)",
        "**", "_", "`", "~~", ":", ",", "q=", "trust=data", "more=c_1.2", "A1\told\tnew", "",
    ];
    prop_oneof![
        8 => any::<char>().prop_map(|c| c.to_string()),
        8 => proptest::sample::select(sigils).prop_map(String::from),
        1 => "[ -~]{0,6}".prop_map(String::from),
    ]
}

fn hostile_text() -> impl Strategy<Value = String> {
    proptest::collection::vec(fragment(), 0..8).prop_map(|v| v.concat())
}

fn texts() -> impl Strategy<Value = Vec<String>> {
    proptest::collection::vec(hostile_text(), 4..9)
}

// ---------------------------------------------------------------- primitives

proptest! {
    #![proptest_config(config())]

    #[test]
    fn escape_round_trips_to_the_sanitised_text(x in hostile_text()) {
        let (e, _) = escape(&x);
        prop_assert_eq!(unescape(&e).unwrap(), sanitize(&x));
        prop_assert!(!e.contains('\n') && !e.contains('\t') && !e.chars().any(char::is_control));
        // Sanitising is idempotent.
        prop_assert_eq!(sanitize(&sanitize(&x)), sanitize(&x));
    }

    #[test]
    fn escaped_text_never_forges_a_placeholder_or_an_escape(x in hostile_text()) {
        let (e, _) = escape(&x);
        // Every placeholder bracket in escaped document text is preceded by a backslash
        // that is not itself escaped.
        let mut chars = e.chars().peekable();
        while let Some(c) = chars.next() {
            if c == '\\' {
                chars.next();
            } else {
                prop_assert!(c != '⟦' && c != '⟧', "{e:?}");
            }
        }
    }

    #[test]
    fn a_line_never_starts_like_structure(x in hostile_text()) {
        for ctx in [Ctx::Block, Ctx::Slide, Ctx::Diff] {
            let (l, _) = escape_line(&x, ctx, "");
            prop_assert_eq!(unescape(&l).unwrap(), sanitize(&x));
            let mut it = l.chars();
            let first = it.next();
            let second = it.next();
            let lone_or_space = matches!(second, None | Some(' '));
            prop_assert!(first != Some('@'));
            prop_assert!(!(matches!(first, Some('#' | '=' | '-')) && lone_or_space));
            if ctx == Ctx::Slide { prop_assert!(first != Some('.')); }
            if ctx == Ctx::Diff { prop_assert!(!matches!(first, Some('~' | '+' | '>' | '-'))); }
        }
    }

    #[test]
    fn header_values_cannot_inject_pairs(x in hostile_text(), y in hostile_text()) {
        let text = HeaderBuilder::new(Tool::Find).kv_quoted("q", &x).kv("project", &y).kv("trust", "data").assemble("");
        let (h, lines) = split_response(&text).unwrap();
        prop_assert!(lines.is_empty());
        prop_assert_eq!(h.get("q").unwrap(), sanitize_header(&x));
        prop_assert_eq!(h.get("project").unwrap(), sanitize_header(&y));
        prop_assert_eq!(h.pairs.iter().filter(|(k, _)| k == "trust").count(), 1);
        prop_assert_eq!(h.pairs.len(), 4);
        prop_assert!(parse_header(text.lines().next().unwrap()).is_ok());
    }
}

/// A header value round-trips as the sanitised text (bare tokens hold only safe ASCII, so
/// they are unchanged).
fn sanitize_header(x: &str) -> String {
    sanitize(x)
}

// ---------------------------------------------------------------- documents

fn document_from(texts: &[String]) -> (Builder, Fixture) {
    let mut b = Builder::new(FileKind::Document, "hostile-document", 0xA1);
    let root = b.node(NodeType::Form, None, 0, json!({"label": "Hostile"}));
    let mut pos = 0;
    let mut add = |b: &mut Builder, content: serde_json::Value| {
        pos += 10;
        let label = engine_core::richtext::plain_text(&content);
        b.node(NodeType::Item, Some(&root), pos, json!({"label": label, "content": content}))
    };
    add(&mut b, fixtures::heading(1, &texts[0]));
    for t in texts {
        add(&mut b, fixtures::para(t));
    }
    let item = |t: &str| json!({"type": "listItem", "content": [fixtures::para(t)]});
    add(&mut b, json!({"type": "bulletList", "content": texts.iter().map(|t| item(t)).collect::<Vec<_>>()}));
    let cell = |t: &str| json!({"type": "tableCell", "content": [fixtures::para(t)]});
    let row = |a: &str, c: &str| json!({"type": "tableRow", "content": [cell(a), cell(c)]});
    add(&mut b, json!({"type": "table", "content": [row(&texts[0], &texts[1]), row(&texts[2], &texts[3])]}));
    add(&mut b, json!({"type": "codeBlock", "content": [{"type": "text", "text": texts[1]}]}));
    add(&mut b, json!({"type": "blockquote", "content": [fixtures::para(&texts[2]), fixtures::para(&texts[3])]}));
    let fx = b.snapshot();
    (b, fx)
}

fn lines_of(text: &str) -> (engine_agent::dct::ParsedHeader, Vec<&str>) {
    split_response(text).unwrap_or_else(|e| panic!("{e}\n{text:?}"))
}

fn verify_document(texts: &[String]) {
    let (mut b, base) = document_from(texts);
    let all = |o: OutlineOptions| outline(&base.meta, &base.state, &OutlineOptions { limit: Some(1000), max_bytes: Some(64_000), ..o });

    // outline depth 1 and 0
    for depth in [1u8, 0] {
        let r = all(OutlineOptions { depth: Some(depth), preview: Some(120), ..Default::default() }).unwrap();
        let (h, lines) = lines_of(&r.text);
        assert_eq!(h.tool, Tool::Outline);
        let rows = parse::outline_blocks(&lines, depth).unwrap_or_else(|e| panic!("{e}\n{}", r.text));
        let typed = r.data.blocks.as_ref().unwrap();
        assert_eq!(rows.len(), typed.len());
        for (p, t) in rows.iter().zip(typed) {
            assert_eq!((&p.r, &p.t, &p.h, p.w), (&t.r, &t.t, &t.h, t.w));
            assert_eq!(p.preview, t.p, "preview of {}", p.r);
            assert_eq!(p.under, t.under);
        }
        assert_eq!(h.get("blocks").unwrap(), r.data.count.to_string());
    }

    // read: every block, plain and markdown
    let idx = RefIndex::from_state(&base.state);
    let refs: Vec<String> = base
        .state
        .nodes
        .values()
        .filter(|n| n.current_fields["content"].is_object())
        .map(|n| idx.ref_of(&n.id).unwrap().to_string())
        .collect();
    for fmt in [engine_agent::text::Fmt::Plain, engine_agent::text::Fmt::Md] {
        let r = read(&base.meta, &base.state, &ReadOptions { refs: refs.clone(), fmt, max_bytes: Some(64_000), ..Default::default() }).unwrap();
        let (_, lines) = lines_of(&r.text);
        let blocks = parse::read_blocks(&lines).unwrap_or_else(|e| panic!("{e}\n{}", r.text));
        assert_eq!(blocks.len(), refs.len());
        for (p, t) in blocks.iter().zip(&r.data.items) {
            assert_eq!((&p.label, &p.t, p.h.as_ref()), (&t.r, &t.t, t.h.as_ref()));
            if fmt == engine_agent::text::Fmt::Plain {
                match p.t.as_str() {
                    "P" | "H1" | "CODE" | "QUOTE" => assert_eq!(p.text().unwrap(), t.lines[0], "text of {}", p.label),
                    t if t.starts_with("UL") => {
                        let items: Vec<String> = parse::list_items(p).unwrap().into_iter().map(|(_, s)| s).collect();
                        assert_eq!(items, sanitized(texts));
                    }
                    t if t.starts_with("TBL") => {
                        let cells: Vec<String> = parse::table_rows(p).unwrap().into_iter().flatten().collect();
                        let want = sanitized(&[texts[0].clone(), texts[1].clone(), texts[2].clone(), texts[3].clone()]);
                        assert_eq!(cells, want);
                    }
                    _ => {}
                }
            }
        }
    }

    // find: literal search for a piece of the first non-empty text
    let probe: String = texts.iter().map(|t| sanitize(t)).find(|t| !t.trim().is_empty()).map(|t| t.chars().take(3).collect()).unwrap_or_default();
    if !probe.is_empty() {
        let r = find(&base.meta, &base.state, &FindOptions { q: probe.clone(), limit: Some(100), max_bytes: Some(64_000), ..Default::default() }).unwrap();
        let (h, lines) = lines_of(&r.text);
        let hits = parse::find_hits(&lines).unwrap_or_else(|e| panic!("{e}\n{}", r.text));
        assert_eq!(hits.len(), r.data.hits.len());
        for (p, t) in hits.iter().zip(&r.data.hits) {
            assert_eq!((p.at.as_str(), p.snippet.as_str()), (t.at.as_str(), t.snippet.as_str()));
        }
        assert_eq!(h.get("q").unwrap(), probe);
    }

    // diff after an edit session
    let ids: Vec<_> = base.state.nodes.values().filter(|n| n.current_fields["content"].is_object()).map(|n| n.id.clone()).collect();
    b.rewrite_content(&ids[0], fixtures::para(&texts[texts.len() - 1]));
    let victim = ids[ids.len() - 1].clone();
    b.delete(&victim);
    let root = base.state.nodes.values().find(|n| n.node_type == "form").unwrap().id.clone();
    b.node(NodeType::Item, Some(&root), 5, json!({"label": "x", "content": fixtures::para(&texts[1])}));
    let edited = b.snapshot();
    for detail in [Detail::Summary, Detail::Text] {
        let r = diff(&edited.meta, base.meta.rev, &base.state, &edited.state, &DiffOptions { detail, max_bytes: Some(64_000), ..Default::default() }).unwrap();
        let (_, lines) = lines_of(&r.text);
        let entries = parse::diff_lines(&lines).unwrap_or_else(|e| panic!("{e}\n{}", r.text));
        assert_eq!(entries.len(), r.data.entries.len(), "{}", r.text);
        for (p, t) in entries.iter().zip(&r.data.entries) {
            assert_eq!((p.op.to_string(), &p.r, &p.t), (t.op.clone(), &t.r, &t.t));
            assert_eq!((p.old.as_ref(), p.new.as_ref()), (t.old.as_ref(), t.new.as_ref()));
        }
    }
}

fn sanitized(texts: &[String]) -> Vec<String> {
    texts.iter().map(|t| sanitize(t)).collect()
}

#[test]
fn corpus_document() {
    let corpus = fixtures::injection_corpus();
    for chunk in corpus.chunks(4).filter(|c| c.len() == 4) {
        verify_document(chunk);
    }
    // And each string alone in every position.
    for s in &corpus {
        verify_document(&[s.clone(), s.clone(), s.clone(), s.clone()]);
    }
}

proptest! {
    #![proptest_config(config())]
    #[test]
    fn document_output_parses_back_to_what_was_written(t in texts()) {
        verify_document(&t);
    }
}

// ---------------------------------------------------------------- spreadsheets

fn verify_workbook(texts: &[String]) {
    let mut b = Builder::new(FileKind::Spreadsheet, "hostile-workbook", 0xB2);
    let root = b.node(NodeType::Form, None, 0, json!({"label": "Hostile"}));
    let sheet = b.node(NodeType::Section, Some(&root), 1, json!({"label": texts[0], "kind": "sheet", "sheet_layout": {"frozenRows": 1}}));
    let mut rows = Vec::new();
    for (i, t) in texts.iter().enumerate() {
        let cells = [t.clone(), texts[(i + 1) % texts.len()].clone(), format!("={}", t)];
        let id = b.node(NodeType::Item, Some(&sheet), i as u64, json!({"cell_0": cells[0], "cell_1": cells[1], "cell_2": cells[2]}));
        rows.push((id, cells));
    }
    let base = b.snapshot();

    let r = outline(&base.meta, &base.state, &OutlineOptions { depth: Some(0), ..Default::default() }).unwrap();
    let (_, lines) = lines_of(&r.text);
    let listed = parse::outline_sheets(&lines).unwrap_or_else(|e| panic!("{e}\n{}", r.text));
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].name, sanitize(&texts[0]));

    let r = outline(&base.meta, &base.state, &OutlineOptions::default()).unwrap();
    let (_, lines) = lines_of(&r.text);
    let (name, cols) = parse::profile(&lines).unwrap_or_else(|e| panic!("{e}\n{}", r.text));
    assert_eq!(name, sanitize(&texts[0]));
    assert_eq!(cols.len(), 3);
    assert_eq!(cols[0].header, sanitize(&rows[0].1[0]));

    for row_refs in [false, true] {
        let range = format!("A1:C{}", texts.len());
        let r = read(&base.meta, &base.state, &ReadOptions { range: Some(range), row_refs, max_bytes: Some(64_000), ..Default::default() });
        // A range over 2,000 cells cannot happen here (at most 8 rows).
        let r = r.unwrap();
        let (h, lines) = lines_of(&r.text);
        let body = parse::range_body(&lines, row_refs).unwrap_or_else(|e| panic!("{e}\n{}", r.text));
        assert_eq!(body.rows.len(), texts.len());
        for (p, (_, cells)) in body.rows.iter().zip(&rows) {
            assert_eq!(p.2, sanitized(cells));
        }
        assert!(parse_header(&r.text.lines().next().unwrap().to_string()).is_ok());
        assert_eq!(h.get("calc"), Some("none"));
    }

    let probe: String = sanitize(&texts[0]).chars().take(2).collect();
    if !probe.trim().is_empty() {
        for scope in [Scope::Text, Scope::Formulas] {
            let r = find(&base.meta, &base.state, &FindOptions { q: probe.clone(), scope, limit: Some(100), max_bytes: Some(64_000), ..Default::default() }).unwrap();
            let (_, lines) = lines_of(&r.text);
            let hits = parse::find_hits(&lines).unwrap_or_else(|e| panic!("{e}\n{}", r.text));
            assert_eq!(hits.len(), r.data.hits.len());
        }
    }

    b.edit(&rows[1].0, "cell_0", json!(texts[2]));
    b.delete(&rows[2].0);
    let edited = b.snapshot();
    for detail in [Detail::Summary, Detail::Text] {
        let r = diff(&edited.meta, base.meta.rev, &base.state, &edited.state, &DiffOptions { detail, max_bytes: Some(64_000), ..Default::default() }).unwrap();
        let (_, lines) = lines_of(&r.text);
        let entries = parse::diff_lines(&lines).unwrap_or_else(|e| panic!("{e}\n{}", r.text));
        assert_eq!(entries.len(), r.data.entries.len());
    }
}

#[test]
fn corpus_workbook() {
    let corpus = fixtures::injection_corpus();
    for chunk in corpus.chunks(4).filter(|c| c.len() == 4) {
        verify_workbook(chunk);
    }
    for s in &corpus {
        verify_workbook(&[s.clone(), s.clone(), s.clone(), s.clone()]);
    }
}

proptest! {
    #![proptest_config(config())]
    #[test]
    fn workbook_output_parses_back_to_what_was_written(t in texts()) {
        verify_workbook(&t);
    }
}

// ---------------------------------------------------------------- presentations

fn verify_deck(texts: &[String]) {
    let mut b = Builder::new(FileKind::Presentation, "hostile-deck", 0xC3);
    let root = b.node(NodeType::Form, None, 0, json!({"label": "Hostile"}));
    let slide = b.node(NodeType::Section, Some(&root), 1, json!({"label": texts[0], "kind": "slide", "layout": "title-content", "notes": texts[1]}));
    let mk = |role: Option<&str>, kind: &str, text: &str, bullets: bool| {
        let mut f = json!({"kind": kind, "text": text, "label": text, "style": {"bullets": bullets}});
        if let Some(r) = role {
            f["role"] = json!(r);
        }
        f
    };
    b.node(NodeType::Item, Some(&slide), 1, mk(Some("title"), "text", &texts[0], false));
    let body = b.node(NodeType::Item, Some(&slide), 2, mk(Some("body"), "text", &texts[2], false));
    let bullets = b.node(NodeType::Item, Some(&slide), 3, mk(Some("body"), "text", &texts[3], true));
    b.node(NodeType::Item, Some(&slide), 4, mk(None, "image", "", false));
    let base = b.snapshot();

    for depth in [0u8, 1] {
        let r = outline(&base.meta, &base.state, &OutlineOptions { depth: Some(depth), preview: Some(60), ..Default::default() }).unwrap();
        let (_, lines) = lines_of(&r.text);
        let rows = parse::outline_slides(&lines).unwrap_or_else(|e| panic!("{e}\n{}", r.text));
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].title, sanitize(&texts[0]));
        if depth == 1 {
            assert_eq!(rows[0].shapes.len(), 4);
        }
    }
    let r = read(&base.meta, &base.state, &ReadOptions { slides: vec!["s1".into()], max_bytes: Some(64_000), ..Default::default() }).unwrap();
    let (_, lines) = lines_of(&r.text);
    let slides = parse::read_slides(&lines).unwrap_or_else(|e| panic!("{e}\n{}", r.text));
    assert_eq!(slides.len(), 1);
    let s = &slides[0];
    assert_eq!(s.shapes.len(), 4);
    let join = |sb: &parse::ShapeBody| sb.lines.iter().map(|(_, t)| t.clone()).collect::<Vec<_>>().join("\n");
    assert_eq!(join(&s.shapes[0]), sanitize(&texts[0]));
    assert_eq!(join(&s.shapes[1]), sanitize(&texts[2]));
    assert_eq!(join(&s.shapes[2]), sanitize(&texts[3]));
    assert!(s.shapes[1].lines.iter().all(|(bullet, _)| !bullet));
    assert!(s.shapes[2].lines.iter().all(|(bullet, _)| *bullet));
    match &s.notes {
        Some((_, n)) => assert_eq!(n, &sanitize(&texts[1])),
        None => assert!(texts[1].is_empty()),
    }

    let probe: String = sanitize(&texts[2]).chars().take(2).collect();
    if !probe.trim().is_empty() {
        for scope in [Scope::Text, Scope::Notes] {
            let r = find(&base.meta, &base.state, &FindOptions { q: probe.clone(), scope, limit: Some(100), max_bytes: Some(64_000), ..Default::default() }).unwrap();
            let (_, lines) = lines_of(&r.text);
            parse::find_hits(&lines).unwrap_or_else(|e| panic!("{e}\n{}", r.text));
        }
    }

    b.edit(&body, "text", json!(texts[3]));
    b.edit(&bullets, "text", json!(texts[2]));
    b.edit(&slide, "notes", json!(texts[3]));
    let edited = b.snapshot();
    for detail in [Detail::Summary, Detail::Text] {
        let r = diff(&edited.meta, base.meta.rev, &base.state, &edited.state, &DiffOptions { detail, max_bytes: Some(64_000), ..Default::default() }).unwrap();
        let (_, lines) = lines_of(&r.text);
        let entries = parse::diff_lines(&lines).unwrap_or_else(|e| panic!("{e}\n{}", r.text));
        assert_eq!(entries.len(), r.data.entries.len());
    }
}

#[test]
fn corpus_deck() {
    let corpus = fixtures::injection_corpus();
    for chunk in corpus.chunks(4).filter(|c| c.len() == 4) {
        verify_deck(chunk);
    }
    for s in &corpus {
        verify_deck(&[s.clone(), s.clone(), s.clone(), s.clone()]);
    }
}

proptest! {
    #![proptest_config(config())]
    #[test]
    fn deck_output_parses_back_to_what_was_written(t in texts()) {
        verify_deck(&t);
    }

    #[test]
    fn file_listings_parse_back(t in texts()) {
        let entries: Vec<FileEntry> = t
            .iter()
            .enumerate()
            .map(|(i, title)| FileEntry {
                id: engine_shared::DocumentId(uuid::Uuid::from_u128(i as u128 + 1)),
                kind: FileKind::Document,
                rev: i as u64,
                project_id: uuid::Uuid::nil(),
                project: title.clone(),
                role: "editor".into(),
                title: title.clone(),
            })
            .collect();
        let r = files(&entries, &FilesOptions { max_bytes: Some(64_000), ..Default::default() }).unwrap();
        let (_, lines) = lines_of(&r.text);
        let rows = parse::files_lines(&lines).unwrap_or_else(|e| panic!("{e}\n{}", r.text));
        prop_assert_eq!(rows.len(), t.len());
        let mut want: Vec<String> = t.iter().map(|s| sanitize(s)).collect();
        want.sort();
        let mut got: Vec<String> = rows.iter().map(|r| r.title.clone()).collect();
        got.sort();
        prop_assert_eq!(got, want);
    }
}

#[test]
fn the_documented_attacks() {
    // AC12: a block whose text is a block line renders with an escaped marker and parses as one block.
    let (_, fx) = document_from(&["@AB12 P #000000".into(), "x".into(), "y".into(), "z".into()]);
    let idx = RefIndex::from_state(&fx.state);
    let first = fx
        .state
        .nodes
        .values()
        .find(|n| n.current_fields["label"] == "@AB12 P #000000" && n.node_type == "item" && n.current_fields["content"]["type"] == "paragraph")
        .unwrap();
    let r = read(&fx.meta, &fx.state, &ReadOptions { refs: vec![idx.ref_of(&first.id).unwrap().to_string()], ..Default::default() }).unwrap();
    assert!(r.text.contains("\n\\@AB12 P #000000\n"), "{}", r.text);
    let (_, lines) = lines_of(&r.text);
    assert_eq!(parse::read_blocks(&lines).unwrap().len(), 1);
    // AC13: tag characters and bidi controls are dropped and counted.
    let (_, fx) = document_from(&["a\u{e0041}b\u{202e}c".into(), "x".into(), "y".into(), "z".into()]);
    let r = outline(&fx.meta, &fx.state, &OutlineOptions::default()).unwrap();
    assert!(r.text.lines().next().unwrap().contains("warn=hidden"), "{}", r.text);
    assert!(!r.text.contains('\u{e0041}') && !r.text.contains('\u{202e}'));
}
