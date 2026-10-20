//! The project and file agent policy (docs/AI-AGENT-CONNECTORS.md sections 5 and 7.2,
//! migration 0043) through the real router and the restricted runtime login: default deny
//! on the read routes, the role floor, kinds and file overrides, the management routes and
//! who may call them, revisions and concurrency, the kill switch, the usage ledger, the
//! per-grant budgets and the rule that a policy change never touches history. Requires a
//! reachable Postgres (DATABASE_URL).

use std::collections::BTreeSet;

use axum::http::StatusCode;
use chrono::{DateTime, Duration, Utc};
use engine_api::auth::token::{self, DeviceClaims};
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

mod common;

mod agent_world;
use agent_world::*;

// ----------------------------------------------------------------------- helpers

/// A document with one paragraph, in the world's project.
async fn note(w: &World, title: &str, text: &str) -> Uuid {
    let mut ops = Ops::default();
    let root = ops.node("form", None, 0, json!({"label":title}));
    ops.block(&root, 10, para(text));
    w.file(title, "document", ops).await
}

/// A workbook with one sheet, in the world's project.
async fn ledger(w: &World, title: &str) -> Uuid {
    let mut ops = Ops::default();
    let root = ops.node("form", None, 0, json!({"label":title}));
    let layout =
        json!({"widths":{},"hiddenColumns":[],"frozenRows":1,"frozenColumns":0,"merges":[]});
    let sheet = ops.node(
        "section",
        Some(&root),
        1,
        json!({"label":"Sheet1","kind":"sheet","columns":2,"sheet_layout":layout}),
    );
    ops.node(
        "item",
        Some(&sheet),
        0,
        json!({"cell_0":"Region","cell_1":"Total"}),
    );
    ops.node(
        "item",
        Some(&sheet),
        1,
        json!({"cell_0":"North","cell_1":"120"}),
    );
    w.file(title, "spreadsheet", ops).await
}

fn policy_url(project: Uuid) -> String {
    format!("/projects/{project}/agent-policy")
}

async fn read_policy(w: &World, login: &str, project: Uuid) -> Reply {
    get(&w.app, &policy_url(project), login).await
}

async fn put_policy(w: &World, login: &str, project: Uuid, if_match: &str, body: Value) -> Reply {
    raw(
        &w.app,
        "PUT",
        &policy_url(project),
        Some(login),
        &[("if-match", if_match)],
        Some(body),
    )
    .await
}

/// Save `body` as the project's policy using the revision the server reports now.
async fn set_policy(w: &World, project: Uuid, body: Value) -> Reply {
    let current = read_policy(w, &w.owner_login, project).await;
    assert_eq!(current.code(), 200, "{}", current.text());
    let revision = current.json()["revision"].as_i64().unwrap();
    let reply = put_policy(w, &w.owner_login, project, &format!("\"{revision}\""), body).await;
    assert_eq!(reply.code(), 200, "{}", reply.text());
    reply
}

async fn kill(w: &World, login: &str, project: Uuid) -> Reply {
    raw(
        &w.app,
        "POST",
        &format!("{}/kill", policy_url(project)),
        Some(login),
        &[],
        None,
    )
    .await
}

async fn usage(w: &World, login: &str, project: Uuid, query: &str) -> Reply {
    get(
        &w.app,
        &format!("/projects/{project}/agent-usage{query}"),
        login,
    )
    .await
}

async fn file_policy(w: &World, login: &str, document: Uuid, access: &str) -> Reply {
    raw(
        &w.app,
        "PUT",
        &format!("/documents/{document}/agent-policy"),
        Some(login),
        &[],
        Some(json!({"access":access})),
    )
    .await
}

/// A project member with an agent key, an active grant and a session token.
async fn reader_of(w: &World, email: &str, role: &str, seed: u8) -> (Uuid, String, Agent, String) {
    let (id, login) = person(w.pool(), email).await;
    w.member(w.project, id, role).await;
    let (agent, token) = w.reader(&login, seed).await;
    (id, login, agent, token)
}

/// A person's key (not an agent key) with a `project:read` session.
async fn person_session(w: &World, login: &str, seed: u8) -> String {
    let (key, key_id, _, status, body) = w.enroll(login, seed, json!({})).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let agent = Agent {
        login: login.to_owned(),
        key,
        key_id,
    };
    let (status, body) = w.grant(&agent, w.project, &["project:read"]).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    w.token(&agent, w.project, &["project:read"]).await
}

async fn outcomes(w: &World) -> Vec<(String, i64, i64)> {
    sqlx::query_as("select outcome,count(*),coalesce(sum(bytes_out),0)::bigint from agent_call group by outcome order by outcome")
        .fetch_all(&w.runtime.owner)
        .await
        .unwrap()
}

async fn audit_rows(w: &World, like: &str) -> Vec<(Uuid, String, Uuid, Value)> {
    sqlx::query_as("select actor_id,action,resource_id,detail from operation_audit where action like $1 order by id")
        .bind(like)
        .fetch_all(&w.runtime.owner)
        .await
        .unwrap()
}

/// Counts of every table a policy change must not touch (the audit table excluded: it is
/// supposed to grow).
async fn footprint(w: &World) -> Vec<(String, i64)> {
    let mut out = Vec::new();
    for table in [
        "event",
        "node",
        "snapshot",
        "document",
        "workspace_draft",
        "draft_edit",
        "document_access",
        "space_member",
        "notification",
        "idempotency_key",
        "device_session",
        "device_key_project_grant",
        "change_request_review",
        "provenance_bundle",
        "document_version",
        "agent_call",
    ] {
        let n: i64 = sqlx::query_scalar(&format!("select count(*) from {table}"))
            .fetch_one(&w.runtime.owner)
            .await
            .unwrap();
        out.push((table.to_owned(), n));
    }
    out
}

async fn heads(w: &World) -> Vec<(Uuid, Vec<u8>)> {
    sqlx::query_as("select distinct on (document_id) document_id,chain_hash from event order by document_id,seq desc")
        .fetch_all(&w.runtime.owner)
        .await
        .unwrap()
}

/// One ledger row, written as the owner role (the runtime role may only insert too).
#[allow(clippy::too_many_arguments)]
async fn ledger_row(
    w: &World,
    key: Uuid,
    principal: Uuid,
    project: Uuid,
    ago: &str,
    bytes: i32,
    outcome: &str,
    client: Option<&str>,
) {
    sqlx::query("insert into agent_call(at,principal_id,key_id,project_id,tool,request_digest,bytes_out,tok_est,outcome,client) values(now()-$1::interval,$2,$3,$4,'files',decode(repeat('00',32),'hex'),$5,0,$6,$7)")
        .bind(ago)
        .bind(principal)
        .bind(key)
        .bind(project)
        .bind(bytes)
        .bind(outcome)
        .bind(client)
        .execute(&w.runtime.owner)
        .await
        .unwrap();
}

fn keys(value: &Value) -> BTreeSet<String> {
    value.as_object().unwrap().keys().cloned().collect()
}

/// The response's keys are the schema's properties (the optional ones may be absent).
fn matches_schema(spec: &Value, schema: &str, value: &Value, optional: &[&str]) {
    let properties: BTreeSet<String> = keys(&spec["components"]["schemas"][schema]["properties"]);
    let present = keys(value);
    assert!(present.is_subset(&properties), "{schema}: {present:?}");
    for name in &properties {
        assert!(
            present.contains(name) || optional.contains(&name.as_str()),
            "{schema} lacks {name}"
        );
    }
}

// ------------------------------------------------------------------ default deny

#[sqlx::test(migrations = "../../migrations")]
async fn a_project_with_no_policy_serves_no_agent_request(pool: PgPool) {
    let w = World::bare(pool, true).await;
    let doc = note(&w, "Plan", "Budget talks begin on Monday.").await;
    let book = ledger(&w, "Ledger").await;
    // The key, the grant and the session all work: they are inert, not refused.
    let (_, token) = w.owner_reader().await;
    // A member who left the project is refused at the membership check, the answer every
    // layer must match.
    let (stranger, login) = person(w.pool(), "stranger@example.test").await;
    w.member(w.project, stranger, "viewer").await;
    let (_, stranger_token) = w.reader(&login, 11).await;
    sqlx::query("delete from space_member where space_id=$1 and identity_id=$2")
        .bind(w.project)
        .bind(stranger)
        .execute(w.pool())
        .await
        .unwrap();
    let before = footprint(&w).await;
    let heads_before = heads(&w).await;
    let audit_before = audit_rows(&w, "%").await.len();

    let routes: Vec<(&str, String, Option<Value>)> = vec![
        ("GET", w.url("/files"), None),
        ("GET", w.url("/access"), None),
        ("GET", w.url(&format!("/files/{doc}/outline")), None),
        ("GET", w.url(&format!("/files/{doc}/blocks?refs=zz")), None),
        (
            "POST",
            w.url(&format!("/files/{doc}/blocks:read")),
            Some(json!({"refs":["zz"]})),
        ),
        (
            "GET",
            w.url(&format!("/files/{book}/cells?range=Sheet1!A1:B2")),
            None,
        ),
        (
            "GET",
            w.url(&format!("/files/{book}/sheets/Sheet1/profile")),
            None,
        ),
        ("GET", w.url(&format!("/files/{doc}/slides?ids=s1")), None),
        ("GET", w.url(&format!("/files/{doc}/find?q=Budget")), None),
        ("GET", w.url(&format!("/files/{doc}/diff?from=0")), None),
        (
            "GET",
            w.url(&format!("/files/{}/outline", Uuid::new_v4())),
            None,
        ),
    ];
    for (method, uri, body) in &routes {
        let refused = raw(&w.app, method, uri, Some(&token), &[], body.clone()).await;
        let stranger_reply = raw(
            &w.app,
            method,
            uri,
            Some(&stranger_token),
            &[],
            body.clone(),
        )
        .await;
        assert_eq!(refused.code(), 403, "{method} {uri}");
        assert_eq!(stranger_reply.code(), 403, "{method} {uri}");
        assert_eq!(
            refused.bytes, stranger_reply.bytes,
            "a disabled project answers like a non-member: {method} {uri}"
        );
        assert_eq!(refused.json()["code"], "forbidden");
        assert_eq!(refused.json()["details"]["agent_code"], "forbidden");
    }
    // The discovery document is not project data.
    let discovery = raw(&w.app, "GET", "/.well-known/dynodoc-agent", None, &[], None).await;
    assert_eq!(discovery.code(), 200);

    // Nothing was read, written or created. The admitted refusals of the owner's grant are
    // one internal ledger row that says why (a repeated refusal is written once a minute per
    // grant and outcome, so probing cannot fill the ledger), and the stranger's refusals
    // (no membership) leave none.
    assert_eq!(heads(&w).await, heads_before);
    let mut after = footprint(&w).await;
    let mut expected = before;
    expected.last_mut().unwrap().1 += 1;
    after.sort();
    expected.sort();
    assert_eq!(after, expected);
    assert_eq!(audit_rows(&w, "%").await.len(), audit_before);
    assert_eq!(
        outcomes(&w).await,
        vec![("policy_denied".to_owned(), 1, 0)],
        "{} refusals, one row",
        routes.len()
    );
    let rows: i64 = sqlx::query_scalar("select count(*) from project_agent_policy")
        .fetch_one(&w.runtime.owner)
        .await
        .unwrap();
    assert_eq!(rows, 0, "a refusal creates no policy row");
    assert_eq!(
        w.cache.usage().0,
        0,
        "nothing was loaded for a refused request"
    );
    w.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn enabling_opens_the_routes_and_disabling_stops_the_very_next_request(pool: PgPool) {
    let w = World::bare(pool, true).await;
    let doc = note(&w, "Plan", "Budget talks begin on Monday.").await;
    let (_, token) = w.owner_reader().await;
    assert_eq!(get(&w.app, &w.url("/files"), &token).await.code(), 403);

    let view = read_policy(&w, &w.owner_login, w.project).await;
    assert_eq!(view.code(), 200);
    assert_eq!(view.header("etag").as_deref(), Some("\"0\""));
    let view = view.json();
    assert_eq!(
        (
            view["configured"].clone(),
            view["enabled"].clone(),
            view["revision"].clone()
        ),
        (json!(false), json!(false), json!(0))
    );
    assert_eq!(view["gateway"], true);
    assert_eq!(view["min_role"], "viewer");
    assert_eq!(
        view["kinds"],
        json!(["document", "spreadsheet", "presentation"])
    );
    assert_eq!(
        (
            view["calls_per_minute"].clone(),
            view["read_bytes_per_day"].clone()
        ),
        (json!(60), json!(20_000_000))
    );

    let saved = put_policy(
        &w,
        &w.owner_login,
        w.project,
        "\"0\"",
        json!({"enabled":true}),
    )
    .await;
    assert_eq!(saved.code(), 200, "{}", saved.text());
    assert_eq!(saved.header("etag").as_deref(), Some("\"1\""));
    let saved = saved.json();
    assert_eq!(
        (
            saved["revision"].clone(),
            saved["changed"].clone(),
            saved["configured"].clone()
        ),
        (json!(1), json!(true), json!(true))
    );

    // The very next request is served.
    let files = get(&w.app, &w.url("/files"), &token).await;
    assert_eq!(files.code(), 200, "{}", files.text());
    assert_eq!(files.json()["files"][0]["title"], "Plan");
    let outline = get(&w.app, &w.url(&format!("/files/{doc}/outline")), &token).await;
    assert_eq!(outline.code(), 200);
    assert_eq!(w.cache.usage().0, 1);
    let access = get(&w.app, &w.url("/access"), &token).await.json();
    assert_eq!(access["budget"]["calls_per_minute"]["limit"], 60);
    assert_eq!(access["budget"]["calls_per_minute"]["used"], 2);
    assert_eq!(access["budget"]["read_bytes_per_day"]["limit"], 20_000_000);
    assert!(
        access["budget"]["read_bytes_per_day"]["used"]
            .as_i64()
            .unwrap()
            > 0
    );

    // Disabling: the next request is refused, and the project's cached text is dropped.
    let off = put_policy(
        &w,
        &w.owner_login,
        w.project,
        "\"1\"",
        json!({"enabled":false}),
    )
    .await;
    assert_eq!(off.code(), 200, "{}", off.text());
    assert_eq!(off.json()["revision"], 2);
    assert_eq!(get(&w.app, &w.url("/files"), &token).await.code(), 403);
    assert_eq!(
        get(&w.app, &w.url(&format!("/files/{doc}/outline")), &token)
            .await
            .code(),
        403
    );
    assert_eq!(w.cache.usage().0, 0);
    // Enabling again needs nothing but the policy: the grant and the session never changed.
    set_policy(&w, w.project, json!({"enabled":true})).await;
    assert_eq!(get(&w.app, &w.url("/files"), &token).await.code(), 200);
    w.close().await;
}

// -------------------------------------------------------------- floor, kinds, files

#[sqlx::test(migrations = "../../migrations")]
async fn the_role_floor_caps_whose_agents_may_read(pool: PgPool) {
    let w = World::new(pool, true).await;
    note(&w, "Plan", "text").await;
    let stored = ["viewer", "reviewer", "approver", "editor", "manager"];
    let mut tokens = Vec::new();
    for (i, role) in stored.iter().enumerate() {
        let (_, _, _, token) =
            reader_of(&w, &format!("{role}@example.test"), role, 20 + i as u8).await;
        tokens.push((*role, token));
    }
    let (_, owner_token) = w.owner_reader().await;
    tokens.push(("owner", owner_token));

    // Product names: reviewer is stored as contributor-level, approver as reviewer.
    let floors = [
        ("viewer", 6),
        ("contributor", 5),
        ("reviewer", 4),
        ("editor", 3),
        ("manager", 2),
        ("owner", 1),
    ];
    for (floor, allowed) in floors {
        set_policy(
            &w,
            w.project,
            json!({"enabled":true,"calls_per_minute":600,"min_role":floor}),
        )
        .await;
        for (i, (role, token)) in tokens.iter().enumerate() {
            let reply = get(&w.app, &w.url("/files"), token).await;
            let admitted = i >= tokens.len() - allowed;
            assert_eq!(
                reply.code(),
                if admitted { 200 } else { 403 },
                "floor {floor}, role {role}"
            );
            if !admitted {
                assert_eq!(reply.json()["details"]["agent_code"], "forbidden");
            }
        }
    }
    // The refused calls are internal ledger rows that say why.
    assert!(outcomes(&w)
        .await
        .iter()
        .any(|(o, n, _)| o == "policy_denied" && *n > 0));
    w.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn kinds_and_a_file_policy_of_none_hide_a_file_with_the_uniform_404(pool: PgPool) {
    let w = World::new(pool, true).await;
    let keep = note(&w, "Keep", "kept text").await;
    let secret = note(&w, "Contract", "the secret clause").await;
    let book = ledger(&w, "Ledger").await;
    let (_, token) = w.owner_reader().await;
    let listed = |reply: Reply| -> Vec<String> {
        reply.json()["files"]
            .as_array()
            .unwrap()
            .iter()
            .map(|f| f["title"].as_str().unwrap().to_owned())
            .collect()
    };
    assert_eq!(
        listed(get(&w.app, &w.url("/files"), &token).await),
        ["Contract", "Keep", "Ledger"]
    );
    let first = get(&w.app, &w.url(&format!("/files/{secret}/outline")), &token).await;
    assert_eq!(first.code(), 200);
    assert_eq!(w.cache.usage().0, 1);
    let stale_etag = w.etag(secret).await;

    // Deny the file: invisible at once, and its cached text is dropped.
    let denied = file_policy(&w, &w.owner_login, secret, "none").await;
    assert_eq!(denied.code(), 200, "{}", denied.text());
    assert_eq!(denied.json()["changed"], true);
    assert_eq!(denied.json()["project_id"], json!(w.project));
    assert_eq!(w.cache.usage().0, 0);

    let routes = |file: Uuid| -> Vec<(&'static str, String, Option<Value>)> {
        vec![
            ("GET", w.url(&format!("/files/{file}/outline")), None),
            ("GET", w.url(&format!("/files/{file}/blocks?refs=zz")), None),
            (
                "POST",
                w.url(&format!("/files/{file}/blocks:read")),
                Some(json!({"refs":["zz"]})),
            ),
            (
                "GET",
                w.url(&format!("/files/{file}/cells?range=Sheet1!A1:B2")),
                None,
            ),
            (
                "GET",
                w.url(&format!("/files/{file}/sheets/Sheet1/profile")),
                None,
            ),
            ("GET", w.url(&format!("/files/{file}/slides?ids=s1")), None),
            ("GET", w.url(&format!("/files/{file}/find?q=secret")), None),
            ("GET", w.url(&format!("/files/{file}/diff?from=0")), None),
        ]
    };
    let invented = routes(Uuid::new_v4());
    for (i, (method, uri, body)) in routes(secret).iter().enumerate() {
        let hidden = raw(&w.app, method, uri, Some(&token), &[], body.clone()).await;
        let absent = raw(
            &w.app,
            invented[i].0,
            &invented[i].1,
            Some(&token),
            &[],
            invented[i].2.clone(),
        )
        .await;
        assert_eq!((hidden.code(), absent.code()), (404, 404), "{method} {uri}");
        assert_eq!(
            hidden.bytes, absent.bytes,
            "same answer as an absent file: {uri}"
        );
    }
    // A conditional request cannot reveal "unchanged" for a hidden file, and the 8
    // character prefix no longer finds it; another file's prefix still does.
    let conditional = raw(
        &w.app,
        "GET",
        &w.url(&format!("/files/{secret}/outline")),
        Some(&token),
        &[("if-none-match", stale_etag.as_str())],
        None,
    )
    .await;
    assert_eq!(conditional.code(), 404);
    let prefix = |id: Uuid| id.simple().to_string()[..8].to_owned();
    for (id, expected) in [(secret, 404), (keep, 200)] {
        let reply = get(
            &w.app,
            &w.url(&format!("/files/{}/outline", prefix(id))),
            &token,
        )
        .await;
        assert_eq!(reply.code(), expected, "prefix of {id}");
    }
    assert_eq!(
        listed(get(&w.app, &w.url("/files"), &token).await),
        ["Keep", "Ledger"]
    );
    let found = get(
        &w.app,
        &w.url(&format!("/files/{keep}/find?q=kept")),
        &token,
    )
    .await;
    assert_eq!(found.code(), 200);

    // The file layer only narrows: `read` is stored and changes nothing here, and a file
    // override cannot enable a project whose policy is off.
    assert_eq!(
        file_policy(&w, &w.owner_login, keep, "read").await.code(),
        200
    );
    assert_eq!(
        get(&w.app, &w.url(&format!("/files/{keep}/outline")), &token)
            .await
            .code(),
        200
    );
    let overrides = read_policy(&w, &w.owner_login, w.project).await.json();
    assert_eq!(overrides["file_overrides"].as_array().unwrap().len(), 2);
    assert_eq!(overrides["file_overrides_truncated"], false);
    set_policy(&w, w.project, json!({"enabled":false})).await;
    assert_eq!(
        get(&w.app, &w.url(&format!("/files/{keep}/outline")), &token)
            .await
            .code(),
        403
    );
    set_policy(
        &w,
        w.project,
        json!({"enabled":true,"calls_per_minute":600}),
    )
    .await;

    // Back to inherit: readable again; the override list forgets it.
    let restored = file_policy(&w, &w.owner_login, secret, "inherit").await;
    assert_eq!(restored.json()["changed"], true);
    assert_eq!(
        get(&w.app, &w.url(&format!("/files/{secret}/outline")), &token)
            .await
            .code(),
        200
    );
    let overrides = read_policy(&w, &w.owner_login, w.project).await.json();
    assert_eq!(overrides["file_overrides"].as_array().unwrap().len(), 1);
    assert_eq!(overrides["file_overrides"][0]["document_id"], json!(keep));

    // Kinds: the policy admits documents only. The workbook is as absent as an invented
    // file, the listing leaves it out, and a kind filter for it is empty, not an error.
    set_policy(
        &w,
        w.project,
        json!({"enabled":true,"calls_per_minute":600,"kinds":["document","document"]}),
    )
    .await;
    let book_outline = get(&w.app, &w.url(&format!("/files/{book}/outline")), &token).await;
    let invented_outline = get(
        &w.app,
        &w.url(&format!("/files/{}/outline", Uuid::new_v4())),
        &token,
    )
    .await;
    assert_eq!(book_outline.code(), 404);
    assert_eq!(book_outline.bytes, invented_outline.bytes);
    assert_eq!(
        listed(get(&w.app, &w.url("/files"), &token).await),
        ["Contract", "Keep"]
    );
    let sheets = get(&w.app, &w.url("/files?kind=spreadsheet"), &token).await;
    assert_eq!(sheets.code(), 200);
    assert!(listed(sheets).is_empty());
    w.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn only_a_files_owner_or_a_manager_sets_its_policy(pool: PgPool) {
    let w = World::new(pool, true).await;
    let doc = note(&w, "Plan", "text").await;
    let mut seed = 30;
    let mut logins = Vec::new();
    for role in ["viewer", "reviewer", "approver", "editor", "manager"] {
        let (id, login) = person(w.pool(), &format!("{role}@example.test")).await;
        w.member(w.project, id, role).await;
        logins.push((role, id, login));
        seed += 1;
    }
    let _ = seed;
    for (role, _, login) in &logins {
        let reply = file_policy(&w, login, doc, "none").await;
        let expected = if *role == "manager" { 200 } else { 403 };
        assert_eq!(reply.code(), expected, "{role}: {}", reply.text());
    }
    // The manager's change stands; an editor who did not create the file cannot undo it.
    let editor = logins.iter().find(|(r, _, _)| *r == "editor").unwrap();
    let manager = logins.iter().find(|(r, _, _)| *r == "manager").unwrap();
    assert_eq!(file_policy(&w, &editor.2, doc, "inherit").await.code(), 403);
    // The editor who created the file is its owner, whatever the project role says ...
    sqlx::query("update document set created_by=$2 where id=$1")
        .bind(doc)
        .bind(editor.1)
        .execute(&w.runtime.owner)
        .await
        .unwrap();
    // ... but lower layers narrow, never widen: the creator cannot undo the manager's
    // narrowing, step by step or all at once. Repeating it is harmless.
    for loosen in ["inherit", "read"] {
        let reply = file_policy(&w, &editor.2, doc, loosen).await;
        assert_eq!(reply.code(), 403, "{loosen}: {}", reply.text());
    }
    let same = file_policy(&w, &editor.2, doc, "none").await;
    assert_eq!(
        (same.code(), same.json()["changed"].clone()),
        (200, json!(false))
    );
    // A project manager may loosen it, and the creator may then tighten it again.
    let lifted = file_policy(&w, &manager.2, doc, "inherit").await;
    assert_eq!(
        (lifted.code(), lifted.json()["changed"].clone()),
        (200, json!(true))
    );
    let tightened = file_policy(&w, &editor.2, doc, "none").await;
    assert_eq!(
        (tightened.code(), tightened.json()["changed"].clone()),
        (200, json!(true))
    );
    // The middle step is a narrowing too: from read the creator may close but not open.
    assert_eq!(file_policy(&w, &manager.2, doc, "read").await.code(), 200);
    assert_eq!(file_policy(&w, &editor.2, doc, "inherit").await.code(), 403);
    assert_eq!(file_policy(&w, &editor.2, doc, "none").await.code(), 200);
    // A creator demoted to viewer, or one who has left the project, is still the file's
    // owner and may tighten, and still may not loosen.
    sqlx::query("update space_member set role='viewer' where space_id=$1 and identity_id=$2")
        .bind(w.project)
        .bind(editor.1)
        .execute(w.pool())
        .await
        .unwrap();
    assert_eq!(file_policy(&w, &editor.2, doc, "read").await.code(), 403);
    sqlx::query("delete from space_member where space_id=$1 and identity_id=$2")
        .bind(w.project)
        .bind(editor.1)
        .execute(w.pool())
        .await
        .unwrap();
    assert_eq!(file_policy(&w, &editor.2, doc, "inherit").await.code(), 403);
    assert_eq!(file_policy(&w, &editor.2, doc, "none").await.code(), 200);
    assert_eq!(
        file_policy(&w, &w.owner_login, doc, "none").await.code(),
        200
    );

    // Not a member: the file is as absent as an invented one. So is a trashed file and a
    // file that is in no project.
    let (_, outsider) = person(w.pool(), "outsider@example.test").await;
    let outsider_reply = file_policy(&w, &outsider, doc, "none").await;
    let invented = file_policy(&w, &outsider, Uuid::new_v4(), "none").await;
    assert_eq!((outsider_reply.code(), invented.code()), (404, 404));
    assert_eq!(outsider_reply.bytes, invented.bytes);
    let trashed = note(&w, "Old", "text").await;
    sqlx::query("update document set deleted_at=now() where id=$1")
        .bind(trashed)
        .execute(&w.runtime.owner)
        .await
        .unwrap();
    assert_eq!(
        file_policy(&w, &w.owner_login, trashed, "none")
            .await
            .code(),
        404
    );
    let (status, loose) = call(
        &w.app,
        "POST",
        "/documents",
        Some(&w.owner_login),
        json!({"title":"Personal"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{loose}");
    let loose = Uuid::parse_str(loose["id"].as_str().unwrap()).unwrap();
    assert_eq!(
        file_policy(&w, &w.owner_login, loose, "none").await.code(),
        404
    );

    // Values: a documented value problem is 400, an unknown field or shape 422.
    for bad in ["propose", "deny", "", "NONE"] {
        assert_eq!(
            file_policy(&w, &w.owner_login, doc, bad).await.code(),
            400,
            "{bad}"
        );
    }
    for body in [
        json!({"access":"none","extra":1}),
        json!({}),
        json!({"access":1}),
    ] {
        let reply = raw(
            &w.app,
            "PUT",
            &format!("/documents/{doc}/agent-policy"),
            Some(&w.owner_login),
            &[],
            Some(body),
        )
        .await;
        assert_eq!(reply.code(), 422);
    }
    // A repeat changes nothing and writes no audit row.
    let before = audit_rows(&w, "agent.file_policy.changed").await.len();
    let again = file_policy(&w, &w.owner_login, doc, "none").await;
    assert_eq!(again.json()["changed"], false);
    assert_eq!(
        audit_rows(&w, "agent.file_policy.changed").await.len(),
        before
    );
    w.close().await;
}

// ------------------------------------------------------------- management plane

#[sqlx::test(migrations = "../../migrations")]
async fn management_routes_need_an_owner_or_manager_and_an_account_session(pool: PgPool) {
    let w = World::new(pool, true).await;
    let doc = note(&w, "Plan", "text").await;
    let mut logins = Vec::new();
    for (i, role) in ["viewer", "reviewer", "approver", "editor", "manager"]
        .iter()
        .enumerate()
    {
        let (id, login) = person(w.pool(), &format!("{role}@example.test")).await;
        w.member(w.project, id, role).await;
        logins.push((*role, login, 40 + i as u8));
    }
    let project = w.project;
    let revision = "\"1\"";
    let body = json!({"enabled":true,"calls_per_minute":600});
    let (_, outsider) = person(w.pool(), "outsider@example.test").await;

    // Roles that are not owner or manager, a stranger and an unknown project all get the
    // one 403.
    let mut refused: Vec<Reply> = Vec::new();
    for (role, login, _) in &logins[..4] {
        let replies = [
            read_policy(&w, login, project).await,
            put_policy(&w, login, project, revision, body.clone()).await,
            kill(&w, login, project).await,
            usage(&w, login, project, "").await,
        ];
        for reply in replies {
            assert_eq!(reply.code(), 403, "{role}");
            refused.push(reply);
        }
    }
    for reply in [
        read_policy(&w, &outsider, project).await,
        put_policy(&w, &outsider, project, revision, body.clone()).await,
        kill(&w, &outsider, project).await,
        usage(&w, &outsider, project, "").await,
        read_policy(&w, &w.owner_login, Uuid::new_v4()).await,
        put_policy(&w, &w.owner_login, Uuid::new_v4(), revision, body.clone()).await,
        kill(&w, &w.owner_login, Uuid::new_v4()).await,
        usage(&w, &w.owner_login, Uuid::new_v4(), "").await,
    ] {
        assert_eq!(reply.code(), 403);
        refused.push(reply);
    }
    assert!(refused.iter().all(|r| r.bytes == refused[0].bytes));
    // Nothing the refusals tried took effect.
    let after = read_policy(&w, &w.owner_login, project).await.json();
    assert_eq!(
        (after["enabled"].clone(), after["revision"].clone()),
        (json!(true), json!(1))
    );
    assert_eq!(after["calls_per_minute"], 600);

    // The manager and the owner may read and write.
    let manager = &logins[4].1;
    for login in [manager, &w.owner_login] {
        assert_eq!(read_policy(&w, login, project).await.code(), 200);
        assert_eq!(usage(&w, login, project, "").await.code(), 200);
    }
    set_policy(&w, project, json!({"enabled":true,"calls_per_minute":500})).await;
    let by_manager = put_policy(&w, manager, project, "\"2\"", json!({"enabled":true})).await;
    assert_eq!(by_manager.code(), 200, "{}", by_manager.text());
    assert_eq!(file_policy(&w, manager, doc, "none").await.code(), 200);
    assert_eq!(kill(&w, manager, project).await.code(), 200);

    // Credentials of the wrong class never get past authentication: no token, an invented
    // one, a connector-style key, a person's device session and an agent session.
    let (_, _, _, agent_token) = reader_of(&w, "agent@example.test", "viewer", 60).await;
    let owner_person_key = person_session(&w, &w.owner_login, 61).await;
    let connector_like = "ab".repeat(32);
    let tokens: Vec<Option<&str>> = vec![
        None,
        Some("garbage"),
        Some(&connector_like),
        Some(&owner_person_key),
        Some(&agent_token),
    ];
    let paths = [
        ("GET", policy_url(project)),
        ("PUT", policy_url(project)),
        ("POST", format!("{}/kill", policy_url(project))),
        ("GET", format!("/projects/{project}/agent-usage")),
        ("PUT", format!("/documents/{doc}/agent-policy")),
        ("GET", policy_url(Uuid::new_v4())),
    ];
    let before = footprint(&w).await;
    for (method, path) in &paths {
        let mut answers = Vec::new();
        for token in &tokens {
            let body = (*method == "PUT").then(|| json!({"enabled":true,"access":"none"}));
            let reply = raw(&w.app, method, path, *token, &[("if-match", "\"3\"")], body).await;
            assert_eq!(reply.code(), 401, "{method} {path} with {token:?}");
            answers.push(reply.bytes);
        }
        assert!(answers.iter().all(|a| *a == answers[0]));
    }
    assert_eq!(footprint(&w).await, before);
    let state = read_policy(&w, &w.owner_login, project).await.json();
    assert_eq!(
        state["enabled"], false,
        "only the manager's kill changed it"
    );
    w.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn revisions_conflicts_and_validation(pool: PgPool) {
    let w = World::bare(pool, true).await;
    let owner = w.owner_login.clone();
    let project = w.project;
    let full = json!({"enabled":true,"min_role":"editor","kinds":["spreadsheet","document"],
        "calls_per_minute":30,"read_bytes_per_day":50000});

    // If-Match is required, and must be a revision.
    let no_header = raw(
        &w.app,
        "PUT",
        &policy_url(project),
        Some(&owner),
        &[],
        Some(full.clone()),
    )
    .await;
    assert_eq!(no_header.code(), 400);
    for bad in ["abc", "*", "-1", "\"\"", "1.5"] {
        let reply = put_policy(&w, &owner, project, bad, full.clone()).await;
        assert_eq!(reply.code(), 400, "{bad}");
    }
    // A stale or wrong revision is a conflict and writes nothing.
    assert_eq!(
        put_policy(&w, &owner, project, "\"4\"", full.clone())
            .await
            .code(),
        409
    );
    assert_eq!(
        read_policy(&w, &owner, project).await.json()["configured"],
        false
    );

    let first = put_policy(&w, &owner, project, "\"0\"", full.clone()).await;
    assert_eq!(first.code(), 200, "{}", first.text());
    let first = first.json();
    assert_eq!(first["revision"], 1);
    assert_eq!(
        first["kinds"],
        json!(["document", "spreadsheet"]),
        "canonical order"
    );
    assert_eq!(first["min_role"], "editor");
    assert_eq!(first["read_bytes_per_day"], 50000);

    // The same body again: nothing changes, nothing is audited, the revision stays.
    let audited = audit_rows(&w, "agent.policy.changed").await.len();
    let same = put_policy(&w, &owner, project, "\"1\"", full.clone()).await;
    assert_eq!(
        (
            same.code(),
            same.json()["changed"].clone(),
            same.json()["revision"].clone()
        ),
        (200, json!(false), json!(1))
    );
    assert_eq!(audit_rows(&w, "agent.policy.changed").await.len(), audited);
    // A stale revision is still a conflict even for an identical body; weak and bare forms
    // of the validator are understood.
    assert_eq!(
        put_policy(&w, &owner, project, "\"0\"", full.clone())
            .await
            .code(),
        409
    );
    let weak = put_policy(
        &w,
        &owner,
        project,
        "W/\"1\"",
        json!({"enabled":true,"min_role":"manager"}),
    )
    .await;
    assert_eq!(weak.code(), 200);
    assert_eq!(weak.json()["revision"], 2);
    // What a body leaves out keeps its saved value: it cannot silently widen the floor, the
    // kinds or the budgets.
    let view = weak.json();
    assert_eq!(view["kinds"], json!(["document", "spreadsheet"]));
    assert_eq!(
        (
            view["calls_per_minute"].clone(),
            view["read_bytes_per_day"].clone()
        ),
        (json!(30), json!(50000))
    );
    // `enabled` is the one field that must be said.
    for body in [
        json!({}),
        json!({"min_role":"viewer"}),
        json!({"calls_per_minute":600}),
    ] {
        let reply = put_policy(&w, &owner, project, "2", body.clone()).await;
        assert_eq!(reply.code(), 422, "{body}");
    }
    assert_eq!(read_policy(&w, &owner, project).await.json()["revision"], 2);
    let bare = put_policy(&w, &owner, project, "2", json!({"enabled":false})).await;
    assert_eq!(bare.json()["enabled"], false);
    assert_eq!(
        bare.json()["min_role"],
        "manager",
        "kept, not reset to viewer"
    );
    assert_eq!(bare.header("etag").as_deref(), Some("\"3\""));
    assert_eq!(
        read_policy(&w, &owner, project)
            .await
            .header("etag")
            .as_deref(),
        Some("\"3\"")
    );

    // Values.
    for (body, why) in [
        (json!({"min_role":"god"}), "min_role"),
        (json!({"min_role":"Viewer"}), "case"),
        (json!({"kinds":[]}), "no kinds"),
        (json!({"kinds":["pdf"]}), "kind"),
        (json!({"calls_per_minute":0}), "calls low"),
        (json!({"calls_per_minute":601}), "calls high"),
        (json!({"calls_per_minute":-5}), "calls negative"),
        (json!({"read_bytes_per_day":9999}), "bytes low"),
        (json!({"read_bytes_per_day":1_000_000_001i64}), "bytes high"),
    ] {
        let mut body = body;
        body["enabled"] = json!(true);
        let reply = put_policy(&w, &owner, project, "3", body).await;
        assert_eq!(reply.code(), 400, "{why}: {}", reply.text());
    }
    for (body, why) in [
        (json!({"enable":true}), "unknown field"),
        (json!({"enabled":"yes"}), "type"),
        (json!({"calls_per_minute":1.5}), "float"),
        (json!([1]), "array"),
    ] {
        let reply = put_policy(&w, &owner, project, "3", body).await;
        assert_eq!(reply.code(), 422, "{why}");
    }
    let edges = put_policy(&w, &owner, project, "3", json!({"enabled":true,"calls_per_minute":600,"read_bytes_per_day":1_000_000_000i64,"min_role":"owner"})).await;
    assert_eq!(edges.code(), 200, "{}", edges.text());
    let edges = put_policy(
        &w,
        &owner,
        project,
        "4",
        json!({"enabled":true,"calls_per_minute":1,"read_bytes_per_day":10000}),
    )
    .await;
    assert_eq!(edges.code(), 200, "{}", edges.text());
    // The floor set to owner survives a body that only restates `enabled` and the budgets.
    assert_eq!(edges.json()["min_role"], "owner");
    // The shape of every answer is the one OpenAPI describes.
    let spec = raw(&w.app, "GET", "/openapi.json", None, &[], None)
        .await
        .json();
    matches_schema(&spec, "PolicyView", &edges.json(), &[]);
    matches_schema(
        &spec,
        "PolicyView",
        &read_policy(&w, &owner, project).await.json(),
        &["changed"],
    );
    w.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn every_change_is_audited_with_its_actor_and_the_fields_that_changed(pool: PgPool) {
    let w = World::bare(pool, true).await;
    let doc = note(&w, "Plan", "text").await;
    let (manager, manager_login) = person(w.pool(), "manager@example.test").await;
    w.member(w.project, manager, "manager").await;
    let before = audit_rows(&w, "agent.%").await.len();
    assert_eq!(before, 0);

    put_policy(
        &w,
        &w.owner_login,
        w.project,
        "0",
        json!({"enabled":true,"min_role":"editor","calls_per_minute":100}),
    )
    .await;
    put_policy(
        &w,
        &manager_login,
        w.project,
        "1",
        json!({"enabled":true,"min_role":"editor","calls_per_minute":100,"kinds":["document"]}),
    )
    .await;
    // No effective change, no row.
    put_policy(
        &w,
        &manager_login,
        w.project,
        "2",
        json!({"enabled":true,"min_role":"editor","calls_per_minute":100,"kinds":["document"]}),
    )
    .await;
    assert_eq!(
        file_policy(&w, &manager_login, doc, "none").await.code(),
        200
    );
    assert_eq!(
        file_policy(&w, &manager_login, doc, "none").await.code(),
        200
    );
    assert_eq!(
        file_policy(&w, &w.owner_login, doc, "inherit").await.code(),
        200
    );
    assert_eq!(kill(&w, &w.owner_login, w.project).await.code(), 200);

    let rows = audit_rows(&w, "agent.%").await;
    let summary: Vec<(String, Uuid, Uuid)> = rows
        .iter()
        .map(|(a, n, r, _)| (n.clone(), *a, *r))
        .collect();
    assert_eq!(
        summary,
        vec![
            ("agent.policy.changed".to_owned(), w.owner, w.project),
            ("agent.policy.changed".to_owned(), manager, w.project),
            ("agent.file_policy.changed".to_owned(), manager, doc),
            ("agent.file_policy.changed".to_owned(), w.owner, doc),
            ("agent.killswitch".to_owned(), w.owner, w.project),
        ]
    );
    // The first save: every field that differs from the defaults, with before and after.
    assert_eq!(rows[0].3["revision"], 1);
    assert_eq!(rows[0].3["previous_revision"], 0);
    assert_eq!(
        rows[0].3["changed"],
        json!(["enabled", "min_role", "calls_per_minute"])
    );
    assert_eq!(
        rows[0].3["from"],
        json!({"enabled":false,"min_role":"viewer","calls_per_minute":60})
    );
    assert_eq!(
        rows[0].3["to"],
        json!({"enabled":true,"min_role":"editor","calls_per_minute":100})
    );
    // The second names only what it changed.
    assert_eq!(rows[1].3["changed"], json!(["kinds"]));
    assert_eq!(
        rows[1].3["from"],
        json!({"kinds":["document","spreadsheet","presentation"]})
    );
    assert_eq!(rows[1].3["to"], json!({"kinds":["document"]}));
    assert_eq!(
        rows[2].3,
        json!({"document_id":doc,"project_id":w.project,"from":"inherit","to":"none"})
    );
    assert_eq!(rows[3].3["from"], "none");
    assert_eq!(
        rows[4].3,
        json!({"was_enabled":true,"revision":3,"revoked_grants":0})
    );
    // Policy values only: nothing a caller typed beyond enumerated values and numbers.
    for (_, _, _, detail) in &rows {
        assert!(detail.to_string().len() < 400);
    }
    w.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn concurrent_updates_are_serialised(pool: PgPool) {
    let w = World::bare(pool, true).await;
    let doc = note(&w, "Plan", "text").await;
    // Eight managers save the first policy at once with the same If-Match: exactly one wins.
    let mut logins = Vec::new();
    for i in 0..8 {
        let (id, login) = person(w.pool(), &format!("m{i}@example.test")).await;
        w.member(w.project, id, "manager").await;
        logins.push(login);
    }
    let racing = |revision: &'static str, base: usize| {
        logins
            .iter()
            .enumerate()
            .map(|(i, login)| {
                let app = w.app.clone();
                let login = login.clone();
                let url = policy_url(w.project);
                tokio::spawn(async move {
                    raw(
                        &app,
                        "PUT",
                        &url,
                        Some(&login),
                        &[("if-match", revision)],
                        Some(json!({"enabled":true,"calls_per_minute":base + i})),
                    )
                    .await
                    .code()
                })
            })
            .collect::<Vec<_>>()
    };
    // Each round's bodies differ from the saved policy and from each other.
    for (revision, base, expected) in [("0", 10, 1), ("1", 100, 2), ("2", 200, 3)] {
        let codes: Vec<u16> = futures::future::join_all(racing(revision, base))
            .await
            .into_iter()
            .map(|r| r.unwrap())
            .collect();
        assert_eq!(
            codes.iter().filter(|c| **c == 200).count(),
            1,
            "{revision}: {codes:?}"
        );
        assert_eq!(
            codes.iter().filter(|c| **c == 409).count(),
            7,
            "{revision}: {codes:?}"
        );
        let view = read_policy(&w, &w.owner_login, w.project).await.json();
        assert_eq!(view["revision"], expected);
    }
    // One audit row per winner, and the revisions are exactly 1, 2, 3.
    let rows = audit_rows(&w, "agent.policy.changed").await;
    let revisions: Vec<i64> = rows
        .iter()
        .map(|r| r.3["revision"].as_i64().unwrap())
        .collect();
    assert_eq!(revisions, [1, 2, 3]);

    // Concurrent file policy changes: both succeed, exactly one is a change.
    let mut tasks = Vec::new();
    for login in logins.iter().take(4) {
        let app = w.app.clone();
        let login = login.clone();
        let url = format!("/documents/{doc}/agent-policy");
        tasks.push(tokio::spawn(async move {
            raw(
                &app,
                "PUT",
                &url,
                Some(&login),
                &[],
                Some(json!({"access":"none"})),
            )
            .await
            .json()
        }));
    }
    let answers: Vec<Value> = futures::future::join_all(tasks)
        .await
        .into_iter()
        .map(|r| r.unwrap())
        .collect();
    assert_eq!(
        answers.iter().filter(|a| a["changed"] == true).count(),
        1,
        "{answers:?}"
    );
    assert_eq!(audit_rows(&w, "agent.file_policy.changed").await.len(), 1);
    // Concurrent kills are idempotent.
    let mut tasks = Vec::new();
    for login in logins.iter().take(4) {
        let app = w.app.clone();
        let login = login.clone();
        let url = format!("{}/kill", policy_url(w.project));
        tasks.push(tokio::spawn(async move {
            raw(&app, "POST", &url, Some(&login), &[], None)
                .await
                .json()
        }));
    }
    let answers: Vec<Value> = futures::future::join_all(tasks)
        .await
        .into_iter()
        .map(|r| r.unwrap())
        .collect();
    assert_eq!(
        answers.iter().filter(|a| a["was_enabled"] == true).count(),
        1,
        "{answers:?}"
    );
    // Every pull is one change of the policy row: revisions 4 to 7, each once.
    let mut revisions: Vec<i64> = answers
        .iter()
        .map(|a| a["revision"].as_i64().unwrap())
        .collect();
    revisions.sort();
    assert_eq!(revisions, [4, 5, 6, 7]);
    w.close().await;
}

// ------------------------------------------------------------------ kill switch

#[sqlx::test(migrations = "../../migrations")]
async fn the_kill_switch_ends_every_agent_grant_purges_the_cache_and_is_audited(pool: PgPool) {
    let w = World::new(pool, true).await;
    let doc = note(&w, "Plan", "text").await;
    let (viewer, viewer_login, viewer_agent, viewer_token) =
        reader_of(&w, "viewer@example.test", "viewer", 70).await;
    let (owner_agent, owner_token) = w.owner_reader().await;
    // A person's key with its own grant: not an agent grant, so it must survive.
    let person_token = person_session(&w, &w.owner_login, 71).await;
    // And an agent grant in another project, which must survive too.
    let other = w.project("Elsewhere").await;
    w.enable_agents(other).await;
    let elsewhere = w.grant(&owner_agent, other, &["content:read"]).await;
    assert_eq!(elsewhere.0, StatusCode::OK, "{}", elsewhere.1);
    let other_token = w.token(&owner_agent, other, &["content:read"]).await;

    for token in [&viewer_token, &owner_token] {
        assert_eq!(
            get(&w.app, &w.url(&format!("/files/{doc}/outline")), token)
                .await
                .code(),
            200
        );
    }
    assert_eq!(w.cache.usage().0, 1);
    let before = footprint(&w).await;
    let heads_before = heads(&w).await;

    // Only an owner or manager may pull it; a refusal changes nothing.
    assert_eq!(kill(&w, &viewer_login, w.project).await.code(), 403);
    assert_eq!(kill(&w, "garbage", w.project).await.code(), 401);
    assert_eq!(kill(&w, &viewer_token, w.project).await.code(), 401);
    assert_eq!(
        get(&w.app, &w.url("/files"), &owner_token).await.code(),
        200
    );

    let killed = kill(&w, &w.owner_login, w.project).await;
    assert_eq!(killed.code(), 200, "{}", killed.text());
    let killed = killed.json();
    assert_eq!(
        (
            killed["enabled"].clone(),
            killed["was_enabled"].clone(),
            killed["revoked_grants"].clone(),
            killed["revision"].clone()
        ),
        (json!(false), json!(true), json!(2), json!(2))
    );
    let spec = raw(&w.app, "GET", "/openapi.json", None, &[], None)
        .await
        .json();
    matches_schema(&spec, "KillView", &killed, &[]);

    // Every agent session of the project is dead at once, and the cache is empty.
    for token in [&viewer_token, &owner_token] {
        assert_eq!(get(&w.app, &w.url("/files"), token).await.code(), 403);
        assert_eq!(get(&w.app, &w.url("/access"), token).await.code(), 403);
    }
    assert_eq!(w.cache.usage().0, 0);
    // The grants are revoked, not deleted; the person's grant and the other project stay.
    let grants: Vec<(Uuid, bool)> = sqlx::query_as("select key_id,revoked_at is not null from device_key_project_grant where project_id=$1 order by created_at")
        .bind(w.project)
        .fetch_all(&w.runtime.owner)
        .await
        .unwrap();
    assert_eq!(grants.len(), 3);
    assert_eq!(grants.iter().filter(|(_, revoked)| *revoked).count(), 2);
    let person_grants = get(
        &w.app,
        &format!("/projects/{}/files", w.project),
        &person_token,
    )
    .await;
    assert_eq!(person_grants.code(), 200, "a person's key is untouched");
    assert_eq!(
        get(
            &w.app,
            &format!("/projects/{other}/agent/files"),
            &other_token
        )
        .await
        .code(),
        200
    );

    // Enabling again restores nothing that was revoked: the old tokens stay dead, no new
    // session can be opened until the key holders grant again.
    set_policy(
        &w,
        w.project,
        json!({"enabled":true,"calls_per_minute":600}),
    )
    .await;
    for token in [&viewer_token, &owner_token] {
        assert_eq!(get(&w.app, &w.url("/files"), token).await.code(), 403);
    }
    let (status, _) = w.open(&viewer_agent, w.project, &["content:read"]).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(
        w.grant(&viewer_agent, w.project, &["content:read"]).await.0,
        StatusCode::OK
    );
    let fresh = w.token(&viewer_agent, w.project, &["content:read"]).await;
    assert_eq!(get(&w.app, &w.url("/files"), &fresh).await.code(), 200);
    assert_eq!(
        get(&w.app, &w.url("/files"), &viewer_token).await.code(),
        403,
        "the old session stays dead after a new grant"
    );

    // Nothing but grants (revoked) and audit rows changed; history is untouched.
    let mut after = footprint(&w).await;
    let mut expected = before;
    for (table, n) in &mut expected {
        // A new grant row and one device session for the viewer; ledger rows of reads.
        match table.as_str() {
            "device_key_project_grant" => *n += 1,
            "device_session" => *n += 1,
            _ => {}
        }
    }
    after.retain(|(t, _)| t != "agent_call");
    expected.retain(|(t, _)| t != "agent_call");
    assert_eq!(after, expected);
    assert_eq!(heads(&w).await, heads_before);

    // The audit: the switch, and one revocation per ended grant under the key holder's own
    // action, all naming the manager.
    let switch = audit_rows(&w, "agent.killswitch").await;
    assert_eq!(switch.len(), 1);
    assert_eq!(switch[0].0, w.owner);
    assert_eq!(
        switch[0].3,
        json!({"was_enabled":true,"revision":2,"revoked_grants":2})
    );
    let revoked = audit_rows(&w, "device.grant.revoked").await;
    assert_eq!(revoked.len(), 2);
    let mut revoked_keys: Vec<Uuid> = revoked.iter().map(|r| r.2).collect();
    revoked_keys.sort();
    let mut expected_keys = vec![viewer_agent.key_id, owner_agent.key_id];
    expected_keys.sort();
    assert_eq!(revoked_keys, expected_keys);
    for (actor, _, _, detail) in &revoked {
        assert_eq!(*actor, w.owner);
        assert_eq!(detail["reason"], "agent_killswitch");
        assert_eq!(detail["project_id"], json!(w.project));
        assert_eq!(detail["scopes"], json!(["content:read"]));
    }
    let _ = viewer;

    // Pulling it again is harmless and still audited; with nothing to end it says so.
    let again = kill(&w, &w.owner_login, w.project).await.json();
    assert_eq!(
        (
            again["was_enabled"].clone(),
            again["revoked_grants"].clone(),
            again["revision"].clone()
        ),
        (json!(true), json!(1), json!(4))
    );
    assert_eq!(
        get(&w.app, &w.url("/files"), &fresh).await.code(),
        403,
        "the new session is cut off by the later pull"
    );
    let twice = kill(&w, &w.owner_login, w.project).await.json();
    assert_eq!(
        (
            twice["was_enabled"].clone(),
            twice["revoked_grants"].clone(),
            twice["revision"].clone()
        ),
        (json!(false), json!(0), json!(5))
    );
    assert_eq!(audit_rows(&w, "agent.killswitch").await.len(), 3);
    let view = read_policy(&w, &w.owner_login, w.project).await.json();
    assert!(view["killed_at"].is_string());

    // A project that never had a policy: the switch creates the (disabled) row that holds
    // the cut-off.
    let fresh_project = w.project("Never configured").await;
    let reply = kill(&w, &w.owner_login, fresh_project).await.json();
    assert_eq!(
        (reply["revision"].clone(), reply["was_enabled"].clone()),
        (json!(1), json!(false))
    );
    let view = read_policy(&w, &w.owner_login, fresh_project).await.json();
    assert_eq!(
        (view["configured"].clone(), view["enabled"].clone()),
        (json!(true), json!(false))
    );
    w.close().await;
}

// ------------------------------------------------------------------ usage

#[sqlx::test(migrations = "../../migrations")]
async fn usage_reports_calls_bytes_and_denials_by_day_and_client_within_bounds(pool: PgPool) {
    let w = World::new(pool, true).await;
    let doc = note(&w, "Plan", "text").await;
    let (agent, token) = w.owner_reader().await;
    let tester = [("x-dynodoc-client", "tester/1")];
    let other = [("x-dynodoc-client", "other/2")];
    for headers in [&tester[..], &tester[..], &tester[..], &other[..], &[][..]] {
        let reply = raw(
            &w.app,
            "GET",
            &w.url(&format!("/files/{doc}/outline")),
            Some(&token),
            headers,
            None,
        )
        .await;
        assert_eq!(reply.code(), 200);
    }
    // A denial while disabled is counted too.
    set_policy(&w, w.project, json!({"enabled":false})).await;
    assert_eq!(
        raw(&w.app, "GET", &w.url("/files"), Some(&token), &tester, None)
            .await
            .code(),
        403
    );
    set_policy(
        &w,
        w.project,
        json!({"enabled":true,"calls_per_minute":600}),
    )
    .await;
    // An older day, an old day outside the default window and another project's call.
    ledger_row(
        &w,
        agent.key_id,
        w.owner,
        w.project,
        "3 days",
        700,
        "ok",
        Some("tester/1"),
    )
    .await;
    ledger_row(
        &w,
        agent.key_id,
        w.owner,
        w.project,
        "40 days",
        900,
        "ok",
        Some("tester/1"),
    )
    .await;
    let elsewhere = w.project("Elsewhere").await;
    ledger_row(
        &w,
        agent.key_id,
        w.owner,
        elsewhere,
        "1 minute",
        111,
        "ok",
        Some("tester/1"),
    )
    .await;

    let today: String = sqlx::query_scalar("select (now() at time zone 'utc')::date::text")
        .fetch_one(&w.runtime.owner)
        .await
        .unwrap();
    let three: String = sqlx::query_scalar("select ((now() at time zone 'utc')::date-3)::text")
        .fetch_one(&w.runtime.owner)
        .await
        .unwrap();
    let reply = usage(&w, &w.owner_login, w.project, "").await;
    assert_eq!(reply.code(), 200, "{}", reply.text());
    let body = reply.json();
    let spec = raw(&w.app, "GET", "/openapi.json", None, &[], None)
        .await
        .json();
    matches_schema(&spec, "UsageView", &body, &[]);
    for item in body["items"].as_array().unwrap() {
        matches_schema(&spec, "UsageRow", item, &[]);
    }
    assert_eq!(body["to"], today.as_str());
    assert_eq!(body["truncated"], false);
    let bytes_of: i64 = sqlx::query_scalar("select coalesce(sum(bytes_out),0)::bigint from agent_call where project_id=$1 and client='tester/1' and outcome='ok' and at>now()-interval '1 day'")
        .bind(w.project)
        .fetch_one(&w.runtime.owner)
        .await
        .unwrap();
    let rows: Vec<(String, Option<String>, i64, i64, i64, i64)> = body["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| {
            (
                i["day"].as_str().unwrap().to_owned(),
                i["client"].as_str().map(str::to_owned),
                i["calls"].as_i64().unwrap(),
                i["bytes_out"].as_i64().unwrap(),
                i["denials"].as_i64().unwrap(),
                i["errors"].as_i64().unwrap(),
            )
        })
        .collect();
    // Today: the tester made 3 reads and 1 denial, "other" 1 read, one call named no client.
    let find = |day: &str, client: Option<&str>| {
        rows.iter()
            .find(|r| r.0 == day && r.1.as_deref() == client)
            .cloned()
            .unwrap_or_else(|| panic!("{day} {client:?} in {rows:?}"))
    };
    let t = find(&today, Some("tester/1"));
    assert_eq!((t.2, t.4, t.5), (4, 1, 0));
    assert_eq!(t.3, bytes_of);
    assert!(t.3 > 0);
    let o = find(&today, Some("other/2"));
    assert_eq!((o.2, o.4), (1, 0));
    assert!(o.3 > 0);
    let n = find(&today, None);
    assert_eq!((n.2, n.4), (1, 0));
    let old = find(&three, Some("tester/1"));
    assert_eq!((old.2, old.3, old.4), (1, 700, 0));
    assert_eq!(
        rows.len(),
        4,
        "the 40 day old row and the other project are not here: {rows:?}"
    );
    // Newest day first.
    assert!(rows[0].0 >= rows[rows.len() - 1].0);

    // An explicit range finds the old row; the bounds are refused, not clamped.
    let wide = usage(
        &w,
        &w.owner_login,
        w.project,
        "?from=2026-01-01&to=2026-01-31",
    )
    .await;
    assert_eq!(wide.code(), 200);
    assert!(wide.json()["items"].as_array().unwrap().is_empty());
    for query in [
        "?from=2026-01-01&to=2026-02-01",
        "?from=2026-02-01&to=2026-01-01",
        "?from=banana",
        "?to=2026-02-30",
        "?from=2026-1-1",
        "?extra=1",
        "?from=2026-01-01T00:00:00Z",
    ] {
        assert_eq!(
            usage(&w, &w.owner_login, w.project, query).await.code(),
            400,
            "{query}"
        );
    }
    let one = usage(
        &w,
        &w.owner_login,
        w.project,
        &format!("?from={three}&to={three}"),
    )
    .await
    .json();
    assert_eq!(one["items"].as_array().unwrap().len(), 1);

    // At most 500 rows, and it says when there were more.
    sqlx::query("insert into agent_call(principal_id,key_id,project_id,tool,request_digest,bytes_out,tok_est,outcome,client) select $1,$2,$3,'files',decode(repeat('00',32),'hex'),10,3,'ok','client-'||g from generate_series(1,520) g")
        .bind(w.owner)
        .bind(agent.key_id)
        .bind(w.project)
        .execute(&w.runtime.owner)
        .await
        .unwrap();
    let big = usage(&w, &w.owner_login, w.project, "").await.json();
    assert_eq!(big["items"].as_array().unwrap().len(), 500);
    assert_eq!(big["truncated"], true);
    // Metadata only: the response holds no document text, title or request value.
    assert!(!big.to_string().contains("Plan"));
    w.close().await;
}

// ------------------------------------------------------------------ budgets

#[sqlx::test(migrations = "../../migrations")]
async fn budgets_are_enforced_per_grant_from_the_ledger(pool: PgPool) {
    let w = World::new(pool, true).await;
    let doc = note(&w, "Plan", "text").await;
    let (agent, token) = w.owner_reader().await;
    let (_, _, _, second_token) = reader_of(&w, "second@example.test", "editor", 80).await;
    set_policy(&w, w.project, json!({"enabled":true,"calls_per_minute":3})).await;
    // Rows older than a minute do not count.
    for _ in 0..5 {
        ledger_row(
            &w,
            agent.key_id,
            w.owner,
            w.project,
            "2 minutes",
            10,
            "ok",
            None,
        )
        .await;
    }
    for _ in 0..3 {
        assert_eq!(get(&w.app, &w.url("/files"), &token).await.code(), 200);
    }
    let refused = get(&w.app, &w.url("/files"), &token).await;
    assert_eq!(refused.code(), 429, "{}", refused.text());
    assert_eq!(refused.json()["details"]["agent_code"], "budget_exceeded");
    let wait: u64 = refused.header("retry-after").unwrap().parse().unwrap();
    assert!((1..=60).contains(&wait), "{wait}");
    assert_eq!(refused.json()["details"]["retry_after"], wait);
    // The file routes and the access view share the budget; refusals do not extend it.
    for uri in [w.url(&format!("/files/{doc}/outline")), w.url("/access")] {
        assert_eq!(get(&w.app, &uri, &token).await.code(), 429, "{uri}");
    }
    // Another grant of the project has its own.
    assert_eq!(
        get(&w.app, &w.url("/files"), &second_token).await.code(),
        200
    );
    let kinds = outcomes(&w).await;
    assert!(
        kinds.contains(&("budget_exceeded".to_owned(), 1, 0)),
        "four refusals in a minute are one row: {kinds:?}"
    );
    // The refused calls are recorded, and the grant recovers when its calls age out:
    // simulate by raising the limit (the ledger is append-only and cannot be edited).
    set_policy(&w, w.project, json!({"enabled":true,"calls_per_minute":50})).await;
    let access = get(&w.app, &w.url("/access"), &token).await;
    assert_eq!(access.code(), 200);
    assert_eq!(access.json()["budget"]["calls_per_minute"]["limit"], 50);
    assert_eq!(
        access.json()["budget"]["calls_per_minute"]["used"],
        3,
        "refusals are not counted"
    );

    // The byte budget counts the grant's response bytes since midnight UTC.
    set_policy(
        &w,
        w.project,
        json!({"enabled":true,"calls_per_minute":600,"read_bytes_per_day":10000}),
    )
    .await;
    assert_eq!(get(&w.app, &w.url("/files"), &token).await.code(), 200);
    // Yesterday's bytes and another grant's bytes do not count.
    ledger_row(
        &w,
        agent.key_id,
        w.owner,
        w.project,
        "25 hours",
        1_000_000,
        "ok",
        None,
    )
    .await;
    let second_key: Uuid =
        sqlx::query_scalar("select id from device_signing_key where identity_id<>$1 limit 1")
            .bind(w.owner)
            .fetch_one(&w.runtime.owner)
            .await
            .unwrap();
    ledger_row(
        &w, second_key, w.owner, w.project, "1 minute", 1_000_000, "ok", None,
    )
    .await;
    assert_eq!(get(&w.app, &w.url("/files"), &token).await.code(), 200);
    ledger_row(
        &w,
        agent.key_id,
        w.owner,
        w.project,
        "1 minute",
        10_000,
        "ok",
        None,
    )
    .await;
    let spent = get(&w.app, &w.url("/files"), &token).await;
    assert_eq!(spent.code(), 429, "{}", spent.text());
    let wait: u64 = spent.header("retry-after").unwrap().parse().unwrap();
    assert!((1..=86_400).contains(&wait), "{wait}");
    set_policy(
        &w,
        w.project,
        json!({"enabled":true,"calls_per_minute":600,"read_bytes_per_day":1_000_000}),
    )
    .await;
    let access = get(&w.app, &w.url("/access"), &token).await.json();
    assert!(
        access["budget"]["read_bytes_per_day"]["used"]
            .as_i64()
            .unwrap()
            >= 10_000
    );
    assert_eq!(access["budget"]["read_bytes_per_day"]["limit"], 1_000_000);
    w.close().await;
}

// ------------------------------------------------------------------ history

#[sqlx::test(migrations = "../../migrations")]
async fn policy_changes_never_touch_history_tables(pool: PgPool) {
    let w = World::new(pool, true).await;
    let doc = note(&w, "Plan", "text").await;
    let (_, token) = w.owner_reader().await;
    assert_eq!(
        get(&w.app, &w.url(&format!("/files/{doc}/outline")), &token)
            .await
            .code(),
        200
    );
    let before = footprint(&w).await;
    let heads_before = heads(&w).await;
    let audit_before: i64 = sqlx::query_scalar("select count(*) from operation_audit")
        .fetch_one(&w.runtime.owner)
        .await
        .unwrap();

    set_policy(&w, w.project, json!({"enabled":true,"min_role":"editor"})).await;
    set_policy(&w, w.project, json!({"enabled":true,"kinds":["document"]})).await;
    assert_eq!(
        file_policy(&w, &w.owner_login, doc, "none").await.code(),
        200
    );
    assert_eq!(
        file_policy(&w, &w.owner_login, doc, "inherit").await.code(),
        200
    );
    assert_eq!(usage(&w, &w.owner_login, w.project, "").await.code(), 200);
    assert_eq!(kill(&w, &w.owner_login, w.project).await.code(), 200);

    // The kill switch revoked the owner's grant (its row stays, so the count is the same);
    // nothing else moved: no event, snapshot, draft, review, member, access or ledger row.
    assert_eq!(footprint(&w).await, before);
    assert_eq!(heads(&w).await, heads_before);
    let audit_after: i64 = sqlx::query_scalar("select count(*) from operation_audit")
        .fetch_one(&w.runtime.owner)
        .await
        .unwrap();
    // 2 saves, 2 file changes, the switch and the one grant it ended.
    assert_eq!(audit_after - audit_before, 6);
    w.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_runtime_login_can_edit_but_never_delete_or_skip_a_revision(pool: PgPool) {
    let w = World::new(pool, true).await;
    let doc = note(&w, "Plan", "text").await;
    let other = w.project("Elsewhere").await;
    assert_eq!(
        file_policy(&w, &w.owner_login, doc, "none").await.code(),
        200
    );
    let runtime = &w.runtime.pool;
    // No delete or truncate, at the privilege level, even for zero rows.
    for statement in [
        "delete from project_agent_policy where false",
        "truncate project_agent_policy cascade",
        "delete from file_agent_policy where false",
        "truncate file_agent_policy cascade",
    ] {
        let error = sqlx::raw_sql(statement).execute(runtime).await.unwrap_err();
        assert_eq!(
            error.as_database_error().and_then(|e| e.code()).as_deref(),
            Some("42501"),
            "{statement}: {error}"
        );
    }
    // The revision moves by exactly one, and the key never changes.
    for (statement, message) in [
        (format!("update project_agent_policy set revision=revision+2 where project_id='{}'", w.project), "exactly one"),
        (format!("update project_agent_policy set revision=revision where project_id='{}'", w.project), "exactly one"),
        (format!("update project_agent_policy set project_id='{other}',revision=revision+1 where project_id='{}'", w.project), "cannot change"),
        (format!("update file_agent_policy set document_id='{}' where document_id='{doc}'", Uuid::new_v4()), "cannot change"),
    ] {
        let error = sqlx::raw_sql(&statement).execute(runtime).await.unwrap_err();
        assert!(error.to_string().contains(message), "{statement}: {error}");
    }
    let ok = sqlx::raw_sql(&format!(
        "update project_agent_policy set revision=revision+1 where project_id='{}'",
        w.project
    ))
    .execute(runtime)
    .await;
    assert!(ok.is_ok());
    // The kill switch's cut-off never moves back and never returns to null (NULL < x is
    // NULL, so a plain comparison would let `set ... = null` through).
    assert_eq!(kill(&w, &w.owner_login, w.project).await.code(), 200);
    for set in ["null", "sessions_valid_after - interval '1 second'"] {
        let statement = format!("update project_agent_policy set sessions_valid_after={set},revision=revision+1 where project_id='{}'", w.project);
        let error = sqlx::raw_sql(&statement)
            .execute(runtime)
            .await
            .unwrap_err();
        assert!(
            error.to_string().contains("cannot move back"),
            "{statement}: {error}"
        );
    }
    let forward = sqlx::raw_sql(&format!("update project_agent_policy set sessions_valid_after=sessions_valid_after + interval '1 second',revision=revision+1 where project_id='{}'", w.project))
        .execute(runtime)
        .await;
    assert!(forward.is_ok(), "{forward:?}");
    // From no cut-off to one is fine: that is the first pull.
    sqlx::raw_sql(&format!(
        "insert into project_agent_policy(project_id,updated_by) values('{other}','{}')",
        w.owner
    ))
    .execute(runtime)
    .await
    .unwrap();
    let first = sqlx::raw_sql(&format!("update project_agent_policy set sessions_valid_after=now(),revision=revision+1 where project_id='{other}'"))
        .execute(runtime)
        .await;
    assert!(first.is_ok(), "{first:?}");
    sqlx::raw_sql(&format!("update project_agent_policy set enabled=false,revision=revision+1 where project_id='{other}'"))
        .execute(runtime)
        .await
        .unwrap();
    // Values the application would never write are refused by the table too.
    for (columns, values) in [
        ("kinds", "'{}'"),
        ("kinds", "'{pdf}'"),
        ("kinds", "'{document,document}'"),
        ("kinds", "'{document,spreadsheet,document}'"),
        ("kinds", "'{presentation,presentation,presentation}'"),
        ("min_role", "'god'"),
        ("calls_per_minute", "0"),
        ("calls_per_minute", "601"),
        ("read_bytes_per_day", "9999"),
        ("read_bytes_per_day", "1000000001"),
        ("revision", "0"),
    ] {
        let statement = format!("insert into project_agent_policy(project_id,updated_by,{columns}) values('{other}','{}',{values})", w.owner);
        let error = sqlx::raw_sql(&statement)
            .execute(runtime)
            .await
            .unwrap_err();
        assert_eq!(
            error.as_database_error().and_then(|e| e.code()).as_deref(),
            Some("23514"),
            "{statement}: {error}"
        );
    }
    let error = sqlx::raw_sql(&format!("insert into file_agent_policy(document_id,access,updated_by) values('{doc}','propose','{}')", w.owner))
        .execute(runtime)
        .await
        .unwrap_err();
    assert!(
        error.to_string().contains("23514")
            || error.as_database_error().and_then(|e| e.code()).as_deref() == Some("23514")
            || error.as_database_error().and_then(|e| e.code()).as_deref() == Some("23505")
    );
    // A bare insert is the disabled default: a row alone enables nothing.
    let enabled: bool =
        sqlx::query_scalar("select enabled from project_agent_policy where project_id=$1")
            .bind(other)
            .fetch_one(runtime)
            .await
            .unwrap();
    assert!(!enabled);
    w.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn erasing_a_document_removes_its_file_policy_and_keeps_the_audit_row(pool: PgPool) {
    let w = World::new(pool, true).await;
    let (_, admin_login) = person(w.pool(), "admin@example.test").await;
    let doc = note(&w, "Plan", "text").await;
    let keep = note(&w, "Keep", "text").await;
    for id in [doc, keep] {
        assert_eq!(
            file_policy(&w, &w.owner_login, id, "none").await.code(),
            200
        );
    }
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
        json!({"kind":"documents","id":doc,"confirmation":"Plan","revision":preview["revision"],"reason":"owner_request","acknowledge_backups":true}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{result}");
    let left: Vec<Uuid> = sqlx::query_scalar("select document_id from file_agent_policy")
        .fetch_all(&w.runtime.owner)
        .await
        .unwrap();
    assert_eq!(left, [keep]);
    let audited = audit_rows(&w, "agent.file_policy.changed").await;
    assert_eq!(audited.len(), 2, "the audit is history and stays");
    w.close().await;
}

// ------------------------------------------------------------------ gateway off

#[sqlx::test(migrations = "../../migrations")]
async fn the_management_routes_exist_whether_or_not_the_gateway_is_on(pool: PgPool) {
    let w = World::bare(pool, false).await;
    let doc = note(&w, "Plan", "text").await;
    let view = read_policy(&w, &w.owner_login, w.project).await;
    assert_eq!(view.code(), 200);
    assert_eq!(view.json()["gateway"], false);
    set_policy(&w, w.project, json!({"enabled":true})).await;
    assert_eq!(
        file_policy(&w, &w.owner_login, doc, "none").await.code(),
        200
    );
    assert_eq!(usage(&w, &w.owner_login, w.project, "").await.code(), 200);
    assert_eq!(kill(&w, &w.owner_login, w.project).await.code(), 200);
    // The agent routes are still absent whatever the policy says.
    let missing = get(&w.app, &w.url("/files"), "anything").await;
    assert_eq!(missing.code(), 404);

    // OpenAPI: with the gateway off the document drops the agent plane but keeps these.
    let spec = raw(&w.app, "GET", "/openapi.json", None, &[], None)
        .await
        .json();
    let paths = keys(&spec["paths"]);
    for path in [
        "/projects/{id}/agent-policy",
        "/projects/{id}/agent-policy/kill",
        "/projects/{id}/agent-usage",
        "/documents/{id}/agent-policy",
    ] {
        assert!(paths.contains(path), "{path}");
    }
    assert!(!paths.iter().any(|p| p.contains("/agent/")));
    // Honest descriptions: they name the gateway and the default.
    let get_text = spec["paths"]["/projects/{id}/agent-policy"]["get"]["responses"]["200"]
        ["description"]
        .as_str()
        .unwrap();
    assert!(get_text.contains("disabled until") && get_text.contains("gateway"));
    for (path, method) in [
        ("/projects/{id}/agent-policy", "put"),
        ("/projects/{id}/agent-policy/kill", "post"),
        ("/documents/{id}/agent-policy", "put"),
    ] {
        let op = &spec["paths"][path][method];
        assert_eq!(op["security"], json!([{"paseto":[]}]), "{path}");
        assert!(op["responses"]["401"].is_object(), "{path}");
    }
    w.close().await;
}

// ------------------------------------------------------- review follow-ups

#[sqlx::test(migrations = "../../migrations")]
async fn a_flood_of_refusals_leaves_one_ledger_row_a_minute_per_grant_and_outcome(pool: PgPool) {
    let w = World::bare(pool, true).await;
    let (_, token) = w.owner_reader().await;
    let (_, _, _, second_token) = reader_of(&w, "second@example.test", "editor", 90).await;
    let (_, _, third, third_token) = reader_of(&w, "third@example.test", "editor", 91).await;
    // A refusal from more than a minute ago does not stand in for a new one.
    ledger_row(
        &w,
        third.key_id,
        w.owner,
        w.project,
        "2 minutes",
        0,
        "policy_denied",
        None,
    )
    .await;

    // Disabled: forty probes with invented file ids, as fast as admission lets them in.
    for _ in 0..40 {
        let uri = w.url(&format!("/files/{}/outline", Uuid::new_v4()));
        assert_eq!(get(&w.app, &uri, &token).await.code(), 403);
    }
    assert_eq!(
        outcomes(&w).await,
        vec![("policy_denied".to_owned(), 2, 0)],
        "the old row, and one for forty refusals"
    );
    // Another grant has its own row; the one with only an old row gets a new one.
    for t in [&second_token, &third_token] {
        for _ in 0..3 {
            assert_eq!(get(&w.app, &w.url("/files"), t).await.code(), 403);
        }
    }
    assert_eq!(outcomes(&w).await, vec![("policy_denied".to_owned(), 4, 0)]);
    // The owner still sees that agents tried.
    let seen = usage(&w, &w.owner_login, w.project, "").await.json();
    let denials: i64 = seen["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["denials"].as_i64().unwrap())
        .sum();
    assert_eq!(denials, 4);

    // A budget refusal is throttled the same way, and admitted calls are never throttled.
    set_policy(&w, w.project, json!({"enabled":true,"calls_per_minute":5})).await;
    for _ in 0..5 {
        assert_eq!(get(&w.app, &w.url("/files"), &token).await.code(), 200);
    }
    for _ in 0..25 {
        assert_eq!(get(&w.app, &w.url("/files"), &token).await.code(), 429);
    }
    let kinds = outcomes(&w).await;
    assert_eq!(
        kinds,
        vec![
            ("budget_exceeded".to_owned(), 1, 0),
            ("ok".to_owned(), 5, kinds[1].2),
            ("policy_denied".to_owned(), 4, 0),
        ]
    );
    assert!(kinds[1].2 > 0, "ok rows carry their bytes");
    w.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_minute_budget_waits_for_the_call_that_must_age_out(pool: PgPool) {
    let w = World::new(pool, true).await;
    let (long, long_token) = w.owner_reader().await;
    let (_, _, exact, exact_token) = reader_of(&w, "exact@example.test", "editor", 92).await;
    set_policy(&w, w.project, json!({"enabled":true,"calls_per_minute":3})).await;
    // Five calls in the window (an overrun, or a limit lowered since): ages 50 to 10
    // seconds. Room means at most two left, so the third newest (30 seconds old) must age
    // out: about 30 seconds. The oldest alone (10 seconds) would be too early.
    for ago in [
        "50 seconds",
        "40 seconds",
        "30 seconds",
        "20 seconds",
        "10 seconds",
    ] {
        ledger_row(&w, long.key_id, w.owner, w.project, ago, 0, "ok", None).await;
    }
    let refused = get(&w.app, &w.url("/files"), &long_token).await;
    assert_eq!(refused.code(), 429, "{}", refused.text());
    let wait: u64 = refused.header("retry-after").unwrap().parse().unwrap();
    assert!((28..=31).contains(&wait), "{wait}");
    // Exactly at the limit it is the oldest.
    for ago in ["50 seconds", "30 seconds", "10 seconds"] {
        ledger_row(&w, exact.key_id, w.owner, w.project, ago, 0, "ok", None).await;
    }
    let refused = get(&w.app, &w.url("/files"), &exact_token).await;
    assert_eq!(refused.code(), 429);
    let wait: u64 = refused.header("retry-after").unwrap().parse().unwrap();
    assert!((8..=11).contains(&wait), "{wait}");
    w.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_budget_queries_are_index_ranges_of_the_covering_index(pool: PgPool) {
    use engine_api::agent::policy::{BYTES_TODAY_SQL, CALLS_LAST_MINUTE_SQL};
    let w = World::new(pool, true).await;
    let (agent, _) = w.owner_reader().await;
    sqlx::query("insert into agent_call(at,principal_id,key_id,project_id,tool,request_digest,bytes_out,tok_est,outcome) select now()-(g||' seconds')::interval,$1,$2,$3,'files',decode(repeat('00',32),'hex'),10,3,'ok' from generate_series(1,400) g")
        .bind(w.owner)
        .bind(agent.key_id)
        .bind(w.project)
        .execute(&w.runtime.owner)
        .await
        .unwrap();
    sqlx::raw_sql("vacuum analyze agent_call")
        .execute(&w.runtime.owner)
        .await
        .unwrap();
    // The planner may rightly prefer a scan of a table this small; the question is whether
    // the index can answer, so ask it not to scan the table.
    let mut tx = w.runtime.owner.begin().await.unwrap();
    sqlx::raw_sql("set local enable_seqscan=off; set local enable_bitmapscan=off")
        .execute(&mut *tx)
        .await
        .unwrap();
    for (name, sql) in [("calls", CALLS_LAST_MINUTE_SQL), ("bytes", BYTES_TODAY_SQL)] {
        let sql = sql
            .replace("$1", &format!("'{}'::uuid", agent.key_id))
            .replace("$2", &format!("'{}'::uuid", w.project));
        let plan: Vec<String> = sqlx::query_scalar(&format!("explain {sql}"))
            .fetch_all(&mut *tx)
            .await
            .unwrap();
        let plan = plan.join("\n");
        assert!(!plan.contains("Seq Scan"), "{name}: {plan}");
        // The time bound is an index condition, not a filter over the whole day.
        assert!(
            plan.contains("Index Cond") && plan.contains("(at >"),
            "{name}: {plan}"
        );
        if name == "bytes" {
            // Only the covering index can answer without the table.
            assert!(
                plan.contains("Index Only Scan using agent_call_key_project_at"),
                "{name}: {plan}"
            );
        } else {
            // The planner may use this index or the older (project_id, at) one; either way
            // the scan covers the last minute, not the day.
            assert!(
                plan.contains("Index Scan using agent_call_"),
                "{name}: {plan}"
            );
        }
    }
    tx.rollback().await.unwrap();
    // And they count what they say: 59 calls in the last minute (ages 1 to 59 seconds, 60
    // is past it) and every byte of today.
    let (calls, bytes): (i64, i64) = sqlx::query_as(&format!(
        "select ({CALLS_LAST_MINUTE_SQL}),({BYTES_TODAY_SQL})"
    ))
    .bind(agent.key_id)
    .bind(w.project)
    .fetch_one(&w.runtime.owner)
    .await
    .unwrap();
    assert!((58..=60).contains(&calls), "{calls}");
    assert!(bytes > 0 && bytes <= 4000, "{bytes}");
    w.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn management_has_its_own_admission_bucket_charged_after_authorisation(pool: PgPool) {
    let w = World::new(pool, true).await;
    let (_, token) = w.owner_reader().await;
    // The agent, running as the sole owner, spends the owner's identity bucket.
    let mut limited = false;
    for _ in 0..130 {
        if get(&w.app, &w.url("/files"), &token).await.code() == 429 {
            limited = true;
            break;
        }
    }
    assert!(limited, "the identity bucket is spent by agent reads");
    // The owner can still change the policy and pull the kill switch.
    set_policy(
        &w,
        w.project,
        json!({"enabled":true,"calls_per_minute":100}),
    )
    .await;
    assert_eq!(usage(&w, &w.owner_login, w.project, "").await.code(), 200);
    assert_eq!(kill(&w, &w.owner_login, w.project).await.code(), 200);

    // Authorisation comes first: a member who may not manage spends nothing, however often
    // they ask (and cannot reach the owner's bucket in any case).
    let (viewer, viewer_login) = person(w.pool(), "viewer@example.test").await;
    w.member(w.project, viewer, "viewer").await;
    for _ in 0..130 {
        assert_eq!(usage(&w, &viewer_login, w.project, "").await.code(), 403);
    }
    assert_eq!(usage(&w, &w.owner_login, w.project, "").await.code(), 200);
    // The manager's own bucket does bound the manager (120 a minute here).
    let mut bounded = false;
    for _ in 0..130 {
        if usage(&w, &w.owner_login, w.project, "").await.code() == 429 {
            bounded = true;
            break;
        }
    }
    assert!(bounded, "management is limited, by its own bucket");
    // Another manager has a bucket of their own.
    let (second, second_login) = person(w.pool(), "second@example.test").await;
    w.member(w.project, second, "manager").await;
    assert_eq!(usage(&w, &second_login, w.project, "").await.code(), 200);
    w.close().await;
}

/// A project device session token for `agent` whose ledger row says it was issued at
/// `issued_at`, or that has no ledger row (`None`).
async fn forged_session(
    w: &World,
    agent: &Agent,
    generation: i64,
    issued_at: Option<DateTime<Utc>>,
) -> String {
    let session = Uuid::new_v4();
    if let Some(issued_at) = issued_at {
        sqlx::query("insert into device_session(id,key_id,identity_id,project_id,scopes,issued_at,expires_at) values($1,$2,$3,$4,array['content:read'],$5,$5+interval '30 minutes')")
            .bind(session)
            .bind(agent.key_id)
            .bind(w.owner)
            .bind(w.project)
            .bind(issued_at)
            .execute(&w.runtime.owner)
            .await
            .unwrap();
    }
    token::mint_project_device(
        &KEY,
        &DeviceClaims {
            identity: w.owner,
            key: agent.key_id,
            project: w.project,
            session,
            scopes: vec!["content:read".into()],
            generation,
        },
        3600,
    )
    .unwrap()
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_session_stamped_at_the_cut_off_instant_is_refused(pool: PgPool) {
    let w = World::new(pool, true).await;
    let (agent, _) = w.owner_reader().await;
    assert_eq!(kill(&w, &w.owner_login, w.project).await.code(), 200);
    set_policy(&w, w.project, json!({"enabled":true})).await;
    assert_eq!(
        w.grant(&agent, w.project, &["content:read"]).await.0,
        StatusCode::OK
    );
    let cutoff: DateTime<Utc> = sqlx::query_scalar(
        "select sessions_valid_after from project_agent_policy where project_id=$1",
    )
    .bind(w.project)
    .fetch_one(&w.runtime.owner)
    .await
    .unwrap();
    let generation: i64 = sqlx::query_scalar("select session_generation from identity where id=$1")
        .bind(w.owner)
        .fetch_one(&w.runtime.owner)
        .await
        .unwrap();
    // Only a session whose ledger row is strictly later than the cut-off counts.
    for (issued_at, expected, why) in [
        (Some(cutoff - Duration::microseconds(1)), 403, "before"),
        (Some(cutoff), 403, "the same instant counts as older"),
        (None, 403, "no ledger row, no proof of age"),
        (Some(cutoff + Duration::microseconds(1)), 200, "after"),
    ] {
        let token = forged_session(&w, &agent, generation, issued_at).await;
        let reply = get(&w.app, &w.url("/files"), &token).await;
        assert_eq!(reply.code(), expected, "{why}: {}", reply.text());
    }
    w.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn usage_dates_outside_2000_to_2200_are_a_400_not_a_panic(pool: PgPool) {
    let w = World::new(pool, true).await;
    for query in [
        "?to=-262143-01-01",
        "?from=-262143-01-01&to=-262143-01-02",
        "?to=262142-12-31",
        "?from=1999-12-31&to=2000-01-02",
        "?from=2200-12-31&to=2201-01-01",
        "?from=2201-01-01&to=2201-01-01",
        // from defaults to six days before to, which here falls before 2000
        "?to=2000-01-03",
    ] {
        let reply = usage(&w, &w.owner_login, w.project, query).await;
        assert_eq!(reply.code(), 400, "{query}: {}", reply.text());
    }
    for query in [
        "?from=2000-01-01&to=2000-01-01",
        "?to=2200-12-31",
        "?from=2200-12-31&to=2200-12-31",
        "?to=2000-01-07",
    ] {
        let reply = usage(&w, &w.owner_login, w.project, query).await;
        assert_eq!(reply.code(), 200, "{query}: {}", reply.text());
    }
    w.close().await;
}
