//! Project activity, contributors, file last-change, profile calendars and the
//! administrator project/audit review, through the real router and the restricted
//! runtime login. Requires a reachable Postgres (DATABASE_URL).

use axum::body::{to_bytes, Body};
use axum::http::{header, Request, StatusCode};
use axum::Router;
use engine_api::auth::{store as auth_store, token, AuthConfig};
use engine_api::{app, AppState};
use serde_json::{json, Value};
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

mod common;

const KEY: [u8; 32] = [9u8; 32];

fn state(pool: PgPool, admin: &str) -> AppState {
    let mut state = AppState::new(
        pool,
        AuthConfig {
            paseto_key: KEY,
            service_key: None,
            token_ttl_seconds: 3600,
            public_base_url: "http://test.local".into(),
        },
    );
    state.admin_emails = admin.into();
    state
}
async fn person(pool: &PgPool, email: &str, name: &str) -> (String, Uuid) {
    let id = auth_store::upsert_identity(pool, email, Some(name))
        .await
        .unwrap()
        .id
        .0;
    (
        format!("Bearer {}", token::mint(&KEY, id, 3600).unwrap()),
        id,
    )
}
async fn send(router: &Router, req: Request<Body>) -> (StatusCode, Value) {
    let resp = router.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}
async fn post(router: &Router, uri: &str, token: &str, body: Value) -> (StatusCode, Value) {
    let req = Request::builder()
        .method("POST")
        .uri(uri)
        .header("content-type", "application/json")
        .header(header::AUTHORIZATION, token)
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap();
    send(router, req).await
}
async fn get(router: &Router, uri: &str, token: &str) -> (StatusCode, Value) {
    let mut req = Request::builder().uri(uri);
    if !token.is_empty() {
        req = req.header(header::AUTHORIZATION, token);
    }
    send(router, req.body(Body::empty()).unwrap()).await
}
fn para(text: &str) -> Value {
    json!({"type":"paragraph","content":[{"type":"text","text":text}]})
}
fn edit(block: &str, text: &str) -> Vec<Value> {
    vec![
        json!({"type":"FieldEdited","node_id":block,"field":"content","value":para(text)}),
        json!({"type":"FieldEdited","node_id":block,"field":"label","value":text}),
    ]
}
/// A two-block document (two events, one transaction), owned by `owner`.
async fn document(router: &Router, owner: &str, title: &str) -> (Uuid, String) {
    let (status, body) = post(router, "/documents", owner, json!({"title":title})).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let doc = Uuid::parse_str(body["id"].as_str().unwrap()).unwrap();
    let root = ulid::Ulid::generate().to_string();
    let block = ulid::Ulid::generate().to_string();
    assert_eq!(
        post(
            router,
            &format!("/documents/{doc}/metadata"),
            owner,
            json!({"settings":{"kind":"document"}})
        )
        .await
        .0,
        StatusCode::OK
    );
    let ops = json!([
        {"type":"NodeCreated","node_id":root,"node_type":"form","pos":"000000","fields":{"label":"Report"}},
        {"type":"NodeCreated","node_id":block,"node_type":"item","parent_id":root,"pos":"00000000","fields":{"label":"Original","content":para("Original")}}
    ]);
    let (status, body) = post(
        router,
        &format!("/documents/{doc}/batch"),
        owner,
        json!({"base_seq":0,"ops":ops}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    (doc, block)
}
async fn project(router: &Router, owner: &str, name: &str, members: &[(&str, &str)]) -> String {
    let (status, p) = post(router, "/projects", owner, json!({"name":name})).await;
    assert_eq!(status, StatusCode::OK, "{p}");
    let pid = p["id"].as_str().unwrap().to_owned();
    for (email, role) in members {
        let (status, body) = post(
            router,
            &format!("/spaces/{pid}/members"),
            owner,
            json!({"email":email,"role":role,"send_email":false}),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
    }
    pid
}
async fn move_into(router: &Router, owner: &str, doc: Uuid, pid: &str) {
    let (status, body) = post(
        router,
        &format!("/documents/{doc}/location"),
        owner,
        json!({"space_id":pid}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
}
fn items(value: &Value) -> Vec<Value> {
    value["items"].as_array().unwrap().clone()
}

#[sqlx::test(migrations = "../../migrations")]
async fn activity_contributors_and_last_change_are_member_only_ordered_and_paged(pool: PgPool) {
    let runtime = common::RuntimeDatabase::new(pool.clone()).await;
    let router = app(state(runtime.pool.clone(), ""));
    let (maya, maya_id) = person(&pool, "maya@example.test", "Maya Chen").await;
    let (sam, _) = person(&pool, "sam@example.test", "Sam Okoye").await;
    let (priya, priya_id) = person(&pool, "priya@example.test", "Priya Nair").await;
    let (stranger, _) = person(&pool, "outsider@example.test", "Out Sider").await;
    let pid = project(
        &router,
        &maya,
        "Launch team",
        &[
            ("sam@example.test", "editor"),
            ("priya@example.test", "reviewer"),
        ],
    )
    .await;
    let (doc, block) = document(&router, &maya, "Board update").await;
    move_into(&router, &maya, doc, &pid).await;
    // A file without any events must not gain a last_change.
    let (_, empty) = post(&router, "/documents", &maya, json!({"title":"Empty"})).await;
    move_into(
        &router,
        &maya,
        Uuid::parse_str(empty["id"].as_str().unwrap()).unwrap(),
        &pid,
    )
    .await;

    let (status, files) = get(&router, &format!("/projects/{pid}/files"), &sam).await;
    assert_eq!(status, StatusCode::OK, "{files}");
    assert_eq!(files["more"], false);
    assert_eq!(files["offset"], 0);
    let listed = items(&files);
    assert_eq!(listed.len(), 2);
    let board = listed.iter().find(|f| f["id"] == doc.to_string()).unwrap();
    for key in ["id", "title", "kind", "updated_at", "role", "latest_seq"] {
        assert!(board.get(key).is_some(), "old key {key} kept: {board}");
    }
    assert_eq!(board["latest_seq"], 2);
    assert_eq!(board["last_change"]["seq"], 2);
    assert_eq!(board["last_change"]["actor_name"], "Maya Chen");
    assert_eq!(board["last_change"]["summary"], "Added a block");
    assert!(board["last_change"]["at"].as_str().unwrap().contains('T'));
    let unused = listed.iter().find(|f| f["title"] == "Empty").unwrap();
    assert_eq!(unused["latest_seq"], 0);
    assert!(unused.get("last_change").is_none(), "{unused}");

    // Direct team edit (one transaction, two events), a merged request, a declined
    // request and a named version.
    let (status, body) = post(
        &router,
        &format!("/documents/{doc}/batch"),
        &maya,
        json!({"base_seq":2,"ops":edit(&block,"Team wording")}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (status, request) = post(
        &router,
        &format!("/documents/{doc}/change-requests"),
        &priya,
        json!({"name":"Update forecast","base_seq":4,"ops":edit(&block,"Forecast wording"),"submit":true}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{request}");
    let uri = format!(
        "/documents/{doc}/change-requests/{}",
        request["id"].as_str().unwrap()
    );
    let (_, detail) = get(&router, &uri, &maya).await;
    let (status, merged) = post(
        &router,
        &format!("{uri}/merge"),
        &maya,
        json!({"team_seq":detail["team_seq"],"revision":detail["revision"]}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{merged}");
    assert_eq!(merged["completed"], true);
    let (status, rejected) = post(
        &router,
        &format!("/documents/{doc}/change-requests"),
        &sam,
        json!({"name":"Rejected idea","base_seq":merged["team_seq"],"ops":edit(&block,"Idea"),"submit":true}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{rejected}");
    let (status, _) = post(
        &router,
        &format!(
            "/documents/{doc}/change-requests/{}/decline",
            rejected["id"].as_str().unwrap()
        ),
        &maya,
        json!({"note":"Not now"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, version) = post(
        &router,
        &format!("/documents/{doc}/versions"),
        &maya,
        json!({"name":"Release 1"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{version}");

    // The newest team event is named by the request that produced it.
    let (_, files) = get(&router, &format!("/projects/{pid}/files"), &priya).await;
    let board = items(&files)
        .into_iter()
        .find(|f| f["id"] == doc.to_string())
        .unwrap();
    assert_eq!(board["last_change"]["summary"], "Update forecast");
    assert_eq!(board["last_change"]["actor_name"], "Maya Chen");
    assert_eq!(board["last_change"]["seq"], board["latest_seq"]);

    // Activity: newest first, every derivable kind, readable names, no secrets.
    let endpoint = format!("/projects/{pid}/activity");
    let (status, feed) = get(&router, &endpoint, &sam).await;
    assert_eq!(status, StatusCode::OK, "{feed}");
    assert!(feed["next"].is_null(), "{feed}");
    let all = items(&feed);
    let kinds: std::collections::BTreeSet<&str> =
        all.iter().map(|i| i["kind"].as_str().unwrap()).collect();
    assert_eq!(
        kinds.into_iter().collect::<Vec<_>>(),
        [
            "declined",
            "edit",
            "file_added",
            "member_added",
            "merged",
            "request_opened",
            "version"
        ],
        "{feed}"
    );
    let times: Vec<&str> = all.iter().map(|i| i["at"].as_str().unwrap()).collect();
    assert!(
        times
            .windows(2)
            .all(|w| chrono::DateTime::parse_from_rfc3339(w[0]).unwrap()
                >= chrono::DateTime::parse_from_rfc3339(w[1]).unwrap()),
        "{times:?}"
    );
    let find = |kind: &str, summary: &str| {
        all.iter()
            .find(|i| i["kind"] == kind && i["summary"] == summary)
            .unwrap_or_else(|| panic!("{kind} {summary} in {feed}"))
            .clone()
    };
    let merged_item = find("merged", "Update forecast");
    assert_eq!(merged_item["actor_name"], "Maya Chen");
    assert_eq!(merged_item["actor_id"], maya_id.to_string());
    assert_eq!(merged_item["document_title"], "Board update");
    assert_eq!(merged_item["document_id"], doc.to_string());
    assert!(merged_item["count"].as_i64().unwrap() >= 1);
    assert_eq!(merged_item["seq"], board["latest_seq"]);
    // Events of the merge transaction fold into the merged item.
    assert!(
        !all.iter()
            .any(|i| i["kind"] == "edit" && i["at"] == merged_item["at"]),
        "{feed}"
    );
    assert_eq!(
        find("request_opened", "Update forecast")["actor_name"],
        "Priya Nair"
    );
    assert_eq!(find("declined", "Rejected idea")["actor_name"], "Maya Chen");
    let version_item = all.iter().find(|i| i["kind"] == "version").unwrap();
    assert_eq!(version_item["summary"], "Release 1");
    assert_eq!(version_item["seq"], board["latest_seq"]);
    let file_added = all
        .iter()
        .find(|i| i["kind"] == "file_added" && i["document_title"] == "Board update")
        .unwrap();
    assert_eq!(file_added["summary"], "Added a document");
    assert_eq!(
        find("file_added", "Added a file")["document_title"],
        "Empty"
    );
    find("member_added", "Gave Sam Okoye editor access");
    let contributor_grant = find("member_added", "Gave Priya Nair contributor access");
    assert!(contributor_grant["document_id"].is_null());
    assert!(contributor_grant["document_title"].is_null());
    // The initial two-event write is one edit item.
    let created = all
        .iter()
        .find(|i| i["kind"] == "edit" && i["seq"] == 2)
        .unwrap();
    assert_eq!(created["summary"], "Made 2 changes");
    assert_eq!(created["count"], 2);
    // Private drafts are never part of project activity.
    let body = feed.to_string();
    assert!(!body.contains("outsider@example.test"));

    // Keyset pages carry the same history without repeating a row.
    let mut paged = Vec::new();
    let mut cursor = String::new();
    let mut pages = 0;
    loop {
        let uri = if cursor.is_empty() {
            format!("{endpoint}?limit=2")
        } else {
            format!("{endpoint}?limit=2&before={cursor}")
        };
        let (status, page) = get(&router, &uri, &priya).await;
        assert_eq!(status, StatusCode::OK, "{page}");
        let batch = items(&page);
        assert!(batch.len() <= 2);
        paged.extend(batch);
        pages += 1;
        assert!(pages < 20, "cursor must terminate");
        match page["next"].as_str() {
            Some(next) => cursor = next.to_owned(),
            None => break,
        }
    }
    assert!(pages > 1);
    let others = |list: &[Value]| -> Vec<(String, String, String)> {
        list.iter()
            .filter(|i| i["kind"] != "edit")
            .map(|i| {
                (
                    i["at"].to_string(),
                    i["kind"].to_string(),
                    i["summary"].to_string(),
                )
            })
            .collect()
    };
    assert_eq!(others(&paged), others(&all));
    let edits = |list: &[Value]| -> i64 {
        list.iter()
            .filter(|i| i["kind"] == "edit")
            .map(|i| i["count"].as_i64().unwrap())
            .sum()
    };
    assert_eq!(edits(&paged), edits(&all));
    // Older history is found by the widening look-back windows (40 and 400 days ago),
    // and sorts after everything newer.
    for (n, age) in [40, 400, 401].into_iter().enumerate() {
        sqlx::query("insert into event(id,document_id,seq,type,actor_id,payload,content_hash,prev_chain_hash,chain_hash,created_at) select $1,$2,coalesce(max(seq),0)+1,'FieldEdited',$3,'{}'::jsonb,decode(repeat('01',32),'hex'),decode(repeat('02',32),'hex'),decode(repeat($4,32),'hex'),now()-make_interval(days=>$5) from event where document_id=$2")
            .bind(ulid::Ulid::generate().to_string())
            .bind(doc)
            .bind(maya_id)
            .bind(format!("{:02x}", 32 + n))
            .bind(age)
            .execute(&pool)
            .await
            .unwrap();
    }
    let (status, deep) = get(&router, &format!("{endpoint}?limit=100"), &sam).await;
    assert_eq!(status, StatusCode::OK, "{deep}");
    let deep_items = items(&deep);
    assert_eq!(deep_items.len(), all.len() + 3, "{deep}");
    assert_eq!(edits(&deep_items), edits(&all) + 3);
    assert_eq!(&deep_items[..all.len()], &all[..]);
    assert!(deep_items[all.len()..].iter().all(|i| i["kind"] == "edit"));
    let mut cursor = String::new();
    let mut walked = Vec::new();
    for _ in 0..40 {
        let (_, page) = get(
            &router,
            &format!("{endpoint}?limit=2&before={cursor}"),
            &priya,
        )
        .await;
        walked.extend(items(&page));
        match page["next"].as_str() {
            Some(next) => cursor = next.to_owned(),
            None => break,
        }
    }
    assert_eq!(others(&walked), others(&deep_items));
    assert_eq!(edits(&walked), edits(&deep_items));
    // limit is bounded, not rejected.
    let (status, big) = get(&router, &format!("{endpoint}?limit=100000"), &sam).await;
    assert_eq!(status, StatusCode::OK);
    assert!(items(&big).len() <= 100);
    for bad in [
        "before=not-a-cursor",
        // Valid base64 JSON, wrong shape.
        "before=eyJsYXN0X3NlcSI6MSwibGFzdF9pZCI6IngifQ",
    ] {
        assert_eq!(
            get(&router, &format!("{endpoint}?{bad}"), &sam).await.0,
            StatusCode::BAD_REQUEST,
            "{bad}"
        );
    }

    // Contributors: current members with merged requests and authored events.
    let (status, people) = get(&router, &format!("/projects/{pid}/contributors"), &sam).await;
    assert_eq!(status, StatusCode::OK, "{people}");
    assert_eq!(people["capped"], false, "{people}");
    let list = people["contributors"].as_array().unwrap();
    assert_eq!(list.len(), 3, "{people}");
    assert_eq!(list[0]["identity_id"], priya_id.to_string());
    assert_eq!(list[0]["name"], "Priya Nair");
    assert_eq!(list[0]["merges"], 1);
    assert_eq!(list[0]["edits"], 0);
    assert_eq!(list[1]["identity_id"], maya_id.to_string());
    let authored: i64 = sqlx::query_scalar("select count(*) from event where actor_id=$1")
        .bind(maya_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(list[1]["edits"], authored);
    assert_eq!(list[1]["merges"], 0);
    assert!(list[1]["last_at"].is_string());
    assert_eq!(list[2]["name"], "Sam Okoye");
    assert_eq!(list[2]["edits"], 0);
    assert!(list[2]["last_at"].is_null());
    // Removing a member removes them from the totals.
    assert_eq!(
        post(
            &router,
            &format!("/spaces/{pid}/members"),
            &maya,
            json!({"email":"sam@example.test","role":"remove"})
        )
        .await
        .0,
        StatusCode::OK
    );
    let (_, people) = get(&router, &format!("/projects/{pid}/contributors"), &maya).await;
    assert_eq!(people["contributors"].as_array().unwrap().len(), 2);
    // The removed member and a stranger are refused; so is a missing token.
    for uri in [
        format!("/projects/{pid}/activity"),
        format!("/projects/{pid}/contributors"),
    ] {
        for who in [&stranger, &sam] {
            assert_eq!(get(&router, &uri, who).await.0, StatusCode::FORBIDDEN);
        }
        assert_eq!(get(&router, &uri, "").await.0, StatusCode::UNAUTHORIZED);
    }
    let unknown = Uuid::new_v4();
    assert_eq!(
        get(&router, &format!("/projects/{unknown}/activity"), &maya)
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    // A document id is not a project.
    assert_eq!(
        get(&router, &format!("/projects/{doc}/contributors"), &maya)
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    runtime.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn profile_calendar_counts_only_files_both_people_can_access(pool: PgPool) {
    let runtime = common::RuntimeDatabase::new(pool.clone()).await;
    let router = app(state(runtime.pool.clone(), ""));
    let (maya, maya_id) = person(&pool, "calendar-maya@example.test", "Maya Chen").await;
    let (sam, _) = person(&pool, "calendar-sam@example.test", "Sam Okoye").await;
    let (stranger, _) = person(&pool, "calendar-outsider@example.test", "Out Sider").await;
    let shared = project(
        &router,
        &maya,
        "Shared",
        &[("calendar-sam@example.test", "viewer")],
    )
    .await;
    let private = project(&router, &maya, "Only Maya", &[]).await;
    let (shared_doc, _) = document(&router, &maya, "Shared file").await;
    move_into(&router, &maya, shared_doc, &shared).await;
    let (private_doc, _) = document(&router, &maya, "Private file").await;
    move_into(&router, &maya, private_doc, &private).await;
    let (loose, _) = document(&router, &maya, "Loose file").await;
    // Direct sharing of a standalone file is not a shared project.
    assert_eq!(
        post(
            &router,
            &format!("/documents/{loose}/members"),
            &maya,
            json!({"email":"calendar-sam@example.test","role":"viewer","send_email":false})
        )
        .await
        .0,
        StatusCode::OK
    );
    // Three historic events in the shared file, dated in 2025.
    for (n, day) in [
        "2025-03-01T10:00:00Z",
        "2025-03-01T22:30:00Z",
        "2025-03-02T00:00:00Z",
    ]
    .iter()
    .enumerate()
    {
        sqlx::query("insert into event(id,document_id,seq,type,actor_id,payload,content_hash,prev_chain_hash,chain_hash,created_at) values($1,$2,$3,'FieldEdited',$4,'{}'::jsonb,decode(repeat('01',32),'hex'),decode(repeat('02',32),'hex'),decode(repeat($5,32),'hex'),$6::timestamptz)")
            .bind(ulid::Ulid::generate().to_string())
            .bind(shared_doc)
            .bind(100 + n as i64)
            .bind(maya_id)
            .bind(format!("{:02x}", 16 + n))
            .bind(day)
            .execute(&pool)
            .await
            .unwrap();
    }
    let own = format!("/profiles/{maya_id}/activity");
    // Everything Maya wrote today: 2 events in each of the three files.
    let (status, mine) = get(&router, &own, &maya).await;
    assert_eq!(status, StatusCode::OK, "{mine}");
    assert_eq!(mine["year"], chrono::Datelike::year(&chrono::Utc::now()));
    assert_eq!(mine["total"], 6, "{mine}");
    let day = mine["days"].as_array().unwrap();
    assert!(
        day.iter()
            .map(|d| d["count"].as_i64().unwrap())
            .sum::<i64>()
            == 6
    );
    assert_eq!(day[0]["date"].as_str().unwrap().len(), 10);
    // A teammate sees only the project they share.
    let (status, theirs) = get(&router, &own, &sam).await;
    assert_eq!(status, StatusCode::OK, "{theirs}");
    assert_eq!(theirs["total"], 2, "{theirs}");
    // Titles and project names never appear.
    let text = theirs.to_string();
    for secret in ["Private file", "Loose file", "Only Maya", "Shared file"] {
        assert!(!text.contains(secret));
    }
    // Year filter, UTC days, same visibility rule.
    let (_, old_self) = get(&router, &format!("{own}?year=2025"), &maya).await;
    assert_eq!(old_self["total"], 3);
    assert_eq!(
        old_self["days"],
        json!([{"date":"2025-03-01","count":2},{"date":"2025-03-02","count":1}])
    );
    let (_, old_peer) = get(&router, &format!("{own}?year=2025"), &sam).await;
    assert_eq!(old_peer, old_self);
    let (_, none) = get(&router, &format!("{own}?year=2024"), &sam).await;
    assert_eq!(none, json!({"year":2024,"total":0,"days":[]}));
    for year in ["1999", "2101", "abc"] {
        assert_eq!(
            get(&router, &format!("{own}?year={year}"), &maya).await.0,
            StatusCode::BAD_REQUEST,
            "{year}"
        );
    }
    // Same access rule as the profile itself.
    assert_eq!(get(&router, &own, &stranger).await.0, StatusCode::NOT_FOUND);
    assert_eq!(get(&router, &own, "").await.0, StatusCode::UNAUTHORIZED);
    assert_eq!(
        get(
            &router,
            &format!("/profiles/{}/activity", Uuid::new_v4()),
            &maya
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    // Revoking the shared membership closes the calendar.
    assert_eq!(
        post(
            &router,
            &format!("/spaces/{shared}/members"),
            &maya,
            json!({"email":"calendar-sam@example.test","role":"remove"})
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(get(&router, &own, &sam).await.0, StatusCode::NOT_FOUND);
    runtime.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn admin_project_rows_and_audit_detail_are_gated_filtered_and_bounded(pool: PgPool) {
    let runtime = common::RuntimeDatabase::new(pool.clone()).await;
    let router = app(state(runtime.pool.clone(), "operator@example.test"));
    let (operator, operator_id) = person(&pool, "operator@example.test", "Operator").await;
    let (maya, maya_id) = person(&pool, "admin-maya@example.test", "Maya Chen").await;
    let pid = project(
        &router,
        &maya,
        "Quarterly review",
        &[("operator@example.test", "viewer")],
    )
    .await;
    let other = project(&router, &maya, "Archive", &[]).await;
    let (doc, _) = document(&router, &maya, "Budget").await;
    move_into(&router, &maya, doc, &pid).await;
    let (status, _) = post(
        &router,
        &format!("/documents/{doc}/uploads"),
        &maya,
        json!({"filename":"budget.txt","content_type":"text/plain","content":"aGVsbG8gd29ybGQ=","report":{}}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    assert_eq!(
        get(&router, "/admin/records?kind=projects", &maya).await.0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        get(&router, "/admin/records?kind=projects", "").await.0,
        StatusCode::UNAUTHORIZED
    );
    let (status, rows) = get(&router, "/admin/records?kind=projects", &operator).await;
    assert_eq!(status, StatusCode::OK, "{rows}");
    let list = items(&rows);
    assert_eq!(list.len(), 2);
    let row = list.iter().find(|r| r["id"] == pid).unwrap();
    assert_eq!(row["name"], "Quarterly review");
    assert_eq!(row["members"], 2);
    assert_eq!(row["files"], 1);
    assert_eq!(row["bytes"], 11);
    assert!(row["created_at"].is_string());
    let instant = |v: &Value| chrono::DateTime::parse_from_rfc3339(v.as_str().unwrap()).unwrap();
    assert!(instant(&row["last_activity_at"]) >= instant(&row["created_at"]));
    let quiet = list.iter().find(|r| r["id"] == other).unwrap();
    assert_eq!(quiet["members"], 1);
    assert_eq!(quiet["files"], 0);
    assert_eq!(quiet["bytes"], 0);
    // Search and paging use the same parameters as the other record kinds.
    let (_, found) = get(
        &router,
        "/admin/records?kind=projects&q=QUARTERLY",
        &operator,
    )
    .await;
    assert_eq!(items(&found).len(), 1);
    let (_, later) = get(&router, "/admin/records?kind=projects&offset=50", &operator).await;
    assert!(items(&later).is_empty());
    assert_eq!(
        get(&router, "/admin/records?kind=nonsense", &operator)
            .await
            .0,
        StatusCode::BAD_REQUEST
    );
    // Existing kinds keep their row shape.
    let (_, users) = get(&router, "/admin/records?kind=users", &operator).await;
    assert!(items(&users)[0].get("detail").is_none());

    // Audit rows carry redacted, bounded detail and accept actor / time filters.
    sqlx::query("insert into operation_audit(actor_id,action,resource_id,detail) values($1,'test.secret_detail',$2,$3),($1,'test.large_detail',$2,$4)")
        .bind(maya_id)
        .bind(pid.parse::<Uuid>().unwrap())
        .bind(json!({"role":"editor","session_token":"abc","nested":{"Password":"p","kept":1},"list":[{"api_key":"k"}]}))
        .bind(json!({"blob":"x".repeat(6000),"token":"abc"}))
        .execute(&pool)
        .await
        .unwrap();
    let (status, audit) = get(&router, "/admin/records?kind=audit&q=test.", &operator).await;
    assert_eq!(status, StatusCode::OK, "{audit}");
    let secret = items(&audit)
        .into_iter()
        .find(|r| r["label"] == "test.secret_detail")
        .unwrap();
    assert_eq!(secret["actor_id"], maya_id.to_string());
    assert_eq!(
        secret["detail"],
        json!({"role":"editor","session_token":"[redacted]","nested":{"Password":"[redacted]","kept":1},"list":[{"api_key":"[redacted]"}]})
    );
    let large = items(&audit)
        .into_iter()
        .find(|r| r["label"] == "test.large_detail")
        .unwrap();
    assert_eq!(large["detail"]["truncated"], true);
    assert!(large["detail"].to_string().len() <= 4096);
    assert!(!audit.to_string().contains("\"abc\""));
    let (_, by_email) = get(
        &router,
        "/admin/records?kind=audit&actor=ADMIN-MAYA@",
        &operator,
    )
    .await;
    assert!(!items(&by_email).is_empty());
    assert!(items(&by_email)
        .iter()
        .all(|r| r["actor"] == "admin-maya@example.test"));
    let (_, by_id) = get(
        &router,
        &format!("/admin/records?kind=audit&actor={operator_id}"),
        &operator,
    )
    .await;
    assert!(items(&by_id)
        .iter()
        .all(|r| r["actor_id"] == operator_id.to_string()));
    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let tomorrow = (chrono::Utc::now() + chrono::Duration::days(1))
        .format("%Y-%m-%d")
        .to_string();
    let (_, today_rows) = get(
        &router,
        &format!("/admin/records?kind=audit&since={today}&until={today}"),
        &operator,
    )
    .await;
    assert!(
        !items(&today_rows).is_empty(),
        "date-only until includes its day"
    );
    let (_, future) = get(
        &router,
        &format!("/admin/records?kind=audit&since={tomorrow}"),
        &operator,
    )
    .await;
    assert!(items(&future).is_empty());
    let (_, past) = get(
        &router,
        "/admin/records?kind=audit&until=2020-01-01T00:00:00Z",
        &operator,
    )
    .await;
    assert!(items(&past).is_empty());
    assert_eq!(
        get(
            &router,
            "/admin/records?kind=audit&since=yesterday",
            &operator
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        get(&router, "/admin/records?kind=audit", &maya).await.0,
        StatusCode::FORBIDDEN
    );
    runtime.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn project_search_scopes_folders_and_rechecks_membership(pool: PgPool) {
    let runtime = common::RuntimeDatabase::new(pool.clone()).await;
    let router = app(state(runtime.pool.clone(), ""));
    let (owner, _) = person(&pool, "search-owner@example.test", "Owner").await;
    let (member, member_id) = person(&pool, "search-member@example.test", "Member").await;
    let (stranger, _) = person(&pool, "search-stranger@example.test", "Stranger").await;
    let pid = project(
        &router,
        &owner,
        "Selected",
        &[("search-member@example.test", "editor")],
    )
    .await;
    let other = project(&router, &owner, "Other", &[]).await;
    let (status, folder) = post(
        &router,
        "/spaces",
        &owner,
        json!({"name":"Nested", "kind":"folder", "parent_id":pid}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{folder}");
    let folder = folder["id"].as_str().unwrap();
    let (doc, block) = document(&router, &owner, "Selected report").await;
    move_into(&router, &owner, doc, folder).await;
    let (elsewhere, _) = document(&router, &owner, "Other report").await;
    move_into(&router, &owner, elsewhere, &other).await;
    let (_personal, _) = document(&router, &owner, "Personal report").await;
    let uri = format!("/search?q=Original&project_id={pid}");
    for actor in [&owner, &member] {
        let (status, found) = get(&router, &uri, actor).await;
        assert_eq!(status, StatusCode::OK, "{found}");
        assert_eq!(items(&found).len(), 1, "{found}");
        assert_eq!(found["items"][0]["document_id"], doc.to_string());
        assert_eq!(found["more"], false);
    }
    assert_eq!(
        items(&get(&router, "/search?q=Original", &owner).await.1).len(),
        3
    );
    let (status, draft) = post(
        &router,
        &format!("/documents/{doc}/drafts"),
        &owner,
        json!({"name":"Private"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{draft}");
    let draft = draft["id"].as_str().unwrap();
    let (status, body) = post(&router, &format!("/documents/{doc}/drafts/{draft}"), &owner, json!({"revision":0, "ops":[{"type":"FieldEdited","node_id":block,"field":"label","value":"SecretHidden"}]})).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    for actor in [&owner, &member] {
        assert!(items(
            &get(
                &router,
                &format!("/search?q=SecretHidden&project_id={pid}"),
                actor
            )
            .await
            .1
        )
        .is_empty());
    }
    for actor in [&stranger, &member] {
        let forbidden = if actor == &member { &other } else { &pid };
        for term in ["Original", "x"] {
            assert_eq!(
                get(
                    &router,
                    &format!("/search?q={term}&project_id={forbidden}"),
                    actor
                )
                .await
                .0,
                StatusCode::FORBIDDEN
            );
        }
    }
    assert_eq!(get(&router, &uri, "").await.0, StatusCode::UNAUTHORIZED);
    assert_eq!(
        get(
            &router,
            &format!("/search?q=Original&project_id={}", Uuid::new_v4()),
            &owner
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        get(&router, "/search?q=Original&project_id=invalid", &owner)
            .await
            .0,
        StatusCode::BAD_REQUEST
    );
    sqlx::query("delete from space_member where space_id=$1 and identity_id=$2")
        .bind(Uuid::parse_str(&pid).unwrap())
        .bind(member_id)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(get(&router, &uri, &member).await.0, StatusCode::FORBIDDEN);
    runtime.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn project_search_pages_tied_positions_and_treats_wildcards_literally(pool: PgPool) {
    let runtime = common::RuntimeDatabase::new(pool.clone()).await;
    let router = app(state(runtime.pool.clone(), ""));
    let (owner, _) = person(&pool, "search-pages@example.test", "Owner").await;
    let pid = project(&router, &owner, "Search pages", &[]).await;
    let (doc, block) = document(&router, &owner, "Corpus").await;
    move_into(&router, &owner, doc, &pid).await;
    let mut ids = Vec::new();
    let ops: Vec<_> = (0..53).flat_map(|_| {
        let id = ulid::Ulid::generate().to_string(); ids.push(id.clone());
        [json!({"type":"ChoiceAdded", "node_id":block, "choice_id":id, "fields":{"label":"Literal %_ fixture"}}), json!({"type":"NodeMoved", "node_id":id, "new_parent_id":block, "new_pos":"a0"})]
    }).collect();
    let (status, body) = post(
        &router,
        &format!("/documents/{doc}/batch"),
        &owner,
        json!({"ops":ops}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    ids.sort();
    let uri = format!("/search?q=%25_&project_id={pid}");
    let (status, first) = get(&router, &uri, &owner).await;
    assert_eq!(status, StatusCode::OK, "{first}");
    assert_eq!(first["more"], true);
    assert_eq!(items(&first).len(), 50);
    let second = get(&router, &format!("{uri}&offset=50"), &owner).await.1;
    assert_eq!(second["more"], false);
    assert_eq!(second["offset"], 50);
    let found: Vec<_> = items(&first)
        .into_iter()
        .chain(items(&second))
        .map(|x| x["node_id"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(found, ids);
    assert_eq!(get(&router, &uri, &owner).await.1, first);
    assert!(items(&get(&router, &format!("{uri}&offset=100"), &owner).await.1).is_empty());
    for offset in [-1, 1_000_001] {
        assert_eq!(
            get(&router, &format!("{uri}&offset={offset}"), &owner)
                .await
                .0,
            StatusCode::BAD_REQUEST
        );
    }
    // A title-only match still returns a document that has no node rows.
    let (status, empty) = post(
        &router,
        "/documents",
        &owner,
        json!({"title":"Literal %_ empty"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let empty = Uuid::parse_str(empty["id"].as_str().unwrap()).unwrap();
    move_into(&router, &owner, empty, &pid).await;
    let found = get(&router, &format!("{uri}&offset=0"), &owner).await.1;
    assert!(items(&found)
        .iter()
        .any(|x| x["document_id"] == empty.to_string() && x["node_id"].is_null()));
    let (status, body) = post(
        &router,
        &format!("/documents/{empty}/metadata"),
        &owner,
        json!({"archived":true}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(!items(&get(&router, &uri, &owner).await.1)
        .iter()
        .any(|x| x["document_id"] == empty.to_string()));
    runtime.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn project_request_notifications_require_current_watch_and_membership(pool: PgPool) {
    let runtime = common::RuntimeDatabase::new(pool.clone()).await;
    let router = app(state(runtime.pool.clone(), ""));
    let (owner, owner_id) = person(&pool, "watch-owner@example.test", "Owner").await;
    let (watcher, watcher_id) = person(&pool, "watch-reader@example.test", "Reader").await;
    let (_, editor_id) = person(&pool, "watch-editor@example.test", "Editor").await;
    let (_, ignored_id) = person(&pool, "watch-ignored@example.test", "Ignored").await;
    let (revoked, revoked_id) = person(&pool, "watch-revoked@example.test", "Revoked").await;
    let (disabled, disabled_id) = person(&pool, "watch-disabled@example.test", "Disabled").await;
    let pid = project(
        &router,
        &owner,
        "Watch policy",
        &[
            ("watch-reader@example.test", "viewer"),
            ("watch-editor@example.test", "editor"),
            ("watch-ignored@example.test", "viewer"),
            ("watch-revoked@example.test", "viewer"),
            ("watch-disabled@example.test", "viewer"),
        ],
    )
    .await;
    let (doc, _) = document(&router, &owner, "Notification target").await;
    move_into(&router, &owner, doc, &pid).await;
    for actor in [&watcher, &revoked, &disabled] {
        let (status, body) = post(
            &router,
            &format!("/projects/{pid}/watch"),
            actor,
            json!({"level":"requests"}),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
    }
    let project_id = Uuid::parse_str(&pid).unwrap();
    sqlx::query("delete from space_member where space_id=$1 and identity_id=$2")
        .bind(project_id)
        .bind(revoked_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("update identity set disabled_at=now() where id=$1")
        .bind(disabled_id)
        .execute(&pool)
        .await
        .unwrap();
    let subject = Uuid::new_v4();
    {
        let mut db = runtime.pool.acquire().await.unwrap();
        engine_api::notifications::notify_reviewers(
            &mut db,
            doc,
            owner_id,
            "change_request_submitted",
            Some(subject),
            json!({}),
        )
        .await
        .unwrap();
    }
    let recipients: Vec<Uuid> = sqlx::query_scalar(
        "select recipient_id from notification where subject_id=$1 order by recipient_id",
    )
    .bind(subject)
    .fetch_all(&runtime.pool)
    .await
    .unwrap();
    let mut expected = vec![editor_id, watcher_id];
    expected.sort();
    assert_eq!(recipients, expected);
    assert!(!recipients.contains(&owner_id));
    assert!(!recipients.contains(&ignored_id));
    assert!(!recipients.contains(&revoked_id));
    assert!(!recipients.contains(&disabled_id));
    let inbox = get(&router, "/notifications", &watcher).await.1;
    assert!(items(&inbox)
        .iter()
        .any(|n| n["kind"] == "change_request_submitted"));
    let (status, body) = post(
        &router,
        &format!("/projects/{pid}/watch"),
        &watcher,
        json!({"level":"ignore"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let second = Uuid::new_v4();
    {
        let mut db = runtime.pool.acquire().await.unwrap();
        engine_api::notifications::notify_reviewers(
            &mut db,
            doc,
            owner_id,
            "change_request_submitted",
            Some(second),
            json!({}),
        )
        .await
        .unwrap();
    }
    let recipients: Vec<Uuid> =
        sqlx::query_scalar("select recipient_id from notification where subject_id=$1")
            .bind(second)
            .fetch_all(&runtime.pool)
            .await
            .unwrap();
    assert_eq!(recipients, vec![editor_id]);
    runtime.close().await;
}
