//! Conformance tests (protocol spec 17): error table, truncation and paging, selectors,
//! determinism, shared vectors and the rules the crate promises about its own source.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use engine_agent::budget::Cursor;
use engine_agent::dct::{parse_header, split_response};
use engine_agent::diff::{diff, Detail, DiffOptions};
use engine_agent::error::{AgentError, Retry, ALL_CODES};
use engine_agent::files::{files, FileEntry, FilesOptions};
use engine_agent::find::{find, FindOptions};
use engine_agent::fixtures::{self, Fixture};
use engine_agent::hash::{
    block_hash, notes_hash, outline_hash, range_hash, row_hash, shape_hash, Hash,
};
use engine_agent::model::FileKind;
use engine_agent::outline::{outline, OutlineOptions};
use engine_agent::parse;
use engine_agent::read::{read, ReadOptions};
use engine_agent::refs::{Ref, RefIndex, ResolveError};
use engine_core::materializer::DocumentState;
use engine_shared::NodeId;
use serde_json::{json, Value};

fn vectors_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("vectors")
}

fn check_json(name: &str, value: &Value) {
    let path = vectors_dir().join(name);
    let text = serde_json::to_string_pretty(value).unwrap() + "\n";
    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        std::fs::create_dir_all(vectors_dir()).unwrap();
        std::fs::write(&path, &text).unwrap();
    } else {
        let want = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("missing {}: {e} (UPDATE_GOLDEN=1)", path.display()));
        assert_eq!(
            text,
            want.replace("\r\n", "\n"),
            "{name} differs (UPDATE_GOLDEN=1 to regenerate)"
        );
    }
}

// ---------------------------------------------------------------- error table (spec 10)

#[test]
fn error_table_matches_the_spec() {
    let rf = |s: &str| Ref::parse(s).unwrap();
    let h = engine_agent::hash::ShortHash::parse("a41c07").unwrap();
    // (agent_code, code, status, retry) exactly as the table of section 10 reads.
    let table: Vec<(AgentError, &str, &str, u16, Retry)> = vec![
        (
            AgentError::Unauthorized,
            "unauthorized",
            "unauthorized",
            401,
            Retry::AfterReauth,
        ),
        (
            AgentError::Forbidden,
            "forbidden",
            "forbidden",
            403,
            Retry::No,
        ),
        (
            AgentError::PolicyDenied { policy: "x" },
            "policy_denied",
            "forbidden",
            403,
            Retry::No,
        ),
        (
            AgentError::NotFound { refs: vec![] },
            "not_found",
            "not_found",
            404,
            Retry::No,
        ),
        (
            AgentError::BadRequest { field: "f" },
            "bad_request",
            "bad_request",
            400,
            Retry::AfterFixing,
        ),
        (
            AgentError::UnsupportedOp,
            "unsupported_op",
            "bad_request",
            400,
            Retry::No,
        ),
        (
            AgentError::UnknownRef {
                refs: vec![rf("AB12")],
            },
            "unknown_ref",
            "bad_request",
            400,
            Retry::AfterRead,
        ),
        (
            AgentError::AmbiguousRef { refs: vec![] },
            "ambiguous_ref",
            "bad_request",
            400,
            Retry::AfterFixing,
        ),
        (
            AgentError::StaleBase {
                target: rf("AB12"),
                current: h,
            },
            "stale_base",
            "conflict",
            409,
            Retry::AfterRead,
        ),
        (
            AgentError::MatchCount {
                found: 1,
                expected: 2,
                refs: vec![],
            },
            "match_count",
            "conflict",
            409,
            Retry::AfterFixing,
        ),
        (AgentError::NoMatch, "no_match", "conflict", 409, Retry::No),
        (
            AgentError::NoChange,
            "no_change",
            "bad_request",
            400,
            Retry::No,
        ),
        (
            AgentError::UnsupportedStructure,
            "unsupported_structure",
            "bad_request",
            400,
            Retry::No,
        ),
        (
            AgentError::RichBlock,
            "rich_block",
            "bad_request",
            400,
            Retry::No,
        ),
        (
            AgentError::RedactedBlock { refs: vec![] },
            "redacted_block",
            "forbidden",
            403,
            Retry::No,
        ),
        (
            AgentError::Protected,
            "protected",
            "conflict",
            409,
            Retry::No,
        ),
        (
            AgentError::LimitExceeded {
                field: "max_bytes",
                limit: 1,
                value: 2,
            },
            "limit_exceeded",
            "bad_request",
            413,
            Retry::AfterFixing,
        ),
        (
            AgentError::IdempotencyConflict,
            "idempotency_conflict",
            "conflict",
            409,
            Retry::No,
        ),
        (
            AgentError::ProposalClosed,
            "proposal_closed",
            "conflict",
            409,
            Retry::No,
        ),
        (
            AgentError::Conflict { refs: vec![] },
            "conflict",
            "conflict",
            409,
            Retry::No,
        ),
        (
            AgentError::BudgetExceeded { retry_after: 1 },
            "budget_exceeded",
            "rate_limited",
            429,
            Retry::AfterDelay,
        ),
        (
            AgentError::RateLimited { retry_after: 1 },
            "rate_limited",
            "rate_limited",
            429,
            Retry::AfterDelay,
        ),
        (AgentError::Busy, "busy", "busy", 503, Retry::Yes),
        (
            AgentError::DeadlineExceeded,
            "deadline_exceeded",
            "deadline_exceeded",
            504,
            Retry::YesForReads,
        ),
        (
            AgentError::StorageLimit,
            "storage_limit",
            "storage_limit",
            507,
            Retry::No,
        ),
        (
            AgentError::Internal,
            "internal",
            "internal",
            500,
            Retry::Maybe,
        ),
    ];
    assert_eq!(table.len(), ALL_CODES.len());
    for ((e, agent, code, status, retry), listed) in table.iter().zip(ALL_CODES) {
        assert_eq!((e.agent_code(), listed), (*agent, *agent));
        assert_eq!(
            (e.code(), e.http_status(), e.retry()),
            (*code, *status, *retry),
            "{agent}"
        );
        // A count limit is a plain 400.
        let line = e.to_dct();
        let (hdr, body) = split_response(&line).unwrap();
        assert!(body.is_empty());
        assert_eq!(hdr.get("code"), Some(*agent));
        assert_eq!(hdr.get("hint"), Some(e.hint()));
        assert!(e.hint().ends_with('.') && e.hint().is_ascii());
    }
    assert_eq!(
        AgentError::LimitExceeded {
            field: "refs",
            limit: 100,
            value: 101
        }
        .http_status(),
        400
    );
}

// ---------------------------------------------------------------- selectors and refs through the tools

#[test]
fn read_selector_rules() {
    let d = fixtures::document();
    let fx = &d.base;
    let r = |names: &[&str]| -> Vec<String> {
        let idx = RefIndex::from_state(&fx.state);
        names
            .iter()
            .map(|n| idx.ref_of(&d.ids[n]).unwrap().to_string())
            .collect()
    };
    let go = |o: ReadOptions| read(&fx.meta, &fx.state, &o);
    assert_eq!(
        go(ReadOptions::default()).unwrap_err(),
        AgentError::BadRequest { field: "selector" }
    );
    assert_eq!(
        go(ReadOptions {
            refs: r(&["intro"]),
            section: Some(r(&["purpose_h"])[0].clone()),
            ..Default::default()
        })
        .unwrap_err(),
        AgentError::BadRequest { field: "selector" }
    );
    assert_eq!(
        go(ReadOptions {
            range: Some("A1".into()),
            ..Default::default()
        })
        .unwrap_err(),
        AgentError::BadRequest { field: "range" }
    );
    assert_eq!(
        go(ReadOptions {
            slides: vec!["s1".into()],
            ..Default::default()
        })
        .unwrap_err(),
        AgentError::BadRequest { field: "slides" }
    );
    // A section needs a heading.
    assert_eq!(
        go(ReadOptions {
            section: Some(r(&["intro"])[0].clone()),
            ..Default::default()
        })
        .unwrap_err(),
        AgentError::BadRequest { field: "section" }
    );
    // 101 refs.
    let many = vec![r(&["intro"])[0].clone(); 101];
    assert!(matches!(
        go(ReadOptions {
            refs: many,
            ..Default::default()
        })
        .unwrap_err(),
        AgentError::LimitExceeded {
            field: "refs",
            limit: 100,
            value: 101
        }
    ));
    // Bad syntax, unknown, tombstone, a non-block node.
    assert_eq!(
        go(ReadOptions {
            refs: vec!["no".into()],
            ..Default::default()
        })
        .unwrap_err(),
        AgentError::BadRequest { field: "refs" }
    );
    assert!(matches!(
        go(ReadOptions {
            refs: vec!["QQQQ".into()],
            ..Default::default()
        })
        .unwrap_err(),
        AgentError::UnknownRef { .. }
    ));
    assert!(matches!(
        go(ReadOptions {
            refs: r(&["removed"]),
            ..Default::default()
        })
        .unwrap_err(),
        AgentError::NotFound { .. }
    ));
    let root = fx
        .state
        .nodes
        .values()
        .find(|n| n.node_type == "form")
        .unwrap();
    let root_ref = RefIndex::from_state(&fx.state)
        .ref_of(&root.id)
        .unwrap()
        .to_string();
    assert_eq!(
        go(ReadOptions {
            refs: vec![root_ref],
            ..Default::default()
        })
        .unwrap_err(),
        AgentError::BadRequest { field: "refs" }
    );
    // Sub-addresses: wrong structure, out of range.
    let intro = r(&["intro"])[0].clone();
    assert_eq!(
        go(ReadOptions {
            refs: vec![format!("{intro}.1")],
            ..Default::default()
        })
        .unwrap_err(),
        AgentError::UnsupportedStructure
    );
    let list = r(&["bullets"])[0].clone();
    assert!(matches!(
        go(ReadOptions {
            refs: vec![format!("{list}.9")],
            ..Default::default()
        })
        .unwrap_err(),
        AgentError::UnknownRef { .. }
    ));
    // Every selected block withheld: the call fails.
    let mut redacted = BTreeSet::new();
    redacted.insert(d.ids["intro"].clone());
    assert!(matches!(
        go(ReadOptions {
            refs: r(&["intro"]),
            redacted,
            ..Default::default()
        })
        .unwrap_err(),
        AgentError::RedactedBlock { .. }
    ));
    // Kind mismatch.
    let w = fixtures::workbook();
    assert_eq!(
        read(
            &w.base.meta,
            &w.base.state,
            &ReadOptions {
                section: Some("ZZZZ".into()),
                ..Default::default()
            }
        )
        .unwrap_err(),
        AgentError::BadRequest { field: "section" }
    );
    let k = fixtures::deck();
    assert_eq!(
        read(
            &k.base.meta,
            &k.base.state,
            &ReadOptions {
                range: Some("A1".into()),
                ..Default::default()
            }
        )
        .unwrap_err(),
        AgentError::BadRequest { field: "range" }
    );
}

#[test]
fn ambiguous_refs_name_their_candidates() {
    // Two blocks whose ULIDs share the last 6 characters.
    let mut b = fixtures::Builder::new(FileKind::Document, "ambiguous", 0xD4);
    let root = b.node(
        engine_shared::NodeType::Form,
        None,
        0,
        json!({"label": "t"}),
    );
    let mut ids = Vec::new();
    for (i, id) in ["01K7Q9XM2A0000000000XAB123", "01K7Q9XM2A0000000000YAB123"]
        .iter()
        .enumerate()
    {
        b.apply(engine_shared::EventPayload::NodeCreated {
            node_id: NodeId((*id).into()),
            node_type: engine_shared::NodeType::Item,
            parent_id: Some(root.clone()),
            pos: format!("{:08}", i),
            fields: json!({"label": "x", "content": fixtures::para("x")}),
            var_name: None,
        });
        ids.push(id.to_string());
    }
    let fx = b.snapshot();
    let err = read(
        &fx.meta,
        &fx.state,
        &ReadOptions {
            refs: vec!["AB123".into()],
            ..Default::default()
        },
    )
    .unwrap_err();
    match &err {
        AgentError::AmbiguousRef { refs } => {
            let got: Vec<_> = refs.iter().map(Ref::as_str).collect();
            assert_eq!(got, ["XAB123", "YAB123"]);
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(
        err.to_dct().lines().next().unwrap().split(' ').nth(3),
        Some("refs=XAB123,YAB123")
    );
    // The candidates read.
    let ok = read(
        &fx.meta,
        &fx.state,
        &ReadOptions {
            refs: vec!["xab123".into(), "YAB123".into()],
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(ok.data.n, 2);
}

// ---------------------------------------------------------------- budgets and paging

fn rows_of(text: &str, depth: u8) -> Vec<parse::OutlineRow> {
    let (_, lines) = split_response(text).unwrap();
    parse::outline_blocks(&lines, depth).unwrap()
}

#[test]
fn every_budget_is_respected_and_pages_cover_the_outline_exactly() {
    let big = fixtures::scaled_document(200);
    let whole = outline(
        &big.meta,
        &big.state,
        &OutlineOptions {
            limit: Some(1000),
            max_bytes: Some(64_000),
            ..Default::default()
        },
    )
    .unwrap();
    let all = rows_of(&whole.text, 1);
    assert_eq!(all.len(), 200);
    let mut previous_taken = 0;
    for max in (500..=4000).step_by(37) {
        let r = outline(
            &big.meta,
            &big.state,
            &OutlineOptions {
                max_bytes: Some(max),
                limit: Some(1000),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(r.text.len() <= max, "{} > {max}", r.text.len());
        let n = r.data.blocks.as_ref().unwrap().len();
        assert!(
            n >= previous_taken,
            "taking fewer at a larger budget: {n} < {previous_taken} at {max}"
        );
        previous_taken = n;
        assert_eq!(r.data.more.is_some(), n < 200);
    }
    // Page through with a budget and a limit; the pages are the whole outline, in order.
    for (max, limit) in [(700usize, 1000usize), (1500, 1000), (64_000, 9), (900, 13)] {
        let mut cursor: Option<String> = None;
        let mut got = Vec::new();
        let mut pages = 0;
        loop {
            let r = outline(
                &big.meta,
                &big.state,
                &OutlineOptions {
                    max_bytes: Some(max),
                    limit: Some(limit),
                    cursor: cursor.clone(),
                    ..Default::default()
                },
            )
            .unwrap();
            assert!(r.text.len() <= max);
            let (h, _) = split_response(&r.text).unwrap();
            assert_eq!(
                h.get("rev"),
                Some(big.meta.rev.to_string().as_str()),
                "continuations say rev="
            );
            got.extend(rows_of(&r.text, 1));
            pages += 1;
            assert!(pages < 400);
            match r.data.more {
                Some(c) => {
                    assert_eq!(h.get("warn").map(|w| w.contains("truncated")), Some(true));
                    assert_eq!(h.get("more"), Some(c.as_str()));
                    cursor = Some(c);
                }
                None => {
                    assert!(h.get("more").is_none());
                    break;
                }
            }
        }
        assert_eq!(got, all, "max={max} limit={limit}");
    }
}

#[test]
fn boundary_budget_is_exact() {
    let d = fixtures::document();
    let fx = &d.base;
    let rd = |max: usize| {
        read(
            &fx.meta,
            &fx.state,
            &ReadOptions {
                section: Some(
                    RefIndex::from_state(&fx.state)
                        .ref_of(&d.ids["scope_h"])
                        .unwrap()
                        .to_string(),
                ),
                max_bytes: Some(max),
                ..Default::default()
            },
        )
    };
    let full = rd(64_000).unwrap();
    assert_eq!(full.data.n, 3);
    let len = full.text.len();
    // Exactly enough: everything, no cursor. One byte short: a strict prefix with a cursor, still within budget.
    if len >= 500 {
        let exact = rd(len).unwrap();
        assert_eq!((exact.data.n, exact.data.more.is_none()), (3, true));
        let short = rd(len - 1).unwrap();
        assert!(short.data.n < 3 && short.data.more.is_some());
        assert!(short.text.len() < len);
        // The partial page never holds half a block: every block parses with its full body.
        let (_, lines) = split_response(&short.text).unwrap();
        parse::read_blocks(&lines).unwrap();
    }
    // A block larger than the budget is an error, never a partial block.
    let long = RefIndex::from_state(&fx.state)
        .ref_of(&d.ids["long"])
        .unwrap()
        .to_string();
    let err = read(
        &fx.meta,
        &fx.state,
        &ReadOptions {
            refs: vec![long],
            max_bytes: Some(500),
            ..Default::default()
        },
    )
    .unwrap_err();
    assert!(matches!(
        err,
        AgentError::LimitExceeded {
            field: "max_bytes",
            limit: 500,
            ..
        }
    ));
    // Budget range.
    for bad in [0usize, 499] {
        assert_eq!(
            rd(bad).unwrap_err(),
            AgentError::BadRequest { field: "max_bytes" }
        );
    }
    assert!(matches!(
        rd(64_001).unwrap_err(),
        AgentError::LimitExceeded {
            limit: 64_000,
            value: 64_001,
            ..
        }
    ));
}

#[test]
fn cursors_pin_their_revision() {
    let d = fixtures::document();
    let (base, edited) = (&d.base, &d.edited);
    let first = outline(
        &base.meta,
        &base.state,
        &OutlineOptions {
            limit: Some(5),
            ..Default::default()
        },
    )
    .unwrap();
    let cursor = first.data.more.clone().unwrap();
    let c: Cursor = cursor.parse().unwrap();
    assert_eq!((c.rev, c.pos), (base.meta.rev, 5));
    // At the same revision it continues; against a newer state it is refused.
    assert!(outline(
        &base.meta,
        &base.state,
        &OutlineOptions {
            cursor: Some(cursor.clone()),
            ..Default::default()
        }
    )
    .is_ok());
    assert_eq!(
        outline(
            &edited.meta,
            &edited.state,
            &OutlineOptions {
                cursor: Some(cursor),
                ..Default::default()
            }
        )
        .unwrap_err(),
        AgentError::BadRequest { field: "cursor" }
    );
    for bad in ["", "c_", "x", "c_1"] {
        assert_eq!(
            outline(
                &base.meta,
                &base.state,
                &OutlineOptions {
                    cursor: Some(bad.into()),
                    ..Default::default()
                }
            )
            .unwrap_err(),
            AgentError::BadRequest { field: "cursor" }
        );
    }
    // A diff cursor also pins the `from` revision.
    let page = diff(
        &edited.meta,
        base.meta.rev,
        &base.state,
        &edited.state,
        &DiffOptions {
            max_bytes: Some(500),
            ..Default::default()
        },
    )
    .unwrap();
    if let Some(c) = page.data.more {
        let ok = diff(
            &edited.meta,
            base.meta.rev,
            &base.state,
            &edited.state,
            &DiffOptions {
                cursor: Some(c.clone()),
                ..Default::default()
            },
        );
        assert!(ok.is_ok());
        let err = diff(
            &edited.meta,
            base.meta.rev - 1,
            &base.state,
            &edited.state,
            &DiffOptions {
                cursor: Some(c),
                ..Default::default()
            },
        )
        .unwrap_err();
        assert_eq!(err, AgentError::BadRequest { field: "cursor" });
    }
}

#[test]
fn find_and_files_page_through() {
    let d = fixtures::document();
    let fx = &d.base;
    let want = find(
        &fx.meta,
        &fx.state,
        &FindOptions {
            q: "the".into(),
            limit: Some(100),
            ..Default::default()
        },
    )
    .unwrap();
    assert!(want.data.total >= 8);
    let mut cursor = None;
    let mut got = Vec::new();
    loop {
        let r = find(
            &fx.meta,
            &fx.state,
            &FindOptions {
                q: "the".into(),
                limit: Some(3),
                cursor: cursor.clone(),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(r.data.total, want.data.total);
        got.extend(r.data.hits.clone());
        match r.data.more {
            Some(c) => cursor = Some(c),
            None => break,
        }
    }
    assert_eq!(got, want.data.hits);
    // Bounds and bad patterns.
    assert_eq!(
        find(&fx.meta, &fx.state, &FindOptions::default()).unwrap_err(),
        AgentError::BadRequest { field: "q" }
    );
    assert!(matches!(
        find(
            &fx.meta,
            &fx.state,
            &FindOptions {
                q: "x".repeat(201),
                ..Default::default()
            }
        )
        .unwrap_err(),
        AgentError::LimitExceeded { field: "q", .. }
    ));
    for pattern in ["(", "(?=a)b", r"(a)\1", "[a-"] {
        let e = find(
            &fx.meta,
            &fx.state,
            &FindOptions {
                q: pattern.into(),
                regex: true,
                ..Default::default()
            },
        )
        .unwrap_err();
        assert_eq!(e, AgentError::BadRequest { field: "q" }, "{pattern}");
    }
    // A pattern that backtracks catastrophically elsewhere finishes at once.
    let started = std::time::Instant::now();
    find(
        &fx.meta,
        &fx.state,
        &FindOptions {
            q: "(a+)+$".into(),
            regex: true,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(started.elapsed().as_secs() < 5);
    // Scope that does not fit the kind.
    assert_eq!(
        find(
            &fx.meta,
            &fx.state,
            &FindOptions {
                q: "a".into(),
                scope: engine_agent::find::Scope::Formulas,
                ..Default::default()
            }
        )
        .unwrap_err(),
        AgentError::BadRequest { field: "in" }
    );

    let project = uuid::Uuid::from_u128(7);
    let entries: Vec<FileEntry> = (0..120u128)
        .map(|i| FileEntry {
            id: engine_shared::DocumentId(uuid::Uuid::from_u128(i + 1)),
            kind: FileKind::Document,
            rev: i as u64,
            project_id: project,
            project: "P".into(),
            role: "viewer".into(),
            title: format!("File {i:03}"),
        })
        .collect();
    let mut cursor = None;
    let mut seen = Vec::new();
    loop {
        let r = files(
            &entries,
            &FilesOptions {
                limit: Some(50),
                cursor: cursor.clone(),
                ..Default::default()
            },
        )
        .unwrap();
        seen.extend(r.data.files.iter().map(|f| f.title.clone()));
        match r.data.more {
            Some(c) => cursor = Some(c),
            None => break,
        }
    }
    let mut want: Vec<_> = entries.iter().map(|e| e.title.clone()).collect();
    want.sort();
    assert_eq!(seen, want);
    assert!(matches!(
        files(
            &entries,
            &FilesOptions {
                limit: Some(51),
                ..Default::default()
            }
        )
        .unwrap_err(),
        AgentError::LimitExceeded { limit: 50, .. }
    ));
    assert_eq!(
        files(
            &entries,
            &FilesOptions {
                limit: Some(0),
                ..Default::default()
            }
        )
        .unwrap_err(),
        AgentError::BadRequest { field: "limit" }
    );
    assert_eq!(
        files(
            &entries,
            &FilesOptions {
                q: Some("file 07".into()),
                ..Default::default()
            }
        )
        .unwrap()
        .data
        .n,
        10
    );
}

// ---------------------------------------------------------------- behaviour

#[test]
fn outline_cache_key_and_empty_state() {
    let d = fixtures::document();
    let first = outline(&d.base.meta, &d.base.state, &OutlineOptions::default()).unwrap();
    let oh = first.data.oh.clone().unwrap();
    let again = outline(
        &d.base.meta,
        &d.base.state,
        &OutlineOptions {
            if_none_match: Some(oh.clone()),
            ..Default::default()
        },
    )
    .unwrap();
    assert!(
        again.data.unchanged
            && again.text.lines().count() == 1
            && again.text.contains("unchanged=1")
    );
    // After the edit session the key differs, so the outline comes back.
    let edited = outline(
        &d.edited.meta,
        &d.edited.state,
        &OutlineOptions {
            if_none_match: Some(oh.clone()),
            ..Default::default()
        },
    )
    .unwrap();
    assert!(!edited.data.unchanged);
    assert_ne!(edited.data.oh.unwrap(), oh);
    // A bad key.
    assert_eq!(
        outline(
            &d.base.meta,
            &d.base.state,
            &OutlineOptions {
                if_none_match: Some("zz".into()),
                ..Default::default()
            }
        )
        .unwrap_err(),
        AgentError::BadRequest {
            field: "if_none_match"
        }
    );
    // rev 0: the empty initial state.
    let empty = Fixture {
        meta: engine_agent::model::FileMeta {
            id: d.base.meta.id,
            kind: FileKind::Document,
            rev: 0,
            chain: "0".repeat(64),
        },
        state: DocumentState::default(),
    };
    let r = outline(&empty.meta, &empty.state, &OutlineOptions::default()).unwrap();
    assert!(
        r.text.starts_with(
            "dd/1 outline file=7034d450 rev=0 chain=00000000 kind=document blocks=0 words=0 "
        ),
        "{}",
        r.text
    );
    let r = read(
        &empty.meta,
        &empty.state,
        &ReadOptions {
            refs: vec!["AB12".into()],
            ..Default::default()
        },
    );
    assert!(matches!(r.unwrap_err(), AgentError::UnknownRef { .. }));
    // Depth, preview and limit ranges.
    assert_eq!(
        outline(
            &d.base.meta,
            &d.base.state,
            &OutlineOptions {
                depth: Some(2),
                ..Default::default()
            }
        )
        .unwrap_err(),
        AgentError::BadRequest { field: "depth" }
    );
    assert!(matches!(
        outline(
            &d.base.meta,
            &d.base.state,
            &OutlineOptions {
                preview: Some(121),
                ..Default::default()
            }
        )
        .unwrap_err(),
        AgentError::LimitExceeded {
            field: "preview",
            ..
        }
    ));
    assert!(matches!(
        outline(
            &d.base.meta,
            &d.base.state,
            &OutlineOptions {
                limit: Some(1001),
                ..Default::default()
            }
        )
        .unwrap_err(),
        AgentError::LimitExceeded { field: "limit", .. }
    ));
}

#[test]
fn sections_follow_heading_levels() {
    let d = fixtures::document();
    let fx = &d.base;
    let idx = RefIndex::from_state(&fx.state);
    let rf = |n: &str| idx.ref_of(&d.ids[n]).unwrap().to_string();
    let sec = |n: &str| {
        let r = read(
            &fx.meta,
            &fx.state,
            &ReadOptions {
                section: Some(rf(n)),
                max_bytes: Some(64_000),
                ..Default::default()
            },
        )
        .unwrap();
        r.data.items.iter().map(|i| i.r.clone()).collect::<Vec<_>>()
    };
    // H1 holds everything; H2 stops at the next H2; H3 runs to the end.
    let o = outline(
        &fx.meta,
        &fx.state,
        &OutlineOptions {
            depth: Some(0),
            ..Default::default()
        },
    )
    .unwrap();
    let total = o.data.count;
    assert_eq!(sec("title").len(), total);
    assert_eq!(
        sec("purpose_h"),
        ["purpose_h", "purpose", "bengali", "atoms", "bullets"].map(rf)
    );
    // The depth-0 outline counts the blocks under each heading; reading the section gives
    // the heading plus exactly those blocks.
    let rows = o.data.blocks.unwrap();
    assert_eq!(rows.len(), 5);
    for (row, name) in
        rows.iter()
            .zip(["title", "purpose_h", "scope_h", "procedure_h", "appendix_h"])
    {
        assert_eq!(row.r, rf(name));
        assert_eq!(sec(name).len(), row.under.unwrap().0 + 1, "{name}");
    }
    assert_eq!(rows[0].under.unwrap().0, total - 1);
    assert_eq!(rows[1].under.unwrap().0, 4);
}

#[test]
fn have_hashes_and_formats() {
    let d = fixtures::document();
    let fx = &d.base;
    let idx = RefIndex::from_state(&fx.state);
    let intro = idx.ref_of(&d.ids["intro"]).unwrap().to_string();
    let plain = read(
        &fx.meta,
        &fx.state,
        &ReadOptions {
            refs: vec![intro.clone()],
            ..Default::default()
        },
    )
    .unwrap();
    let h = plain.data.items[0].h.clone().unwrap();
    // The hash is the engine fingerprint of the content, 6 characters.
    let content = &fx.state.nodes[&d.ids["intro"]].current_fields["content"];
    assert_eq!(h, block_hash(content).h6());
    // A stale hash gets the text; 6 and 12 characters and upper case both match.
    for have in [
        h.clone(),
        block_hash(content).h12().to_string(),
        h.to_uppercase(),
    ] {
        let r = read(
            &fx.meta,
            &fx.state,
            &ReadOptions {
                refs: vec![intro.clone()],
                have: BTreeMap::from([(intro.to_lowercase(), have)]),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(
            r.data.items[0].equal && r.text.contains(&format!("@{intro} = #{h}\n")),
            "{}",
            r.text
        );
    }
    let r = read(
        &fx.meta,
        &fx.state,
        &ReadOptions {
            refs: vec![intro.clone()],
            have: BTreeMap::from([(intro.clone(), "000000".into())]),
            ..Default::default()
        },
    )
    .unwrap();
    assert!(!r.data.items[0].equal);
    assert_eq!(
        read(
            &fx.meta,
            &fx.state,
            &ReadOptions {
                refs: vec![intro.clone()],
                have: BTreeMap::from([(intro.clone(), "xyz".into())]),
                ..Default::default()
            }
        )
        .unwrap_err(),
        AgentError::BadRequest { field: "have" }
    );
    // A re-read of unchanged blocks is one short line each.
    let all: Vec<String> = fx
        .state
        .nodes
        .values()
        .filter(|n| !n.deleted && n.current_fields["content"].is_object())
        .map(|n| idx.ref_of(&n.id).unwrap().to_string())
        .collect();
    let first = read(
        &fx.meta,
        &fx.state,
        &ReadOptions {
            refs: all.clone(),
            max_bytes: Some(64_000),
            ..Default::default()
        },
    )
    .unwrap();
    let have: BTreeMap<String, String> = first
        .data
        .items
        .iter()
        .map(|i| (i.r.clone(), i.h.clone().unwrap()))
        .collect();
    let second = read(
        &fx.meta,
        &fx.state,
        &ReadOptions {
            refs: all,
            have: have.clone(),
            max_bytes: Some(64_000),
            ..Default::default()
        },
    )
    .unwrap();
    assert!(second.data.items.iter().all(|i| i.equal));
    assert!(
        second.text.len() * 3 < first.text.len(),
        "{} vs {}",
        second.text.len(),
        first.text.len()
    );
    // After an edit the block's hash changes and the text returns.
    let e = &d.edited;
    let eidx = RefIndex::from_state(&e.state);
    let purpose = eidx.ref_of(&d.ids["purpose"]).unwrap().to_string();
    let was = have[&purpose].clone();
    let r = read(
        &e.meta,
        &e.state,
        &ReadOptions {
            refs: vec![purpose.clone()],
            have: BTreeMap::from([(purpose, was)]),
            ..Default::default()
        },
    )
    .unwrap();
    assert!(!r.data.items[0].equal);
}

#[test]
fn tool_output_is_deterministic() {
    let render = || {
        let d = fixtures::document();
        let idx = RefIndex::from_state(&d.base.state);
        let refs: Vec<String> = d
            .ids
            .values()
            .filter(|id| {
                d.base
                    .state
                    .nodes
                    .get(*id)
                    .is_some_and(|n| !n.deleted && n.current_fields["content"].is_object())
            })
            .filter_map(|id| idx.ref_of(id))
            .map(|r| r.to_string())
            .collect();
        let a = outline(&d.base.meta, &d.base.state, &OutlineOptions::default())
            .unwrap()
            .text;
        let b = read(
            &d.base.meta,
            &d.base.state,
            &ReadOptions {
                refs,
                max_bytes: Some(64_000),
                ..Default::default()
            },
        )
        .unwrap()
        .text;
        let c = diff(
            &d.edited.meta,
            d.base.meta.rev,
            &d.base.state,
            &d.edited.state,
            &DiffOptions {
                detail: Detail::Text,
                ..Default::default()
            },
        )
        .unwrap()
        .text;
        (a, b, c)
    };
    assert_eq!(render(), render());
}

// ---------------------------------------------------------------- vectors

#[test]
fn ref_vectors() {
    // Inputs chosen to cover: the floor of 4, a longer suffix forced by a neighbour, a
    // tombstone that still counts, case-insensitive input, ambiguity, unknown, full ULID.
    let ids = [
        "01K7Q9XM2A0000000000AB1234",
        "01K7Q9XM2A0000000000CD1234",
        "01K7Q9XM2A0000000000EF5678",
        "01K7Q9XM2A0000000000GH9ZZZ",
        "01K7Q9XM2A0000000000XXAB12",
    ]
    .map(|s| NodeId(s.into()));
    let deleted = [ids[1].clone()];
    let later = NodeId("01K7Q9XM2A0000000000QQ1234".into());
    let at_rev = RefIndex::build(&ids);
    let after = RefIndex::build(ids.iter().chain([&later]));
    let show = |idx: &RefIndex, r: &str| -> Value {
        match idx.resolve(&Ref::parse(r).unwrap()) {
            Ok(id) => json!({"id": id.0}),
            Err(ResolveError::Unknown) => json!("unknown"),
            Err(ResolveError::Ambiguous(c)) => {
                json!({"ambiguous": c.iter().map(|r| r.to_string()).collect::<Vec<_>>()})
            }
        }
    };
    let probes = [
        "AB1234",
        "ab1234",
        "1234",
        "EF5678",
        "5678",
        "ZZZZ",
        "AB12",
        "XXAB12",
        "QQ1234",
        "01K7Q9XM2A0000000000EF5678",
        "9ZZZ",
    ];
    let value = json!({
        "description": "Minimal unique suffix refs (protocol spec 5.3). `deleted` ids are tombstones: they still count for uniqueness. `later` is created after the revision: it never matches there.",
        "ids": ids.iter().map(|i| &i.0).collect::<Vec<_>>(),
        "deleted": deleted.iter().map(|i| &i.0).collect::<Vec<_>>(),
        "later": later.0,
        "refs": ids.iter().map(|i| (i.0.clone(), at_rev.ref_of(i).unwrap().to_string())).collect::<BTreeMap<_, _>>(),
        "refs_after_later": ids.iter().map(|i| (i.0.clone(), after.ref_of(i).unwrap().to_string())).collect::<BTreeMap<_, _>>(),
        "resolve_at_rev": probes.iter().map(|p| ((*p).to_string(), show(&at_rev, p))).collect::<BTreeMap<_, _>>(),
        "resolve_after_later": probes.iter().map(|p| ((*p).to_string(), show(&after, p))).collect::<BTreeMap<_, _>>(),
    });
    check_json("refs.json", &value);
    // And the file says what the spec requires.
    let v: Value =
        serde_json::from_str(&std::fs::read_to_string(vectors_dir().join("refs.json")).unwrap())
            .unwrap();
    assert_eq!(v["refs"]["01K7Q9XM2A0000000000EF5678"], "5678");
    assert_eq!(v["resolve_at_rev"]["QQ1234"], "unknown");
    assert_eq!(v["resolve_after_later"]["QQ1234"], json!({"id": later.0}));
    assert_eq!(
        v["resolve_at_rev"]["1234"]["ambiguous"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
}

#[test]
fn hash_vectors() {
    let content = json!({"type": "paragraph", "content": [{"type": "text", "text": "Hello ", "marks": [{"type": "bold"}]}, {"type": "text", "text": "world"}]});
    let reordered = json!({"content": [{"marks": [{"type": "bold"}], "text": "Hello ", "type": "text"}, {"text": "world", "type": "text"}], "type": "paragraph"});
    assert_eq!(block_hash(&content), block_hash(&reordered));
    let cells: Vec<String> = ["Region", "Q1", "=SUM(B2:D2)"].map(String::from).to_vec();
    let grid = vec![cells.clone(), vec!["North".into(), "120".into(), "".into()]];
    let pairs = vec![
        (Ref::parse("27DA").unwrap(), block_hash(&content)),
        (
            Ref::parse("J1KB").unwrap(),
            block_hash(&json!({"type": "heading"})),
        ),
    ];
    let value = json!({
        "description": "Hashes of protocol spec 5.4: SHA-256 over canonical JSON (sorted keys, compact) via engine_core::richtext::fingerprint. Generated by this crate; the TypeScript side must reproduce them.",
        "block": {"content": content, "hex": block_hash(&content).as_hex()},
        "row": {"cells": cells, "hex": row_hash(&cells).as_hex()},
        "range": {"rows": grid, "hex": range_hash(&grid).as_hex()},
        "shape": {"kind": "text", "role": "title", "text": "Highlights", "hex": shape_hash("text", Some("title"), "Highlights").as_hex()},
        "shape_no_role": {"kind": "image", "role": null, "text": "", "hex": shape_hash("image", None, "").as_hex()},
        "notes": {"notes": "Mention the audit finding first.", "hex": notes_hash("Mention the audit finding first.").as_hex()},
        "outline": {"pairs": pairs.iter().map(|(r, h)| json!([r.as_str(), h.h6()])).collect::<Vec<_>>(), "hex": outline_hash(&pairs).as_hex()},
    });
    check_json("hashes.json", &value);
    let v: Value =
        serde_json::from_str(&std::fs::read_to_string(vectors_dir().join("hashes.json")).unwrap())
            .unwrap();
    let canonical = |v: &Value| Hash::of(v).as_hex().to_string();
    assert_eq!(v["block"]["hex"], canonical(&v["block"]["content"]));
    assert_eq!(
        v["row"]["hex"],
        canonical(&json!({"cell_0": "Region", "cell_1": "Q1", "cell_2": "=SUM(B2:D2)"}))
    );
    assert_eq!(v["range"]["hex"], canonical(&v["range"]["rows"]));
    assert_eq!(
        v["shape"]["hex"],
        canonical(&json!({"kind": "text", "role": "title", "text": "Highlights"}))
    );
    assert_eq!(
        v["notes"]["hex"],
        canonical(&json!({"notes": "Mention the audit finding first."}))
    );
    assert_eq!(
        v["outline"]["hex"],
        canonical(&json!(format!(
            "27DA:{},J1KB:{};n=2",
            block_hash(&content).h6(),
            block_hash(&json!({"type": "heading"})).h6()
        )))
    );
}

// ---------------------------------------------------------------- the crate's own rules

#[test]
fn source_has_no_clock_randomness_or_unordered_maps() {
    // Deterministic output is a promise of the crate: ordered maps only, no clock, no
    // randomness, no I/O. Checked on the text of every module.
    let sources: &[(&str, &str)] = &[
        ("a1", include_str!("../src/a1.rs")),
        ("budget", include_str!("../src/budget.rs")),
        ("corpus", include_str!("../src/corpus.rs")),
        ("dct", include_str!("../src/dct.rs")),
        ("diff", include_str!("../src/diff.rs")),
        ("doc", include_str!("../src/doc.rs")),
        ("error", include_str!("../src/error.rs")),
        ("escape", include_str!("../src/escape.rs")),
        ("files", include_str!("../src/files.rs")),
        ("find", include_str!("../src/find.rs")),
        ("fixtures", include_str!("../src/fixtures.rs")),
        ("hash", include_str!("../src/hash.rs")),
        ("lib", include_str!("../src/lib.rs")),
        ("model", include_str!("../src/model.rs")),
        ("outline", include_str!("../src/outline.rs")),
        ("paging", include_str!("../src/paging.rs")),
        ("parse", include_str!("../src/parse.rs")),
        ("read", include_str!("../src/read.rs")),
        ("refs", include_str!("../src/refs.rs")),
        ("response", include_str!("../src/response.rs")),
        ("rev", include_str!("../src/rev.rs")),
        ("sheet", include_str!("../src/sheet.rs")),
        ("slides", include_str!("../src/slides.rs")),
        ("text", include_str!("../src/text.rs")),
    ];
    let forbidden = [
        "HashMap",
        "HashSet",
        "SystemTime",
        "Instant",
        "thread_rng",
        "rand::",
        "Utc::now",
        "std::fs",
        "std::net",
        "tokio",
        "sqlx::",
    ];
    for (name, text) in sources {
        // Tests inside the modules may time things; only the non-test part is checked.
        let code = text.split("#[cfg(test)]").next().unwrap();
        for word in forbidden {
            assert!(!code.contains(word), "{name}.rs mentions {word}");
        }
    }
}

#[test]
fn headers_parse_for_every_tool() {
    let d = fixtures::document();
    let fx = &d.base;
    let texts = [
        outline(&fx.meta, &fx.state, &OutlineOptions::default())
            .unwrap()
            .text,
        find(
            &fx.meta,
            &fx.state,
            &FindOptions {
                q: "policy".into(),
                ..Default::default()
            },
        )
        .unwrap()
        .text,
        diff(
            &d.edited.meta,
            fx.meta.rev,
            &fx.state,
            &d.edited.state,
            &DiffOptions::default(),
        )
        .unwrap()
        .text,
    ];
    for t in texts {
        let first = t.lines().next().unwrap();
        let h = parse_header(first).unwrap();
        assert_eq!(h.get("file"), Some("7034d450"));
        assert!(h.get("~tok").is_some());
    }
}
