//! Copy detection, the role ladder and review rules, unrelated pushes and
//! administrator deletion, driven through the real router. Requires a reachable
//! Postgres (DATABASE_URL), like the other API integration tests.

use axum::body::{to_bytes, Body};
use axum::http::{header, Request, StatusCode};
use axum::Router;
use engine_api::auth::{store as auth_store, token, AuthConfig};
use engine_api::{app, copies, AppState};
use serde_json::{json, Value};
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

const KEY: [u8; 32] = [7u8; 32];
const ADMIN: &str = "admin-copies@example.test";

fn test_state(pool: PgPool) -> AppState {
    let mut state = AppState::new(
        pool,
        AuthConfig {
            paseto_key: KEY,
            service_key: None,
            token_ttl_seconds: 3600,
            public_base_url: "http://test.local".into(),
        },
    );
    state.admin_emails = ADMIN.into();
    state
}
async fn identity(pool: &PgPool, email: &str) -> (String, Uuid) {
    let id = auth_store::upsert_identity(pool, email, Some(email.split('@').next().unwrap()))
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
    let req = Request::builder()
        .uri(uri)
        .header(header::AUTHORIZATION, token)
        .body(Body::empty())
        .unwrap();
    send(router, req).await
}
fn para(text: &str) -> Value {
    json!({"type":"paragraph","content":[{"type":"text","text":text}]})
}

const REPORT: [&str; 6] = [
    "The survey reached 1,240 households across four districts between March and May.",
    "Response rates were highest in Sylhet, where enumerators returned for a second visit.",
    "Household size averaged 4.6 people, and 38 percent of households kept livestock.",
    "Access to clean water improved compared with the previous round of data collection.",
    "We recommend a follow-up study focusing on seasonal migration and school attendance.",
    "Appendix tables list every indicator with its confidence interval and sample size.",
];
const BUDGET: [&str; 4] = [
    "Quarterly budget for the regional office, including travel and equipment.",
    "Staff costs rose by nine percent because two field officers were hired.",
    "Printing and translation costs were lower than planned this quarter.",
    "The board approved the revised allocation for the next financial year.",
];

/// Create a document with paragraphs; optionally record an uploaded original file.
async fn document(
    router: &Router,
    token: &str,
    title: &str,
    paragraphs: &[&str],
    upload: Option<&str>,
) -> (Uuid, Vec<String>) {
    let (status, created) = post(router, "/documents", token, json!({"title":title})).await;
    assert_eq!(status, StatusCode::OK, "{created}");
    let doc = Uuid::parse_str(created["id"].as_str().unwrap()).unwrap();
    post(
        router,
        &format!("/documents/{doc}/metadata"),
        token,
        json!({"settings":{"kind":"document"}}),
    )
    .await;
    if let Some(filename) = upload {
        let (status, body) = post(router, &format!("/documents/{doc}/uploads"), token, json!({"filename":filename,"content_type":"application/vnd.openxmlformats-officedocument.wordprocessingml.document","content":"UEsDBA==","report":{}})).await;
        assert_eq!(status, StatusCode::OK, "{body}");
    }
    let root = ulid::Ulid::new().to_string();
    let ids: Vec<String> = paragraphs
        .iter()
        .map(|_| ulid::Ulid::new().to_string())
        .collect();
    let mut ops = vec![
        json!({"type":"NodeCreated","node_id":root,"node_type":"form","pos":"000000","fields":{"label":title}}),
    ];
    for (i, (id, text)) in ids.iter().zip(paragraphs).enumerate() {
        ops.push(json!({"type":"NodeCreated","node_id":id,"node_type":"item","parent_id":root,"pos":format!("0000{i:04}"),"fields":{"label":text,"content":para(text)}}));
    }
    let (status, body) = post(
        router,
        &format!("/documents/{doc}/batch"),
        token,
        json!({"base_seq":0,"ops":ops}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    (doc, ids)
}
async fn share(router: &Router, owner: &str, doc: Uuid, email: &str, role: &str) {
    let (status, body) = post(
        router,
        &format!("/documents/{doc}/members"),
        owner,
        json!({"email":email,"role":role}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
}
async fn inbox(router: &Router, token: &str) -> Vec<Value> {
    let (status, body) = get(router, "/notifications", token).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    body["items"].as_array().unwrap().clone()
}

#[sqlx::test(migrations = "../../migrations")]
async fn renamed_copies_notify_the_owner_without_revealing_the_original(pool: PgPool) {
    let router = app(test_state(pool.clone()));
    let (owner, _) = identity(&pool, "owner-copy@example.test").await;
    let (tariq, tariq_id) = identity(&pool, "tariq-copy@example.test").await;
    let (colleague, _) = identity(&pool, "colleague-copy@example.test").await;
    let (original, _) = document(&router, &owner, "Group report", &REPORT, None).await;
    copies::run_once(&pool).await.unwrap();

    // Tariq never had access; he uploads an edited copy under another name.
    let mut edited: Vec<&str> = REPORT.to_vec();
    edited[1] = "Response rates were highest in Sylhet and Khulna after a second visit.";
    edited.push("New section: limitations of the sampling frame used in the study.");
    let blocks: Vec<String> = edited.iter().map(|s| s.to_string()).collect();
    // Before uploading, the check reveals nothing he cannot open.
    let (status, check) = post(
        &router,
        "/similarity/check",
        &tariq,
        json!({"title":"report - Tariq.docx","blocks":blocks}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{check}");
    assert!(check["matches"].as_array().unwrap().is_empty(), "{check}");
    let (copy, _) = document(
        &router,
        &tariq,
        "report - Tariq",
        &edited,
        Some("report - Tariq.docx"),
    )
    .await;
    // The owner's own check finds the original.
    let (_, own_check) = post(
        &router,
        "/similarity/check",
        &owner,
        json!({"title":"anything.docx","blocks":blocks}),
    )
    .await;
    assert_eq!(own_check["matches"][0]["document_id"], original.to_string());
    assert!(
        own_check["matches"][0]["comparison"]["score"]
            .as_f64()
            .unwrap()
            > 0.6
    );

    copies::run_once(&pool).await.unwrap();
    let notes = inbox(&router, &owner).await;
    let note = notes
        .iter()
        .find(|n| n["kind"] == "copy_detected")
        .unwrap_or_else(|| panic!("{notes:?}"));
    assert_eq!(note["document"]["id"], original.to_string());
    assert_eq!(note["source"]["id"], copy.to_string());
    assert_eq!(note["source"]["filename"], "report - Tariq.docx");
    assert!(note["source"]["role"].is_null(), "the owner can't open it");
    assert_eq!(note["actor"]["email"], "tariq-copy@example.test");
    assert!(note["payload"]["score"].as_f64().unwrap() > 0.6);
    assert!(inbox(&router, &tariq).await.is_empty());
    // Indexing again does not repeat the notification.
    sqlx::query(
        "update document_fingerprint set updated_at=now()-interval '1 hour', through_seq=0",
    )
    .execute(&pool)
    .await
    .unwrap();
    copies::run_once(&pool).await.unwrap();
    assert_eq!(
        inbox(&router, &owner)
            .await
            .iter()
            .filter(|n| n["kind"] == "copy_detected")
            .count(),
        1
    );
    let (_, count) = get(&router, "/notifications/count", &owner).await;
    assert_eq!(count["unread"], 1);

    // Only the owner or a manager answers; inviting grants contributor access.
    let match_id = note["match"]["id"].as_str().unwrap();
    let (status, _) = post(
        &router,
        &format!("/copies/{match_id}"),
        &tariq,
        json!({"action":"invite"}),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, body) = post(
        &router,
        &format!("/copies/{match_id}"),
        &owner,
        json!({"action":"invite"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let invite = inbox(&router, &tariq).await;
    assert_eq!(invite[0]["kind"], "copy_invitation", "{invite:?}");
    assert_eq!(invite[0]["document"]["role"], "contributor");
    let (_, check) = post(
        &router,
        "/similarity/check",
        &tariq,
        json!({"title":"report - Tariq.docx","blocks":blocks}),
    )
    .await;
    // His own upload is the closest match; the original is now visible too.
    let found = check["matches"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["document_id"] == original.to_string())
        .unwrap_or_else(|| panic!("{check}"));
    assert_eq!(found["can_push"], true);
    assert_eq!(found["owner"]["is_you"], false);
    let (_, listed) = get(&router, &format!("/documents/{original}/copies"), &owner).await;
    assert_eq!(listed["items"][0]["status"], "invited", "{listed}");
    assert_eq!(listed["items"][0]["direction"], "copy_of_this");
    let (status, _) = post(&router, "/notifications/read", &owner, json!({"all":true})).await;
    assert_eq!(status, StatusCode::OK);
    let (_, count) = get(&router, "/notifications/count", &owner).await;
    assert_eq!(count["unread"], 0);

    // A collaborator uploading a different file under the same name is flagged; a
    // stranger doing the same is not (ordinary names are reused).
    share(
        &router,
        &owner,
        original,
        "colleague-copy@example.test",
        "contributor",
    )
    .await;
    document(
        &router,
        &colleague,
        "Group report",
        &BUDGET,
        Some("Group report.docx"),
    )
    .await;
    let (stranger, _) = identity(&pool, "stranger-copy@example.test").await;
    document(
        &router,
        &stranger,
        "Group report",
        &BUDGET[..3],
        Some("Group report (v2).docx"),
    )
    .await;
    copies::run_once(&pool).await.unwrap();
    let named: Vec<Value> = inbox(&router, &owner)
        .await
        .into_iter()
        .filter(|n| n["kind"] == "same_name_detected")
        .collect();
    assert_eq!(named.len(), 1, "{named:?}");
    assert_eq!(named[0]["actor"]["email"], "colleague-copy@example.test");

    // "Make a copy" by a collaborator is recorded as linked without a notification.
    let before = inbox(&router, &owner).await.len();
    let (in_app, _) = document(&router, &colleague, "Group report — copy", &REPORT, None).await;
    post(
        &router,
        &format!("/documents/{in_app}/metadata"),
        &colleague,
        json!({"settings":{"kind":"document","copied_from":original}}),
    )
    .await;
    copies::run_once(&pool).await.unwrap();
    assert_eq!(inbox(&router, &owner).await.len(), before);
    let status: String =
        sqlx::query_scalar("select status from document_match where source_document_id=$1")
            .bind(in_app)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(status, "linked");
    let _ = tariq_id;
}

#[sqlx::test(migrations = "../../migrations")]
async fn an_original_still_being_typed_stays_the_original(pool: PgPool) {
    let router = app(test_state(pool.clone()));
    let (owner, _) = identity(&pool, "owner-typing@example.test").await;
    let (tariq, _) = identity(&pool, "tariq-typing@example.test").await;
    // Indexed while the owner has typed only the first sentence.
    let (original, ids) = document(&router, &owner, "Group report", &REPORT[..1], None).await;
    copies::run_once(&pool).await.unwrap();
    // Tariq uploads a full copy that arrived by email.
    document(&router, &tariq, "report", &REPORT, Some("report.docx")).await;
    copies::run_once(&pool).await.unwrap();
    // The owner finishes typing; re-indexing finds the copy from the other side.
    let root: String = sqlx::query_scalar("select parent_id from node where id=$1")
        .bind(&ids[0])
        .fetch_one(&pool)
        .await
        .unwrap();
    let ops: Vec<Value> = REPORT[1..]
        .iter()
        .enumerate()
        .map(|(i, text)| json!({"type":"NodeCreated","node_id":ulid::Ulid::new().to_string(),"node_type":"item","parent_id":root,"pos":format!("0001{i:04}"),"fields":{"label":text,"content":para(text)}}))
        .collect();
    let (status, body) = post(
        &router,
        &format!("/documents/{original}/batch"),
        &owner,
        json!({"ops":ops}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    sqlx::query("update document_fingerprint set updated_at=now()-interval '1 minute'")
        .execute(&pool)
        .await
        .unwrap();
    copies::run_once(&pool).await.unwrap();
    let notes = inbox(&router, &owner).await;
    assert!(
        notes.iter().any(|n| n["kind"] == "copy_detected"),
        "the owner hears about the upload: {notes:?}"
    );
    assert!(inbox(&router, &tariq).await.is_empty(), "not the uploader");
    let target: Uuid = sqlx::query_scalar("select target_document_id from document_match")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(target, original);
}

#[sqlx::test(migrations = "../../migrations")]
async fn widely_copied_content_still_reaches_every_owner_once(pool: PgPool) {
    let router = app(test_state(pool.clone()));
    let mut owners = Vec::new();
    for i in 0..7 {
        let (token, _) = identity(&pool, &format!("owner{i}-wide@example.test")).await;
        document(&router, &token, &format!("Consent form {i}"), &REPORT, None).await;
        owners.push(token);
    }
    copies::run_once(&pool).await.unwrap();
    for token in &owners {
        let before = inbox(&router, token).await.len();
        assert!(before <= 7);
    }
    let (tariq, _) = identity(&pool, "tariq-wide@example.test").await;
    document(&router, &tariq, "form", &REPORT, Some("form.docx")).await;
    copies::run_once(&pool).await.unwrap();
    for token in &owners {
        let notes = inbox(&router, token).await;
        let about_tariq = notes
            .iter()
            .filter(|n| n["actor"]["email"] == "tariq-wide@example.test")
            .count();
        assert_eq!(about_tariq, 1, "{notes:?}");
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn copies_that_predate_detection_are_listed_not_announced(pool: PgPool) {
    let router = app(test_state(pool.clone()));
    let (owner, _) = identity(&pool, "owner-history@example.test").await;
    let (tariq, _) = identity(&pool, "tariq-history@example.test").await;
    let (original, _) = document(&router, &owner, "Old report", &REPORT, None).await;
    document(&router, &tariq, "old copy", &REPORT, Some("old copy.docx")).await;
    // Both documents existed before detection was installed.
    sqlx::query("update copy_detection_baseline set started_at=now()+interval '1 minute'")
        .execute(&pool)
        .await
        .unwrap();
    copies::run_once(&pool).await.unwrap();
    assert!(inbox(&router, &owner).await.is_empty());
    let (_, listed) = get(&router, &format!("/documents/{original}/copies"), &owner).await;
    assert_eq!(listed["items"].as_array().unwrap().len(), 1, "{listed}");
    assert_eq!(listed["items"][0]["status"], "open");
}

#[sqlx::test(migrations = "../../migrations")]
async fn roles_rules_and_approvals_govern_merging(pool: PgPool) {
    let router = app(test_state(pool.clone()));
    let (owner, _) = identity(&pool, "owner-rules@example.test").await;
    let (editor, _) = identity(&pool, "editor-rules@example.test").await;
    let (reviewer, _) = identity(&pool, "reviewer-rules@example.test").await;
    let (contributor, _) = identity(&pool, "contributor-rules@example.test").await;
    let (viewer, _) = identity(&pool, "viewer-rules@example.test").await;
    let (doc, ids) = document(&router, &owner, "Protocol", &REPORT, None).await;
    for (email, role) in [
        ("editor-rules@example.test", "editor"),
        ("reviewer-rules@example.test", "reviewer"),
        ("contributor-rules@example.test", "contributor"),
        ("viewer-rules@example.test", "viewer"),
    ] {
        share(&router, &owner, doc, email, role).await;
    }
    let (_, people) = get(&router, &format!("/documents/{doc}/people"), &viewer).await;
    assert_eq!(people["owner"]["email"], "owner-rules@example.test");
    assert_eq!(people["you"]["role"], "viewer");
    let roles: Vec<&str> = people["members"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["role"].as_str().unwrap())
        .collect();
    assert_eq!(
        roles,
        vec!["owner", "editor", "reviewer", "contributor", "viewer"],
        "{people}"
    );
    // Only the owner or a manager changes the rules.
    let rules = json!({"protect_team_version":true,"required_approvals":1,"merge_roles":"editors"});
    let (status, _) = post(
        &router,
        &format!("/documents/{doc}/policy"),
        &editor,
        rules.clone(),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, body) = post(&router, &format!("/documents/{doc}/policy"), &owner, rules).await;
    assert_eq!(status, StatusCode::OK, "{body}");

    // The protected team version refuses an editor's direct edit, not the owner's.
    let edit = |text: &str| json!({"base_seq":null,"ops":[{"type":"FieldEdited","node_id":ids[0],"field":"label","value":text}]});
    let (status, body) = post(
        &router,
        &format!("/documents/{doc}/batch"),
        &editor,
        edit("Editor"),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert!(body["error"].as_str().unwrap().contains("protected"));
    let (status, body) = post(
        &router,
        &format!("/documents/{doc}/batch"),
        &owner,
        edit("Owner"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    // Comments stay open to contributors.
    let (status, body) = post(
        &router,
        &format!("/documents/{doc}/batch"),
        &contributor,
        json!({"ops":[{"type":"CommentAdded","node_id":ids[1],"body":"Check this"}]}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let (_, points) = get(&router, &format!("/documents/{doc}/history-points"), &owner).await;
    let seq = points["latest_seq"].as_i64().unwrap();
    let ops = json!([{"type":"FieldEdited","node_id":ids[2],"field":"label","value":"Household size averaged 4.8 people."},
        {"type":"FieldEdited","node_id":ids[2],"field":"content","value":para("Household size averaged 4.8 people.")}]);
    let (status, created) = post(
        &router,
        &format!("/documents/{doc}/change-requests"),
        &contributor,
        json!({"name":"Fix household size","base_seq":seq,"ops":ops,"submit":true}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{created}");
    assert_eq!(created["relation"]["unrelated"], false, "{created}");
    let draft = created["id"].as_str().unwrap().to_string();
    let uri = format!("/documents/{doc}/change-requests/{draft}");
    // The owner, editor and reviewer hear about it; the viewer does not.
    for token in [&owner, &editor, &reviewer] {
        assert!(inbox(&router, token)
            .await
            .iter()
            .any(|n| n["kind"] == "change_request_submitted"));
    }
    assert!(!inbox(&router, &viewer)
        .await
        .iter()
        .any(|n| n["kind"] == "change_request_submitted"));

    let (_, detail) = get(&router, &uri, &editor).await;
    assert_eq!(detail["review"]["required_approvals"], 1);
    assert_eq!(detail["review"]["satisfied"], false);
    let merge = |detail: &Value, override_rules: bool| json!({"team_seq":detail["team_seq"],"revision":detail["revision"],"override_rules":override_rules});
    let (status, body) = post(
        &router,
        &format!("{uri}/merge"),
        &editor,
        merge(&detail, false),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    // Contributors and viewers cannot approve; nobody approves their own request.
    for token in [&contributor, &viewer] {
        let (status, _) = post(
            &router,
            &format!("{uri}/reviews"),
            token,
            json!({"verdict":"approved"}),
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN);
    }
    let (status, body) = post(
        &router,
        &format!("{uri}/reviews"),
        &reviewer,
        json!({"verdict":"changes_requested","note":"Cite the table","node_id":ids[2]}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body["review"]["changes_requested_by"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let (status, body) = post(
        &router,
        &format!("{uri}/reviews"),
        &reviewer,
        json!({"verdict":"approved"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["review"]["satisfied"], true, "{body}");
    assert!(inbox(&router, &contributor)
        .await
        .iter()
        .any(|n| n["kind"] == "change_request_reviewed"));

    // Editing the request afterwards makes the approval stale.
    let (_, drafts) = get(
        &router,
        &format!("/documents/{doc}/drafts/{draft}"),
        &contributor,
    )
    .await;
    let revision = drafts["draft"]["revision"].as_i64().unwrap();
    let (status, body) = post(&router, &format!("/documents/{doc}/drafts/{draft}"), &contributor, json!({"revision":revision,"ops":[{"type":"FieldEdited","node_id":ids[3],"field":"label","value":"Water access improved."}]})).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (_, detail) = get(&router, &uri, &editor).await;
    assert_eq!(detail["review"]["satisfied"], false, "{detail}");
    let (status, _) = post(
        &router,
        &format!("{uri}/merge"),
        &editor,
        merge(&detail, false),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    // An editor cannot override; the owner can, and it is audited.
    let (status, _) = post(
        &router,
        &format!("{uri}/merge"),
        &editor,
        merge(&detail, true),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, merged) = post(
        &router,
        &format!("{uri}/merge"),
        &owner,
        merge(&detail, true),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{merged}");
    let overridden: bool = sqlx::query_scalar("select (detail->>'rules_overridden')::boolean from operation_audit where action='document.change_request_merged' and resource_id=$1")
        .bind(doc)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(overridden);
    assert!(inbox(&router, &contributor)
        .await
        .iter()
        .any(|n| n["kind"] == "change_request_merged"));

    // With merging reserved for owners, editors cannot merge or decline.
    post(
        &router,
        &format!("/documents/{doc}/policy"),
        &owner,
        json!({"protect_team_version":false,"required_approvals":0,"merge_roles":"owners"}),
    )
    .await;
    let (_, points) = get(&router, &format!("/documents/{doc}/history-points"), &owner).await;
    let (_, second) = post(&router, &format!("/documents/{doc}/change-requests"), &contributor, json!({"name":"Second","base_seq":points["latest_seq"],"ops":[{"type":"FieldEdited","node_id":ids[4],"field":"label","value":"Follow-up."}],"submit":true})).await;
    let uri = format!(
        "/documents/{doc}/change-requests/{}",
        second["id"].as_str().unwrap()
    );
    let (_, detail) = get(&router, &uri, &editor).await;
    let (status, _) = post(
        &router,
        &format!("{uri}/merge"),
        &editor,
        merge(&detail, false),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _) = post(&router, &format!("{uri}/decline"), &editor, json!({})).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _) = post(
        &router,
        &format!("{uri}/decline"),
        &owner,
        json!({"note":"Not now"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    // Ownership moves to a member; the previous owner stays an editor.
    let (status, _) = post(
        &router,
        &format!("/documents/{doc}/owner"),
        &owner,
        json!({"email":"nobody@example.test"}),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, body) = post(
        &router,
        &format!("/documents/{doc}/owner"),
        &owner,
        json!({"email":"editor-rules@example.test"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (_, people) = get(&router, &format!("/documents/{doc}/people"), &owner).await;
    assert_eq!(people["owner"]["email"], "editor-rules@example.test");
    assert_eq!(people["you"]["role"], "editor");
    assert!(inbox(&router, &editor)
        .await
        .iter()
        .any(|n| n["kind"] == "ownership_transferred"));
}

#[sqlx::test(migrations = "../../migrations")]
async fn unrelated_content_under_the_same_name_needs_explicit_choices(pool: PgPool) {
    let router = app(test_state(pool.clone()));
    let (owner, _) = identity(&pool, "owner-unrelated@example.test").await;
    let (doc, ids) = document(&router, &owner, "report", &REPORT, None).await;
    let (_, points) = get(&router, &format!("/documents/{doc}/history-points"), &owner).await;
    let seq = points["latest_seq"].as_i64().unwrap();
    // A different report.docx: most paragraphs removed, others replaced, new ones added.
    let root: String =
        sqlx::query_scalar("select id from node where document_id=$1 and parent_id is null")
            .bind(doc)
            .fetch_one(&pool)
            .await
            .unwrap();
    let mut ops = vec![];
    for id in &ids[1..] {
        ops.push(json!({"type":"NodeDeleted","node_id":id}));
    }
    ops.push(json!({"type":"FieldEdited","node_id":ids[0],"field":"label","value":BUDGET[0]}));
    ops.push(
        json!({"type":"FieldEdited","node_id":ids[0],"field":"content","value":para(BUDGET[0])}),
    );
    for (i, text) in BUDGET[1..].iter().enumerate() {
        ops.push(json!({"type":"NodeCreated","node_id":ulid::Ulid::new().to_string(),"node_type":"item","parent_id":root,"pos":format!("0000{:04}",20+i),"fields":{"label":text,"content":para(text)}}));
    }
    let (status, created) = post(&router, &format!("/documents/{doc}/change-requests"), &owner, json!({"name":"report.docx","base_seq":seq,"ops":ops,"submit":true,"source":{"kind":"file","filename":"report.docx"}})).await;
    assert_eq!(status, StatusCode::OK, "{created}");
    assert_eq!(created["relation"]["unrelated"], true, "{created}");
    let uri = format!(
        "/documents/{doc}/change-requests/{}",
        created["id"].as_str().unwrap()
    );
    let (_, detail) = get(&router, &uri, &owner).await;
    let conflicts = detail["conflicts"].as_array().unwrap();
    // Every existing paragraph it removes or replaces needs a choice.
    assert_eq!(conflicts.len(), 6, "{detail}");
    assert!(conflicts.iter().all(|c| c["field"] == "$block"));
    let body = json!({"team_seq":detail["team_seq"],"revision":detail["revision"]});
    let (status, _) = post(&router, &format!("{uri}/merge"), &owner, body).await;
    assert_eq!(status, StatusCode::CONFLICT);
    // Keep the team's paragraphs, take their replacement of the first one.
    let mut resolutions = serde_json::Map::new();
    for c in conflicts {
        let key = c["key"].as_str().unwrap().to_string();
        let side = if key.starts_with(&ids[0]) {
            "draft"
        } else {
            "team"
        };
        resolutions.insert(key, json!(side));
    }
    let body = json!({"team_seq":detail["team_seq"],"revision":detail["revision"],"resolutions":resolutions});
    let (status, merged) = post(&router, &format!("{uri}/merge"), &owner, body).await;
    assert_eq!(status, StatusCode::OK, "{merged}");
    let (_, team) = get(&router, &format!("/documents/{doc}"), &owner).await;
    let nodes = team["state"]["nodes"].as_object().unwrap();
    assert_eq!(nodes[&ids[0]]["current_fields"]["label"], BUDGET[0]);
    for id in &ids[1..] {
        assert_eq!(nodes[id]["deleted"], false, "kept the team's paragraph");
    }
    let added = nodes
        .values()
        .filter(|n| {
            n["deleted"] == false
                && BUDGET[1..].contains(&n["current_fields"]["label"].as_str().unwrap_or(""))
        })
        .count();
    assert_eq!(added, 3, "new paragraphs merge without a choice");
}

#[sqlx::test(migrations = "../../migrations")]
async fn administrators_permanently_delete_active_and_trashed_documents(pool: PgPool) {
    let router = app(test_state(pool.clone()));
    let (admin, _) = identity(&pool, ADMIN).await;
    let (owner, _) = identity(&pool, "owner-erase@example.test").await;
    let (active, _) = document(&router, &owner, "Active report", &REPORT, Some("a.docx")).await;
    copies::run_once(&pool).await.unwrap();
    let (_, preview) = get(
        &router,
        &format!("/admin/impact?kind=documents&id={active}"),
        &admin,
    )
    .await;
    assert!(
        preview["blockers"].as_array().unwrap().is_empty(),
        "{preview}"
    );
    assert_eq!(preview["in_trash"], false);
    let (status, body) = post(&router, "/admin/erase", &admin, json!({"kind":"documents","id":active,"confirmation":"Active report","revision":preview["revision"],"reason":"duplicate","acknowledge_backups":true})).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let left: i64 = sqlx::query_scalar("select (select count(*) from document where id=$1)+(select count(*) from event where document_id=$1)+(select count(*) from document_fingerprint where document_id=$1)+(select count(*) from document_block_hash where document_id=$1)")
        .bind(active)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(left, 0);
    let trashed: Vec<String> =
        sqlx::query_scalar("select action from operation_audit where resource_id=$1 order by id")
            .bind(active)
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(
        trashed,
        vec!["document.trashed_by_admin", "documents.erased"]
    );

    // Batch deletion needs the typed count; each document gets its own receipt.
    let (a, _) = document(&router, &owner, "Batch A", &BUDGET, None).await;
    let (b, _) = document(&router, &owner, "Batch B", &BUDGET[..2], None).await;
    post(
        &router,
        &format!("/documents/{b}/trash"),
        &owner,
        json!({"deleted":true}),
    )
    .await;
    let (_, trash) = get(
        &router,
        "/admin/records?kind=documents&status=trash",
        &admin,
    )
    .await;
    assert_eq!(trash["items"].as_array().unwrap().len(), 1, "{trash}");
    let body = json!({"ids":[a,b],"confirmation":"DELETE 2 DOCUMENT","reason":"policy","acknowledge_backups":true});
    let (status, _) = post(&router, "/admin/erase-batch", &admin, body).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, _) = post(&router, "/admin/erase-batch", &owner, json!({"ids":[a],"confirmation":"DELETE 1 DOCUMENT","reason":"policy","acknowledge_backups":true})).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let body = json!({"ids":[a,b,Uuid::new_v4()],"confirmation":"DELETE 3 DOCUMENTS","reason":"policy","acknowledge_backups":true});
    let (status, result) = post(&router, "/admin/erase-batch", &admin, body).await;
    assert_eq!(status, StatusCode::OK, "{result}");
    assert_eq!(result["erased"], 2, "{result}");
    let receipts: i64 =
        sqlx::query_scalar("select count(*) from erasure_receipt where resource_id=any($1)")
            .bind(vec![a, b])
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(receipts, 2);
}
