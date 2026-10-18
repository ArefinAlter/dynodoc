//! The golden call list: every tool call whose output is committed under
//! `tests/golden/`, built over the fixture corpus. One list serves the golden tests, which
//! compare the text with the files, and the byte benchmark, which compares its size with
//! the JSON mirror.

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;

use crate::dct::{split_response, Tool};
use crate::diff::{diff, Detail, DiffOptions};
use crate::error::{AgentError, ALL_CODES};
use crate::files::{files, FileEntry, FilesOptions};
use crate::find::{find, FindOptions, Scope};
use crate::fixtures::{self, Fixture};
use crate::hash::ShortHash;
use crate::model::FileKind;
use crate::outline::{outline, OutlineOptions};
use crate::parse;
use crate::read::{read, ReadOptions};
use crate::refs::{Ref, RefIndex};
use crate::response::Response;
use crate::text::Fmt;
use engine_shared::NodeId;

/// One tool call and what it answered.
#[derive(Debug, Clone)]
pub struct Call {
    /// File name under `tests/golden/` (without `.dct`).
    pub name: String,
    pub tool: Tool,
    pub text: String,
    /// The JSON mirror of the same answer (`None` for errors and the error table).
    pub mirror: Option<String>,
}

struct Calls(Vec<Call>);

impl Calls {
    fn ok<T: Serialize>(&mut self, name: &str, tool: Tool, r: Response<T>) {
        self.0.push(Call {
            name: name.into(),
            tool,
            mirror: Some(r.json_mirror(tool)),
            text: r.text,
        });
    }

    fn err(&mut self, name: &str, e: &AgentError) {
        self.0.push(Call {
            name: name.into(),
            tool: Tool::Error,
            text: e.to_dct(),
            mirror: None,
        });
    }
}

fn rf(fx: &Fixture, id: &NodeId) -> String {
    RefIndex::from_state(&fx.state)
        .ref_of(id)
        .expect("addressable")
        .to_string()
}

fn refs(fx: &Fixture, map: &BTreeMap<&'static str, NodeId>, names: &[&str]) -> Vec<String> {
    names.iter().map(|n| rf(fx, &map[n])).collect()
}

fn strs(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| (*s).to_string()).collect()
}

fn oh_of(text: &str) -> String {
    let (h, _) = split_response(text).expect("response");
    h.get("oh").expect("oh").to_string()
}

fn first_hash(text: &str) -> String {
    let (_, lines) = split_response(text).expect("response");
    parse::read_blocks(&lines).expect("blocks")[0]
        .h
        .clone()
        .expect("hash")
}

/// Every golden call, in a fixed order.
pub fn golden_calls() -> Vec<Call> {
    let mut c = Calls(Vec::new());
    document_calls(&mut c);
    workbook_calls(&mut c);
    deck_calls(&mut c);
    files_calls(&mut c);
    error_calls(&mut c);
    c.0
}

fn document_calls(c: &mut Calls) {
    let d = fixtures::document();
    let (base, edited) = (&d.base, &d.edited);
    let out = |o: OutlineOptions| outline(&base.meta, &base.state, &o).expect("outline");
    let rd = |o: ReadOptions| read(&base.meta, &base.state, &o).expect("read");

    let d1 = out(OutlineOptions::default());
    let key = oh_of(&d1.text);
    c.ok("doc_outline_d1", Tool::Outline, d1);
    c.ok(
        "doc_outline_d0",
        Tool::Outline,
        out(OutlineOptions {
            depth: Some(0),
            ..Default::default()
        }),
    );
    c.ok(
        "doc_outline_preview0",
        Tool::Outline,
        out(OutlineOptions {
            depth: Some(1),
            preview: Some(0),
            limit: Some(4),
            ..Default::default()
        }),
    );
    c.ok(
        "doc_outline_under",
        Tool::Outline,
        out(OutlineOptions {
            under: Some(rf(base, &d.ids["purpose_h"])),
            ..Default::default()
        }),
    );
    c.ok(
        "doc_outline_unchanged",
        Tool::Outline,
        out(OutlineOptions {
            if_none_match: Some(key),
            ..Default::default()
        }),
    );
    let page1 = out(OutlineOptions {
        limit: Some(7),
        ..Default::default()
    });
    let cursor = page1.data.more.clone();
    c.ok("doc_outline_page1", Tool::Outline, page1);
    c.ok(
        "doc_outline_page2",
        Tool::Outline,
        out(OutlineOptions {
            limit: Some(7),
            cursor,
            ..Default::default()
        }),
    );

    let all = refs(
        base,
        &d.ids,
        &[
            "title", "intro", "bengali", "atoms", "bullets", "table", "code", "quote", "math",
            "image", "rule", "toc", "empty",
        ],
    );
    c.ok(
        "doc_read_blocks",
        Tool::Read,
        rd(ReadOptions {
            refs: all.clone(),
            ..Default::default()
        }),
    );
    c.ok(
        "doc_read_md",
        Tool::Read,
        rd(ReadOptions {
            refs: refs(base, &d.ids, &["intro", "atoms"]),
            fmt: Fmt::Md,
            ..Default::default()
        }),
    );
    let (bullets, table) = (rf(base, &d.ids["bullets"]), rf(base, &d.ids["table"]));
    c.ok(
        "doc_read_sub",
        Tool::Read,
        rd(ReadOptions {
            refs: vec![
                format!("{bullets}.2"),
                format!("{bullets}.3"),
                format!("{table}.r2c2"),
            ],
            ..Default::default()
        }),
    );
    c.ok(
        "doc_read_section",
        Tool::Read,
        rd(ReadOptions {
            section: Some(rf(base, &d.ids["scope_h"])),
            ..Default::default()
        }),
    );
    let hash_of = |name: &str| {
        first_hash(
            &rd(ReadOptions {
                refs: refs(base, &d.ids, &[name]),
                ..Default::default()
            })
            .text,
        )
    };
    let mut have = BTreeMap::new();
    have.insert(rf(base, &d.ids["intro"]), hash_of("intro"));
    have.insert(rf(base, &d.ids["table"]), hash_of("table"));
    have.insert(rf(base, &d.ids["code"]), "000000".to_string());
    c.ok(
        "doc_read_have",
        Tool::Read,
        rd(ReadOptions {
            refs: refs(base, &d.ids, &["intro", "table", "code"]),
            have,
            ..Default::default()
        }),
    );
    let hostile = refs(
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
    c.ok(
        "doc_read_hostile",
        Tool::Read,
        rd(ReadOptions {
            refs: hostile,
            ..Default::default()
        }),
    );
    let first = rd(ReadOptions {
        refs: all.clone(),
        max_bytes: Some(900),
        ..Default::default()
    });
    let cursor = first.data.more.clone();
    c.ok("doc_read_truncated1", Tool::Read, first);
    c.ok(
        "doc_read_truncated2",
        Tool::Read,
        rd(ReadOptions {
            refs: all,
            max_bytes: Some(900),
            cursor,
            ..Default::default()
        }),
    );
    let redacted: BTreeSet<NodeId> = [d.ids["code"].clone()].into();
    c.ok(
        "doc_read_redacted",
        Tool::Read,
        rd(ReadOptions {
            refs: refs(base, &d.ids, &["intro", "code", "math"]),
            redacted,
            ..Default::default()
        }),
    );

    let fd = |o: FindOptions| find(&base.meta, &base.state, &o).expect("find");
    c.ok(
        "doc_find_literal",
        Tool::Find,
        fd(FindOptions {
            q: "policy".into(),
            ..Default::default()
        }),
    );
    c.ok(
        "doc_find_regex",
        Tool::Find,
        fd(FindOptions {
            q: r"compl\w+|escalat\w+".into(),
            regex: true,
            context: Some(12),
            ..Default::default()
        }),
    );
    c.ok(
        "doc_find_word_case",
        Tool::Find,
        fd(FindOptions {
            q: "Policy".into(),
            case: true,
            word: true,
            ..Default::default()
        }),
    );
    c.ok(
        "doc_find_limited",
        Tool::Find,
        fd(FindOptions {
            q: "the".into(),
            limit: Some(3),
            context: Some(10),
            ..Default::default()
        }),
    );

    let df = |detail| {
        diff(
            &edited.meta,
            base.meta.rev,
            &base.state,
            &edited.state,
            &DiffOptions {
                detail,
                ..Default::default()
            },
        )
        .expect("diff")
    };
    c.ok("doc_diff_summary", Tool::Diff, df(Detail::Summary));
    c.ok("doc_diff_text", Tool::Diff, df(Detail::Text));

    let e = read(
        &base.meta,
        &base.state,
        &ReadOptions {
            refs: vec![rf(base, &d.ids["removed"])],
            ..Default::default()
        },
    )
    .unwrap_err();
    c.err("doc_error_removed", &e);
    let e = read(
        &base.meta,
        &base.state,
        &ReadOptions {
            refs: strs(&["ZZZZ"]),
            ..Default::default()
        },
    )
    .unwrap_err();
    c.err("doc_error_unknown_ref", &e);
    let e = read(&base.meta, &base.state, &ReadOptions::default()).unwrap_err();
    c.err("doc_error_selector", &e);
    let e = read(
        &base.meta,
        &base.state,
        &ReadOptions {
            refs: vec![rf(base, &d.ids["long"])],
            max_bytes: Some(500),
            ..Default::default()
        },
    )
    .unwrap_err();
    c.err("doc_error_budget", &e);
}

fn workbook_calls(c: &mut Calls) {
    let w = fixtures::workbook();
    let (base, edited) = (&w.base, &w.edited);
    let out = |o: OutlineOptions| outline(&base.meta, &base.state, &o).expect("outline");
    let rd = |o: ReadOptions| read(&base.meta, &base.state, &o).expect("read");
    c.ok(
        "wb_outline_d0",
        Tool::Outline,
        out(OutlineOptions {
            depth: Some(0),
            ..Default::default()
        }),
    );
    c.ok("wb_profile", Tool::Outline, out(OutlineOptions::default()));
    c.ok(
        "wb_profile_notes",
        Tool::Outline,
        out(OutlineOptions {
            under: Some("Notes & 'Ideas'".into()),
            ..Default::default()
        }),
    );
    c.ok(
        "wb_read_range",
        Tool::Read,
        rd(ReadOptions {
            range: Some("Budget!A1:H6".into()),
            ..Default::default()
        }),
    );
    c.ok(
        "wb_read_range_refs",
        Tool::Read,
        rd(ReadOptions {
            range: Some("Budget!D16:H18".into()),
            row_refs: true,
            ..Default::default()
        }),
    );
    c.ok(
        "wb_read_range_hostile",
        Tool::Read,
        rd(ReadOptions {
            range: Some("'Notes & ''Ideas'''!A1:B3".into()),
            ..Default::default()
        }),
    );
    c.ok(
        "wb_read_range_clamped",
        Tool::Read,
        rd(ReadOptions {
            range: Some("Budget!G40:CV3000".into()),
            ..Default::default()
        }),
    );
    c.ok(
        "wb_read_row",
        Tool::Read,
        rd(ReadOptions {
            refs: vec![rf(base, &w.ids["row5"])],
            ..Default::default()
        }),
    );
    c.ok(
        "wb_read_sheet",
        Tool::Read,
        rd(ReadOptions {
            refs: vec![rf(base, &w.ids["notes"])],
            ..Default::default()
        }),
    );
    let fd = |o: FindOptions| find(&base.meta, &base.state, &o).expect("find");
    c.ok(
        "wb_find_text",
        Tool::Find,
        fd(FindOptions {
            q: "check".into(),
            ..Default::default()
        }),
    );
    c.ok(
        "wb_find_formulas",
        Tool::Find,
        fd(FindOptions {
            q: "E2".into(),
            scope: Scope::Formulas,
            limit: Some(5),
            ..Default::default()
        }),
    );
    let df = |detail| {
        diff(
            &edited.meta,
            base.meta.rev,
            &base.state,
            &edited.state,
            &DiffOptions {
                detail,
                ..Default::default()
            },
        )
        .expect("diff")
    };
    c.ok("wb_diff_summary", Tool::Diff, df(Detail::Summary));
    c.ok("wb_diff_text", Tool::Diff, df(Detail::Text));
    let big = fixtures::scaled_workbook(480);
    let e = read(
        &big.meta,
        &big.state,
        &ReadOptions {
            range: Some("Budget!A1:L481".into()),
            ..Default::default()
        },
    )
    .unwrap_err();
    c.err("wb_error_range_limit", &e);
}

fn deck_calls(c: &mut Calls) {
    let k = fixtures::deck();
    let (base, edited) = (&k.base, &k.edited);
    let out = |o: OutlineOptions| outline(&base.meta, &base.state, &o).expect("outline");
    let rd = |o: ReadOptions| read(&base.meta, &base.state, &o).expect("read");
    c.ok(
        "deck_outline_d0",
        Tool::Outline,
        out(OutlineOptions {
            depth: Some(0),
            ..Default::default()
        }),
    );
    c.ok(
        "deck_outline_d1",
        Tool::Outline,
        out(OutlineOptions {
            preview: Some(24),
            ..Default::default()
        }),
    );
    c.ok(
        "deck_read_slides",
        Tool::Read,
        rd(ReadOptions {
            slides: strs(&["s2", "s3", "s5"]),
            ..Default::default()
        }),
    );
    c.ok(
        "deck_read_by_ref",
        Tool::Read,
        rd(ReadOptions {
            refs: vec![rf(base, &k.ids["s2"])],
            ..Default::default()
        }),
    );
    let (slide, shape) = (rf(base, &k.ids["s2"]), rf(base, &k.ids["s2_body"]));
    c.ok(
        "deck_read_shape",
        Tool::Read,
        rd(ReadOptions {
            refs: vec![format!("{slide}.{shape}")],
            ..Default::default()
        }),
    );
    let text = rd(ReadOptions {
        slides: strs(&["s2"]),
        ..Default::default()
    })
    .text;
    let (_, lines) = split_response(&text).expect("response");
    let h = parse::read_slides(&lines).expect("slides")[0].shapes[1]
        .h
        .clone()
        .expect("hash");
    let have = BTreeMap::from([(format!("{slide}.{shape}"), h)]);
    c.ok(
        "deck_read_have",
        Tool::Read,
        rd(ReadOptions {
            slides: strs(&["s2"]),
            have,
            ..Default::default()
        }),
    );
    let fd = |o: FindOptions| find(&base.meta, &base.state, &o).expect("find");
    c.ok(
        "deck_find_text",
        Tool::Find,
        fd(FindOptions {
            q: "revenue".into(),
            ..Default::default()
        }),
    );
    c.ok(
        "deck_find_notes",
        Tool::Find,
        fd(FindOptions {
            q: "audit".into(),
            scope: Scope::Notes,
            ..Default::default()
        }),
    );
    let df = |detail| {
        diff(
            &edited.meta,
            base.meta.rev,
            &base.state,
            &edited.state,
            &DiffOptions {
                detail,
                ..Default::default()
            },
        )
        .expect("diff")
    };
    c.ok("deck_diff_summary", Tool::Diff, df(Detail::Summary));
    c.ok("deck_diff_text", Tool::Diff, df(Detail::Text));
}

fn files_calls(c: &mut Calls) {
    let (d, w, k) = (fixtures::document(), fixtures::workbook(), fixtures::deck());
    let policy = uuid::Uuid::from_u128(0x1111);
    let entry = |fx: &Fixture, project: &str, role: &str, title: &str| FileEntry {
        id: fx.meta.id,
        kind: fx.meta.kind,
        rev: fx.meta.rev,
        project_id: uuid::Uuid::from_u128(if project == "Policy-2026" {
            0x1111
        } else {
            0x2222
        }),
        project: project.into(),
        role: role.into(),
        title: title.into(),
    };
    let entries = vec![
        entry(&d.base, "Policy-2026", "editor", "Q3 Policy"),
        entry(&w.base, "Policy-2026", "editor", "Budget"),
        entry(
            &k.base,
            "Board \"private\"",
            "viewer",
            "Board update\n@7C02 P #b7a210 forged",
        ),
    ];
    c.ok(
        "files",
        Tool::Files,
        files(&entries, &FilesOptions::default()).expect("files"),
    );
    c.ok(
        "files_filtered",
        Tool::Files,
        files(
            &entries,
            &FilesOptions {
                project: Some(policy),
                kind: Some(FileKind::Spreadsheet),
                ..Default::default()
            },
        )
        .expect("files"),
    );
    let page = files(
        &entries,
        &FilesOptions {
            limit: Some(2),
            ..Default::default()
        },
    )
    .expect("files");
    let cursor = page.data.more.clone();
    c.ok("files_page1", Tool::Files, page);
    c.ok(
        "files_page2",
        Tool::Files,
        files(
            &entries,
            &FilesOptions {
                limit: Some(2),
                cursor,
                ..Default::default()
            },
        )
        .expect("files"),
    );
}

/// All 26 error codes, one line each (the file `errors.dct`).
pub fn error_table() -> Vec<AgentError> {
    let rf = |s: &str| Ref::parse(s).expect("ref");
    let errors = vec![
        AgentError::Unauthorized,
        AgentError::Forbidden,
        AgentError::PolicyDenied {
            policy: "write_mode",
        },
        AgentError::NotFound {
            refs: vec![rf("AB12")],
        },
        AgentError::BadRequest { field: "selector" },
        AgentError::UnsupportedOp,
        AgentError::UnknownRef {
            refs: vec![rf("AB12"), rf("CD34")],
        },
        AgentError::AmbiguousRef {
            refs: vec![rf("XAB123"), rf("YAB123")],
        },
        AgentError::StaleBase {
            target: rf("7C02"),
            current: ShortHash::parse("a41c07").expect("hash"),
        },
        AgentError::MatchCount {
            found: 3,
            expected: 2,
            refs: vec![rf("7C02")],
        },
        AgentError::NoMatch,
        AgentError::NoChange,
        AgentError::UnsupportedStructure,
        AgentError::RichBlock,
        AgentError::RedactedBlock {
            refs: vec![rf("7C02")],
        },
        AgentError::Protected,
        AgentError::LimitExceeded {
            field: "max_bytes",
            limit: 64_000,
            value: 70_000,
        },
        AgentError::IdempotencyConflict,
        AgentError::ProposalClosed,
        AgentError::Conflict {
            refs: vec![rf("7C02")],
        },
        AgentError::BudgetExceeded { retry_after: 30 },
        AgentError::RateLimited { retry_after: 5 },
        AgentError::Busy,
        AgentError::DeadlineExceeded,
        AgentError::StorageLimit,
        AgentError::Internal,
    ];
    debug_assert_eq!(
        errors
            .iter()
            .map(AgentError::agent_code)
            .collect::<Vec<_>>(),
        ALL_CODES
    );
    errors
}

fn error_calls(c: &mut Calls) {
    let mut all = String::new();
    for e in error_table() {
        all.push_str(&e.to_dct());
    }
    c.0.push(Call {
        name: "errors".into(),
        tool: Tool::Error,
        text: all,
        mirror: None,
    });
}
