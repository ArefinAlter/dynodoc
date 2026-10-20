//! The agent read routes (docs/AI-AGENT-PROTOCOL-SPEC.md section 12.1, routes 1 to 9,
//! 14 and 15) through the real router and the restricted runtime login: credentials and
//! scopes, grants, roles, the gateway flag, ETags, budgets and cursors, hostile document
//! text, and the rule that a read writes nothing but call metadata. Requires a reachable
//! Postgres (DATABASE_URL).

use axum::http::StatusCode;
use engine_agent::{dct::Tool, fixtures, model::FileKind, parse};
use engine_api::{
    app,
    auth::{
        token::{self, DeviceClaims},
        AuthConfig,
    },
    AppState,
};
use engine_core::repository::device::{Challenge, Proof};
use engine_shared::DocumentId;
use ring::signature::KeyPair;
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

mod common;

mod agent_world;
use agent_world::*;

// -------------------------------------------------------------------------- files

/// A document with headings, paragraphs, a list and a table. Returns the id.
async fn policy(w: &World) -> Uuid {
    let mut ops = Ops::default();
    let root = ops.node("form", None, 0, json!({"label":"Q3 Policy"}));
    ops.block(&root, 10, heading(1, "Introduction"));
    ops.block(
        &root,
        20,
        para("The purpose of this policy is to describe how the organisation handles complaints."),
    );
    ops.block(&root, 30, heading(2, "Scope"));
    ops.block(
        &root,
        40,
        para("Each clause applies to all staff. This clause is binding on contractors too."),
    );
    ops.block(
        &root,
        50,
        json!({"type":"bulletList","content":[
            {"type":"listItem","content":[para("First item")]},
            {"type":"listItem","content":[para("Second item")]}]}),
    );
    ops.block(
        &root,
        60,
        json!({"type":"table","content":[
            {"type":"tableRow","content":[
                {"type":"tableHeader","attrs":{"colspan":1,"rowspan":1},"content":[para("Name")]},
                {"type":"tableHeader","attrs":{"colspan":1,"rowspan":1},"content":[para("Owner")]}]},
            {"type":"tableRow","content":[
                {"type":"tableCell","attrs":{"colspan":1,"rowspan":1},"content":[para("Intake")]},
                {"type":"tableCell","attrs":{"colspan":1,"rowspan":1},"content":[para("Ana")]}]}]}),
    );
    w.file("Q3 Policy", "document", ops).await
}

fn cells(values: &[&str]) -> Value {
    let mut fields = serde_json::Map::new();
    for (i, v) in values.iter().enumerate() {
        fields.insert(format!("cell_{i}"), json!(v));
    }
    Value::Object(fields)
}

/// A workbook with a `Budget` sheet of numbers and formulas and a `Notes` sheet.
async fn budget(w: &World) -> Uuid {
    let mut ops = Ops::default();
    let root = ops.node("form", None, 0, json!({"label":"Budget 2026"}));
    let layout =
        json!({"widths":{},"hiddenColumns":[],"frozenRows":1,"frozenColumns":0,"merges":[]});
    let sheet = ops.node(
        "section",
        Some(&root),
        1,
        json!({"label":"Budget","kind":"sheet","columns":4,"sheet_layout":layout}),
    );
    ops.node(
        "item",
        Some(&sheet),
        0,
        cells(&["Region", "Q1", "Q2", "Total"]),
    );
    ops.node(
        "item",
        Some(&sheet),
        1,
        cells(&["North", "120", "130", "=SUM(B2:C2)"]),
    );
    ops.node(
        "item",
        Some(&sheet),
        2,
        cells(&["South", "98", "101", "=SUM(B3:C3)"]),
    );
    ops.node(
        "item",
        Some(&sheet),
        3,
        cells(&["East", "5", "9", "=SUM(B4:C4)"]),
    );
    let notes = ops.node(
        "section",
        Some(&root),
        2,
        json!({"label":"Notes","kind":"sheet","columns":2}),
    );
    ops.node(
        "item",
        Some(&notes),
        0,
        cells(&["idea 1", "review the clause wording"]),
    );
    w.file("Budget", "spreadsheet", ops).await
}

fn shape(role: &str, kind: &str, text: &str) -> Value {
    json!({"kind":kind,"text":text,"label":text,"role":role,"x":10,"y":10,"w":80,"h":20,"style":{}})
}

/// A two-slide deck with notes.
async fn deck(w: &World) -> Uuid {
    let mut ops = Ops::default();
    let root = ops.node(
        "form",
        None,
        0,
        json!({"label":"Board update","slide_size":"16:9","theme":"office"}),
    );
    let s1 = ops.node(
        "section",
        Some(&root),
        1,
        json!({"label":"Board update","kind":"slide","layout":"title","notes":"","hidden":false}),
    );
    ops.node("item", Some(&s1), 1, shape("title", "text", "Board update"));
    ops.node("item", Some(&s1), 2, shape("subtitle", "text", "Q3 2026"));
    let s2 = ops.node(
        "section",
        Some(&root),
        2,
        json!({"label":"Highlights","kind":"slide","layout":"title-content",
               "notes":"Mention the audit finding first.","hidden":false}),
    );
    ops.node("item", Some(&s2), 1, shape("title", "text", "Highlights"));
    ops.node(
        "item",
        Some(&s2),
        2,
        shape(
            "body",
            "text",
            "Revenue up 4 percent\nTwo contracts renewed",
        ),
    );
    w.file("Board", "presentation", ops).await
}

/// Counts of every table a read must not touch.
async fn footprint(w: &World) -> Vec<(String, i64)> {
    let mut out = Vec::new();
    for table in [
        "event",
        "node",
        "snapshot",
        "document",
        "workspace_draft",
        "draft_edit",
        "operation_audit",
        "document_access",
        "space_member",
        "notification",
        "idempotency_key",
        "device_session",
        "device_key_project_grant",
        "change_request_review",
        "provenance_bundle",
        "document_version",
    ] {
        let n: i64 = sqlx::query_scalar(&format!("select count(*) from {table}"))
            .fetch_one(&w.runtime.owner)
            .await
            .unwrap();
        out.push((table.to_owned(), n));
    }
    out
}

/// The chain head of every document.
async fn heads(w: &World) -> Vec<(Uuid, Vec<u8>)> {
    sqlx::query_as("select distinct on (document_id) document_id,chain_hash from event order by document_id,seq desc")
        .fetch_all(&w.runtime.owner)
        .await
        .unwrap()
}

async fn calls_of(w: &World, document: Uuid) -> i64 {
    sqlx::query_scalar("select count(*) from agent_call where document_id=$1")
        .bind(document)
        .fetch_one(&w.runtime.owner)
        .await
        .unwrap()
}

async fn calls(w: &World) -> i64 {
    sqlx::query_scalar("select count(*) from agent_call")
        .fetch_one(&w.runtime.owner)
        .await
        .unwrap()
}

// ------------------------------------------------------------------------- tests

#[sqlx::test(migrations = "../../migrations")]
async fn every_route_reads_a_document_a_workbook_and_a_deck(pool: PgPool) {
    let w = World::new(pool, true).await;
    let doc = policy(&w).await;
    let wb = budget(&w).await;
    let dk = deck(&w).await;
    let (_, token) = w.owner_reader().await;

    // 1. files, JSON and DCT.
    let files = get(&w.app, &w.url("/files"), &token).await;
    assert_eq!(files.code(), 200, "{}", files.text());
    assert_eq!(files.header("content-type").unwrap(), "application/json");
    let body = files.json();
    assert_eq!(body["protocol"], "dynodoc.agent/1");
    assert_eq!(body["tool"], "files");
    assert_eq!(body["trust"], "data");
    assert_eq!(body["n"], 3);
    let titles: Vec<&str> = body["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["title"].as_str().unwrap())
        .collect();
    assert_eq!(titles, ["Board", "Budget", "Q3 Policy"]);
    for f in body["files"].as_array().unwrap() {
        assert_eq!(f["role"], "owner");
        assert!(f["file"].as_str().unwrap().len() == 8);
    }
    let dct = get_dct(&w.app, &w.url("/files"), &token).await;
    assert_eq!(
        dct.header("content-type").unwrap(),
        "text/vnd.dynodoc.dct; v=1; charset=utf-8"
    );
    let text = dct.text();
    let (header, lines) = engine_agent::dct::split_response(&text).unwrap();
    assert_eq!(header.tool, Tool::Files);
    assert_eq!(parse::files_lines(&lines).unwrap().len(), 3);
    for (query, expected) in [("?kind=spreadsheet", "Budget"), ("?q=policy", "Q3 Policy")] {
        let one = get(&w.app, &w.url(&format!("/files{query}")), &token).await;
        assert_eq!(one.json()["files"][0]["title"], expected);
        assert_eq!(one.json()["n"], 1);
    }

    // 2. outline: byte-for-byte what the library produces for the same state.
    let (meta, state) = w.expected(doc, FileKind::Document).await;
    let want = engine_agent::outline::outline(&meta, &state, &Default::default()).unwrap();
    let json = get(&w.app, &w.url(&format!("/files/{doc}/outline")), &token).await;
    assert_eq!(json.code(), 200, "{}", json.text());
    assert_eq!(json.text(), want.json_mirror(Tool::Outline));
    let dct = get_dct(&w.app, &w.url(&format!("/files/{doc}/outline")), &token).await;
    assert_eq!(dct.text(), want.text);
    assert_eq!(json.header("etag").unwrap(), w.etag(doc).await);
    assert_eq!(json.header("vary").unwrap(), "Accept");
    assert!(json.header("cache-control").unwrap().contains("no-cache"));
    // The 8-character file prefix names the same file.
    let short = &doc.simple().to_string()[..8];
    let by_prefix = get(&w.app, &w.url(&format!("/files/{short}/outline")), &token).await;
    assert_eq!(by_prefix.text(), json.text());

    // 3. blocks and 4. blocks:read with the hash cache.
    let blocks: Vec<Value> = json.json()["blocks"].as_array().unwrap().clone();
    assert_eq!(blocks.len(), 6);
    let (first, second) = (
        blocks[0]["ref"].as_str().unwrap(),
        blocks[1]["ref"].as_str().unwrap(),
    );
    let read = get(
        &w.app,
        &w.url(&format!("/files/{doc}/blocks?refs={first},{second}")),
        &token,
    )
    .await;
    assert_eq!(read.code(), 200, "{}", read.text());
    let items = read.json()["items"].as_array().unwrap().clone();
    assert_eq!(items.len(), 2);
    assert_eq!(items[0]["lines"][0], "Introduction");
    let want = engine_agent::read::read(
        &meta,
        &state,
        &engine_agent::read::ReadOptions {
            refs: vec![first.into(), second.into()],
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(read.text(), want.json_mirror(Tool::Read));
    let hash = blocks[0]["h"].as_str().unwrap();
    let batch = raw(
        &w.app,
        "POST",
        &w.url(&format!("/files/{doc}/blocks:read")),
        Some(&token),
        &[("accept", DCT)],
        Some(json!({"refs":[first,second],"have":{first:hash}})),
    )
    .await;
    assert_eq!(batch.code(), 200, "{}", batch.text());
    let text = batch.text();
    assert!(text.contains(&format!("@{first} = #{hash}")), "{text}");
    assert!(text.contains("The purpose of this policy"), "{text}");
    let section = get(
        &w.app,
        &w.url(&format!(
            "/files/{doc}/blocks?section={}",
            blocks[2]["ref"].as_str().unwrap()
        )),
        &token,
    )
    .await;
    assert_eq!(section.code(), 200, "{}", section.text());
    assert!(section.json()["n"].as_u64().unwrap() >= 2);
    // Exactly one selector.
    let none = get(&w.app, &w.url(&format!("/files/{doc}/blocks")), &token).await;
    assert_eq!(none.code(), 400);
    assert_eq!(
        none.json()["details"],
        json!({"agent_code":"bad_request","field":"selector"})
    );

    // 8. find.
    let found = get(
        &w.app,
        &w.url(&format!("/files/{doc}/find?q=clause&word=true")),
        &token,
    )
    .await;
    assert_eq!(found.code(), 200, "{}", found.text());
    assert_eq!(found.json()["total"], 2);
    assert_eq!(found.json()["hits"].as_array().unwrap().len(), 2);
    let regex = get(
        &w.app,
        &w.url(&format!(
            "/files/{doc}/find?q=cla%5Ba-z%5D%2B&regex=true&limit=1"
        )),
        &token,
    )
    .await;
    assert_eq!(regex.json()["n"], 1);
    assert!(regex.json()["more"].is_string());

    // 5. cells and 6. profile of the workbook.
    let range = get(
        &w.app,
        &w.url(&format!("/files/{wb}/cells?range=Budget!A1:D4")),
        &token,
    )
    .await;
    assert_eq!(range.code(), 200, "{}", range.text());
    let rows = range.json()["range"]["rows"].as_array().unwrap().clone();
    assert_eq!(rows.len(), 4);
    assert_eq!(rows[1]["cells"][3], "=SUM(B2:C2)");
    assert_eq!(range.json()["range"]["calc"], "none");
    let refs = get(
        &w.app,
        &w.url(&format!("/files/{wb}/cells?range=Budget!A1:B2&rows=refs")),
        &token,
    )
    .await;
    assert!(refs.json()["range"]["rows"][0]["ref"].is_string());
    let profile = get(
        &w.app,
        &w.url(&format!("/files/{wb}/sheets/Budget/profile")),
        &token,
    )
    .await;
    assert_eq!(profile.code(), 200, "{}", profile.text());
    assert_eq!(
        profile.json()["profile"]["columns"]
            .as_array()
            .unwrap()
            .len(),
        4
    );
    assert_eq!(profile.json()["profile"]["name"], "Budget");
    let (wmeta, wstate) = w.expected(wb, FileKind::Spreadsheet).await;
    let want = engine_agent::outline::outline(
        &wmeta,
        &wstate,
        &engine_agent::outline::OutlineOptions {
            depth: Some(1),
            under: Some("Budget".into()),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(profile.text(), want.json_mirror(Tool::Outline));
    // A sheet profile of a document is a client error; an unknown sheet too.
    let bad = get(
        &w.app,
        &w.url(&format!("/files/{doc}/sheets/Budget/profile")),
        &token,
    )
    .await;
    assert_eq!(bad.code(), 400);
    let sheet_list = get(
        &w.app,
        &w.url(&format!("/files/{wb}/outline?depth=0")),
        &token,
    )
    .await;
    assert_eq!(sheet_list.json()["sheets"].as_array().unwrap().len(), 2);

    // 7. slides.
    let slides = get(
        &w.app,
        &w.url(&format!("/files/{dk}/slides?ids=s2")),
        &token,
    )
    .await;
    assert_eq!(slides.code(), 200, "{}", slides.text());
    let slide = &slides.json()["slides"][0];
    assert_eq!(slide["layout"], "title-content");
    assert_eq!(slide["notes"][1], "Mention the audit finding first.");
    assert!(slide["shapes"].as_array().unwrap().len() >= 2);
    let (dmeta, dstate) = w.expected(dk, FileKind::Presentation).await;
    let want = engine_agent::read::read(
        &dmeta,
        &dstate,
        &engine_agent::read::ReadOptions {
            slides: vec!["s2".into()],
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(slides.text(), want.json_mirror(Tool::Read));

    // 9. diff after a real edit.
    let before = w.head(doc).await;
    let mut ops = Ops::default();
    let target = state
        .nodes
        .values()
        .find(|n| n.current_fields["label"] == "Introduction")
        .unwrap()
        .id
        .0
        .clone();
    ops.edit(&target, "content", heading(1, "Introduction and aims"));
    ops.edit(&target, "label", json!("Introduction and aims"));
    w.edit(doc, ops).await;
    let diff = get(
        &w.app,
        &w.url(&format!("/files/{doc}/diff?from={before}&detail=text")),
        &token,
    )
    .await;
    assert_eq!(diff.code(), 200, "{}", diff.text());
    assert_eq!(diff.json()["changed"], 1);
    assert_eq!(diff.json()["from"], before);
    assert_eq!(diff.json()["to"], w.head(doc).await);
    assert_eq!(diff.header("etag").unwrap(), w.etag(doc).await);
    let (meta, state) = w.expected(doc, FileKind::Document).await;
    let before_state = engine_core::snapshot::read_state_at(
        &mut w.pool().acquire().await.unwrap(),
        DocumentId(doc),
        before,
    )
    .await
    .unwrap()
    .state;
    let want = engine_agent::diff::diff(
        &meta,
        before as u64,
        &before_state,
        &state,
        &engine_agent::diff::DiffOptions {
            detail: engine_agent::diff::Detail::Text,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(diff.text(), want.json_mirror(Tool::Diff));
    // `from` is required, `from > to` is refused and a proposal is never visible.
    let missing = get(&w.app, &w.url(&format!("/files/{doc}/diff")), &token).await;
    assert_eq!(missing.json()["details"]["field"], "from");
    let future = get(
        &w.app,
        &w.url(&format!("/files/{doc}/diff?from=999")),
        &token,
    )
    .await;
    assert_eq!(future.code(), 400);
    let proposal = get(
        &w.app,
        &w.url(&format!(
            "/files/{doc}/diff?from=0&proposal={}",
            Uuid::new_v4()
        )),
        &token,
    )
    .await;
    assert_eq!(proposal.code(), 404);
    assert_eq!(proposal.json()["details"]["agent_code"], "not_found");

    // 14. access and 15. discovery.
    let access = get(&w.app, &w.url("/access"), &token).await;
    assert_eq!(access.code(), 200, "{}", access.text());
    let a = access.json();
    assert_eq!(a["scopes"], json!(["content:read"]));
    assert_eq!(a["role"], "owner");
    assert_eq!(a["write_mode"], "none");
    assert_eq!(a["project"], json!(w.project));
    assert_eq!(a["capabilities"]["propose"], false);
    let access_dct = get_dct(&w.app, &w.url("/access"), &token).await;
    assert!(
        access_dct.text().starts_with("dd/1 status what=access"),
        "{}",
        access_dct.text()
    );
    let discovery = raw(&w.app, "GET", "/.well-known/dynodoc-agent", None, &[], None).await;
    assert_eq!(discovery.code(), 200);
    let d = discovery.json();
    assert_eq!(d["protocol"], "dynodoc.agent/1");
    assert_eq!(d["dct"], "dd/1");
    assert_eq!(d["mcp"], Value::Null);
    assert_eq!(d["scopes"], json!(["content:read"]));
    assert_eq!(d["limits"]["read_max_bytes"], 64000);
    assert_eq!(d["capabilities"]["calc"], "none");
    assert_eq!(d["http"]["base"], "http://test.local");

    w.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn with_the_gateway_off_every_agent_route_is_absent(pool: PgPool) {
    // The credential is real: it was issued while the gateway was on elsewhere. Off, the
    // routes do not exist for anybody, including the discovery document.
    let on = World::new(pool.clone(), true).await;
    let doc = policy(&on).await;
    let (agent, token) = on.owner_reader().await;
    let _ = agent;
    let off = {
        let mut state = AppState::new(
            on.runtime.pool.clone(),
            AuthConfig {
                paseto_key: KEY,
                service_key: None,
                token_ttl_seconds: 3600,
                public_base_url: "http://test.local".into(),
            },
        );
        assert!(!state.agent_gateway, "off is the default");
        state.agent_gateway = false;
        app(state)
    };
    let project = on.project;
    let uris = [
        format!("/projects/{project}/agent/files"),
        format!("/projects/{project}/agent/files/{doc}/outline"),
        format!("/projects/{project}/agent/files/{doc}/blocks?refs=ABCD"),
        format!("/projects/{project}/agent/files/{doc}/cells?range=A1:B2"),
        format!("/projects/{project}/agent/files/{doc}/sheets/Budget/profile"),
        format!("/projects/{project}/agent/files/{doc}/slides?ids=s1"),
        format!("/projects/{project}/agent/files/{doc}/find?q=x"),
        format!("/projects/{project}/agent/files/{doc}/diff?from=0"),
        format!("/projects/{project}/agent/access"),
        "/.well-known/dynodoc-agent".to_owned(),
    ];
    let absent = raw(
        &off,
        "GET",
        "/projects/anything/never-existed",
        None,
        &[],
        None,
    )
    .await;
    assert_eq!(absent.code(), 404);
    for uri in &uris {
        for credential in [Some(token.as_str()), Some(on.owner_login.as_str()), None] {
            let reply = raw(&off, "GET", uri, credential, &[], None).await;
            assert_eq!(reply.code(), 404, "{uri}");
            assert_eq!(reply.bytes, absent.bytes, "{uri}: as if it never existed");
        }
    }
    let post = raw(
        &off,
        "POST",
        &format!("/projects/{project}/agent/files/{doc}/blocks:read"),
        Some(&token),
        &[],
        Some(json!({"refs":["ABCD"]})),
    )
    .await;
    assert_eq!(post.code(), 404);
    // And on, the same requests are served (a client error for the ones aimed at the
    // wrong kind of file, never a 404 for the route).
    let on_codes = [200, 200, 400, 400, 400, 400, 200, 200, 200];
    for (uri, want) in uris[..9].iter().zip(on_codes) {
        assert_eq!(get(&on.app, uri, &token).await.code(), want, "{uri}");
    }
    assert_eq!(
        raw(
            &on.app,
            "GET",
            "/.well-known/dynodoc-agent",
            None,
            &[],
            None
        )
        .await
        .code(),
        200
    );
    // The public OpenAPI document advertises the feature only while it is on.
    let spec_off = raw(&off, "GET", "/openapi.json", None, &[], None).await;
    assert_eq!(spec_off.code(), 200);
    let spec_off = spec_off.text();
    assert!(!spec_off.contains("/agent/") && !spec_off.contains("dynodoc-agent"));
    let spec_on = raw(&on.app, "GET", "/openapi.json", None, &[], None).await;
    assert!(spec_on.text().contains("/agent/files"));
    on.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn only_an_agent_session_with_the_scope_for_this_project_is_accepted(pool: PgPool) {
    let w = World::new(pool, true).await;
    let doc = policy(&w).await;
    let other = w.project("Second project").await;
    let (agent, token) = w.owner_reader().await;
    let outline = w.url(&format!("/files/{doc}/outline"));
    assert_eq!(get(&w.app, &outline, &token).await.code(), 200);

    // No credential, garbage, an account login and an expired agent token: 401.
    assert_eq!(
        raw(&w.app, "GET", &outline, None, &[], None).await.code(),
        401
    );
    assert_eq!(get(&w.app, &outline, "not-a-token").await.code(), 401);
    let account = get(&w.app, &outline, &w.owner_login).await;
    assert_eq!(account.code(), 401);
    assert_eq!(account.json()["details"]["agent_code"], "unauthorized");
    let expired = token::mint_project_device(
        &KEY,
        &DeviceClaims {
            identity: w.owner,
            key: agent.key_id,
            project: w.project,
            session: Uuid::new_v4(),
            scopes: vec!["content:read".into()],
            generation: 0,
        },
        -10,
    )
    .unwrap();
    assert_eq!(get(&w.app, &outline, &expired).await.code(), 401);

    // The agent token is refused by every other extractor: account routes and the
    // project routes that accept ordinary device tokens.
    for uri in [
        "/auth/me".to_owned(),
        "/projects".to_owned(),
        format!("/documents/{doc}"),
        format!("/projects/{}/files", w.project),
        format!("/projects/{}/history/refs", w.project),
        format!("/projects/{}/activity", w.project),
    ] {
        let code = get(&w.app, &uri, &token).await.code();
        assert!(code == 401 || code == 403, "{uri}: {code}");
    }

    // Wrong project: the same token on another project's agent routes.
    assert_eq!(
        get(&w.app, &format!("/projects/{other}/agent/files"), &token)
            .await
            .code(),
        403
    );
    // A session for a project the key has no grant for cannot even be issued.
    assert_eq!(
        w.open(&agent, other, &["content:read"]).await.0,
        StatusCode::FORBIDDEN
    );

    // A device token without the agent scope: a person's key with history scopes.
    let person_key = {
        let key = key(21);
        let (_, challenge) = call(
            &w.app,
            "POST",
            "/devices/challenge",
            Some(&w.owner_login),
            json!({"public_key":hex::encode(key.public_key().as_ref())}),
        )
        .await;
        let challenge: Challenge = serde_json::from_value(challenge).unwrap();
        let proof = Proof::sign(
            challenge.clone(),
            challenge.authority,
            challenge.identity,
            &key,
        )
        .unwrap();
        let mut body = json!(proof);
        body["project_access"] = json!("all");
        assert_eq!(
            call(&w.app, "POST", "/devices", Some(&w.owner_login), body)
                .await
                .0,
            StatusCode::OK
        );
        Agent {
            login: w.owner_login.clone(),
            key,
            key_id: challenge.id,
        }
    };
    let history = w
        .token(&person_key, w.project, &["history:read", "project:read"])
        .await;
    assert_eq!(
        get(&w.app, &format!("/projects/{}/files", w.project), &history)
            .await
            .code(),
        200
    );
    assert_eq!(get(&w.app, &outline, &history).await.code(), 403);
    // A person's key cannot hold or obtain the agent scope, even in `all` mode.
    let (status, body) = w.grant(&person_key, w.project, &["content:read"]).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(
        w.open(&person_key, w.project, &["content:read"]).await.0,
        StatusCode::FORBIDDEN
    );
    // An agent key cannot hold or obtain a history scope, and `all` mode does not
    // stand in for a grant.
    let (status, body) = w.grant(&agent, w.project, &["history:read"]).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(
        w.open(&agent, w.project, &["history:read"]).await.0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        w.open(&agent, w.project, &["content:read", "history:read"])
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    let broad = w.agent_key(&w.owner_login, 22, Some("all")).await;
    assert_eq!(
        w.open(&broad, w.project, &["content:read"]).await.0,
        StatusCode::FORBIDDEN
    );

    // A forged token for a person's key with the agent scope is refused: the key class
    // is read from the database, not from the token.
    let forged = token::mint_project_device(
        &KEY,
        &DeviceClaims {
            identity: w.owner,
            key: person_key.key_id,
            project: w.project,
            session: Uuid::new_v4(),
            scopes: vec!["content:read".into()],
            generation: 0,
        },
        3600,
    )
    .unwrap();
    assert_eq!(get(&w.app, &outline, &forged).await.code(), 403);
    // Even with a grant row forced in behind the API's back, the key class decides: a
    // person's key never reaches the agent plane, and an `all`-mode agent key reaches it
    // only through a grant.
    for key_id in [person_key.key_id, broad.key_id] {
        let forged = token::mint_project_device(
            &KEY,
            &DeviceClaims {
                identity: w.owner,
                key: key_id,
                project: w.project,
                session: Uuid::new_v4(),
                scopes: vec!["content:read".into()],
                generation: 0,
            },
            3600,
        )
        .unwrap();
        assert_eq!(get(&w.app, &outline, &forged).await.code(), 403);
        if key_id == person_key.key_id {
            sqlx::query("insert into device_key_project_grant(key_id,project_id,scopes,created_by) values($1,$2,array['content:read'],$3)")
                .bind(key_id)
                .bind(w.project)
                .bind(w.owner)
                .execute(w.pool())
                .await
                .unwrap();
            assert_eq!(get(&w.app, &outline, &forged).await.code(), 403);
        }
    }
    let (status, body) = w.grant(&broad, w.project, &["content:read"]).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let broad_token = w.token(&broad, w.project, &["content:read"]).await;
    assert_eq!(get(&w.app, &outline, &broad_token).await.code(), 200);

    // No write route exists on this plane.
    for (method, uri) in [
        ("POST", w.url(&format!("/files/{doc}/proposals"))),
        ("POST", w.url("/proposals/x/submit")),
        ("GET", w.url("/proposals/x")),
    ] {
        assert_eq!(
            raw(&w.app, method, &uri, Some(&token), &[], Some(json!({})))
                .await
                .code(),
            404,
            "{uri}"
        );
    }
    w.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn revocation_takes_effect_on_the_very_next_request(pool: PgPool) {
    let w = World::new(pool, true).await;
    let doc = policy(&w).await;
    let outline = w.url(&format!("/files/{doc}/outline"));

    // Grant revoked, then granted again (the old session works again: nothing cached).
    let (agent, token) = w.owner_reader().await;
    assert_eq!(get(&w.app, &outline, &token).await.code(), 200);
    w.ungrant(&agent, w.project).await;
    assert_eq!(get(&w.app, &outline, &token).await.code(), 403);
    assert_eq!(get(&w.app, &w.url("/files"), &token).await.code(), 403);
    assert_eq!(get(&w.app, &w.url("/access"), &token).await.code(), 403);
    assert_eq!(
        w.open(&agent, w.project, &["content:read"]).await.0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        w.grant(&agent, w.project, &["content:read"]).await.0,
        StatusCode::OK
    );
    assert_eq!(get(&w.app, &outline, &token).await.code(), 200);

    // Key revoked: 401.
    let (status, body) = call(
        &w.app,
        "POST",
        &format!("/devices/{}/revoke", agent.key_id),
        Some(&w.owner_login),
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(get(&w.app, &outline, &token).await.code(), 401);

    // Membership removed: a member who is no longer one is a non-member (403), and the
    // grant stays inert.
    let (guest, guest_login) = person(w.pool(), "guest@example.test").await;
    w.member(w.project, guest, "viewer").await;
    let (g, gtoken) = w.reader(&guest_login, 31).await;
    assert_eq!(get(&w.app, &outline, &gtoken).await.code(), 200);
    sqlx::query("delete from space_member where space_id=$1 and identity_id=$2")
        .bind(w.project)
        .bind(guest)
        .execute(w.pool())
        .await
        .unwrap();
    assert_eq!(get(&w.app, &outline, &gtoken).await.code(), 403);
    assert_eq!(get(&w.app, &w.url("/files"), &gtoken).await.code(), 403);
    let _ = g;

    // Account state: a new session generation (logout of every login) ends the token.
    let (second, second_login) = person(w.pool(), "second@example.test").await;
    w.member(w.project, second, "editor").await;
    let (_, stoken) = w.reader(&second_login, 32).await;
    assert_eq!(get(&w.app, &outline, &stoken).await.code(), 200);
    sqlx::query("update identity set session_generation=session_generation+1 where id=$1")
        .bind(second)
        .execute(w.pool())
        .await
        .unwrap();
    assert_eq!(get(&w.app, &outline, &stoken).await.code(), 401);
    let (third, third_login) = person(w.pool(), "third@example.test").await;
    w.member(w.project, third, "editor").await;
    let (_, ttoken) = w.reader(&third_login, 33).await;
    assert_eq!(get(&w.app, &outline, &ttoken).await.code(), 200);
    sqlx::query("update identity set disabled_at=now() where id=$1")
        .bind(third)
        .execute(w.pool())
        .await
        .unwrap();
    assert_eq!(get(&w.app, &outline, &ttoken).await.code(), 401);
    w.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn every_role_reads_and_a_non_member_learns_nothing(pool: PgPool) {
    let w = World::new(pool, true).await;
    let doc = policy(&w).await;
    // The same stored roles the product uses, and what each is called on a file.
    let roles = [
        ("viewer", "viewer"),
        ("reviewer", "contributor"),
        ("approver", "reviewer"),
        ("editor", "editor"),
        ("manager", "manager"),
    ];
    for (i, (stored, product)) in roles.iter().enumerate() {
        let (id, login) = person(w.pool(), &format!("{stored}@example.test")).await;
        w.member(w.project, id, stored).await;
        let (_, token) = w.reader(&login, 40 + i as u8).await;
        let files = get(&w.app, &w.url("/files"), &token).await;
        assert_eq!(files.code(), 200, "{stored}");
        assert_eq!(files.json()["files"][0]["role"], *product, "{stored}");
        assert_eq!(
            get(&w.app, &w.url(&format!("/files/{doc}/outline")), &token)
                .await
                .code(),
            200,
            "{stored}"
        );
        let access = get(&w.app, &w.url("/access"), &token).await.json();
        assert_eq!(access["role"], *product, "{stored}");
        assert_eq!(access["scopes"], json!(["content:read"]));
        // Whatever the role, the plane has nothing that writes.
        assert_eq!(
            raw(
                &w.app,
                "POST",
                &w.url(&format!("/files/{doc}/proposals")),
                Some(&token),
                &[],
                Some(json!({}))
            )
            .await
            .code(),
            404
        );
    }

    // A non-member of the project: the credential is real (their own key, a grant made
    // while they were a member), but membership is read now.
    let (stranger, login) = person(w.pool(), "stranger@example.test").await;
    w.member(w.project, stranger, "viewer").await;
    let (_, token) = w.reader(&login, 60).await;
    sqlx::query("delete from space_member where space_id=$1 and identity_id=$2")
        .bind(w.project)
        .bind(stranger)
        .execute(w.pool())
        .await
        .unwrap();
    let real_file = get(&w.app, &w.url(&format!("/files/{doc}/outline")), &token).await;
    let random_file = get(
        &w.app,
        &w.url(&format!("/files/{}/outline", Uuid::new_v4())),
        &token,
    )
    .await;
    let list = get(&w.app, &w.url("/files"), &token).await;
    assert_eq!(
        (real_file.code(), random_file.code(), list.code()),
        (403, 403, 403)
    );
    assert_eq!(
        real_file.bytes, random_file.bytes,
        "one answer for a real and an invented file"
    );

    // A member of the project: a file in another project, a file that never existed, a
    // trashed file and a questionnaire all answer the same 404.
    let (_, owner_token) = w.owner_reader().await;
    let other = w.project("Elsewhere").await;
    let mut ops = Ops::default();
    let root = ops.node("form", None, 0, json!({"label":"Secret"}));
    ops.block(&root, 10, para("elsewhere only"));
    let foreign = w.file_in(other, "Secret", "document", ops).await;
    let mut quiz = Ops::default();
    quiz.node("form", None, 0, json!({"label":"Survey"}));
    let questionnaire = w.file("Survey", "questionnaire", quiz).await;
    let trashed = policy(&w).await;
    sqlx::query("update document set deleted_at=now() where id=$1")
        .bind(trashed)
        .execute(&w.runtime.owner)
        .await
        .unwrap();
    let mut answers = Vec::new();
    for id in [foreign, Uuid::new_v4(), trashed, questionnaire] {
        answers.push(
            get(
                &w.app,
                &w.url(&format!("/files/{id}/outline")),
                &owner_token,
            )
            .await,
        );
    }
    for a in &answers {
        assert_eq!(a.code(), 404);
        assert_eq!(a.bytes, answers[0].bytes);
    }
    // The listing shows only supported files of this project.
    let listed = get(&w.app, &w.url("/files"), &owner_token).await.json();
    let titles: Vec<&str> = listed["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["title"].as_str().unwrap())
        .collect();
    assert_eq!(titles, ["Q3 Policy"]);
    w.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn etags_answer_if_none_match_with_304_after_authorising(pool: PgPool) {
    let w = World::new(pool, true).await;
    let doc = policy(&w).await;
    let (agent, token) = w.owner_reader().await;
    let outline = w.url(&format!("/files/{doc}/outline"));
    let first = get(&w.app, &outline, &token).await;
    let etag = first.header("etag").unwrap();
    assert_eq!(etag, w.etag(doc).await);
    let pattern = looks_like_etag(&etag);
    assert!(pattern, "{etag}");

    let conditional = |tag: &str| {
        let tag = tag.to_owned();
        let app = w.app.clone();
        let token = token.clone();
        let outline = outline.clone();
        async move {
            raw(
                &app,
                "GET",
                &outline,
                Some(&token),
                &[("if-none-match", &tag)],
                None,
            )
            .await
        }
    };
    let unchanged = conditional(&etag).await;
    assert_eq!(unchanged.code(), 304);
    assert!(unchanged.bytes.is_empty());
    assert_eq!(unchanged.header("etag").unwrap(), etag);
    assert_eq!(conditional(&format!("W/{etag}")).await.code(), 304);
    assert_eq!(
        conditional(&format!("\"0:00000000\", {etag}")).await.code(),
        304
    );
    assert_eq!(conditional("*").await.code(), 304);
    assert_eq!(conditional("\"0:00000000\"").await.code(), 200);
    // The same validator on the other revalidating routes.
    for tail in [
        format!("/files/{doc}/blocks?refs=ABCD"),
        format!("/files/{doc}/find?q=clause"),
    ] {
        let reply = raw(
            &w.app,
            "GET",
            &w.url(&tail),
            Some(&token),
            &[("if-none-match", &etag)],
            None,
        )
        .await;
        assert_eq!(reply.code(), 304, "{tail}");
    }

    // A new revision: the old validator no longer matches and the body is served.
    let mut ops = Ops::default();
    let root = {
        let (_, state) = w.expected(doc, FileKind::Document).await;
        state
            .nodes
            .values()
            .find(|n| n.parent_id.is_none())
            .unwrap()
            .id
            .0
            .clone()
    };
    ops.block(&root, 70, para("A late addition."));
    w.edit(doc, ops).await;
    let changed = conditional(&etag).await;
    assert_eq!(changed.code(), 200);
    assert_ne!(changed.header("etag").unwrap(), etag);
    assert_eq!(changed.header("etag").unwrap(), w.etag(doc).await);

    // Revocation is checked before any 304: a revoked grant never learns "unchanged".
    let current = changed.header("etag").unwrap();
    w.ungrant(&agent, w.project).await;
    assert_eq!(conditional(&current).await.code(), 403);
    w.close().await;
}

fn looks_like_etag(etag: &str) -> bool {
    // "<rev>:<8 hex>" including the quotes.
    let inner = etag.trim_matches('"');
    match inner.split_once(':') {
        Some((rev, chain)) => {
            !rev.is_empty()
                && rev.bytes().all(|b| b.is_ascii_digit())
                && chain.len() == 8
                && chain.bytes().all(|b| b.is_ascii_hexdigit())
        }
        None => false,
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn budgets_truncate_and_cursors_continue_at_their_own_revision(pool: PgPool) {
    let w = World::new(pool, true).await;
    let mut ops = Ops::default();
    let root = ops.node("form", None, 0, json!({"label":"Long report"}));
    for i in 0..60u32 {
        ops.block(
            &root,
            (i + 1) * 10,
            para(&format!(
                "Block {i}: {}",
                fixtures::words(u64::from(i) + 1, 40)
            )),
        );
    }
    let doc = w.file("Long report", "document", ops).await;
    let (_, token) = w.owner_reader().await;
    let outline = w.url(&format!("/files/{doc}/outline"));

    // Page through the outline 25 lines at a time.
    let all = get(&w.app, &format!("{outline}?limit=1000"), &token)
        .await
        .json();
    let all_refs: Vec<String> = all["blocks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| b["ref"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(all_refs.len(), 60);
    assert!(all["more"].is_null());
    let mut seen = Vec::new();
    let mut cursor: Option<String> = None;
    let mut pages = 0;
    loop {
        let uri = match &cursor {
            Some(c) => format!("{outline}?limit=25&cursor={c}"),
            None => format!("{outline}?limit=25"),
        };
        let page = get(&w.app, &uri, &token).await;
        assert_eq!(page.code(), 200, "{}", page.text());
        let json = page.json();
        pages += 1;
        seen.extend(
            json["blocks"]
                .as_array()
                .unwrap()
                .iter()
                .map(|b| b["ref"].as_str().unwrap().to_owned()),
        );
        match json["more"].as_str() {
            Some(next) => cursor = Some(next.to_owned()),
            None => break,
        }
        if pages == 1 {
            // The first page says so in both formats.
            let dct = get_dct(&w.app, &uri, &token).await.text();
            let header = dct.lines().next().unwrap();
            assert!(
                header.contains("more=c_") && header.contains("warn=truncated"),
                "{header}"
            );
        }
    }
    assert_eq!((pages, seen), (3, all_refs.clone()));

    // A byte budget cuts a read at a block boundary and says how to continue.
    let refs = all_refs.join(",");
    let blocks = w.url(&format!("/files/{doc}/blocks"));
    let first = get(
        &w.app,
        &format!("{blocks}?refs={refs}&max_bytes=1500"),
        &token,
    )
    .await;
    assert_eq!(first.code(), 200, "{}", first.text());
    let json = first.json();
    let n = json["items"].as_array().unwrap().len();
    assert!(n > 0 && n < 60, "{n}");
    let next = json["more"].as_str().unwrap().to_owned();
    // The file moves on; the continuation still answers at the revision it was cut at.
    let pinned_rev = json["rev"].as_u64().unwrap();
    let mut late = Ops::default();
    late.block(
        &root_of(&w, doc).await,
        700,
        para("Added after the first page."),
    );
    w.edit(doc, late).await;
    let rest = get(
        &w.app,
        &format!("{blocks}?refs={refs}&max_bytes=1500&cursor={next}"),
        &token,
    )
    .await;
    assert_eq!(rest.code(), 200, "{}", rest.text());
    assert_eq!(rest.json()["rev"].as_u64().unwrap(), pinned_rev);
    assert_eq!(rest.json()["items"][0]["ref"], all_refs[n]);
    // A cursor and a different explicit revision disagree.
    let clash = get(
        &w.app,
        &format!("{blocks}?refs={refs}&rev={}&cursor={next}", pinned_rev + 1),
        &token,
    )
    .await;
    assert_eq!(clash.code(), 400);
    assert_eq!(clash.json()["details"]["field"], "cursor");

    // Historical reads, and the limits.
    let old = get(
        &w.app,
        &format!("{blocks}?refs={}&rev={pinned_rev}", all_refs[0]),
        &token,
    )
    .await;
    assert_eq!(old.json()["rev"].as_u64().unwrap(), pinned_rev);
    for (uri, status, field) in [
        (format!("{outline}?limit=1001"), 400, "limit"),
        (format!("{outline}?preview=121"), 400, "preview"),
        (format!("{outline}?depth=2"), 400, "depth"),
        (
            format!("{blocks}?refs=ABCD&max_bytes=499"),
            400,
            "max_bytes",
        ),
        (
            format!("{blocks}?refs=ABCD&max_bytes=64001"),
            413,
            "max_bytes",
        ),
        (format!("{blocks}?refs=ABCD&rev=99999"), 400, "rev"),
        (format!("{outline}?cursor=garbage"), 400, "cursor"),
        (format!("{outline}?cursor=c_ffff.0"), 400, "cursor"),
    ] {
        let reply = get(&w.app, &uri, &token).await;
        assert_eq!(reply.code(), status, "{uri}: {}", reply.text());
        assert_eq!(reply.json()["details"]["field"], field, "{uri}");
    }
    // A single block larger than the budget is a stated limit, not an endless cursor.
    let mut big = Ops::default();
    let root = root_of(&w, doc).await;
    big.block(&root, 800, para(&fixtures::words(9, 600)));
    w.edit(doc, big).await;
    let all_now = get(&w.app, &format!("{outline}?limit=1000"), &token)
        .await
        .json();
    let big_ref = all_now["blocks"].as_array().unwrap().last().unwrap()["ref"]
        .as_str()
        .unwrap()
        .to_owned();
    let too_big = get(
        &w.app,
        &format!("{blocks}?refs={big_ref}&max_bytes=500"),
        &token,
    )
    .await;
    assert_eq!(too_big.code(), 413);
    assert_eq!(too_big.json()["details"]["field"], "max_bytes");
    // Unknown query parameters are refused loudly.
    let unknown = get(&w.app, &format!("{outline}?depht=0"), &token).await;
    assert_eq!(unknown.code(), 400);
    assert_eq!(unknown.json()["details"]["agent_code"], "bad_request");
    w.close().await;
}

async fn root_of(w: &World, doc: Uuid) -> String {
    let (_, state) = w.expected(doc, FileKind::Document).await;
    state
        .nodes
        .values()
        .find(|n| n.parent_id.is_none() && !n.deleted)
        .unwrap()
        .id
        .0
        .clone()
}

#[sqlx::test(migrations = "../../migrations")]
async fn hostile_document_text_cannot_forge_structure_over_http(pool: PgPool) {
    let w = World::new(pool, true).await;
    // PostgreSQL text cannot hold NUL; every other control character is stored as is.
    let corpus: Vec<String> = fixtures::injection_corpus()
        .into_iter()
        .map(|s| s.replace('\0', ""))
        .collect();
    assert!(corpus.len() >= 40);
    // One document whose every block is a corpus string, a workbook whose cells are, and
    // a deck whose shapes are; every title is hostile too.
    let mut ops = Ops::default();
    let root = ops.node("form", None, 0, json!({"label":corpus[0]}));
    for (i, text) in corpus.iter().enumerate() {
        ops.block(&root, (i as u32 + 1) * 10, para(text));
    }
    let doc = w.file(&corpus[1], "document", ops).await;
    let mut ops = Ops::default();
    let root = ops.node("form", None, 0, json!({"label":"Hostile cells"}));
    let sheet = ops.node(
        "section",
        Some(&root),
        1,
        json!({"label":corpus[2],"kind":"sheet","columns":2}),
    );
    for (i, text) in corpus.iter().enumerate() {
        ops.node(
            "item",
            Some(&sheet),
            i as u32,
            cells(&[&format!("row {i}"), text]),
        );
    }
    let wb = w.file("Hostile cells", "spreadsheet", ops).await;
    let mut ops = Ops::default();
    let root = ops.node("form", None, 0, json!({"label":"Hostile deck"}));
    let slide = ops.node("section", Some(&root), 1, json!({"label":corpus[3],"kind":"slide","layout":"title-content","notes":corpus[4],"hidden":false}));
    for (i, text) in corpus.iter().enumerate() {
        ops.node("item", Some(&slide), i as u32, shape("body", "text", text));
    }
    let dk = w.file("Hostile deck", "presentation", ops).await;
    let (_, token) = w.owner_reader().await;

    // Documents: the outline and a full read parse back to exactly the blocks written.
    let outline = get_dct(
        &w.app,
        &w.url(&format!("/files/{doc}/outline?limit=1000&preview=120")),
        &token,
    )
    .await;
    assert_eq!(outline.code(), 200, "{}", outline.text());
    let text = outline.text();
    let (header, lines) = engine_agent::dct::split_response(&text).unwrap();
    assert_eq!(header.tool, Tool::Outline);
    assert!(lines.iter().all(|l| !l.starts_with("dd/1")), "{text}");
    let rows = parse::outline_blocks(&lines, 1).unwrap_or_else(|e| panic!("{e}\n{text}"));
    assert_eq!(rows.len(), corpus.len());
    let refs: Vec<String> = rows.iter().map(|r| r.r.clone()).collect();
    let read = raw(
        &w.app,
        "POST",
        &w.url(&format!("/files/{doc}/blocks:read")),
        Some(&token),
        &[("accept", DCT)],
        Some(json!({"refs":refs,"max_bytes":64000})),
    )
    .await;
    assert_eq!(read.code(), 200, "{}", read.text());
    let text = read.text();
    let (_, lines) = engine_agent::dct::split_response(&text).unwrap();
    assert!(lines.iter().all(|l| !l.starts_with("dd/1")));
    let blocks = parse::read_blocks(&lines).unwrap_or_else(|e| panic!("{e}\n{text}"));
    assert_eq!(
        blocks.len(),
        corpus.len(),
        "no hostile text added or removed a block\n{text}"
    );
    for (block, original) in blocks.iter().zip(&corpus) {
        assert_eq!(
            block.text().unwrap(),
            engine_agent::escape::sanitize(original),
            "{original:?}"
        );
    }
    // The JSON mirror carries the same sanitised text.
    let mirror = get(
        &w.app,
        &w.url(&format!(
            "/files/{doc}/blocks?refs={}&max_bytes=64000",
            refs.join(",")
        )),
        &token,
    )
    .await
    .json();
    for (item, original) in mirror["items"].as_array().unwrap().iter().zip(&corpus) {
        assert_eq!(
            item["lines"][0],
            json!(engine_agent::escape::sanitize(original))
        );
    }

    // The listing: hostile titles stay inside their own line.
    let files = get_dct(&w.app, &w.url("/files"), &token).await.text();
    let (_, lines) = engine_agent::dct::split_response(&files).unwrap();
    assert_eq!(parse::files_lines(&lines).unwrap().len(), 3, "{files}");

    // Workbook range, sheet outline and find; deck slide read.
    let sheets = get(
        &w.app,
        &w.url(&format!("/files/{wb}/outline?depth=0")),
        &token,
    )
    .await
    .json();
    let sheet = sheets["sheets"][0]["ref"].as_str().unwrap().to_owned();
    let range = get_dct(
        &w.app,
        &w.url(&format!(
            "/files/{wb}/cells?range={sheet}!A1:B{}",
            corpus.len()
        )),
        &token,
    )
    .await;
    assert_eq!(range.code(), 200, "{}", range.text());
    let text = range.text();
    let (_, lines) = engine_agent::dct::split_response(&text).unwrap();
    assert!(lines.iter().all(|l| !l.starts_with("dd/1")));
    let body = parse::range_body(&lines, false).unwrap_or_else(|e| panic!("{e}\n{text}"));
    assert_eq!(body.rows.len(), corpus.len(), "{text}");
    let slides = get_dct(
        &w.app,
        &w.url(&format!("/files/{dk}/slides?ids=s1&max_bytes=64000")),
        &token,
    )
    .await;
    assert_eq!(slides.code(), 200, "{}", slides.text());
    let text = slides.text();
    let (_, lines) = engine_agent::dct::split_response(&text).unwrap();
    assert!(lines.iter().all(|l| !l.starts_with("dd/1")));
    let decks = parse::read_slides(&lines).unwrap_or_else(|e| panic!("{e}\n{text}"));
    assert_eq!(decks.len(), 1, "{text}");
    let hits = get_dct(
        &w.app,
        &w.url(&format!("/files/{doc}/find?q=ignore&limit=100")),
        &token,
    )
    .await;
    assert_eq!(hits.code(), 200);
    let text = hits.text();
    let (_, lines) = engine_agent::dct::split_response(&text).unwrap();
    parse::find_hits(&lines).unwrap_or_else(|e| panic!("{e}\n{text}"));

    // Errors never repeat document text: the query is not echoed in an error body.
    let attack = get(
        &w.app,
        &w.url(&format!("/files/{doc}/blocks?refs=@AB12%20P%20%23000000")),
        &token,
    )
    .await;
    assert!(
        attack.code() == 400 || attack.code() == 404,
        "{}",
        attack.text()
    );
    assert!(!attack.text().contains("@AB12 P"), "{}", attack.text());
    w.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn reads_write_nothing_but_call_metadata(pool: PgPool) {
    let w = World::new(pool, true).await;
    let doc = policy(&w).await;
    let wb = budget(&w).await;
    let dk = deck(&w).await;
    let (agent, token) = w.owner_reader().await;
    let before = footprint(&w).await;
    let calls_before = calls(&w).await;
    let heads_before = heads(&w).await;

    let sent = [
        w.url("/files"),
        w.url(&format!("/files/{doc}/outline")),
        w.url(&format!("/files/{doc}/blocks?refs=ZZZZ")),
        w.url(&format!("/files/{doc}/find?q=clause")),
        w.url(&format!("/files/{doc}/diff?from=0")),
        w.url(&format!("/files/{wb}/cells?range=Budget!A1:B2")),
        w.url(&format!("/files/{wb}/sheets/Budget/profile")),
        w.url(&format!("/files/{dk}/slides?ids=s1")),
        w.url(&format!("/files/{}/outline", Uuid::new_v4())),
        w.url("/access"),
    ];
    for uri in &sent {
        let reply = raw(
            &w.app,
            "GET",
            uri,
            Some(&token),
            &[
                ("x-dynodoc-client", "test-suite/1.0"),
                (
                    "traceparent",
                    "00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01",
                ),
            ],
            None,
        )
        .await;
        assert!(reply.code() < 500, "{uri}: {}", reply.text());
    }
    // The discovery document needs no credential and records nothing.
    raw(&w.app, "GET", "/.well-known/dynodoc-agent", None, &[], None).await;

    // Nothing but the ledger changed.
    assert_eq!(footprint(&w).await, before, "a read changed state");
    assert_eq!(heads(&w).await, heads_before);
    // The request for a file that does not exist is admitted but not recorded.
    assert_eq!(calls(&w).await - calls_before, sent.len() as i64 - 1);

    // The ledger holds metadata only.
    let rows: Vec<Value> =
        sqlx::query_scalar("select to_jsonb(c) from agent_call c order by at,id")
            .fetch_all(&w.runtime.owner)
            .await
            .unwrap();
    let dump = serde_json::to_string(&rows).unwrap();
    for text in [
        "Q3 Policy",
        "Budget",
        "Board",
        "Introduction",
        "clause",
        "Revenue",
        "Region",
    ] {
        assert!(!dump.contains(text), "{text} leaked into the ledger");
    }
    let keys: std::collections::BTreeSet<&str> = rows[0]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(
        keys,
        [
            "at",
            "bytes_out",
            "client",
            "document_id",
            "id",
            "key_id",
            "outcome",
            "principal_id",
            "project_id",
            "request_digest",
            "rev",
            "tok_est",
            "tool",
            "traceparent"
        ]
        .into_iter()
        .collect()
    );
    let by_tool = |tool: &str| rows.iter().filter(|r| r["tool"] == tool).count();
    assert_eq!(
        (
            by_tool("files"),
            by_tool("outline"),
            by_tool("read"),
            by_tool("find"),
            by_tool("diff"),
            by_tool("status")
        ),
        (1, 2, 3, 1, 1, 1)
    );
    for r in &rows {
        assert_eq!(r["principal_id"], json!(w.owner));
        assert_eq!(r["key_id"], json!(agent.key_id));
        assert_eq!(r["project_id"], json!(w.project));
        assert_eq!(r["client"], "test-suite/1.0");
        assert_eq!(
            r["traceparent"],
            "00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01"
        );
        assert!(
            r["request_digest"].as_str().unwrap().len() >= 64,
            "bytea as hex"
        );
    }
    let outcomes: Vec<&str> = rows
        .iter()
        .map(|r| r["outcome"].as_str().unwrap())
        .collect();
    assert!(
        // Failures of a resolved file are recorded; a guessed file id is not.
        outcomes.contains(&"ok")
            && outcomes.contains(&"unknown_ref")
            && !outcomes.contains(&"not_found"),
        "{outcomes:?}"
    );
    let ok_bytes: i64 = rows
        .iter()
        .filter(|r| r["outcome"] == "ok")
        .map(|r| r["bytes_out"].as_i64().unwrap())
        .sum();
    assert!(ok_bytes > 0);

    // Denied requests (no membership, no grant, no token) are not ledger rows.
    let n = calls(&w).await;
    w.ungrant(&agent, w.project).await;
    assert_eq!(get(&w.app, &w.url("/files"), &token).await.code(), 403);
    assert_eq!(
        raw(&w.app, "GET", &w.url("/files"), None, &[], None)
            .await
            .code(),
        401
    );
    assert_eq!(calls(&w).await, n);

    // The runtime login can append to the ledger and nothing else.
    for statement in [
        "update agent_call set outcome='ok'",
        "delete from agent_call",
        "truncate agent_call",
        "update device_key_class set class='person'",
        "delete from device_key_class",
    ] {
        let error = sqlx::raw_sql(statement)
            .execute(w.pool())
            .await
            .unwrap_err();
        assert_eq!(
            error.as_database_error().and_then(|e| e.code()).as_deref(),
            Some("42501"),
            "{statement}"
        );
    }
    // Even the owner cannot edit or delete a call row outside audited erasure.
    let owner_error = sqlx::raw_sql("update agent_call set outcome='ok'")
        .execute(&w.runtime.owner)
        .await
        .unwrap_err();
    assert!(
        owner_error.to_string().contains("append-only"),
        "{owner_error}"
    );
    let owner_error = sqlx::raw_sql("delete from agent_call")
        .execute(&w.runtime.owner)
        .await
        .unwrap_err();
    assert!(
        owner_error.to_string().contains("immutable"),
        "{owner_error}"
    );
    w.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn guessed_files_leave_no_ledger_row_and_old_rows_can_be_pruned(pool: PgPool) {
    let w = World::new(pool, true).await;
    let doc = policy(&w).await;
    let (_, token) = w.owner_reader().await;
    // A request that never resolved a file is not recorded, however often it is made.
    let guess = Uuid::new_v4();
    for _ in 0..3 {
        let reply = get(&w.app, &w.url(&format!("/files/{guess}/outline")), &token).await;
        assert_eq!(reply.code(), 404);
    }
    assert_eq!(calls(&w).await, 0);
    // A real file is.
    assert_eq!(
        get(&w.app, &w.url(&format!("/files/{doc}/outline")), &token)
            .await
            .code(),
        200
    );
    assert_eq!(calls(&w).await, 1);

    // The runtime role cannot delete a row at all; the owner role cannot delete a recent
    // one, but can prune one past the 90-day window.
    let runtime_delete = sqlx::query("delete from agent_call")
        .execute(&w.runtime.pool)
        .await;
    assert!(runtime_delete.is_err());
    let recent = sqlx::query("delete from agent_call")
        .execute(&w.runtime.owner)
        .await
        .unwrap_err();
    assert!(recent.to_string().contains("immutable"), "{recent}");
    // `at` cannot be edited (the row is immutable), so age a copy of the row instead.
    sqlx::query("insert into agent_call(at,principal_id,key_id,project_id,document_id,tool,rev,request_digest,bytes_out,tok_est,outcome) select now() - interval '91 days',principal_id,key_id,project_id,document_id,tool,rev,request_digest,bytes_out,tok_est,outcome from agent_call")
        .execute(&w.runtime.owner)
        .await
        .unwrap();
    assert_eq!(calls(&w).await, 2);
    let pruned = sqlx::query("delete from agent_call where at < now() - interval '90 days'")
        .execute(&w.runtime.owner)
        .await
        .unwrap();
    assert_eq!(pruned.rows_affected(), 1);
    assert_eq!(calls(&w).await, 1);
    w.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn erasure_purges_the_cache_and_the_call_rows_of_the_document(pool: PgPool) {
    let w = World::new(pool, true).await;
    let (_, admin_login) = person(w.pool(), "admin@example.test").await;
    let doc = policy(&w).await;
    let keep = budget(&w).await;
    let (_, token) = w.owner_reader().await;
    for id in [doc, keep] {
        assert_eq!(
            get(&w.app, &w.url(&format!("/files/{id}/outline")), &token)
                .await
                .code(),
            200
        );
    }
    let (entries, bytes) = w.cache.usage();
    assert_eq!(entries, 2);
    assert!(bytes > 0 && bytes <= engine_api::agent::cache::MAX_BYTES);
    assert_eq!(calls_of(&w, doc).await, 1);

    // A second read of the same revision is a cache hit.
    let (hits, misses) = w.cache.lookups();
    assert_eq!((hits, misses), (0, 2));
    assert_eq!(
        get(&w.app, &w.url(&format!("/files/{doc}/outline")), &token)
            .await
            .code(),
        200
    );
    assert_eq!(w.cache.usage().0, 2);
    assert_eq!(w.cache.lookups(), (1, 2));

    // A receipt alone does not let anything delete a call row: only the cascade of an
    // audited document erasure does.
    let mut tx = w.runtime.owner.begin().await.unwrap();
    sqlx::query("insert into erasure_receipt(resource_id,kind,actor_id,reason,counts) values($1,'documents',$2,'other','{}')")
        .bind(doc)
        .bind(w.owner)
        .execute(&mut *tx)
        .await
        .unwrap();
    let direct = sqlx::query("delete from agent_call where document_id=$1")
        .bind(doc)
        .execute(&mut *tx)
        .await
        .unwrap_err();
    assert!(direct.to_string().contains("immutable"), "{direct}");
    tx.rollback().await.unwrap();
    assert_eq!(calls_of(&w, doc).await, 2);

    let (status, preview) = call(
        &w.app,
        "GET",
        &format!("/admin/impact?kind=documents&id={doc}"),
        Some(&admin_login),
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{preview}");
    let (status, result) = call(
        &w.app,
        "POST",
        "/admin/erase",
        Some(&admin_login),
        json!({"kind":"documents","id":doc,"confirmation":"Q3 Policy","revision":preview["revision"],"reason":"owner_request","acknowledge_backups":true}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{result}");

    // Gone from memory and from the ledger; the other document is untouched.
    assert_eq!(w.cache.usage().0, 1);
    assert_eq!(calls_of(&w, doc).await, 0);
    assert_eq!(calls_of(&w, keep).await, 1);
    let after = get(&w.app, &w.url(&format!("/files/{doc}/outline")), &token).await;
    assert_eq!(after.code(), 404);
    let listed = get(&w.app, &w.url("/files"), &token).await.json();
    assert_eq!(listed["n"], 1);
    w.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_document_that_does_not_replay_is_replay_failed_as_on_every_other_read(pool: PgPool) {
    let w = World::new(pool, true).await;
    let doc = policy(&w).await;
    let (_, token) = w.owner_reader().await;
    // A good read first, so the cache holds the healthy revision.
    assert_eq!(
        get(&w.app, &w.url(&format!("/files/{doc}/outline")), &token)
            .await
            .code(),
        200
    );
    // An event no write path accepts: it accepts a suggestion that never existed.
    engine_core::log::append(
        w.pool(),
        DocumentId(doc),
        &engine_shared::EventPayload::SuggestionAccepted {
            suggestion_id: "missing".into(),
        },
        engine_shared::IdentityId(w.owner),
    )
    .await
    .unwrap();
    let broken = w.head(doc).await;
    for tail in [
        format!("/files/{doc}/outline"),
        format!("/files/{doc}/blocks?refs=ABCD"),
        format!("/files/{doc}/find?q=x"),
        format!("/files/{doc}/diff?from=0"),
    ] {
        let reply = get(&w.app, &w.url(&tail), &token).await;
        assert_eq!(reply.code(), 500, "{tail}");
        let body = reply.json();
        assert_eq!(body["code"], "replay_failed", "{tail}: {body}");
        assert_eq!(body["details"]["document_id"], json!(doc));
        assert_eq!(body["details"]["seq"], broken);
        assert_eq!(body["details"]["agent_code"], "internal");
    }
    // Revisions before the break are still readable.
    let old = get(
        &w.app,
        &w.url(&format!("/files/{doc}/blocks?refs=ABCD&rev={}", broken - 1)),
        &token,
    )
    .await;
    assert_eq!(
        old.code(),
        400,
        "an unknown ref, not a replay failure: {}",
        old.text()
    );
    w.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn content_negotiation_and_validation_keep_one_error_envelope(pool: PgPool) {
    let w = World::new(pool, true).await;
    let doc = policy(&w).await;
    let (_, token) = w.owner_reader().await;
    let outline = w.url(&format!("/files/{doc}/outline"));
    let accept = |value: &'static str| {
        let (app, token, outline) = (w.app.clone(), token.clone(), outline.clone());
        async move {
            raw(
                &app,
                "GET",
                &outline,
                Some(&token),
                &[("accept", value)],
                None,
            )
            .await
        }
    };
    assert_eq!(
        accept("application/json")
            .await
            .header("content-type")
            .unwrap(),
        "application/json"
    );
    assert_eq!(
        accept("*/*").await.header("content-type").unwrap(),
        "application/json"
    );
    assert!(accept("text/vnd.dynodoc.dct")
        .await
        .text()
        .starts_with("dd/1 outline "));
    assert!(accept("text/vnd.dynodoc.dct; v=1, application/json;q=0.4")
        .await
        .text()
        .starts_with("dd/1 outline "));
    let wrong = accept("text/vnd.dynodoc.dct; v=2").await;
    assert_eq!(wrong.code(), 400);
    assert_eq!(
        wrong.json()["details"],
        json!({"agent_code":"bad_request","field":"accept"})
    );
    // Errors are JSON even when DCT was asked for.
    let missing = raw(
        &w.app,
        "GET",
        &w.url(&format!("/files/{}/outline", Uuid::new_v4())),
        Some(&token),
        &[("accept", DCT)],
        None,
    )
    .await;
    assert_eq!(missing.code(), 404);
    assert_eq!(missing.json()["code"], "not_found");
    // A malformed body and an unknown body field.
    let post = w.url(&format!("/files/{doc}/blocks:read"));
    let bad = raw(
        &w.app,
        "POST",
        &post,
        Some(&token),
        &[],
        Some(json!({"refs":["A"],"nope":1})),
    )
    .await;
    assert_eq!(bad.code(), 400);
    assert_eq!(bad.json()["details"]["field"], "body");
    let too_many: Vec<String> = (0..101).map(|i| format!("R{i:03}")).collect();
    let many = raw(
        &w.app,
        "POST",
        &post,
        Some(&token),
        &[],
        Some(json!({"refs":too_many})),
    )
    .await;
    assert_eq!(many.code(), 400, "{}", many.text());
    assert_eq!(many.json()["details"]["agent_code"], "limit_exceeded");
    let huge = raw(
        &w.app,
        "POST",
        &post,
        Some(&token),
        &[],
        Some(json!({"refs":["A"],"have":{"A":"x".repeat(300_000)}})),
    )
    .await;
    assert_eq!(huge.code(), 413);
    w.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn openapi_describes_every_response(pool: PgPool) {
    let w = World::new(pool, true).await;
    let doc = policy(&w).await;
    let wb = budget(&w).await;
    let dk = deck(&w).await;
    let (_, token) = w.owner_reader().await;
    let spec = serde_json::to_value(<engine_api::ApiDoc as utoipa::OpenApi>::openapi()).unwrap();

    // Every agent path is documented with the device-session security scheme.
    let paths = spec["paths"].as_object().unwrap();
    let agent_paths: Vec<&String> = paths
        .keys()
        .filter(|p| {
            p.contains("/agent/") || p.contains("dynodoc-agent") || p.ends_with("/agent/files")
        })
        .collect();
    // Eleven read paths (this file's) and the four of the write half (`agent_write_test`).
    assert_eq!(agent_paths.len(), 15, "{agent_paths:?}");
    for path in &agent_paths {
        for (_, operation) in paths[*path].as_object().unwrap() {
            assert_eq!(operation["tags"], json!(["agent"]), "{path}");
            if !path.contains("dynodoc-agent") {
                // Each route names the one scope it needs; the reads need content:read.
                let scope = if path.ends_with("/propose") {
                    "content:propose"
                } else if path.ends_with("/submit") || path.ends_with("/withdraw") {
                    "content:submit"
                } else {
                    "content:read"
                };
                assert_eq!(
                    operation["security"],
                    json!([{"device_session":[scope]}]),
                    "{path}"
                );
            }
        }
    }

    let outline = get(&w.app, &w.url(&format!("/files/{doc}/outline")), &token)
        .await
        .json();
    let blocks = outline["blocks"].as_array().unwrap();
    let first_ref = blocks[0]["ref"].as_str().unwrap().to_owned();
    let of_type = |prefix: &str| {
        blocks
            .iter()
            .find(|b| b["t"].as_str().unwrap().starts_with(prefix))
            .unwrap()["ref"]
            .as_str()
            .unwrap()
            .to_owned()
    };
    let (list_ref, table_ref) = (of_type("UL"), of_type("TBL"));
    let cases: Vec<(&str, String)> = vec![
        ("AgentFiles", w.url("/files")),
        ("AgentOutline", w.url(&format!("/files/{doc}/outline"))),
        (
            "AgentOutline",
            w.url(&format!("/files/{doc}/outline?depth=0")),
        ),
        (
            "AgentOutline",
            w.url(&format!("/files/{wb}/outline?depth=0")),
        ),
        (
            "AgentOutline",
            w.url(&format!("/files/{wb}/sheets/Budget/profile")),
        ),
        ("AgentOutline", w.url(&format!("/files/{dk}/outline"))),
        (
            "AgentRead",
            w.url(&format!("/files/{doc}/blocks?refs={first_ref}")),
        ),
        (
            "AgentRead",
            w.url(&format!(
                "/files/{doc}/blocks?refs={list_ref}.1,{table_ref}.r2c2"
            )),
        ),
        (
            "AgentRead",
            w.url(&format!("/files/{wb}/cells?range=Budget!A1:D4&rows=refs")),
        ),
        ("AgentRead", w.url(&format!("/files/{dk}/slides?ids=s1,s2"))),
        ("AgentFind", w.url(&format!("/files/{doc}/find?q=clause"))),
        (
            "AgentDiff",
            w.url(&format!("/files/{doc}/diff?from=0&detail=text")),
        ),
        ("AgentAccess", w.url("/access")),
    ];
    for (schema, uri) in cases {
        let reply = get(&w.app, &uri, &token).await;
        assert_eq!(reply.code(), 200, "{uri}: {}", reply.text());
        let mut problems = Vec::new();
        conforms(
            &spec,
            &spec["components"]["schemas"][schema],
            &reply.json(),
            schema,
            &mut problems,
        );
        assert!(problems.is_empty(), "{uri}: {problems:?}");
    }
    let discovery = raw(&w.app, "GET", "/.well-known/dynodoc-agent", None, &[], None)
        .await
        .json();
    let mut problems = Vec::new();
    conforms(
        &spec,
        &spec["components"]["schemas"]["AgentDiscovery"],
        &discovery,
        "AgentDiscovery",
        &mut problems,
    );
    assert!(problems.is_empty(), "{problems:?}");
    w.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_cache_serves_one_revision_and_reads_a_new_one_afresh(pool: PgPool) {
    let w = World::new(pool, true).await;
    let doc = policy(&w).await;
    let (_, token) = w.owner_reader().await;
    let outline = w.url(&format!("/files/{doc}/outline"));
    assert_eq!(w.cache.lookups(), (0, 0));
    let first = get(&w.app, &outline, &token).await;
    assert_eq!(w.cache.lookups(), (0, 1));
    let before = w.head(doc).await;
    // Same revision through other tools: hits.
    let blocks = get(
        &w.app,
        &w.url(&format!("/files/{doc}/blocks?refs=ABCD")),
        &token,
    )
    .await;
    assert_eq!(blocks.code(), 400);
    get(
        &w.app,
        &w.url(&format!("/files/{doc}/find?q=clause")),
        &token,
    )
    .await;
    assert_eq!(w.cache.lookups(), (2, 1));
    // A conditional read that matches never reads state at all.
    let etag = first.header("etag").unwrap();
    let unchanged = raw(
        &w.app,
        "GET",
        &outline,
        Some(&token),
        &[("if-none-match", &etag)],
        None,
    )
    .await;
    assert_eq!(unchanged.code(), 304);
    assert_eq!(w.cache.lookups(), (2, 1));
    // A new revision is a new key; the old one is still held and still answers.
    let mut ops = Ops::default();
    ops.block(&root_of(&w, doc).await, 70, para("Another paragraph."));
    w.edit(doc, ops).await;
    let second = get(&w.app, &outline, &token).await;
    assert_ne!(first.bytes, second.bytes);
    assert_eq!(w.cache.lookups(), (2, 2));
    assert_eq!(w.cache.usage().0, 2);
    let old = get(
        &w.app,
        &w.url(&format!("/files/{doc}/diff?from={before}")),
        &token,
    )
    .await;
    assert_eq!(old.code(), 200, "{}", old.text());
    assert_eq!(old.json()["added"], 1);
    // The diff read both revisions from the cache (the head and the one before it).
    assert_eq!(w.cache.lookups(), (4, 2));
    w.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn an_empty_file_is_revision_zero(pool: PgPool) {
    let w = World::new(pool, true).await;
    let (_, token) = w.owner_reader().await;
    for kind in ["document", "spreadsheet", "presentation"] {
        let file = w.empty_file(&format!("Empty {kind}"), kind).await;
        let outline = get(&w.app, &w.url(&format!("/files/{file}/outline")), &token).await;
        assert_eq!(outline.code(), 200, "{kind}: {}", outline.text());
        assert_eq!(outline.header("etag").unwrap(), "\"0:00000000\"");
        let json = outline.json();
        assert_eq!(
            (json["rev"].as_u64(), json["chain"].as_str()),
            (Some(0), Some("00000000"))
        );
        assert_eq!(json["count"], 0);
        let dct = get_dct(&w.app, &w.url(&format!("/files/{file}/outline")), &token).await;
        assert!(dct.text().starts_with("dd/1 outline "), "{kind}");
        let find = get(&w.app, &w.url(&format!("/files/{file}/find?q=x")), &token).await;
        assert_eq!(find.code(), 200, "{kind}: {}", find.text());
        assert_eq!(find.json()["total"], 0);
        let diff = get(
            &w.app,
            &w.url(&format!("/files/{file}/diff?from=0")),
            &token,
        )
        .await;
        assert_eq!(diff.code(), 200, "{kind}: {}", diff.text());
        assert_eq!(diff.json()["blocks"], 0);
        let unknown = get(
            &w.app,
            &w.url(&format!("/files/{file}/blocks?refs=ABCD")),
            &token,
        )
        .await;
        assert_eq!(unknown.json()["details"]["agent_code"], "unknown_ref");
    }
    let files = get(&w.app, &w.url("/files"), &token).await.json();
    assert_eq!(files["n"], 3);
    assert!(files["files"]
        .as_array()
        .unwrap()
        .iter()
        .all(|f| f["rev"] == 0));
    w.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn enrolment_records_the_key_class_and_nothing_can_change_it(pool: PgPool) {
    let w = World::new(pool, true).await;
    let login = w.owner_login.clone();
    // An agent key, a key that names its class person, and a key that names nothing.
    let (_, agent_id, agent_body, status, answer) = w
        .enroll(
            &login,
            1,
            json!({"class":"agent","label":"  Claude Desktop on laptop  "}),
        )
        .await;
    assert_eq!(
        (status, &answer["class"]),
        (StatusCode::OK, &json!("agent")),
        "{answer}"
    );
    let (_, person_id, _, status, answer) = w.enroll(&login, 2, json!({"class":"person"})).await;
    assert_eq!(
        (status, &answer["class"]),
        (StatusCode::OK, &json!("person")),
        "{answer}"
    );
    let (_, legacy_id, _, status, answer) = w.enroll(&login, 3, json!({})).await;
    assert_eq!(
        (status, &answer["class"]),
        (StatusCode::OK, &json!("person")),
        "{answer}"
    );
    assert_eq!(answer["project_access"], "granted");

    let (_, list) = call(&w.app, "GET", "/devices", Some(&login), Value::Null).await;
    let listed = |id: Uuid| {
        list["keys"]
            .as_array()
            .unwrap()
            .iter()
            .find(|k| k["id"] == json!(id))
            .unwrap()
            .clone()
    };
    assert_eq!(
        (
            listed(agent_id)["class"].clone(),
            listed(agent_id)["label"].clone()
        ),
        (json!("agent"), json!("Claude Desktop on laptop"))
    );
    assert_eq!(
        (
            listed(person_id)["class"].clone(),
            listed(person_id)["label"].clone()
        ),
        (json!("person"), Value::Null)
    );
    assert_eq!(listed(legacy_id)["class"], "person");
    // The scopes offered to a person's key did not change.
    assert_eq!(
        list["scopes"],
        json!([
            "history:read",
            "history:review",
            "history:write",
            "project:read"
        ])
    );

    // An exact retry returns the class the key has, whatever the retry says.
    let mut retry = agent_body.clone();
    retry["class"] = json!("person");
    let (status, again) = call(&w.app, "POST", "/devices", Some(&login), retry).await;
    assert_eq!(
        (status, &again["class"]),
        (StatusCode::OK, &json!("agent")),
        "{again}"
    );

    // Bad requests: unknown class, a label without a class, a long or control label.
    for (seed, extra) in [
        (4, json!({"class":"robot"})),
        (5, json!({"label":"orphan"})),
        (6, json!({"class":"agent","label":"x".repeat(81)})),
        (7, json!({"class":"agent","label":"tab\there"})),
    ] {
        let (_, _, _, status, answer) = w.enroll(&login, seed, extra.clone()).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{extra}: {answer}");
    }

    // The class is audited, and no login can edit or remove it.
    let detail: Value = sqlx::query_scalar(
        "select detail from operation_audit where action='device.enrolled' and resource_id=$1",
    )
    .bind(agent_id)
    .fetch_one(w.pool())
    .await
    .unwrap();
    assert_eq!(detail["class"], "agent");
    let rows: i64 = sqlx::query_scalar("select count(*) from device_key_class")
        .fetch_one(w.pool())
        .await
        .unwrap();
    assert_eq!(rows, 2, "only keys that named a class have a row");
    for statement in [
        "update device_key_class set class='person'",
        "delete from device_key_class",
    ] {
        let error = sqlx::raw_sql(statement)
            .execute(&w.runtime.owner)
            .await
            .unwrap_err();
        assert!(
            error.to_string().contains("append-only"),
            "{statement}: {error}"
        );
    }
    w.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn admission_limits_answer_429_and_leave_no_ledger_row(pool: PgPool) {
    let w = World::new(pool, true).await;
    let (_, token) = w.owner_reader().await;
    let access = w.url("/access");
    let mut served = 0;
    let mut refused = None;
    for _ in 0..140 {
        let reply = get(&w.app, &access, &token).await;
        if reply.code() == 429 {
            refused = Some(reply);
            break;
        }
        assert_eq!(reply.code(), 200);
        served += 1;
    }
    let refused = refused.expect("the per-identity budget of 120 a minute applies");
    assert!((100..=120).contains(&served), "{served}");
    assert_eq!(refused.header("retry-after").unwrap(), "60");
    assert_eq!(refused.json()["code"], "rate_limited");
    assert_eq!(refused.json()["details"]["agent_code"], "rate_limited");
    assert_eq!(
        calls(&w).await,
        served,
        "a refused request is not a ledger row"
    );
    w.close().await;
}
