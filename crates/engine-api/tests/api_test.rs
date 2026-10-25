//! Stage-18 API integration tests (DB-backed, real router via `tower::oneshot`).
//!
//! Each `#[sqlx::test(migrations = "../../migrations")]` provisions a fresh migrated
//! database; tests drive the actual Axum app, covering each endpoint's happy path,
//! auth/role rejection, cursor pagination, idempotency, SSE delivery, and the audit
//! surface. Requires a reachable Postgres (DATABASE_URL): the dev DB or CI service.

use axum::body::{to_bytes, Body};
use axum::http::{header, Request, StatusCode};
use axum::Router;
use engine_api::auth::{store as auth_store, token, AuthConfig};
use engine_api::{app, AppState};
use engine_core::governance::Role;
use engine_shared::DocumentId;
use futures::StreamExt;
use serde_json::{json, Value};
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

mod common;

const KEY: [u8; 32] = [7u8; 32];

#[sqlx::test(migrations = "../../migrations")]
async fn waiting_writer_rechecks_revoked_access(pool: PgPool) {
    let router = app(test_state(pool.clone()));
    let owner = bearer(seed_identity(&pool, "lock-owner@example.test").await);
    let editor = seed_identity(&pool, "lock-editor@example.test").await;
    let doc = create_document(&router, &owner, "Permission serialization").await;
    sqlx::query("insert into document_access(document_id,identity_id,role) values($1,$2,'author')")
        .bind(doc)
        .bind(editor)
        .execute(&pool)
        .await
        .unwrap();
    // Hold the same lock taken by workspace membership changes, then revoke.
    let mut permission = pool.begin().await.unwrap();
    sqlx::query("select id from document where id=$1 for no key update")
        .bind(doc)
        .execute(&mut *permission)
        .await
        .unwrap();
    let pid: i32 = sqlx::query_scalar("select pg_backend_pid()")
        .fetch_one(&mut *permission)
        .await
        .unwrap();
    sqlx::query("delete from document_access where document_id=$1 and identity_id=$2")
        .bind(doc)
        .bind(editor)
        .execute(&mut *permission)
        .await
        .unwrap();
    let writer = tokio::spawn(async move {
        send(
            &router,
            auth_post(
                &format!("/documents/{doc}/ops/node-create"),
                &bearer(editor),
                &json!({"node_type":"form","pos":"a0","fields":{}}),
            ),
        )
        .await
    });
    tokio::time::timeout(std::time::Duration::from_secs(5),async {
        loop {
            let blocked:bool=sqlx::query_scalar("select exists(select 1 from pg_stat_activity where datname=current_database() and $1=any(pg_blocking_pids(pid)))")
                .bind(pid).fetch_one(&pool).await.unwrap();
            if blocked {break;}
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    }).await.expect("writer never reached the permission lock");
    permission.commit().await.unwrap();
    let (status, body) = tokio::time::timeout(std::time::Duration::from_secs(5), writer)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    let events: i64 = sqlx::query_scalar("select count(*) from event where document_id=$1")
        .bind(doc)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(events, 0);
}

#[sqlx::test(migrations = "../../migrations")]
async fn draft_sharing_remembers_exclusions_until_explicitly_included(pool: PgPool) {
    let mut shared_state = test_state(pool.clone());
    shared_state.shared_draft_bases = true;
    let router = app(shared_state);
    let owner = bearer(seed_identity(&pool, "selection@example.test").await);
    let doc = create_document(&router, &owner, "Selections").await;
    let root = ulid::Ulid::generate().to_string();
    let child = ulid::Ulid::generate().to_string();
    let (status,result)=send(&router,auth_post(&format!("/documents/{doc}/batch"),&owner,&json!({"base_seq":0,"ops":[
      {"type":"NodeCreated","node_id":root,"node_type":"form","pos":"a0","fields":{"label":"Original root"}},
      {"type":"NodeCreated","node_id":child,"node_type":"item","parent_id":root,"pos":"a0","fields":{"label":"Original child","type":"text","name":"question"}}
    ]}))).await;
    assert_eq!(status, StatusCode::OK, "{result}");
    let (_, draft) = send(
        &router,
        auth_post(
            &format!("/documents/{doc}/drafts"),
            &owner,
            &json!({"name":"Private options"}),
        ),
    )
    .await;
    let uri = format!("/documents/{doc}/drafts/{}", draft["id"].as_str().unwrap());
    let (status, result) = send(
        &router,
        auth_post(
            &uri,
            &owner,
            &json!({"revision":0,"ops":[
              {"type":"FieldEdited","node_id":root,"field":"label","value":"Ready root"},
              {"type":"FieldEdited","node_id":child,"field":"label","value":"Private child"}
            ]}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{result}");
    let sync = format!("{uri}/sync");
    let (_, preview) = send(
        &router,
        auth_post(&sync, &owner, &json!({"action":"preview"})),
    )
    .await;
    let(status,result)=send(&router,auth_post(&sync,&owner,&json!({"action":"share","team_seq":preview["team_seq"],"revision":preview["revision"],"included_nodes":[root],"remember_selection":true}))).await;
    assert_eq!(status, StatusCode::OK, "{result}");
    assert_eq!(result["completed"], false);
    let (_, preview) = send(
        &router,
        auth_post(&sync, &owner, &json!({"action":"preview"})),
    )
    .await;
    assert_eq!(preview["excluded_nodes"], json!([child]));
    let(status,result)=send(&router,auth_post(&sync,&owner,&json!({"action":"share","team_seq":preview["team_seq"],"revision":preview["revision"]}))).await;
    assert_eq!(status, StatusCode::OK, "{result}");
    assert!(result["events"].as_array().unwrap().is_empty());
    let (_, team) = send(&router, auth_post_get(&format!("/documents/{doc}"), &owner)).await;
    assert_eq!(
        team["state"]["nodes"][&child]["current_fields"]["label"],
        "Original child"
    );
    let (_, preview) = send(
        &router,
        auth_post(&sync, &owner, &json!({"action":"preview"})),
    )
    .await;
    let(status,result)=send(&router,auth_post(&sync,&owner,&json!({"action":"share","team_seq":preview["team_seq"],"revision":preview["revision"],"included_nodes":[child],"remember_selection":true}))).await;
    assert_eq!(status, StatusCode::OK, "{result}");
    assert_eq!(result["completed"], true);
    let excluded: Vec<String> =
        sqlx::query_scalar("select excluded_nodes from workspace_draft where id=$1")
            .bind(Uuid::parse_str(draft["id"].as_str().unwrap()).unwrap())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(excluded.is_empty());
    let (_, verification) = send(
        &router,
        auth_post_get(&format!("/documents/{doc}/verify"), &owner),
    )
    .await;
    assert_eq!(verification["ok"], true);
}

#[sqlx::test(migrations = "../../migrations")]
async fn sharing_a_new_block_brings_its_new_parent(pool: PgPool) {
    let router = app(test_state(pool.clone()));
    let owner = bearer(seed_identity(&pool, "new-parent@example.test").await);
    let doc = create_document(&router, &owner, "New parents").await;
    let root = ulid::Ulid::generate().to_string();
    let (status,result)=send(&router,auth_post(&format!("/documents/{doc}/batch"),&owner,&json!({"base_seq":0,"ops":[
      {"type":"NodeCreated","node_id":root,"node_type":"form","pos":"a0","fields":{"label":"Root"}}
    ]}))).await;
    assert_eq!(status, StatusCode::OK, "{result}");
    let (_, draft) = send(
        &router,
        auth_post(
            &format!("/documents/{doc}/drafts"),
            &owner,
            &json!({"name":"New section"}),
        ),
    )
    .await;
    let uri = format!("/documents/{doc}/drafts/{}", draft["id"].as_str().unwrap());
    let section = ulid::Ulid::generate().to_string();
    let child = ulid::Ulid::generate().to_string();
    let (status,result)=send(&router,auth_post(&uri,&owner,&json!({"revision":0,"ops":[
      {"type":"NodeCreated","node_id":section,"node_type":"section","parent_id":root,"pos":"a0","fields":{"label":"New section"}},
      {"type":"NodeCreated","node_id":child,"node_type":"item","parent_id":section,"pos":"a0","fields":{"label":"New child","type":"text","name":"new_child"}}
    ]}))).await;
    assert_eq!(status, StatusCode::OK, "{result}");
    let sync = format!("{uri}/sync");
    let (_, preview) = send(
        &router,
        auth_post(&sync, &owner, &json!({"action":"preview"})),
    )
    .await;
    let(status,result)=send(&router,auth_post(&sync,&owner,&json!({"action":"share","team_seq":preview["team_seq"],"revision":preview["revision"],"included_nodes":[child]}))).await;
    assert_eq!(status, StatusCode::OK, "{result}");
    assert_eq!(result["completed"], true);
    let (_, team) = send(&router, auth_post_get(&format!("/documents/{doc}"), &owner)).await;
    assert_eq!(team["state"]["nodes"][&child]["parent_id"], json!(section));
}
#[sqlx::test(migrations = "../../migrations")]
async fn unreplayable_log_returns_specific_error(pool: PgPool) {
    let router = app(test_state(pool.clone()));
    let actor = seed_identity(&pool, "replay@example.test").await;
    let owner = bearer(actor);
    let doc = create_document(&router, &owner, "Broken history").await;
    // No write path accepts this: it accepts a suggestion that never existed.
    let accepted = engine_shared::EventPayload::SuggestionAccepted {
        suggestion_id: "missing".into(),
    };
    engine_core::log::append(
        &pool,
        DocumentId(doc),
        &accepted,
        engine_shared::IdentityId(actor),
    )
    .await
    .unwrap();
    let root = ulid::Ulid::generate().to_string();
    let (status, body) = send(
        &router,
        auth_post(
            &format!("/documents/{doc}/batch"),
            &owner,
            &json!({"base_seq":1,"ops":[
                {"type":"NodeCreated","node_id":root,"node_type":"form","pos":"a0","fields":{}}
            ]}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(body["code"], "replay_failed", "{body}");
    assert_eq!(body["details"], json!({"document_id": doc, "seq": 1}));
}

#[sqlx::test(migrations = "../../migrations")]
async fn content_neutral_settings_skip_replay_but_review_transitions_fail_closed(pool: PgPool) {
    let router = app(test_state(pool.clone()));
    let actor = seed_identity(&pool, "no-replay@example.test").await;
    let owner = bearer(actor);
    let doc = create_document(&router, &owner, "Broken history").await;
    let (_, draft) = send(
        &router,
        auth_post(
            &format!("/documents/{doc}/drafts"),
            &owner,
            &json!({"name":"Kept"}),
        ),
    )
    .await;
    let draft = format!("/documents/{doc}/drafts/{}", draft["id"].as_str().unwrap());
    let request = draft.replace("/drafts/", "/change-requests/");
    let draft_id: Uuid = draft.rsplit('/').next().unwrap().parse().unwrap();
    let (status, body) = send(
        &router,
        auth_post(
            &format!("{draft}/submit"),
            &owner,
            &json!({"note":"Before corruption"}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let accepted = engine_shared::EventPayload::SuggestionAccepted {
        suggestion_id: "missing".into(),
    };
    engine_core::log::append(
        &pool,
        DocumentId(doc),
        &accepted,
        engine_shared::IdentityId(actor),
    )
    .await
    .unwrap();
    let root = ulid::Ulid::generate().to_string();
    for (uri, body) in [
        (
            format!("/documents/{doc}/members"),
            json!({"email":"reader@example.test","role":"viewer"}),
        ),
        (
            format!("/documents/{doc}/metadata"),
            json!({"title":"Renamed"}),
        ),
        (
            draft.clone(),
            json!({"revision":0,"ops":[{"type":"NodeCreated","node_id":root,"node_type":"form","pos":"a0","fields":{}}]}),
        ),
    ] {
        let (status, body) = send(&router, auth_post(&uri, &owner, &body)).await;
        assert_eq!(status, StatusCode::OK, "{uri}: {body}");
    }
    // Review state must not advance when nobody can reconstruct the content being
    // submitted, reviewed or closed.
    for (uri, body) in [
        (format!("{draft}/submit"), json!({"note":"Ready"})),
        (
            format!("{request}/reviews"),
            json!({"verdict":"commented","note":"Noted"}),
        ),
        (
            format!("{request}/reviews"),
            json!({"verdict":"approved","revision":1}),
        ),
        (format!("{draft}/withdraw"), json!({})),
        (format!("{request}/decline"), json!({"note":"Declined"})),
    ] {
        let (status, body) = send(&router, auth_post(&uri, &owner, &body)).await;
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR, "{uri}: {body}");
        assert_eq!(body["code"], "replay_failed", "{uri}: {body}");
    }
    let (note, outcome): (String, Option<String>) =
        sqlx::query_as("select submission_note,review_outcome from workspace_draft where id=$1")
            .bind(draft_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(note, "Before corruption");
    assert!(outcome.is_none());
    let reviews: i64 =
        sqlx::query_scalar("select count(*) from change_request_review where draft_id=$1")
            .bind(draft_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(reviews, 0);
    // Reading the content still replays it.
    let (_, body) = send(&router, auth_post_get(&format!("/documents/{doc}"), &owner)).await;
    assert_eq!(body["code"], "replay_failed", "{body}");
}

#[sqlx::test(migrations = "../../migrations")]
async fn admin_overview_counts_error_responses(pool: PgPool) {
    let mut state = test_state(pool.clone());
    state.admin_emails = "counters@example.test".into();
    let router = app(state);
    let actor = seed_identity(&pool, "counters@example.test").await;
    let owner = bearer(actor);
    let doc = create_document(&router, &owner, "Counted").await;
    let root = ulid::Ulid::generate().to_string();
    let batch = |base_seq: i64| json!({"base_seq":base_seq,"ops":[{"type":"NodeCreated","node_id":root,"node_type":"form","pos":"a0","fields":{}}]});
    let uri = format!("/documents/{doc}/batch");
    let (status, body) = send(&router, auth_post(&uri, &owner, &batch(0))).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    // The same block again conflicts.
    let (status, _) = send(&router, auth_post(&uri, &owner, &batch(1))).await;
    assert_eq!(status, StatusCode::CONFLICT);
    let accepted = engine_shared::EventPayload::SuggestionAccepted {
        suggestion_id: "missing".into(),
    };
    engine_core::log::append(
        &pool,
        DocumentId(doc),
        &accepted,
        engine_shared::IdentityId(actor),
    )
    .await
    .unwrap();
    let (status, _) = send(&router, auth_post(&uri, &owner, &batch(2))).await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    let (status, overview) = send(&router, auth_post_get("/admin/overview", &owner)).await;
    assert_eq!(status, StatusCode::OK, "{overview}");
    assert_eq!(
        overview["errors"],
        json!({"internal":0,"replay_failed":1,"conflict":1,"rate_limited":0,"busy":0})
    );
    assert_eq!(overview["counts"]["content_patches"], 0);
}

#[sqlx::test(migrations = "../../migrations")]
async fn admin_overview_reports_rows_pruned_by_retention(pool: PgPool) {
    let mut state = test_state(pool.clone());
    state.admin_emails = "retention@example.test".into();
    let actor = seed_identity(&pool, "retention@example.test").await;
    for age in ["40 days", "1 day"] {
        sqlx::query("insert into web_metric(page_id,metric,host,page,value,recorded_at) values(gen_random_uuid(),'visit','homepage','home',1,now()-$1::interval)")
            .bind(age).execute(&pool).await.unwrap();
    }
    let stats = state.retention.clone();
    let router = app(state);
    let (_, overview) = send(&router, auth_post_get("/admin/overview", &bearer(actor))).await;
    assert_eq!(overview["retention"]["runs"], 0);
    assert_eq!(overview["retention"]["last_run_at"], Value::Null);
    let outcome = engine_api::retention::run_once(&pool, &Default::default(), &stats)
        .await
        .unwrap();
    assert!(matches!(outcome, engine_api::retention::Outcome::Ran(_)));
    let (status, overview) = send(&router, auth_post_get("/admin/overview", &bearer(actor))).await;
    assert_eq!(status, StatusCode::OK, "{overview}");
    let retention = &overview["retention"];
    assert_eq!(retention["runs"], 1);
    assert_eq!(retention["failures"], 0);
    assert_eq!(retention["pruned"]["web_metric"], 1);
    assert_eq!(retention["pruned"]["magic_link"], 0);
    assert!(retention["last_run_at"].is_string(), "{retention}");
    // Counts only: nothing but table names and numbers.
    let names: std::collections::BTreeSet<&str> = retention["pruned"]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    let expected: std::collections::BTreeSet<&str> =
        engine_api::retention::table_names().into_iter().collect();
    assert_eq!(names, expected);
}

#[sqlx::test(migrations = "../../migrations")]
async fn shared_draft_bases_reuse_content_preserve_responses_and_enforce_scope(pool: PgPool) {
    let mut state = test_state(pool.clone());
    state.shared_draft_bases = true;
    let router = app(state);
    let actor = seed_identity(&pool, "shared-drafts@example.test").await;
    let owner = bearer(actor);
    let document = create_document(&router, &owner, "Shared drafts").await;
    let (_, created) = send(
        &router,
        auth_post(
            &format!("/documents/{document}/ops/node-create"),
            &owner,
            &json!({"node_type":"form","pos":"a0","fields":{"label":"Original"}}),
        ),
    )
    .await;
    let node = created["event"]["target_node_id"].as_str().unwrap();
    let (_, first) = send(
        &router,
        auth_post(
            &format!("/documents/{document}/drafts"),
            &owner,
            &json!({"name":"First"}),
        ),
    )
    .await;
    let before: i64 =
        sqlx::query_scalar("select count(*) from checkpoint_object where document_id=$1")
            .bind(document)
            .fetch_one(&pool)
            .await
            .unwrap();
    let (_, second) = send(
        &router,
        auth_post(
            &format!("/documents/{document}/drafts"),
            &owner,
            &json!({"name":"Second"}),
        ),
    )
    .await;
    let after: i64 =
        sqlx::query_scalar("select count(*) from checkpoint_object where document_id=$1")
            .bind(document)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(before, after);
    assert!(before > 0);
    let bases: Vec<Value> =
        sqlx::query_scalar("select base_state from workspace_draft where document_id=$1")
            .bind(document)
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(bases.len(), 2);
    assert_eq!(bases[0], bases[1]);
    assert_eq!(bases[0]["$dynodoc_state"], 2);
    assert!(bases[0].to_string().len() < 120);
    let uri = format!(
        "/documents/{document}/drafts/{}",
        first["id"].as_str().unwrap()
    );
    let (status, read) = send(&router, auth_post_get(&uri, &owner)).await;
    assert_eq!(status, StatusCode::OK, "{read}");
    assert_eq!(
        read["draft"]["base_state"]["nodes"][node]["current_fields"]["label"],
        "Original"
    );
    assert!(read["draft"]["base_state"].get("$dynodoc_state").is_none());
    let (status,edited)=send(&router,auth_post(&uri,&owner,&json!({"revision":0,"ops":[{"type":"FieldEdited","node_id":node,"field":"label","value":"Draft"}]}))).await;
    assert_eq!(status, StatusCode::OK, "{edited}");
    let sync = format!("{uri}/sync");
    let (_, preview) = send(
        &router,
        auth_post(&sync, &owner, &json!({"action":"preview"})),
    )
    .await;
    let (status, merged) = send(
        &router,
        auth_post(
            &sync,
            &owner,
            &json!({"action":"share","team_seq":preview["team_seq"],"revision":1}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{merged}");
    let merge_base: Value =
        sqlx::query_scalar("select merge_base_state from workspace_draft where id=$1")
            .bind(Uuid::parse_str(first["id"].as_str().unwrap()).unwrap())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(merge_base["$dynodoc_state"], 2);
    assert_ne!(merge_base, bases[0]);
    let (status, read) = send(&router, auth_post_get(&uri, &owner)).await;
    assert_eq!(status, StatusCode::OK, "{read}");
    assert_eq!(
        read["draft"]["merge_base_state"]["nodes"][node]["current_fields"]["label"],
        "Draft"
    );
    // Initial bases cannot be changed by application-level UPDATE, in either codec.
    assert!(
        sqlx::query("update workspace_draft set base_state='{}' where document_id=$1")
            .bind(document)
            .execute(&pool)
            .await
            .is_err()
    );
    let other = create_document(&router, &owner, "Another document").await;
    assert!(sqlx::query("insert into workspace_draft(document_id,name,base_seq,base_state,created_by) values($1,'Cross scope',0,$2,$3)")
        .bind(other).bind(&bases[0]).bind(actor).execute(&pool).await.is_err());
    let stranger = bearer(seed_identity(&pool, "stranger-shared@example.test").await);
    assert_eq!(
        send(&router, auth_post_get(&uri, &stranger)).await.0,
        StatusCode::FORBIDDEN
    );
    let second_uri = format!(
        "/documents/{document}/drafts/{}",
        second["id"].as_str().unwrap()
    );
    let (_, unchanged) = send(&router, auth_post_get(&second_uri, &owner)).await;
    assert_eq!(
        unchanged["state"]["nodes"][node]["current_fields"]["label"],
        "Original"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn discussion_replies_resolution_permissions_and_replay(pool: PgPool) {
    let router = app(test_state(pool.clone()));
    let owner_id = seed_identity(&pool, "discussion-owner@example.test").await;
    let reviewer_id = seed_identity(&pool, "discussion-reviewer@example.test").await;
    let outsider = seed_identity(&pool, "discussion-outsider@example.test").await;
    let owner = bearer(owner_id);
    let reviewer = bearer(reviewer_id);
    let doc = create_document(&router, &owner, "Discussion").await;
    sqlx::query(
        "insert into document_access(document_id,identity_id,role) values($1,$2,'reviewer')",
    )
    .bind(doc)
    .bind(reviewer_id)
    .execute(&pool)
    .await
    .unwrap();
    let (status, root) = send(
        &router,
        auth_post(
            &format!("/documents/{doc}/ops/comment"),
            &reviewer,
            &json!({"body":"Clarify the wording?"}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{root}");
    let thread = root["event"]["id"].as_str().unwrap();
    let route = format!("/documents/{doc}/discussions/{thread}");
    let list = format!("/documents/{doc}/discussions");
    assert_eq!(
        send(&router, auth_post_get(&list, &bearer(outsider)))
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    let (status, reply) = send(
        &router,
        auth_post(
            &route,
            &reviewer,
            &json!({"revision":1,"body":"Here is my suggested explanation."}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{reply}");
    assert_eq!(reply["events"][0]["type"], "CommentReplied");
    assert_eq!(
        send(
            &router,
            auth_post(
                &route,
                &reviewer,
                &json!({"revision":1,"body":"Stale duplicate"})
            )
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    assert_eq!(
        send(
            &router,
            auth_post(&route, &reviewer, &json!({"revision":2,"resolved":true}))
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        send(
            &router,
            auth_post(&route, &owner, &json!({"revision":2,"resolved":true}))
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(
        send(
            &router,
            auth_post(
                &route,
                &reviewer,
                &json!({"revision":3,"body":"Closed reply"})
            )
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    let result = send(&router, auth_post_get(&list, &owner)).await.1;
    assert_eq!(result["items"][0]["resolved"], true);
    assert_eq!(
        result["items"][0]["replies"][0]["body"],
        "Here is my suggested explanation."
    );
    assert_eq!(
        send(
            &router,
            auth_post(&route, &owner, &json!({"revision":3,"resolved":false}))
        )
        .await
        .0,
        StatusCode::OK
    );
    let other = create_document(&router, &owner, "Other document").await;
    assert_eq!(
        send(
            &router,
            auth_post(
                &format!("/documents/{other}/discussions/{thread}"),
                &owner,
                &json!({"revision":1,"body":"Cross-document attempt"})
            )
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    engine_core::log::verify_chain(&pool, DocumentId(doc))
        .await
        .unwrap();
    let replay = engine_core::snapshot::SnapshotEngine::new(pool.clone())
        .read_current_state(DocumentId(doc))
        .await
        .unwrap();
    assert_eq!(
        replay.comments.len(),
        1,
        "Replies are projected from canonical event IDs, not duplicated as root comments"
    );
    let (_, public) = send(
        &router,
        auth_post(
            &format!("/documents/{doc}/public-links"),
            &owner,
            &json!({"base_seq":4}),
        ),
    )
    .await;
    let encoded = send(
        &router,
        get(&format!(
            "/public/documents/{}",
            public["token"].as_str().unwrap()
        )),
    )
    .await
    .1
    .to_string();
    assert!(!encoded.contains("Clarify") && !encoded.contains("suggested explanation"));
}

#[sqlx::test(migrations = "../../migrations")]
async fn admin_erasure_removes_whole_document_but_cannot_bypass_history_guards(pool: PgPool) {
    let runtime = common::RuntimeDatabase::new(pool).await;
    let pool = runtime.pool.clone();
    let admin_id = seed_identity(&pool, "erase-admin@example.test").await;
    let owner_id = seed_identity(&pool, "erase-owner@example.test").await;
    let mut state = test_state(pool.clone());
    state.admin_emails = "erase-admin@example.test".into();
    state.shared_draft_bases = true;
    state.named_snapshot_storage = engine_core::snapshot::NamedStorage::SharedV2;
    let router = app(state);
    let admin = bearer(admin_id);
    let owner = bearer(owner_id);
    let doc = create_document(&router, &owner, "Remove me").await;
    let keep = create_document(&router, &owner, "Keep me").await;
    let (_, created) = send(
        &router,
        auth_post(
            &format!("/documents/{doc}/ops/node-create"),
            &owner,
            &json!({"node_type":"form","pos":"a","fields":{"label":"Private original"}}),
        ),
    )
    .await;
    let node = created["event"]["target_node_id"].as_str().unwrap();
    let (_, draft) = send(
        &router,
        auth_post(
            &format!("/documents/{doc}/drafts"),
            &owner,
            &json!({"name":"Private draft"}),
        ),
    )
    .await;
    let draft_id = draft["id"].as_str().unwrap();
    assert_eq!(send(&router,auth_post(&format!("/documents/{doc}/drafts/{draft_id}"),&owner,&json!({"revision":0,"ops":[{"type":"FieldEdited","node_id":node,"field":"label","value":"Private draft edit"}]}))).await.0,StatusCode::OK);
    assert_eq!(
        send(
            &router,
            auth_post(
                &format!("/documents/{doc}/versions"),
                &owner,
                &json!({"name":"Checkpoint"})
            )
        )
        .await
        .0,
        StatusCode::OK
    );
    let (_, public) = send(
        &router,
        auth_post(
            &format!("/documents/{doc}/public-links"),
            &owner,
            &json!({"base_seq":1}),
        ),
    )
    .await;
    sqlx::query("insert into document_upload(document_id,filename,content_type,content,created_by) values($1,'private.txt','text/plain',$2,$3)").bind(doc).bind(b"private bytes".as_slice()).bind(owner_id).execute(&pool).await.unwrap();
    let preview_uri = format!("/admin/impact?kind=documents&id={doc}");
    engine_core::shared_checkpoint::postgres::take(
        &pool,
        engine_shared::DocumentId(doc),
        1,
        engine_core::shared_checkpoint::Limits::default(),
    )
    .await
    .unwrap();
    assert_eq!(
        send(&router, auth_post_get(&preview_uri, &owner)).await.0,
        StatusCode::FORBIDDEN
    );
    assert!(sqlx::query("delete from event where document_id=$1")
        .bind(doc)
        .execute(&pool)
        .await
        .is_err());
    assert!(
        sqlx::query("delete from document_version where document_id=$1")
            .bind(doc)
            .execute(&pool)
            .await
            .is_err()
    );
    assert!(sqlx::query("delete from draft_edit")
        .execute(&pool)
        .await
        .is_err());
    assert!(sqlx::query("delete from document where id=$1")
        .bind(doc)
        .execute(&pool)
        .await
        .is_err());
    // An active document can be deleted directly: erasure moves it to Trash in the
    // same transaction, so the preview reports that rather than a blocker.
    let (_, active_plan) = send(&router, auth_post_get(&preview_uri, &admin)).await;
    assert!(active_plan["blockers"].as_array().unwrap().is_empty());
    assert_eq!(active_plan["in_trash"], false);
    assert_eq!(
        send(
            &router,
            auth_post(
                "/admin/actions",
                &admin,
                &json!({"kind":"documents","id":doc,"removed":true,"confirmation":"Remove me"})
            )
        )
        .await
        .0,
        StatusCode::OK
    );
    let (_, preview) = send(&router, auth_post_get(&preview_uri, &admin)).await;
    assert_eq!(preview["counts"]["events"], 1);
    assert_eq!(preview["counts"]["shared_checkpoints"], 1);
    assert!(preview["counts"]["checkpoint_objects"].as_i64().unwrap() > 0);
    assert!(preview["counts"]["checkpoint_bytes"].as_i64().unwrap() > 0);
    let mut erase = json!({"kind":"documents","id":doc,"confirmation":"Remove me","revision":preview["revision"],"reason":"owner_request","acknowledge_backups":true});
    erase["confirmation"] = json!("wrong");
    assert_eq!(
        send(&router, auth_post("/admin/erase", &admin, &erase))
            .await
            .0,
        StatusCode::BAD_REQUEST
    );
    erase["confirmation"] = json!("Remove me");
    // A direct history delete remains forbidden even with a same-transaction receipt.
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("insert into erasure_receipt(resource_id,kind,actor_id,reason,counts) values($1,'documents',$2,'other','{}')").bind(doc).bind(admin_id).execute(&mut *tx).await.unwrap();
    assert!(sqlx::query("delete from event where document_id=$1")
        .bind(doc)
        .execute(&mut *tx)
        .await
        .is_err());
    tx.rollback().await.unwrap();
    sqlx::query("insert into document_upload(document_id,filename,content_type,content,created_by) values($1,'later.txt','text/plain',$2,$3)").bind(doc).bind(b"later".as_slice()).bind(owner_id).execute(&pool).await.unwrap();
    assert_eq!(
        send(&router, auth_post("/admin/erase", &admin, &erase))
            .await
            .0,
        StatusCode::BAD_REQUEST,
        "A stale impact must be rejected"
    );
    erase["revision"] =
        send(&router, auth_post_get(&preview_uri, &admin)).await.1["revision"].clone();
    let (status, result) = send(&router, auth_post("/admin/erase", &admin, &erase)).await;
    assert_eq!(status, StatusCode::OK, "{result}");
    for table in [
        "document",
        "event",
        "node",
        "snapshot",
        "snapshot_verification",
        "shared_checkpoint",
        "checkpoint_object",
        "document_upload",
        "document_version",
        "workspace_draft",
        "public_document_view",
        "document_access",
    ] {
        let column = if table == "document" {
            "id"
        } else {
            "document_id"
        };
        let count: i64 =
            sqlx::query_scalar(&format!("select count(*) from {table} where {column}=$1"))
                .bind(doc)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(count, 0, "{table} must be erased");
    }
    assert_eq!(
        sqlx::query_scalar::<_, i64>("select count(*) from draft_edit")
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        send(
            &router,
            get(&format!(
                "/public/documents/{}",
                public["token"].as_str().unwrap()
            ))
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        send(
            &router,
            auth_post_get(&format!("/documents/{keep}"), &owner)
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("select count(*) from erasure_receipt where resource_id=$1")
            .bind(doc)
            .fetch_one(&pool)
            .await
            .unwrap(),
        1
    );
    assert!(sqlx::query("delete from erasure_receipt")
        .execute(&pool)
        .await
        .is_err());
    assert!(sqlx::query("delete from operation_audit")
        .execute(&pool)
        .await
        .is_err());
    runtime.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn admin_account_erasure_preserves_shared_chain_and_clears_private_drafts(pool: PgPool) {
    let runtime = common::RuntimeDatabase::new(pool).await;
    let pool = runtime.pool.clone();
    let admin_id = seed_identity(&pool, "admin-erasure@example.test").await;
    let user_id = seed_identity(&pool, "erased@example.test").await;
    let receiver = seed_identity(&pool, "receiver@example.test").await;
    let mut state = test_state(pool.clone());
    state.admin_emails = "admin-erasure@example.test".into();
    state.shared_draft_bases = true;
    state.named_snapshot_storage = engine_core::snapshot::NamedStorage::SharedV2;
    let router = app(state);
    let admin = bearer(admin_id);
    let user = bearer(user_id);
    assert_eq!(
        send(
            &router,
            auth_post(
                "/profile",
                &user,
                &json!({"name":"Name to erase","bio":"Private profile to erase","revision":0})
            )
        )
        .await
        .0,
        StatusCode::OK
    );
    let doc = create_document(&router, &user, "Shared document").await;
    let (_, created) = send(
        &router,
        auth_post(
            &format!("/documents/{doc}/ops/node-create"),
            &user,
            &json!({"node_type":"form","pos":"a","fields":{"label":"Retained contribution"}}),
        ),
    )
    .await;
    let node = created["event"]["target_node_id"].as_str().unwrap();
    let (_, draft) = send(
        &router,
        auth_post(
            &format!("/documents/{doc}/drafts"),
            &user,
            &json!({"name":"Unshared private work"}),
        ),
    )
    .await;
    let draft_id = draft["id"].as_str().unwrap();
    send(&router,auth_post(&format!("/documents/{doc}/drafts/{draft_id}"),&user,&json!({"revision":0,"ops":[{"type":"FieldEdited","node_id":node,"field":"label","value":"Private"}]}))).await;
    assert_eq!(send(&router,auth_post("/admin/actions",&admin,&json!({"kind":"users","id":user_id,"removed":true,"confirmation":"erased@example.test"}))).await.0,StatusCode::OK);
    let uri = format!("/admin/impact?kind=users&id={user_id}");
    assert!(
        !send(&router, auth_post_get(&uri, &admin)).await.1["blockers"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(send(&router,auth_post("/admin/control",&admin,&json!({"action":"transfer_document","id":doc,"confirmation":"Shared document","recipient":receiver}))).await.0,StatusCode::OK);
    let preview = send(&router, auth_post_get(&uri, &admin)).await.1;
    assert_eq!(preview["blockers"], json!([]));
    let(status,result)=send(&router,auth_post("/admin/erase",&admin,&json!({"kind":"users","id":user_id,"confirmation":"erased@example.test","revision":preview["revision"],"reason":"owner_request","acknowledge_backups":true}))).await;
    assert_eq!(status, StatusCode::OK, "{result}");
    let name: String = sqlx::query_scalar("select display_name from identity where id=$1")
        .bind(user_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(name, "Deleted account");
    let profile: (String, i64) =
        sqlx::query_as("select bio,profile_revision from identity where id=$1")
            .bind(user_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(profile, (String::new(), 2));
    assert_eq!(
        sqlx::query_scalar::<_, i64>("select count(*) from workspace_draft where created_by=$1")
            .bind(user_id)
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("select count(*) from draft_edit")
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
    engine_core::log::verify_chain(&pool, DocumentId(doc))
        .await
        .unwrap();
    assert_eq!(
        send(&router, auth_post_get("/auth/me", &user)).await.0,
        StatusCode::UNAUTHORIZED
    );
    assert!(
        sqlx::query("update identity set disabled_at=null where id=$1")
            .bind(user_id)
            .execute(&pool)
            .await
            .is_err()
    );
    let recreated = seed_identity(&pool, "erased@example.test").await;
    assert_ne!(
        recreated, user_id,
        "Signing up again must not recover the old identity"
    );
    assert_eq!(
        send(
            &router,
            auth_post_get(&format!("/documents/{doc}"), &bearer(recreated))
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    runtime.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn image_attr_edits_persist_full_embedded_source_payloads(pool: PgPool) {
    let router = app(test_state(pool.clone()));
    let owner = bearer(seed_identity(&pool, "image-size@example.test").await);
    let doc = create_document(&router, &owner, "Embedded image").await;
    let root = ulid::Ulid::generate().to_string();
    let image = ulid::Ulid::generate().to_string();
    let src = format!("data:image/png;base64,{}", "A".repeat(375_000));
    let mut content = json!({"type":"image","attrs":{"src":src,"alt":"Original","width":100}});
    let (status, result) = send(&router, auth_post(&format!("/documents/{doc}/batch"), &owner,
        &json!({"base_seq":0,"ops":[
            {"type":"NodeCreated","node_id":root,"node_type":"form","pos":"a0","fields":{}},
            {"type":"NodeCreated","node_id":image,"node_type":"item","parent_id":root,"pos":"a0","fields":{"content":content}}
        ]}))).await;
    assert_eq!(status, StatusCode::OK, "{status}");
    assert_eq!(result["events"].as_array().unwrap().len(), 2);
    let mut seq = 2_i64;
    for (attribute, value) in [
        ("alt", json!("Revised alternative text")),
        ("width", json!(640)),
    ] {
        content["attrs"][attribute] = value;
        let (status, result) = send(&router, auth_post(&format!("/documents/{doc}/batch"), &owner,
            &json!({"base_seq":seq,"ops":[{"type":"FieldEdited","node_id":image,"field":"content","value":content}]}))).await;
        assert_eq!(status, StatusCode::OK, "image {attribute}: {status}");
        assert_eq!(result["events"].as_array().unwrap().len(), 1);
        seq += 1;
        let (bytes, payload): (i32, Value) = sqlx::query_as(
            "select octet_length(payload::text),payload from event where document_id=$1 and seq=$2",
        )
        .bind(doc)
        .bind(seq)
        .fetch_one(&pool)
        .await
        .unwrap();
        eprintln!("stored image {attribute} edit: {bytes} JSONB payload bytes");
        assert!(bytes > 375_000, "V1 still repeats the image: {bytes}");
        assert!(bytes < 376_024, "unexpected JSONB overhead: {bytes}");
        assert_eq!(payload["type"], "FieldEdited");
        assert_eq!(payload["value"], content);
        assert_eq!(payload["value"]["attrs"]["src"], src);
    }
    let (_, node) = send(
        &router,
        auth_post_get(&format!("/documents/{doc}/node/{image}"), &owner),
    )
    .await;
    assert_eq!(node["current_fields"]["content"], content);
    let (_, verification) = send(
        &router,
        auth_post_get(&format!("/documents/{doc}/verify"), &owner),
    )
    .await;
    assert_eq!(verification["ok"], true);
}

#[sqlx::test(migrations = "../../migrations")]
async fn rich_text_history_replays_compact_edits_and_rejects_stale_patches(pool: PgPool) {
    let router = app(test_state(pool.clone()));
    let owner = seed_identity(&pool, "richtext@example.test").await;
    let token = bearer(owner);
    let doc = create_document(&router, &token, "Research protocol").await;
    let node = ulid::Ulid::generate().to_string();
    let root = ulid::Ulid::generate().to_string();
    let content = json!({"type":"paragraph","content":[{"type":"text","text":"Research evidence. ".repeat(1200)}]});
    let (status, _) = send(&router, auth_post(&format!("/documents/{doc}/batch"), &token, &json!({"base_seq":0,"ops":[{"type":"NodeCreated","node_id":root,"node_type":"form","pos":"a","fields":{}},{"type":"NodeCreated","node_id":node,"node_type":"item","parent_id":root,"pos":"a","fields":{"content":content,"label":engine_core::richtext::plain_text(&content)}}]}))).await;
    assert_eq!(status, StatusCode::OK);
    let mut after = content.clone();
    after["content"][0]["marks"] = json!([{"type":"bold"}]);
    let (status, saved) = send(&router, auth_post(&format!("/documents/{doc}/batch"), &token, &json!({"base_seq":2,"ops":[{"type":"FieldEdited","node_id":node,"field":"content","value":after}]}))).await;
    assert_eq!(status, StatusCode::OK, "{saved}");
    assert_eq!(saved["events"][0]["type"], "RichTextPatched");
    let patch = saved["events"][0]["payload"].clone();
    let (status, _) = send(
        &router,
        auth_post(
            &format!("/documents/{doc}/batch"),
            &token,
            &json!({"base_seq":3,"ops":[patch]}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let replay = engine_core::snapshot::SnapshotEngine::new(pool.clone())
        .read_current_state(DocumentId(doc))
        .await
        .unwrap();
    assert_eq!(
        replay.nodes[&engine_shared::NodeId(node.clone())].current_fields["content"],
        after
    );
    let row: Value = sqlx::query_scalar("select current_fields from node where id=$1")
        .bind(&node)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(row["content"], after);
    engine_core::log::verify_chain(&pool, DocumentId(doc))
        .await
        .unwrap();
    let request = Request::builder()
        .uri(format!("/documents/{doc}/structure"))
        .header(header::AUTHORIZATION, &token)
        .body(Body::empty())
        .unwrap();
    let (status, analysis) = send(&router, request).await;
    assert_eq!(status, StatusCode::OK, "{analysis}");
    assert_eq!(analysis["through_seq"], 3);
    assert_eq!(analysis["blocks"][0]["analysis"]["formatting_runs"], 1);
    let stranger = bearer(seed_identity(&pool, "stranger@example.test").await);
    let request = Request::builder()
        .uri(format!("/documents/{doc}/structure"))
        .header(header::AUTHORIZATION, stranger)
        .body(Body::empty())
        .unwrap();
    assert_eq!(send(&router, request).await.0, StatusCode::FORBIDDEN);
}

#[sqlx::test(migrations = "../../migrations")]
async fn restore_rejects_live_nodes_atomically_and_restores_removed_choices(pool: PgPool) {
    let runtime = common::RuntimeDatabase::new(pool.clone()).await;
    let router = app(test_state(runtime.pool.clone()));
    let token = bearer(seed_identity(&runtime.pool, "restore@example.test").await);
    let doc = create_document(&router, &token, "Restore preconditions").await;
    let [root, item, choice] = std::array::from_fn(|_| ulid::Ulid::generate().to_string());
    let uri = format!("/documents/{doc}/batch");
    let setup = json!({"ops":[
        {"type":"NodeCreated","node_id":root,"node_type":"form","pos":"a0","fields":{"label":"Original"}},
        {"type":"NodeCreated","node_id":item,"node_type":"item","parent_id":root,"pos":"a0","fields":{}},
        {"type":"ChoiceAdded","node_id":item,"choice_id":choice,"fields":{"label":"Option"}}
    ]});
    let (status, body) = send(&router, auth_post(&uri, &token, &setup)).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    for id in [&root, &item, &choice] {
        let (status, body) = send(
            &router,
            auth_post(
                &uri,
                &token,
                &json!({"ops":[
                    {"type":"FieldEdited","node_id":root,"field":"label","value":"Must roll back"},
                    {"type":"NodeRestored","node_id":id}
                ]}),
            ),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
        assert!(body.to_string().contains("already live"), "{body}");
        let seq: i64 = sqlx::query_scalar("select max(seq) from event where document_id=$1")
            .bind(doc)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(seq, 3, "rejected restore must roll back the whole batch");
        let fields: Value = sqlx::query_scalar("select current_fields from node where id=$1")
            .bind(&root)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(fields["label"], "Original");
    }
    for op in [
        json!({"type":"ChoiceRemoved","node_id":item,"choice_id":choice}),
        json!({"type":"NodeRestored","node_id":choice}),
        json!({"type":"NodeDeleted","node_id":choice}),
        json!({"type":"NodeRestored","node_id":choice}),
    ] {
        let (status, body) = send(&router, auth_post(&uri, &token, &json!({"ops":[op]}))).await;
        assert_eq!(status, StatusCode::OK, "{body}");
    }
    let (status, body) = send(
        &router,
        auth_post(
            &uri,
            &token,
            &json!({"ops":[
                {"type":"NodeRestored","node_id":choice}
            ]}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    let (status, body) = send(&router, auth_post_get(&format!("/documents/{doc}"), &token)).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["state"]["nodes"][&choice]["id"], choice);
    assert_eq!(body["state"]["nodes"][&choice]["deleted"], false);
    runtime.close().await;
}

/// Every node-table write path, checked as the runtime login: only the removed-choice
/// encoding differs. Rows written now follow the fold, so null fields match replay.
#[sqlx::test(migrations = "../../migrations")]
async fn node_table_matches_verified_replay(pool: PgPool) {
    use engine_core::projection::{verify_projection, KNOWN_REMOVED_CHOICE};
    let router = app(test_state(pool.clone()));
    let token = bearer(seed_identity(&pool, "author@dynodoc.local").await);
    let doc = create_document(&router, &token, "Projection").await;
    let [root, section, question, notes, empty, keep, drop] =
        std::array::from_fn(|_| ulid::Ulid::generate().to_string());
    let before = json!({"type":"paragraph","content":[{"type":"text","text":"Plain notes"}]});
    let after =
        json!({"type":"paragraph","content":[{"type":"text","text":"Plain notes, revised"}]});
    for ops in [
        json!([
            {"type":"NodeCreated","node_id":root,"node_type":"form","pos":"a0","fields":{"title":"Survey"}},
            {"type":"NodeCreated","node_id":section,"node_type":"section","parent_id":root,"pos":"a0","fields":{}},
            {"type":"NodeCreated","node_id":question,"node_type":"item","parent_id":section,"pos":"a0","fields":{"label":"Age"},"var_name":"age"},
            {"type":"NodeCreated","node_id":notes,"node_type":"item","parent_id":section,"pos":"a1","fields":{"label":"Plain notes","content":before}},
            {"type":"NodeCreated","node_id":empty,"node_type":"item","parent_id":root,"pos":"a1","fields":null},
            {"type":"ChoiceAdded","node_id":question,"choice_id":keep,"fields":{"label":"Under 18"}},
            {"type":"ChoiceAdded","node_id":question,"choice_id":drop,"fields":{"label":"Unused"}},
        ]),
        json!([
            {"type":"FieldEdited","node_id":notes,"field":"content","value":after},
            {"type":"FieldEdited","node_id":question,"field":"var_name","value":"age_group"},
            {"type":"NodeMoved","node_id":notes,"new_parent_id":root,"new_pos":"a2"},
            {"type":"NodeDeleted","node_id":empty},
            {"type":"NodeRestored","node_id":empty},
            {"type":"ChoiceRemoved","node_id":question,"choice_id":drop},
        ]),
    ] {
        let uri = format!("/documents/{doc}/batch");
        let (status, body) = send(&router, auth_post(&uri, &token, &json!({ "ops": ops }))).await;
        assert_eq!(status, StatusCode::OK, "{body}");
    }

    let runtime = common::RuntimeDatabase::new(pool).await;
    let report = verify_projection(&runtime.pool, DocumentId(doc))
        .await
        .unwrap();
    runtime.close().await;
    let found: Vec<_> = report
        .differences
        .iter()
        .map(|d| (d.node_id.0.as_str(), d.field, d.known))
        .collect();
    assert_eq!(
        found,
        [(drop.as_str(), "deleted", Some(KNOWN_REMOVED_CHOICE))]
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn node_rows_follow_restored_choices_and_report_missing_rows(pool: PgPool) {
    use engine_core::projection::verify_projection;
    let router = app(test_state(pool.clone()));
    let token = bearer(seed_identity(&pool, "rows@example.test").await);
    let doc = create_document(&router, &token, "Rows").await;
    let [root, question, kept, lost] = std::array::from_fn(|_| ulid::Ulid::generate().to_string());
    let uri = format!("/documents/{doc}/batch");
    for ops in [
        json!([
            {"type":"NodeCreated","node_id":root,"node_type":"form","pos":"a0","fields":{}},
            {"type":"NodeCreated","node_id":question,"node_type":"item","parent_id":root,"pos":"a0","fields":{"label":"Q"}},
            {"type":"ChoiceAdded","node_id":question,"choice_id":kept,"fields":{"label":"Kept"}},
            {"type":"ChoiceAdded","node_id":question,"choice_id":lost,"fields":{"label":"Lost"}},
        ]),
        // Restoring a removed choice clears the table's deleted flag, as replay does.
        json!([
            {"type":"ChoiceRemoved","node_id":question,"choice_id":kept},
            {"type":"NodeRestored","node_id":kept},
        ]),
    ] {
        let (status, body) = send(&router, auth_post(&uri, &token, &json!({ "ops": ops }))).await;
        assert_eq!(status, StatusCode::OK, "{body}");
    }
    let deleted: bool = sqlx::query_scalar("select deleted from node where id=$1")
        .bind(&kept)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(!deleted);
    let report = verify_projection(&pool, DocumentId(doc)).await.unwrap();
    assert_eq!(report.differences, []);

    // A row the table lost (here moved to another document) is a server fault, not a panic.
    let other = create_document(&router, &token, "Elsewhere").await;
    sqlx::query("update node set document_id=$2 where id=$1")
        .bind(&lost)
        .bind(other)
        .execute(&pool)
        .await
        .unwrap();
    let before: i64 =
        sqlx::query_scalar("select coalesce(max(seq),0) from event where document_id=$1")
            .bind(doc)
            .fetch_one(&pool)
            .await
            .unwrap();
    let removal = json!({"ops":[
        {"type":"FieldEdited","node_id":question,"field":"label","value":"Must roll back"},
        {"type":"ChoiceRemoved","node_id":question,"choice_id":lost}
    ]});
    let (status, body) = send(&router, auth_post(&uri, &token, &removal)).await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR, "{body}");
    let edit =
        json!({"ops":[{"type":"FieldEdited","node_id":lost,"field":"label","value":"Edited"}]});
    let (status, body) = send(&router, auth_post(&uri, &token, &edit)).await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR, "{body}");
    let after: i64 =
        sqlx::query_scalar("select coalesce(max(seq),0) from event where document_id=$1")
            .bind(doc)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(after, before, "projection failures must append no event");
    let question_fields: Value = sqlx::query_scalar("select current_fields from node where id=$1")
        .bind(&question)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        question_fields["label"], "Q",
        "earlier batch writes roll back"
    );
    let fields: Value = sqlx::query_scalar("select current_fields from node where id=$1")
        .bind(&lost)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(fields["label"], "Lost", "another document stays untouched");
}

#[sqlx::test(migrations = "../../migrations")]
async fn removed_choices_reject_new_writes_atomically_until_restored(pool: PgPool) {
    use engine_core::{
        projection::verify_projection,
        snapshot::{merkle_root, SnapshotEngine},
    };
    let runtime = common::RuntimeDatabase::new(pool.clone()).await;
    let router = app(test_state(runtime.pool.clone()));
    let token = bearer(seed_identity(&pool, "choices@example.test").await);
    let doc = create_document(&router, &token, "Choices").await;
    let [root, question, choice] = std::array::from_fn(|_| ulid::Ulid::generate().to_string());
    let uri = format!("/documents/{doc}/batch");
    let before = json!({"type":"paragraph","content":[{"type":"text","text":"Original"}]});
    let after = json!({"type":"paragraph","content":[{"type":"text","text":"Changed"}]});
    let (status, body) = send(&router, auth_post(&uri, &token, &json!({"ops":[
        {"type":"NodeCreated","node_id":root,"node_type":"form","pos":"a0","fields":{}},
        {"type":"NodeCreated","node_id":question,"node_type":"item","parent_id":root,"pos":"a0","fields":{"label":"Q"}},
        {"type":"ChoiceAdded","node_id":question,"choice_id":choice,"fields":{"label":"C","content":before}}
    ]}))).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let snapshots = SnapshotEngine::new(runtime.pool.clone());
    let live = snapshots.read_current_state(DocumentId(doc)).await.unwrap();
    let (status, body) = send(
        &router,
        auth_post(
            &uri,
            &token,
            &json!({"ops":[
                {"type":"ChoiceRemoved","node_id":question,"choice_id":choice}
            ]}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let removed = snapshots.read_current_state(DocumentId(doc)).await.unwrap();
    assert_ne!(live, removed);
    // V1 hashes nodes, not the removed-choice set. Keep historical roots readable.
    assert_eq!(merkle_root(&live).unwrap(), merkle_root(&removed).unwrap());
    // The table marks the removed choice deleted; replay lists it as removed. Nothing else differs.
    let report = verify_projection(&runtime.pool, DocumentId(doc))
        .await
        .unwrap();
    assert_eq!(report.differences.len(), 1, "{:?}", report.differences);
    assert_eq!(
        report.differences[0].known,
        Some(engine_core::projection::KNOWN_REMOVED_CHOICE)
    );
    let patch = engine_core::richtext::make_patch(&before, &after).unwrap();
    for op in [
        json!({"type":"FieldEdited","node_id":choice,"field":"label","value":"Changed"}),
        json!({"type":"FieldEdited","node_id":choice,"field":"var_name","value":"renamed"}),
        json!({"type":"RichTextPatched","node_id":choice,"patch":patch}),
        json!({"type":"NodeMoved","node_id":choice,"new_parent_id":question,"new_pos":"b0"}),
        json!({"type":"ChoiceRemoved","node_id":question,"choice_id":choice}),
    ] {
        let (status, body) = send(&router, auth_post(&uri, &token, &json!({"ops":[
            {"type":"FieldEdited","node_id":question,"field":"label","value":"Must roll back"}, op
        ]}))).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
        let seq: i64 = sqlx::query_scalar("select max(seq) from event where document_id=$1")
            .bind(doc)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(seq, 4);
        assert_eq!(
            snapshots.read_current_state(DocumentId(doc)).await.unwrap(),
            removed
        );
        let label: Value =
            sqlx::query_scalar("select current_fields->'label' from node where id=$1")
                .bind(&question)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(label, "Q");
    }
    let (status, body) = send(
        &router,
        auth_post(
            &uri,
            &token,
            &json!({"ops":[
                {"type":"NodeRestored","node_id":choice},
                {"type":"FieldEdited","node_id":choice,"field":"label","value":"Restored"},
                {"type":"NodeMoved","node_id":choice,"new_parent_id":question,"new_pos":"b0"}
            ]}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(verify_projection(&runtime.pool, DocumentId(doc))
        .await
        .unwrap()
        .differences
        .is_empty());
    runtime.close().await;
}

/// The editors delete a question's whole subtree, removed choices included (state diffs and
/// the questionnaire editor list every child that is not tombstoned). Rejecting that would make
/// a question with a removed choice undeletable.
#[sqlx::test(migrations = "../../migrations")]
async fn a_question_with_a_removed_choice_can_still_be_deleted(pool: PgPool) {
    use engine_core::projection::{verify_projection, KNOWN_REMOVED_CHOICE};
    let runtime = common::RuntimeDatabase::new(pool.clone()).await;
    let router = app(test_state(runtime.pool.clone()));
    let token = bearer(seed_identity(&runtime.pool, "delete-choices@example.test").await);
    let doc = create_document(&router, &token, "Delete with removed choice").await;
    let [root, question, kept, removed] =
        std::array::from_fn(|_| ulid::Ulid::generate().to_string());
    let uri = format!("/documents/{doc}/batch");
    let (status, body) = send(&router, auth_post(&uri, &token, &json!({"ops":[
        {"type":"NodeCreated","node_id":root,"node_type":"form","pos":"a0","fields":{}},
        {"type":"NodeCreated","node_id":question,"node_type":"item","parent_id":root,"pos":"a0","fields":{"label":"Q"}},
        {"type":"ChoiceAdded","node_id":question,"choice_id":kept,"fields":{"label":"Kept"}},
        {"type":"ChoiceAdded","node_id":question,"choice_id":removed,"fields":{"label":"Removed"}},
        {"type":"ChoiceRemoved","node_id":question,"choice_id":removed}
    ]}))).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    // Children first, as the editor orders them: both choices, then the question.
    let (status, body) = send(
        &router,
        auth_post(
            &uri,
            &token,
            &json!({"ops":[
                {"type":"NodeDeleted","node_id":removed},
                {"type":"NodeDeleted","node_id":kept},
                {"type":"NodeDeleted","node_id":question}
            ]}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let deleted: Vec<(String, bool)> =
        sqlx::query_as("select id, deleted from node where document_id=$1 order by id")
            .bind(doc)
            .fetch_all(&pool)
            .await
            .unwrap();
    assert!(
        deleted
            .iter()
            .filter(|(id, _)| [&removed, &kept, &question].contains(&id))
            .all(|(_, deleted)| *deleted),
        "{deleted:?}"
    );
    // Tombstoning twice is still refused, and the refusal appends nothing.
    let (status, body) = send(
        &router,
        auth_post(
            &uri,
            &token,
            &json!({"ops":[{"type":"NodeDeleted","node_id":removed}]}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    let seq: i64 = sqlx::query_scalar("select max(seq) from event where document_id=$1")
        .bind(doc)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(seq, 8);
    let report = verify_projection(&runtime.pool, DocumentId(doc))
        .await
        .unwrap();
    assert!(
        report
            .differences
            .iter()
            .all(|d| d.known == Some(KNOWN_REMOVED_CHOICE)),
        "{:?}",
        report.differences
    );
    // Bringing the choice back is still one explicit restore, parent first.
    let (status, body) = send(
        &router,
        auth_post(
            &uri,
            &token,
            &json!({"ops":[
                {"type":"NodeRestored","node_id":question},
                {"type":"NodeRestored","node_id":removed}
            ]}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let report = verify_projection(&runtime.pool, DocumentId(doc))
        .await
        .unwrap();
    assert!(
        report
            .differences
            .iter()
            .all(|d| d.known == Some(KNOWN_REMOVED_CHOICE)),
        "{:?}",
        report.differences
    );
    runtime.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn reused_node_and_choice_ids_conflict_even_after_deletion_or_removal(pool: PgPool) {
    let runtime = common::RuntimeDatabase::new(pool.clone()).await;
    let router = app(test_state(runtime.pool.clone()));
    let token = bearer(seed_identity(&runtime.pool, "reuse@example.test").await);
    let doc = create_document(&router, &token, "Stable identity regression").await;
    let [root, question, choice] = std::array::from_fn(|_| ulid::Ulid::generate().to_string());
    let uri = format!("/documents/{doc}/batch");
    let (status, body) = send(&router, auth_post(&uri, &token, &json!({"ops":[
        {"type":"NodeCreated","node_id":root,"node_type":"form","pos":"a0","fields":{"label":"Original"}},
        {"type":"NodeCreated","node_id":question,"node_type":"item","parent_id":root,"pos":"a0","fields":{"label":"Question"}},
        {"type":"ChoiceAdded","node_id":question,"choice_id":choice,"fields":{"label":"Choice"}}
    ]}))).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    for (stage, expected_seq) in [3_i64, 4, 6].into_iter().enumerate() {
        let transition = match stage {
            0 => vec![],
            1 => vec![json!({"type":"NodeDeleted","node_id":choice})],
            _ => vec![
                json!({"type":"NodeRestored","node_id":choice}),
                json!({"type":"ChoiceRemoved","node_id":question,"choice_id":choice}),
            ],
        };
        if !transition.is_empty() {
            let (status, body) =
                send(&router, auth_post(&uri, &token, &json!({"ops":transition}))).await;
            assert_eq!(status, StatusCode::OK, "{body}");
        }
        for existing in [&root, &question, &choice] {
            for collision in [
                json!({"type":"NodeCreated","node_id":existing,"node_type":"form","pos":"new","fields":{}}),
                json!({"type":"ChoiceAdded","node_id":question,"choice_id":existing,"fields":{"label":"Replacement"}}),
            ] {
                let (status, body) = send(&router, auth_post(&uri, &token, &json!({"ops":[
                    {"type":"FieldEdited","node_id":root,"field":"label","value":"Must roll back"},
                    collision
                ]}))).await;
                assert_eq!(status, StatusCode::CONFLICT, "stage {stage}: {body}");
                let seq: i64 =
                    sqlx::query_scalar("select max(seq) from event where document_id=$1")
                        .bind(doc)
                        .fetch_one(&pool)
                        .await
                        .unwrap();
                assert_eq!(seq, expected_seq, "ID reuse must append no event");
                let fields: Value =
                    sqlx::query_scalar("select current_fields from node where id=$1")
                        .bind(&root)
                        .fetch_one(&pool)
                        .await
                        .unwrap();
                assert_eq!(fields["label"], "Original", "earlier writes must roll back");
            }
        }
    }
    let report = engine_core::projection::verify_projection(&runtime.pool, DocumentId(doc))
        .await
        .unwrap();
    assert!(
        report
            .differences
            .iter()
            .all(|d| d.known == Some(engine_core::projection::KNOWN_REMOVED_CHOICE)),
        "{:?}",
        report.differences
    );
    runtime.close().await;
}

fn test_state(pool: PgPool) -> AppState {
    AppState::new(
        pool,
        AuthConfig {
            paseto_key: KEY,
            service_key: None,
            token_ttl_seconds: 3600,
            public_base_url: "http://test.local".into(),
        },
    )
}

/// A live access token for `identity_id` (mints directly, bypassing the email flow).
fn bearer(identity_id: Uuid) -> String {
    format!("Bearer {}", token::mint(&KEY, identity_id, 3600).unwrap())
}

async fn send(router: &Router, req: Request<Body>) -> (StatusCode, Value) {
    let resp = router.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or(Value::Null)
    };
    (status, value)
}

fn get(uri: &str) -> Request<Body> {
    Request::builder().uri(uri).body(Body::empty()).unwrap()
}

fn auth_post(uri: &str, token: &str, body: &Value) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(uri)
        .header("content-type", "application/json")
        .header(header::AUTHORIZATION, token)
        .body(Body::from(serde_json::to_vec(body).unwrap()))
        .unwrap()
}

/// Seed an identity row and return its id.
async fn seed_identity(pool: &PgPool, email: &str) -> Uuid {
    auth_store::upsert_identity(pool, email, Some("Tester"))
        .await
        .unwrap()
        .id
        .0
}

/// Create a document (as Author) through the API and return its id.
async fn create_document(router: &Router, token: &str, title: &str) -> Uuid {
    let (status, body) = send(
        router,
        auth_post("/documents", token, &json!({ "title": title })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "create document failed: {body}");
    Uuid::parse_str(body["id"].as_str().unwrap()).unwrap()
}

// --- documents ------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn create_then_read_document(pool: PgPool) {
    let router = app(test_state(pool.clone()));
    let author = seed_identity(&pool, "author@dynodoc.local").await;
    let token = bearer(author);

    let doc_id = create_document(&router, &token, "Survey").await;

    // Public read: metadata + (empty) materialized state.
    let (status, body) = send(
        &router,
        auth_post_get(&format!("/documents/{doc_id}"), &token),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "get document failed: {body}");
    assert_eq!(body["title"], "Survey");
    assert!(body["state"]["nodes"].is_object());

    // Public list includes it.
    let (status, body) = send(&router, auth_post_get("/documents", &token)).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["items"]
        .as_array()
        .unwrap()
        .iter()
        .any(|d| d["id"] == doc_id.to_string()));
}

#[sqlx::test(migrations = "../../migrations")]
async fn create_document_requires_auth(pool: PgPool) {
    let router = app(test_state(pool));
    let (status, _) = send(
        &router,
        Request::builder()
            .method("POST")
            .uri("/documents")
            .header("content-type", "application/json")
            .body(Body::from(json!({ "title": "x" }).to_string()))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrations = "../../migrations")]
async fn anonymous_read_requires_auth(pool: PgPool) {
    let router = app(test_state(pool));
    let (status, _) = send(&router, get(&format!("/documents/{}", Uuid::new_v4()))).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

// --- write path + governance ----------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn new_node_ids_are_strict_at_each_wire_write_boundary(pool: PgPool) {
    let router = app(test_state(pool.clone()));
    let token = bearer(seed_identity(&pool, "strict-ids@example.test").await);
    let malformed = [
        String::new(),
        "<script>".into(),
        "X".repeat(100_000),
        "random".repeat(500),
        "01arz3ndektsv4rrffq69g5fav".into(),
        "8ZZZZZZZZZZZZZZZZZZZZZZZZZ".into(),
    ];
    for kind in ["node-create", "batch", "draft", "review", "suggest"] {
        let doc = create_document(&router, &token, "Synthetic ID boundary").await;
        let draft = if kind == "draft" {
            let (status, body) = send(
                &router,
                auth_post(
                    &format!("/documents/{doc}/drafts"),
                    &token,
                    &json!({"name":"Private"}),
                ),
            )
            .await;
            assert_eq!(status, StatusCode::OK, "{body}");
            body["id"].as_str().unwrap().to_owned()
        } else {
            Uuid::nil().to_string()
        };
        let valid = ulid::Ulid::generate().to_string();
        for (id, expected) in malformed
            .iter()
            .map(|id| (id, StatusCode::BAD_REQUEST))
            .chain(std::iter::once((&valid, StatusCode::OK)))
        {
            let op = json!({"type":"NodeCreated","node_id":id,"node_type":"form","pos":"a0","fields":{}});
            let (path, body) = match kind {
                "node-create" => (format!("/documents/{doc}/ops/node-create"), op.clone()),
                "batch" => (format!("/documents/{doc}/batch"), json!({"ops":[op]})),
                "draft" => (
                    format!("/documents/{doc}/drafts/{draft}"),
                    json!({"revision":0,"ops":[op]}),
                ),
                "review" => (
                    format!("/documents/{doc}/change-requests"),
                    json!({"name":"File change","base_seq":0,"ops":[op]}),
                ),
                _ => (
                    format!("/documents/{doc}/ops/suggest"),
                    json!({"detail":op}),
                ),
            };
            let (status, result) = send(&router, auth_post(&path, &token, &body)).await;
            assert_eq!(status, expected, "{kind}, id length {}: {result}", id.len());
            if expected == StatusCode::BAD_REQUEST {
                assert!(result.to_string().len() < 1024, "do not echo oversized IDs");
            }
        }
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn selection_ids_and_new_choice_ids_are_validated_without_partial_writes(pool: PgPool) {
    let router = app(test_state(pool.clone()));
    let token = bearer(seed_identity(&pool, "selection-ids@example.test").await);
    let doc = create_document(&router, &token, "Synthetic selected IDs").await;
    let (status, draft) = send(
        &router,
        auth_post(
            &format!("/documents/{doc}/drafts"),
            &token,
            &json!({"name":"Selection"}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{draft}");
    let draft = draft["id"].as_str().unwrap();
    for field in ["included_nodes", "dropped_nodes"] {
        for ids in [
            json!([""]),
            json!(["X".repeat(129)]),
            json!(vec![ulid::Ulid::generate().to_string(); 20_001]),
        ] {
            let mut body = json!({"action":"preview"});
            body[field] = ids;
            let (status, result) = send(
                &router,
                auth_post(
                    &format!("/documents/{doc}/drafts/{draft}/sync"),
                    &token,
                    &body,
                ),
            )
            .await;
            assert_eq!(status, StatusCode::BAD_REQUEST, "{field}: {result}");
        }
        // A selection names nodes that already exist, so a historical spelling is accepted.
        for id in [ulid::Ulid::generate().to_string(), "01NODE00000000".into()] {
            let mut body = json!({"action":"preview"});
            body[field] = json!([id]);
            let (status, result) = send(
                &router,
                auth_post(
                    &format!("/documents/{doc}/drafts/{draft}/sync"),
                    &token,
                    &body,
                ),
            )
            .await;
            assert_eq!(status, StatusCode::OK, "{result}");
        }
    }
    let root = ulid::Ulid::generate().to_string();
    let question = ulid::Ulid::generate().to_string();
    let (status,result)=send(&router,auth_post(&format!("/documents/{doc}/batch"),&token,&json!({"ops":[
        {"type":"NodeCreated","node_id":root,"node_type":"form","pos":"a0","fields":{}},
        {"type":"NodeCreated","node_id":question,"parent_id":root,"node_type":"item","pos":"a0","fields":{"label":"Question"}}
    ]}))).await;
    assert_eq!(status, StatusCode::OK, "{result}");
    for choice in ["bad".to_owned(), ulid::Ulid::generate().to_string()] {
        let (status,result)=send(&router,auth_post(&format!("/documents/{doc}/batch"),&token,&json!({"ops":[
            {"type":"ChoiceAdded","node_id":question,"choice_id":choice,"fields":{"label":"Choice"}}
        ]}))).await;
        assert_eq!(
            status,
            if choice == "bad" {
                StatusCode::BAD_REQUEST
            } else {
                StatusCode::OK
            },
            "{result}"
        );
    }
    let other = create_document(&router, &token, "Review selection").await;
    let review_id = ulid::Ulid::generate().to_string();
    let (status, review) = send(
        &router,
        auth_post(
            &format!("/documents/{other}/change-requests"),
            &token,
            &json!({"name":"Selected review","base_seq":0,"submit":true,"ops":[
                {"type":"NodeCreated","node_id":review_id,"node_type":"form","pos":"a0","fields":{}}
            ]}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{review}");
    let review = review["id"].as_str().unwrap();
    for (ids, expected) in [
        (json!([""]), StatusCode::BAD_REQUEST),
        (json!(["X".repeat(129)]), StatusCode::BAD_REQUEST),
        (
            json!(vec![review_id.clone(); 20_001]),
            StatusCode::BAD_REQUEST,
        ),
        // A historical spelling in the selection is accepted next to a canonical one.
        (json!([review_id, "01NODE00000000"]), StatusCode::OK),
    ] {
        let (status, result) = send(
            &router,
            auth_post(
                &format!("/documents/{other}/change-requests/{review}/merge"),
                &token,
                &json!({"team_seq":0,"revision":1,"included_nodes":ids,"override_rules":true}),
            ),
        )
        .await;
        assert_eq!(status, expected, "{result}");
    }
}

#[test]
fn historical_node_ids_still_validate_and_fold_without_rewriting() {
    let id = engine_shared::NodeId("01NODE00000000".into());
    let op = engine_shared::EventPayload::NodeCreated {
        node_id: id.clone(),
        node_type: engine_shared::NodeType::Form,
        parent_id: None,
        pos: "a0".into(),
        fields: json!({}),
        var_name: None,
    };
    let mut state = engine_core::materializer::DocumentState::default();
    engine_core::ops::validate_op(&state, &op).unwrap();
    engine_core::materializer::apply_payload(&mut state, &op).unwrap();
    assert_eq!(state.nodes[&id].id, id);
    assert_eq!(
        serde_json::to_value(&op).unwrap()["node_id"],
        "01NODE00000000"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn node_create_and_field_edit_round_trip(pool: PgPool) {
    let router = app(test_state(pool.clone()));
    let author = seed_identity(&pool, "author@dynodoc.local").await;
    let token = bearer(author);
    let doc_id = create_document(&router, &token, "Form").await;

    // Create a root form node.
    let (status, body) = send(
        &router,
        auth_post(
            &format!("/documents/{doc_id}/ops/node-create"),
            &token,
            &json!({ "node_type": "form", "pos": "a0", "fields": {} }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "node-create failed: {body}");
    let node_id = body["event"]["target_node_id"]
        .as_str()
        .unwrap()
        .to_string();

    // Edit a field of that node.
    let (status, body) = send(
        &router,
        auth_post(
            &format!("/documents/{doc_id}/ops/field-edit"),
            &token,
            &json!({ "node_id": node_id, "field": "title", "value": "Hello" }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "field-edit failed: {body}");

    // The materialized state reflects the edit.
    let (status, body) = send(
        &router,
        auth_post_get(&format!("/documents/{doc_id}/node/{node_id}"), &token),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["current_fields"]["title"], "Hello");
}

#[sqlx::test(migrations = "../../migrations")]
async fn auditor_cannot_write(pool: PgPool) {
    let router = app(test_state(pool.clone()));
    let author = seed_identity(&pool, "author@dynodoc.local").await;
    let auditor = seed_identity(&pool, "auditor@dynodoc.local").await;
    let doc_id = create_document(&router, &bearer(author), "Form").await;

    // Grant the auditor read-only access.
    auth_store::grant_access(&pool, DocumentId(doc_id), auditor, Role::Auditor)
        .await
        .unwrap();

    // An auditor's write is forbidden by the capability matrix.
    let (status, _) = send(
        &router,
        auth_post(
            &format!("/documents/{doc_id}/ops/node-create"),
            &bearer(auditor),
            &json!({ "node_type": "form", "pos": "a0" }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[sqlx::test(migrations = "../../migrations")]
async fn non_participant_is_forbidden(pool: PgPool) {
    let router = app(test_state(pool.clone()));
    let author = seed_identity(&pool, "author@dynodoc.local").await;
    let stranger = seed_identity(&pool, "stranger@dynodoc.local").await;
    let doc_id = create_document(&router, &bearer(author), "Form").await;

    let (status, _) = send(
        &router,
        auth_post(
            &format!("/documents/{doc_id}/ops/comment"),
            &bearer(stranger),
            &json!({ "body": "hi" }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

// --- proposal lifecycle ---------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn suggest_then_accept_applies_the_wrapped_op(pool: PgPool) {
    let router = app(test_state(pool.clone()));
    let author = seed_identity(&pool, "author@dynodoc.local").await;
    let reviewer = seed_identity(&pool, "reviewer@dynodoc.local").await;
    let token = bearer(author);
    let doc_id = create_document(&router, &token, "Form").await;
    auth_store::grant_access(&pool, DocumentId(doc_id), reviewer, Role::Reviewer)
        .await
        .unwrap();

    // Author creates a node.
    let (_, body) = send(
        &router,
        auth_post(
            &format!("/documents/{doc_id}/ops/node-create"),
            &token,
            &json!({ "node_type": "form", "pos": "a0", "fields": { "title": "Old" } }),
        ),
    )
    .await;
    let node_id = body["event"]["target_node_id"]
        .as_str()
        .unwrap()
        .to_string();

    // Reviewer suggests editing the title.
    let suggestion_id = "sug-1";
    let wrapped =
        json!({ "type": "FieldEdited", "node_id": node_id, "field": "title", "value": "New" });
    let (status, _) = send(
        &router,
        auth_post(
            &format!("/documents/{doc_id}/ops/suggest"),
            &bearer(reviewer),
            &json!({ "suggestion_id": suggestion_id, "detail": wrapped }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    // Author accepts; the wrapped edit becomes canonical.
    let (status, _) = send(
        &router,
        auth_post(
            &format!("/documents/{doc_id}/proposals/{suggestion_id}/accept"),
            &token,
            &json!({}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (_, body) = send(
        &router,
        auth_post_get(&format!("/documents/{doc_id}/node/{node_id}"), &token),
    )
    .await;
    assert_eq!(body["current_fields"]["title"], "New");

    let actors: Vec<(String, Uuid)> =
        sqlx::query_as("select type, actor_id from event where document_id=$1 order by seq")
            .bind(doc_id)
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(
        actors,
        vec![
            ("NodeCreated".into(), author),
            ("SuggestionProposed".into(), reviewer),
            ("FieldEdited".into(), author),
            ("SuggestionAccepted".into(), author),
        ]
    );
    assert!(engine_core::log::verify_chain(&pool, DocumentId(doc_id))
        .await
        .is_ok());
}

// --- four-card conflicts (stage 21) ---------------------------------------------

/// Seed a form node, then two pending suggestions editing its `title` to different
/// values. Returns `(doc_id, node_id, author_token, reviewer_id)`.
async fn seed_conflict(pool: &PgPool, router: &Router) -> (Uuid, String, String, Uuid) {
    let author = seed_identity(pool, "author@dynodoc.local").await;
    let reviewer = seed_identity(pool, "reviewer@dynodoc.local").await;
    let token = bearer(author);
    let doc_id = create_document(router, &token, "Form").await;
    auth_store::grant_access(pool, DocumentId(doc_id), reviewer, Role::Reviewer)
        .await
        .unwrap();

    let (_, body) = send(
        router,
        auth_post(
            &format!("/documents/{doc_id}/ops/node-create"),
            &token,
            &json!({ "node_type": "form", "pos": "a0", "fields": { "title": "Old" } }),
        ),
    )
    .await;
    let node_id = body["event"]["target_node_id"]
        .as_str()
        .unwrap()
        .to_string();

    // Two concurrent pending suggestions on the same field, different values.
    for (sid, value) in [("sug-foo", "Foo"), ("sug-bar", "Bar")] {
        let wrapped =
            json!({ "type": "FieldEdited", "node_id": node_id, "field": "title", "value": value });
        let (status, _) = send(
            router,
            auth_post(
                &format!("/documents/{doc_id}/ops/suggest"),
                &bearer(reviewer),
                &json!({ "suggestion_id": sid, "detail": wrapped }),
            ),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
    }
    (doc_id, node_id, token, reviewer)
}

#[sqlx::test(migrations = "../../migrations")]
async fn conflicts_list_groups_two_pending_edits_on_one_field(pool: PgPool) {
    let router = app(test_state(pool.clone()));
    let (doc_id, node_id, token, _reviewer) = seed_conflict(&pool, &router).await;

    let (status, body) = send(
        &router,
        auth_post_get(&format!("/documents/{doc_id}/conflicts"), &token),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "list conflicts failed: {body}");
    let items = body["items"].as_array().unwrap();
    assert_eq!(items.len(), 1, "exactly one conflict");
    let c = &items[0];
    assert_eq!(c["node_id"], node_id);
    assert_eq!(c["field"], "title");
    assert_eq!(c["ancestor"], "Old");
    let options = c["options"].as_array().unwrap();
    assert_eq!(options.len(), 2);
    let values: Vec<&str> = options
        .iter()
        .map(|o| o["value"].as_str().unwrap())
        .collect();
    assert!(values.contains(&"Foo") && values.contains(&"Bar"));
    // Options are attributed to the proposing actor.
    assert!(options.iter().all(|o| o["actor"].is_string()));
}

#[sqlx::test(migrations = "../../migrations")]
async fn resolve_accept_makes_chosen_value_canonical_and_rejects_others(pool: PgPool) {
    let router = app(test_state(pool.clone()));
    let (doc_id, node_id, token, reviewer) = seed_conflict(&pool, &router).await;

    let (_, list) = send(
        &router,
        auth_post_get(&format!("/documents/{doc_id}/conflicts"), &token),
    )
    .await;
    let conflict_id = list["items"][0]["conflict_id"]
        .as_str()
        .unwrap()
        .to_string();

    // Accept the "Foo" suggestion.
    let (status, body) = send(
        &router,
        auth_post(
            &format!("/documents/{doc_id}/conflicts/{conflict_id}/resolve"),
            &token,
            &json!({ "accept_suggestion_id": "sug-foo" }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "resolve failed: {body}");
    assert!(body["events"].as_array().unwrap().len() >= 2);

    let resolver: Uuid =
        sqlx::query_scalar("select actor_id from event where document_id=$1 and seq=1")
            .bind(doc_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_ne!(resolver, reviewer);
    let actors: Vec<(String, Uuid)> = sqlx::query_as(
        "select type,actor_id from event where document_id=$1 and seq>3 order by seq",
    )
    .bind(doc_id)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        actors,
        vec![
            ("FieldEdited".into(), resolver),
            ("SuggestionAccepted".into(), resolver),
            ("SuggestionRejected".into(), resolver),
        ]
    );
    assert!(body["events"]
        .as_array()
        .unwrap()
        .iter()
        .all(|event| event["actor_id"] == resolver.to_string()));
    assert!(engine_core::log::verify_chain(&pool, DocumentId(doc_id))
        .await
        .is_ok());

    // The accepted value is now canonical.
    let (_, node) = send(
        &router,
        auth_post_get(&format!("/documents/{doc_id}/node/{node_id}"), &token),
    )
    .await;
    assert_eq!(node["current_fields"]["title"], "Foo");

    // Suggestion states: foo accepted, bar rejected (superseded).
    let (_, detail) = send(
        &router,
        auth_post_get(&format!("/documents/{doc_id}"), &token),
    )
    .await;
    let suggestions = &detail["state"]["suggestions"];
    assert_eq!(suggestions["sug-foo"]["accepted"], true);
    assert_eq!(
        suggestions["sug-bar"]["rejected_reason"],
        "superseded by conflict resolution"
    );

    // The conflict is gone.
    let (_, list2) = send(
        &router,
        auth_post_get(&format!("/documents/{doc_id}/conflicts"), &token),
    )
    .await;
    assert!(list2["items"].as_array().unwrap().is_empty());
}

#[sqlx::test(migrations = "../../migrations")]
async fn resolve_custom_value_supersedes_all_suggestions(pool: PgPool) {
    let router = app(test_state(pool.clone()));
    let (doc_id, node_id, token, _reviewer) = seed_conflict(&pool, &router).await;

    let (_, list) = send(
        &router,
        auth_post_get(&format!("/documents/{doc_id}/conflicts"), &token),
    )
    .await;
    let conflict_id = list["items"][0]["conflict_id"]
        .as_str()
        .unwrap()
        .to_string();

    let (status, _) = send(
        &router,
        auth_post(
            &format!("/documents/{doc_id}/conflicts/{conflict_id}/resolve"),
            &token,
            &json!({ "custom_value": "Compromise" }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (_, node) = send(
        &router,
        auth_post_get(&format!("/documents/{doc_id}/node/{node_id}"), &token),
    )
    .await;
    assert_eq!(node["current_fields"]["title"], "Compromise");

    // Every pending suggestion in the group is now rejected.
    let (_, detail) = send(
        &router,
        auth_post_get(&format!("/documents/{doc_id}"), &token),
    )
    .await;
    let suggestions = &detail["state"]["suggestions"];
    assert_eq!(
        suggestions["sug-foo"]["rejected_reason"],
        "superseded by conflict resolution"
    );
    assert_eq!(
        suggestions["sug-bar"]["rejected_reason"],
        "superseded by conflict resolution"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn resolve_requires_author(pool: PgPool) {
    let router = app(test_state(pool.clone()));
    let (doc_id, _node_id, token, reviewer) = seed_conflict(&pool, &router).await;

    let (_, list) = send(
        &router,
        auth_post_get(&format!("/documents/{doc_id}/conflicts"), &token),
    )
    .await;
    let conflict_id = list["items"][0]["conflict_id"]
        .as_str()
        .unwrap()
        .to_string();

    // A reviewer (non-author) cannot resolve.
    let (status, _) = send(
        &router,
        auth_post(
            &format!("/documents/{doc_id}/conflicts/{conflict_id}/resolve"),
            &bearer(reviewer),
            &json!({ "accept_suggestion_id": "sug-foo" }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[sqlx::test(migrations = "../../migrations")]
async fn resolve_unknown_conflict_id_is_404(pool: PgPool) {
    let router = app(test_state(pool.clone()));
    let (doc_id, _node_id, token, _reviewer) = seed_conflict(&pool, &router).await;

    let (status, _) = send(
        &router,
        auth_post(
            &format!("/documents/{doc_id}/conflicts/bm90LXJlYWw/resolve"),
            &token,
            &json!({ "accept_suggestion_id": "sug-foo" }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[sqlx::test(migrations = "../../migrations")]
async fn single_suggestion_is_not_a_conflict(pool: PgPool) {
    let router = app(test_state(pool.clone()));
    let author = seed_identity(&pool, "author@dynodoc.local").await;
    let token = bearer(author);
    let doc_id = create_document(&router, &token, "Form").await;

    let (_, body) = send(
        &router,
        auth_post(
            &format!("/documents/{doc_id}/ops/node-create"),
            &token,
            &json!({ "node_type": "form", "pos": "a0", "fields": { "title": "Old" } }),
        ),
    )
    .await;
    let node_id = body["event"]["target_node_id"].as_str().unwrap();

    let wrapped =
        json!({ "type": "FieldEdited", "node_id": node_id, "field": "title", "value": "Solo" });
    send(
        &router,
        auth_post(
            &format!("/documents/{doc_id}/ops/suggest"),
            &token,
            &json!({ "suggestion_id": "only", "detail": wrapped }),
        ),
    )
    .await;

    let (status, body) = send(
        &router,
        auth_post_get(&format!("/documents/{doc_id}/conflicts"), &token),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["items"].as_array().unwrap().is_empty());
}

// --- idempotency ----------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn idempotency_key_dedupes_writes(pool: PgPool) {
    let router = app(test_state(pool.clone()));
    let author = seed_identity(&pool, "author@dynodoc.local").await;
    let token = bearer(author);
    let doc_id = create_document(&router, &token, "Form").await;

    let body = json!({ "node_type": "form", "pos": "a0", "node_id": "01ARZ3NDEKTSV4RRFFQ69G5FAV" });
    let req = || {
        Request::builder()
            .method("POST")
            .uri(format!("/documents/{doc_id}/ops/node-create"))
            .header("content-type", "application/json")
            .header(header::AUTHORIZATION, &token)
            .header("idempotency-key", "key-abc")
            .body(Body::from(body.to_string()))
            .unwrap()
    };

    let (s1, b1) = send(&router, req()).await;
    let (s2, b2) = send(&router, req()).await;
    assert_eq!(s1, StatusCode::OK);
    assert_eq!(s2, StatusCode::OK);
    // Same response both times.
    assert_eq!(b1, b2);

    // Exactly one event was appended (the replay did not re-execute).
    let count: i64 = sqlx::query_scalar(
        "select count(*) from event where document_id = $1 and type = 'NodeCreated'",
    )
    .bind(doc_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(count, 1, "idempotent replay must not duplicate the event");

    // A response stored before the stricter boundary remains replayable during
    // the existing idempotency TTL, even when its historical request ID is old.
    let path = format!("/documents/{doc_id}/ops/node-create");
    let cached = json!({"historical_response": true});
    sqlx::query("insert into idempotency_key(identity_id,idempotency_key,request_path,response_status,response_body) values($1,'legacy-key',$2,200,$3)")
        .bind(author).bind(&path).bind(&cached).execute(&pool).await.unwrap();
    let request = Request::builder()
        .method("POST")
        .uri(&path)
        .header("content-type", "application/json")
        .header(header::AUTHORIZATION, &token)
        .header("idempotency-key", "legacy-key")
        .body(Body::from(
            json!({"node_type":"form","pos":"a0","node_id":"legacy-node"}).to_string(),
        ))
        .unwrap();
    let (status, replay) = send(&router, request).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(replay, cached);
    let after: i64 = sqlx::query_scalar("select count(*) from event where document_id=$1")
        .bind(doc_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(after, 1);
}

// --- pagination -----------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn events_endpoint_paginates_with_cursor(pool: PgPool) {
    let router = app(test_state(pool.clone()));
    let author = seed_identity(&pool, "author@dynodoc.local").await;
    let token = bearer(author);
    let doc_id = create_document(&router, &token, "Form").await;

    // One form node, then several comments (cheap events that need no new node row).
    send(
        &router,
        auth_post(
            &format!("/documents/{doc_id}/ops/node-create"),
            &token,
            &json!({ "node_type": "form", "pos": "a0" }),
        ),
    )
    .await;
    for i in 0..4 {
        send(
            &router,
            auth_post(
                &format!("/documents/{doc_id}/ops/comment"),
                &token,
                &json!({ "body": format!("c{i}") }),
            ),
        )
        .await;
    }

    // First page of 2.
    let (status, page1) = send(
        &router,
        auth_post_get(&format!("/documents/{doc_id}/events?limit=2"), &token),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(page1["items"].as_array().unwrap().len(), 2);
    let cursor = page1["next_cursor"].as_str().unwrap().to_string();

    // Next page continues strictly after the cursor (no overlap).
    let (status, page2) = send(
        &router,
        auth_post_get(
            &format!("/documents/{doc_id}/events?limit=2&cursor={cursor}"),
            &token,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let first_seq_p2 = page2["items"][0]["seq"].as_i64().unwrap();
    let last_seq_p1 = page1["items"][1]["seq"].as_i64().unwrap();
    assert!(first_seq_p2 > last_seq_p1, "cursor page must not overlap");
    let (_, final_page) = send(
        &router,
        auth_post_get(
            &format!("/documents/{doc_id}/events?limit=2&from_seq=4"),
            &token,
        ),
    )
    .await;
    assert_eq!(final_page["items"].as_array().unwrap().len(), 2);
    assert!(
        final_page["next_cursor"].is_null(),
        "an exactly full final page has no successor"
    );
    let cursor = engine_api::pagination::Cursor {
        last_seq: i64::MAX,
        last_id: String::new(),
    }
    .encode();
    let (status, terminal) = send(
        &router,
        auth_post_get(
            &format!("/documents/{doc_id}/events?cursor={cursor}"),
            &token,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(terminal["items"].as_array().unwrap().is_empty());
}

#[sqlx::test(migrations = "../../migrations")]
async fn sse_pages_backfill_then_live_events_without_gaps_and_honors_resume_header(pool: PgPool) {
    let state = test_state(pool.clone());
    let router = app(state.clone());
    let author = seed_identity(&pool, "paged-stream@example.test").await;
    let token = bearer(author);
    let doc = create_document(&router, &token, "Paged stream").await;
    let mut tx = pool.begin().await.unwrap();
    let payload = engine_shared::EventPayload::CommentAdded {
        node_id: None,
        body: "comment".into(),
    };
    for _ in 0..205 {
        engine_core::log::append_in_tx(
            &mut tx,
            DocumentId(doc),
            &payload,
            engine_shared::IdentityId(author),
        )
        .await
        .unwrap();
    }
    tx.commit().await.unwrap();
    let page = engine_core::log::read_range_page(&pool, DocumentId(doc), 1, i64::MAX, 7)
        .await
        .unwrap();
    assert_eq!(page.len(), 7);
    for since in ["-1", "206", "9223372036854775807"] {
        assert_eq!(
            send(
                &router,
                auth_post_get(
                    &format!("/documents/{doc}/stream?since_seq={since}"),
                    &token
                )
            )
            .await
            .0,
            StatusCode::BAD_REQUEST
        );
    }
    let mut req = auth_post_get(&format!("/documents/{doc}/stream"), &token);
    req.headers_mut()
        .insert("last-event-id", "2".parse().unwrap());
    let response = router.clone().oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let mut stream = response.into_body().into_data_stream();
    assert!(
        String::from_utf8_lossy(&stream.next().await.unwrap().unwrap()).contains(": connected")
    );
    // Append after the pinned backfill head, before consuming its three pages.
    let mut tx = pool.begin().await.unwrap();
    let live = engine_core::log::append_in_tx(
        &mut tx,
        DocumentId(doc),
        &payload,
        engine_shared::IdentityId(author),
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    state.subscriptions.publish(DocumentId(doc), live);
    for seq in 3..=206 {
        let frame = tokio::time::timeout(std::time::Duration::from_secs(2), stream.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        let text = String::from_utf8_lossy(&frame);
        assert!(
            text.lines().any(|line| line == format!("id: {seq}")),
            "{text}"
        );
    }
}

/// A GET with a bearer token (the events endpoint is `[auth]`).
fn auth_post_get(uri: &str, token: &str) -> Request<Body> {
    Request::builder()
        .uri(uri)
        .header(header::AUTHORIZATION, token)
        .body(Body::empty())
        .unwrap()
}

fn assert_private_node_conflict(status: StatusCode, body: &Value, foreign: Uuid, node: &str) {
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    let response = body.to_string();
    assert!(!response.contains(&foreign.to_string()));
    assert!(!response.contains(node));
}

/// A person who cannot read the other document learns no more from the conflict than one who
/// can: the answer is the same status and body, and names neither the document, its title,
/// its owner nor its text. (That the ID is taken is unavoidable: IDs are global and random.)
#[sqlx::test(migrations = "../../migrations")]
async fn node_id_conflict_is_identical_for_readable_and_unreadable_documents(pool: PgPool) {
    let runtime = common::RuntimeDatabase::new(pool).await;
    let pool = runtime.pool.clone();
    let router = app(test_state(pool.clone()));
    let owner = bearer(seed_identity(&pool, "private-owner@example.test").await);
    let outsider = bearer(seed_identity(&pool, "private-outsider@example.test").await);
    let private = create_document(&router, &owner, "Confidential board minutes").await;
    let readable = create_document(&router, &outsider, "Outsider reference file").await;
    let target = create_document(&router, &outsider, "Outsider working file").await;
    let secret = ulid::Ulid::generate().to_string();
    let open = ulid::Ulid::generate().to_string();
    let create = |id: &str, text: &str| json!({"type":"NodeCreated","node_id":id,"node_type":"form","pos":"a0","fields":{"label":text}});
    for (doc, token, id, text) in [
        (private, &owner, &secret, "Confidential board text"),
        (readable, &outsider, &open, "Outsider text"),
    ] {
        let (status, body) = send(
            &router,
            auth_post(
                &format!("/documents/{doc}/batch"),
                token,
                &json!({"ops":[create(id, text)]}),
            ),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
    }
    // The outsider really cannot read the private file.
    let (status, _) = send(
        &router,
        auth_post_get(&format!("/documents/{private}/node/{secret}"), &outsider),
    )
    .await;
    assert!(matches!(
        status,
        StatusCode::FORBIDDEN | StatusCode::NOT_FOUND
    ));
    let mut answers = Vec::new();
    for id in [&secret, &open] {
        for (suffix, body) in [
            ("ops/node-create", create(id, "Reused")),
            ("batch", json!({"ops":[create(id, "Reused")]})),
            (
                "change-requests",
                json!({"name":"Reuse","base_seq":0,"ops":[create(id, "Reused")]}),
            ),
        ] {
            let (status, body) = send(
                &router,
                auth_post(&format!("/documents/{target}/{suffix}"), &outsider, &body),
            )
            .await;
            assert_eq!(status, StatusCode::CONFLICT, "{suffix}: {body}");
            let text = body.to_string();
            for leak in [
                private.to_string(),
                readable.to_string(),
                secret.clone(),
                open.clone(),
                "Confidential".into(),
                "private-owner".into(),
            ] {
                assert!(!text.contains(&leak), "{suffix} leaks {leak}: {text}");
            }
            answers.push((suffix, body));
        }
    }
    for pair in answers.chunks(3).collect::<Vec<_>>().windows(2) {
        assert_eq!(pair[0], pair[1], "conflict answers differ by readability");
    }
    let events: i64 = sqlx::query_scalar("select count(*) from event where document_id=$1")
        .bind(target)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(events, 0);
    runtime.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn node_id_from_another_document_is_conflict(pool: PgPool) {
    let runtime = common::RuntimeDatabase::new(pool).await;
    let pool = runtime.pool.clone();
    let router = app(test_state(pool.clone()));
    let owner = bearer(seed_identity(&pool, "collision-owner@example.test").await);
    let foreign = create_document(&router, &owner, "Synthetic foreign file").await;
    let target = create_document(&router, &owner, "Synthetic target file").await;
    let node = ulid::Ulid::generate().to_string();
    let op = json!({"type":"NodeCreated","node_id":node,"node_type":"form","pos":"a0","fields":{}});
    // A private draft saved before another file reserves the ID must still fail
    // at the authoritative insert when it is shared later.
    let (_, draft) = send(
        &router,
        auth_post(
            &format!("/documents/{target}/drafts"),
            &owner,
            &json!({"name":"Private before collision"}),
        ),
    )
    .await;
    let draft_uri = format!(
        "/documents/{target}/drafts/{}",
        draft["id"].as_str().unwrap()
    );
    let (status, result) = send(
        &router,
        auth_post(
            &draft_uri,
            &owner,
            &json!({"revision":0,"ops":[op.clone()]}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{result}");
    assert_eq!(
        send(
            &router,
            auth_post(
                &format!("/documents/{foreign}/ops/node-create"),
                &owner,
                &op
            )
        )
        .await
        .0,
        StatusCode::OK
    );
    for (suffix, body) in [
        ("ops/node-create", op.clone()),
        ("batch", json!({"ops":[op]})),
    ] {
        let (status, body) = send(
            &router,
            auth_post(&format!("/documents/{target}/{suffix}"), &owner, &body),
        )
        .await;
        assert_private_node_conflict(status, &body, foreign, &node);
    }
    let root = ulid::Ulid::generate().to_string();
    let item = ulid::Ulid::generate().to_string();
    let (status, body) = send(&router, auth_post(&format!("/documents/{target}/batch"), &owner,
        &json!({"ops":[
            {"type":"NodeCreated","node_id":root,"node_type":"form","pos":"a0","fields":{}},
            {"type":"NodeCreated","node_id":item,"node_type":"item","parent_id":root,"pos":"a0","fields":{"type":"select_one"}},
            {"type":"ChoiceAdded","node_id":item,"choice_id":node,"fields":{"label":"One"}}
        ]}))).await;
    assert_private_node_conflict(status, &body, foreign, &node);
    let sync = format!("{draft_uri}/sync");
    let (_, preview) = send(
        &router,
        auth_post(&sync, &owner, &json!({"action":"preview"})),
    )
    .await;
    let (status, body) = send(&router, auth_post(&sync, &owner,
        &json!({"action":"share","team_seq":preview["team_seq"],"revision":preview["revision"],"included_nodes":[node],"override_rules":true}))).await;
    assert_private_node_conflict(status, &body, foreign, &node);
    let events: i64 = sqlx::query_scalar("select count(*) from event where document_id=$1")
        .bind(target)
        .fetch_one(&pool)
        .await
        .unwrap();
    let nodes: i64 = sqlx::query_scalar("select count(*) from node where document_id=$1")
        .bind(target)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!((events, nodes), (0, 0));
    let actual: Uuid = sqlx::query_scalar("select document_id from node where id=$1")
        .bind(node)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(actual, foreign);
}

#[sqlx::test(migrations = "../../migrations")]
async fn private_uploads_check_global_ids_before_storing_edits(pool: PgPool) {
    let router = app(test_state(pool.clone()));
    let token = bearer(seed_identity(&pool, "early-collision@example.test").await);
    let foreign = create_document(&router, &token, "Synthetic source").await;
    let target = create_document(&router, &token, "Synthetic destination").await;
    let node = ulid::Ulid::generate().to_string();
    let op = json!({"type":"NodeCreated","node_id":node,"node_type":"form","pos":"a0","fields":{}});
    assert_eq!(
        send(
            &router,
            auth_post(
                &format!("/documents/{foreign}/ops/node-create"),
                &token,
                &op
            )
        )
        .await
        .0,
        StatusCode::OK
    );
    let (status, body) = send(
        &router,
        auth_post(
            &format!("/documents/{target}/change-requests"),
            &token,
            &json!({"name":"Collision","base_seq":0,"ops":[op.clone()]}),
        ),
    )
    .await;
    assert_private_node_conflict(status, &body, foreign, &node);
    let (_, draft) = send(
        &router,
        auth_post(
            &format!("/documents/{target}/drafts"),
            &token,
            &json!({"name":"Private"}),
        ),
    )
    .await;
    let (status, body) = send(
        &router,
        auth_post(
            &format!(
                "/documents/{target}/drafts/{}",
                draft["id"].as_str().unwrap()
            ),
            &token,
            &json!({"revision":0,"ops":[op]}),
        ),
    )
    .await;
    assert_private_node_conflict(status, &body, foreign, &node);
    let (count, revision): (i64, i64) =
        sqlx::query_as("select count(*),max(revision) from workspace_draft where document_id=$1")
            .bind(target)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!((count, revision), (1, 0));
    let edits: i64 = sqlx::query_scalar("select count(*) from draft_edit e join workspace_draft d on d.id=e.draft_id where d.document_id=$1")
        .bind(target).fetch_one(&pool).await.unwrap();
    assert_eq!(edits, 0);
}

#[sqlx::test(migrations = "../../migrations")]
async fn concurrent_global_id_insert_returns_conflict_after_waiting(pool: PgPool) {
    use engine_shared::{EventPayload, IdentityId, NodeId, NodeType};
    let runtime = common::RuntimeDatabase::new(pool).await;
    let pool = runtime.pool.clone();
    let router = app(test_state(pool.clone()));
    let actor = seed_identity(&pool, "id-race@example.test").await;
    let token = bearer(actor);
    let foreign = create_document(&router, &token, "Synthetic first writer").await;
    let target = create_document(&router, &token, "Synthetic second writer").await;
    let node = ulid::Ulid::generate().to_string();
    let mut first = pool.begin().await.unwrap();
    let pid: i32 = sqlx::query_scalar("select pg_backend_pid()")
        .fetch_one(&mut *first)
        .await
        .unwrap();
    sqlx::query(
        "insert into node(id,document_id,type,pos,current_fields) values($1,$2,'form','a0','{}')",
    )
    .bind(&node)
    .bind(foreign)
    .execute(&mut *first)
    .await
    .unwrap();
    engine_core::log::append_in_tx(
        &mut first,
        DocumentId(foreign),
        &EventPayload::NodeCreated {
            node_id: NodeId(node.clone()),
            node_type: NodeType::Form,
            parent_id: None,
            pos: "a0".into(),
            fields: json!({}),
            var_name: None,
        },
        IdentityId(actor),
    )
    .await
    .unwrap();
    let body = json!({"node_type":"form","node_id":node,"pos":"a0"});
    let request = auth_post(
        &format!("/documents/{target}/ops/node-create"),
        &token,
        &body,
    );
    let writer = tokio::spawn(async move { send(&router, request).await });
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            let waiting: bool = sqlx::query_scalar(
                "select exists(select 1 from pg_stat_activity where $1=any(pg_blocking_pids(pid)))",
            )
            .bind(pid)
            .fetch_one(&pool)
            .await
            .unwrap();
            if waiting {
                break;
            }
            assert!(
                !writer.is_finished(),
                "second writer did not wait for the primary key"
            );
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    first.commit().await.unwrap();
    let (status, body) = writer.await.unwrap();
    assert_private_node_conflict(status, &body, foreign, &node);
    let count: i64 = sqlx::query_scalar("select count(*) from event where document_id=$1")
        .bind(target)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
}

#[sqlx::test(migrations = "../../migrations")]
async fn another_unique_constraint_keeps_its_existing_error_mapping(pool: PgPool) {
    // This index exists only in this isolated fixture, to exercise a different
    // SQLSTATE 23505 without changing production constraints.
    sqlx::query(
        "create unique index synthetic_other_unique on node(var_name) where var_name is not null",
    )
    .execute(&pool)
    .await
    .unwrap();
    let router = app(test_state(pool.clone()));
    let token = bearer(seed_identity(&pool, "other-constraint@example.test").await);
    let foreign = create_document(&router, &token, "Synthetic other index").await;
    let target = create_document(&router, &token, "Synthetic failed insert").await;
    let path = |id| format!("/documents/{id}/ops/node-create");
    assert_eq!(
        send(
            &router,
            auth_post(
                &path(foreign),
                &token,
                &json!({"node_type":"form","pos":"a0","var_name":"shared_name"})
            )
        )
        .await
        .0,
        StatusCode::OK
    );
    let (status, body) = send(
        &router,
        auth_post(
            &path(target),
            &token,
            &json!({"node_type":"form","pos":"a0","var_name":"shared_name"}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR, "{body}");
    let count: i64 = sqlx::query_scalar("select count(*) from event where document_id=$1")
        .bind(target)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
}

// --- audit ----------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn verify_requires_membership_and_is_ok_for_a_clean_log(pool: PgPool) {
    let router = app(test_state(pool.clone()));
    let author = seed_identity(&pool, "author@dynodoc.local").await;
    let token = bearer(author);
    let doc_id = create_document(&router, &token, "Form").await;
    send(
        &router,
        auth_post(
            &format!("/documents/{doc_id}/ops/node-create"),
            &token,
            &json!({ "node_type": "form", "pos": "a0" }),
        ),
    )
    .await;

    let (status, body) = send(
        &router,
        auth_post_get(&format!("/documents/{doc_id}/verify"), &token),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["ok"], true);
}

// --- SSE ------------------------------------------------------------------------

#[sqlx::test(migrations = "../../migrations")]
async fn sse_delivers_a_new_event_within_100ms(pool: PgPool) {
    let router = app(test_state(pool.clone()));
    let author = seed_identity(&pool, "author@dynodoc.local").await;
    let token = bearer(author);
    let doc_id = create_document(&router, &token, "Form").await;

    // Open the stream (no backfill yet — the document has no events).
    let stream_req = Request::builder()
        .uri(format!("/documents/{doc_id}/stream"))
        .header(header::AUTHORIZATION, &token)
        .body(Body::empty())
        .unwrap();
    let resp = router.clone().oneshot(stream_req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(
        resp.headers().get(header::CACHE_CONTROL).unwrap(),
        "no-cache, no-transform"
    );
    let mut body = resp.into_body().into_data_stream();

    // An idle subscription must flush immediately without waiting for a write or
    // the 30-second heartbeat. The acknowledgement carries no event/cursor.
    let ready = tokio::time::timeout(std::time::Duration::from_secs(1), body.next())
        .await
        .expect("idle SSE subscription must acknowledge immediately")
        .expect("stream yielded an acknowledgement")
        .expect("acknowledgement is not an error");
    let ready = String::from_utf8_lossy(&ready);
    assert!(ready.starts_with(": connected"));
    assert!(!ready.contains("data:") && !ready.contains("id:"));

    // Append an event from "another connection".
    let writer = router.clone();
    let write_token = token.clone();
    tokio::spawn(async move {
        send(
            &writer,
            auth_post(
                &format!("/documents/{doc_id}/ops/node-create"),
                &write_token,
                &json!({ "node_type": "form", "pos": "a0" }),
            ),
        )
        .await;
    });

    // The subscriber receives the event frame within 100ms.
    let frame = tokio::time::timeout(std::time::Duration::from_millis(100), body.next())
        .await
        .expect("SSE frame should arrive within 100ms")
        .expect("stream yielded a frame")
        .expect("frame is not an error");
    let text = String::from_utf8_lossy(&frame);
    assert!(
        text.contains("NodeCreated"),
        "expected a NodeCreated SSE frame, got: {text}"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn private_document_reads_are_scoped_to_members(pool: PgPool) {
    let router = app(test_state(pool.clone()));
    let owner = seed_identity(&pool, "owner@example.test").await;
    let stranger = seed_identity(&pool, "stranger@example.test").await;
    let doc = create_document(&router, &bearer(owner), "Private study").await;
    let (status, created) = send(
        &router,
        auth_post(
            &format!("/documents/{doc}/ops/node-create"),
            &bearer(owner),
            &json!({"node_type":"form","pos":"a0","fields":{"label":"Private text"}}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{created}");
    let node = created["event"]["target_node_id"].as_str().unwrap();
    let event = created["event"]["id"].as_str().unwrap();
    let snapshot = engine_core::snapshot::SnapshotEngine::new(pool.clone())
        .take(
            DocumentId(doc),
            engine_core::snapshot::SnapshotReason::Periodic,
        )
        .await
        .unwrap();
    for suffix in [
        "".into(),
        "/events".into(),
        "/stream".into(),
        "/verify".into(),
        "/conflicts".into(),
        "/discussions".into(),
        format!("/node/{node}"),
        format!("/merkle-proof/{event}"),
        format!("/snapshot/{}", snapshot.id.0),
    ] {
        let (status, _) = send(
            &router,
            auth_post_get(&format!("/documents/{doc}{suffix}"), &bearer(stranger)),
        )
        .await;
        assert_eq!(
            status,
            StatusCode::FORBIDDEN,
            "unrelated user read {suffix}"
        );
    }
    let (_, list) = send(&router, auth_post_get("/documents", &bearer(stranger))).await;
    assert!(list["items"].as_array().unwrap().is_empty());
}

/// A concurrent projection move can commit after the scoped SELECT and before
/// UPDATE. PostgreSQL must recheck the UPDATE predicate and roll back the event.
#[sqlx::test(migrations = "../../migrations")]
async fn node_updates_never_touch_another_documents_rows(pool: PgPool) {
    let runtime = common::RuntimeDatabase::new(pool.clone()).await;
    let router = app(test_state(runtime.pool.clone()));
    let owner = bearer(seed_identity(&runtime.pool, "update-scope-owner@example.test").await);
    let stranger = bearer(seed_identity(&runtime.pool, "update-scope-other@example.test").await);
    for kind in ["FieldEdited", "NodeDeleted"] {
        let doc = create_document(&router, &owner, "Source projection").await;
        let other = create_document(&router, &stranger, "Foreign projection").await;
        let root = ulid::Ulid::generate().to_string();
        let node = ulid::Ulid::generate().to_string();
        let (status,body)=send(&router,auth_post(&format!("/documents/{doc}/batch"),&owner,&json!({"ops":[
            {"type":"NodeCreated","node_id":root,"node_type":"form","pos":"a0","fields":{}},
            {"type":"NodeCreated","node_id":node,"node_type":"item","parent_id":root,"pos":"a0","fields":{"label":"Original"}}
        ]}))).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let mut mover = pool.begin().await.unwrap();
        let pid: i32 = sqlx::query_scalar("select pg_backend_pid()")
            .fetch_one(&mut *mover)
            .await
            .unwrap();
        sqlx::query("update node set document_id=$2 where id=$1")
            .bind(&node)
            .bind(other)
            .execute(&mut *mover)
            .await
            .unwrap();
        let expected: Value = sqlx::query_scalar("select to_jsonb(n) from node n where id=$1")
            .bind(&node)
            .fetch_one(&mut *mover)
            .await
            .unwrap();
        let op = if kind == "FieldEdited" {
            json!({"type":kind,"node_id":node,"field":"label","value":"Must roll back"})
        } else {
            json!({"type":kind,"node_id":node})
        };
        let (writer_router, writer_token) = (router.clone(), owner.clone());
        let writer = tokio::spawn(async move {
            send(
                &writer_router,
                auth_post(
                    &format!("/documents/{doc}/batch"),
                    &writer_token,
                    &json!({"ops":[op]}),
                ),
            )
            .await
        });
        tokio::time::timeout(std::time::Duration::from_secs(5),async {
            loop {
                let blocked:bool=sqlx::query_scalar("select exists(select 1 from pg_stat_activity where datname=current_database() and $1=any(pg_blocking_pids(pid)) and lower(query) like '%update node set%')")
                    .bind(pid).fetch_one(&pool).await.unwrap();
                if blocked { break; }
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            }
        }).await.expect("writer did not reach the node UPDATE after its scoped SELECT");
        mover.commit().await.unwrap();
        let (status, body) = tokio::time::timeout(std::time::Duration::from_secs(5), writer)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR, "{kind}: {body}");
        let actual: Value = sqlx::query_scalar("select to_jsonb(n) from node n where id=$1")
            .bind(&node)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(actual, expected, "{kind} must not change the foreign row");
        let count: i64 = sqlx::query_scalar("select count(*) from event where document_id=$1")
            .bind(doc)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 2, "a lost projection must append no event");
        let foreign_count: i64 =
            sqlx::query_scalar("select count(*) from event where document_id=$1")
                .bind(other)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(foreign_count, 0);
    }
    runtime.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn cross_document_node_event_and_snapshot_ids_are_not_found(pool: PgPool) {
    let runtime = common::RuntimeDatabase::new(pool.clone()).await;
    let router = app(test_state(runtime.pool.clone()));
    let token = bearer(seed_identity(&runtime.pool, "scope-owner@example.test").await);
    let a = create_document(&router, &token, "First synthetic document").await;
    let b = create_document(&router, &token, "Second synthetic document").await;
    let mut rows = Vec::new();
    for doc in [a, b] {
        let (status, body) = send(
            &router,
            auth_post(
                &format!("/documents/{doc}/ops/node-create"),
                &token,
                &json!({"node_type":"form","pos":"a0","fields":{"label":"Scoped content"}}),
            ),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let snapshot = engine_core::snapshot::SnapshotEngine::new(runtime.pool.clone())
            .take(
                DocumentId(doc),
                engine_core::snapshot::SnapshotReason::Periodic,
            )
            .await
            .unwrap();
        rows.push((
            doc,
            body["event"]["target_node_id"].as_str().unwrap().to_owned(),
            body["event"]["id"].as_str().unwrap().to_owned(),
            snapshot.id.0,
        ));
    }
    for (i, (doc, node, event, snapshot)) in rows.iter().enumerate() {
        for suffix in [
            format!("/node/{node}"),
            format!("/merkle-proof/{event}"),
            format!("/snapshot/{snapshot}"),
        ] {
            let (status, body) = send(
                &router,
                auth_post_get(&format!("/documents/{doc}{suffix}"), &token),
            )
            .await;
            assert_eq!(status, StatusCode::OK, "{suffix}: {body}");
            let other = rows[1 - i].0;
            let (status, body) = send(
                &router,
                auth_post_get(&format!("/documents/{other}{suffix}"), &token),
            )
            .await;
            assert_eq!(status, StatusCode::NOT_FOUND, "{suffix}: {body}");
        }
    }
    runtime.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn stale_field_edits_are_refused_but_disjoint_edits_work(pool: PgPool) {
    let router = app(test_state(pool.clone()));
    let owner = seed_identity(&pool, "owner@example.test").await;
    let token = bearer(owner);
    let doc = create_document(&router, &token, "Study").await;
    let (_, created) = send(
        &router,
        auth_post(
            &format!("/documents/{doc}/ops/node-create"),
            &token,
            &json!({"node_type":"form","pos":"a0","fields":{"label":"Original"}}),
        ),
    )
    .await;
    let node = created["event"]["target_node_id"].as_str().unwrap();
    // Exercise this full workflow from shared storage, including the API's
    // authorized write transaction, stale checks and legacy version/draft bases.
    engine_core::snapshot::SnapshotEngine::new(pool.clone())
        .with_periodic_storage(engine_core::snapshot::PeriodicStorage::SharedV1)
        .ensure_recent(engine_shared::DocumentId(doc), 0)
        .await
        .unwrap();
    let shared: i64 =
        sqlx::query_scalar("select count(*) from shared_checkpoint where document_id=$1")
            .bind(doc)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(shared, 1);

    let first = json!({"node_id":node,"field":"label","value":"First edit","base_seq":1});
    let second = json!({"node_id":node,"field":"label","value":"Stale edit","base_seq":1});
    let third = json!({"node_id":node,"field":"hint","value":"Independent","base_seq":1});
    assert_eq!(
        send(
            &router,
            auth_post(&format!("/documents/{doc}/ops/field-edit"), &token, &first)
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(
        send(
            &router,
            auth_post(&format!("/documents/{doc}/ops/field-edit"), &token, &second)
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    assert_eq!(
        send(
            &router,
            auth_post(&format!("/documents/{doc}/ops/field-edit"), &token, &third)
        )
        .await
        .0,
        StatusCode::OK
    );
    let (_, read) = send(&router, auth_post_get(&format!("/documents/{doc}"), &token)).await;
    assert_eq!(
        read["state"]["nodes"][node]["current_fields"]["label"],
        "First edit"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn concurrent_acceptance_only_applies_once(pool: PgPool) {
    let router = app(test_state(pool.clone()));
    let (doc, node, token, _) = seed_conflict(&pool, &router).await;
    let uri = format!("/documents/{doc}/proposals/sug-foo/accept");
    let (a, b) = tokio::join!(
        send(&router, auth_post(&uri, &token, &json!({}))),
        send(&router, auth_post(&uri, &token, &json!({})))
    );
    assert_eq!(
        usize::from(a.0 == StatusCode::OK) + usize::from(b.0 == StatusCode::OK),
        1
    );
    let count: i64 = sqlx::query_scalar(
        "select count(*) from event where document_id=$1 and type='SuggestionAccepted'",
    )
    .bind(doc)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(count, 1);
    let (_, read) = send(
        &router,
        auth_post_get(&format!("/documents/{doc}/node/{node}"), &token),
    )
    .await;
    assert_eq!(read["current_fields"]["title"], "Foo");
}

#[sqlx::test(migrations = "../../migrations")]
async fn batch_rolls_back_and_deploy_pins_its_exact_state(pool: PgPool) {
    let mut state = test_state(pool.clone());
    state.named_snapshot_storage = engine_core::snapshot::NamedStorage::SharedV2;
    let router = app(state);
    let owner = seed_identity(&pool, "owner@example.test").await;
    let token = bearer(owner);
    let doc = create_document(&router, &token, "Study").await;
    let node = engine_shared::NodeId(ulid::Ulid::generate().to_string());
    let create = json!({"type":"NodeCreated","node_id":node,"node_type":"form","pos":"a0","fields":{"label":"Study"}});
    let bad = json!({"type":"FieldEdited","node_id":"missing","field":"label","value":"bad"});
    let (status, _) = send(
        &router,
        auth_post(
            &format!("/documents/{doc}/batch"),
            &token,
            &json!({"ops":[create.clone(),bad],"base_seq":0}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (_, read) = send(&router, auth_post_get(&format!("/documents/{doc}"), &token)).await;
    assert_eq!(read["latest_seq"], 0);
    assert_eq!(
        send(
            &router,
            auth_post(
                &format!("/documents/{doc}/batch"),
                &token,
                &json!({"ops":[create],"base_seq":0})
            )
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(
        send(
            &router,
            auth_post(&format!("/documents/{doc}/deploy"), &token, &json!({}))
        )
        .await
        .0,
        StatusCode::OK
    );
    let (_, read) = send(&router, auth_post_get(&format!("/documents/{doc}"), &token)).await;
    assert_eq!(read["status"], "deployed");
    let snapshot = read["deployed_snapshot_id"].as_str().unwrap();
    let (_, snap) = send(
        &router,
        auth_post_get(&format!("/documents/{doc}/snapshot/{snapshot}"), &token),
    )
    .await;
    assert_eq!(snap["through_seq"], 2);
}

#[sqlx::test(migrations = "../../migrations")]
async fn ten_researchers_merge_independent_cells_without_loss(pool: PgPool) {
    let router = app(test_state(pool.clone()));
    let owner = seed_identity(&pool, "owner@team.test").await;
    let token = bearer(owner);
    let doc = create_document(&router, &token, "Ten researchers").await;
    let (_, created) = send(
        &router,
        auth_post(
            &format!("/documents/{doc}/ops/node-create"),
            &token,
            &json!({"node_type":"form","pos":"a0","fields":{}}),
        ),
    )
    .await;
    let node = created["event"]["target_node_id"]
        .as_str()
        .unwrap()
        .to_string();
    let mut tokens = Vec::new();
    for i in 0..10 {
        let email = format!("researcher{i}@team.test");
        let person = seed_identity(&pool, &email).await;
        let (status, _) = send(
            &router,
            auth_post(
                &format!("/documents/{doc}/members"),
                &token,
                &json!({"email":email,"role":"author"}),
            ),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        tokens.push(bearer(person));
    }
    // A checkpoint can retain its erasure guard while actual API writers proceed.
    let mut checkpoint_guard = pool.begin().await.unwrap();
    sqlx::query("select id from document where id=$1 for key share")
        .bind(doc)
        .execute(&mut *checkpoint_guard)
        .await
        .unwrap();
    let futures=tokens.iter().enumerate().map(|(i,token)| {let router=router.clone();let node=node.clone();async move {send(&router,auth_post(&format!("/documents/{doc}/batch"),token,&json!({"base_seq":1,"ops":[{"type":"FieldEdited","node_id":node,"field":format!("cell_{i}"),"value":format!("Researcher {i}")}]}))).await}});
    let results = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        futures::future::join_all(futures),
    )
    .await
    .expect("API writers blocked by checkpoint guard");
    checkpoint_guard.rollback().await.unwrap();
    for (status, body) in results {
        assert_eq!(status, StatusCode::OK, "{body}");
    }
    let (_, read) = send(&router, auth_post_get(&format!("/documents/{doc}"), &token)).await;
    assert_eq!(read["latest_seq"], 11);
    for i in 0..10 {
        assert_eq!(
            read["state"]["nodes"][&node]["current_fields"][format!("cell_{i}")],
            format!("Researcher {i}")
        );
    }
    let (_, verify) = send(
        &router,
        auth_post_get(&format!("/documents/{doc}/verify"), &token),
    )
    .await;
    assert_eq!(verify["ok"], true);
}

#[sqlx::test(migrations = "../../migrations")]
async fn personal_draft_conflicts_are_reviewed_and_old_versions_restore_stable_ids(pool: PgPool) {
    let mut state = test_state(pool.clone());
    state.named_snapshot_storage = engine_core::snapshot::NamedStorage::SharedV2;
    let router = app(state);
    let owner = seed_identity(&pool, "owner@draft.test").await;
    let token = bearer(owner);
    let doc = create_document(&router, &token, "Draft test").await;
    let (_, created) = send(
        &router,
        auth_post(
            &format!("/documents/{doc}/ops/node-create"),
            &token,
            &json!({"node_type":"form","pos":"a0","fields":{"label":"Original"}}),
        ),
    )
    .await;
    let node = created["event"]["target_node_id"].as_str().unwrap();
    // Exercise this full workflow from shared storage, including the API's
    // authorized write transaction, stale checks and legacy version/draft bases.
    engine_core::snapshot::SnapshotEngine::new(pool.clone())
        .with_periodic_storage(engine_core::snapshot::PeriodicStorage::SharedV1)
        .ensure_recent(engine_shared::DocumentId(doc), 0)
        .await
        .unwrap();
    let shared: i64 =
        sqlx::query_scalar("select count(*) from shared_checkpoint where document_id=$1")
            .bind(doc)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(shared, 1);

    let (status, version) = send(
        &router,
        auth_post(
            &format!("/documents/{doc}/versions"),
            &token,
            &json!({"name":"Original"}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{version}");
    let (_, draft) = send(
        &router,
        auth_post(
            &format!("/documents/{doc}/drafts"),
            &token,
            &json!({"name":"Alternative"}),
        ),
    )
    .await;
    let draft = draft["id"].as_str().unwrap();
    let draft_uri = format!("/documents/{doc}/drafts/{draft}");
    let (status,edited)=send(&router,auth_post(&draft_uri,&token,&json!({"revision":0,"ops":[{"type":"FieldEdited","node_id":node,"field":"label","value":"Draft"}]}))).await;
    assert_eq!(status, StatusCode::OK, "{edited}");
    let stale=send(&router,auth_post(&draft_uri,&token,&json!({"revision":0,"ops":[{"type":"FieldEdited","node_id":node,"field":"label","value":"Stale"}]}))).await;
    assert_eq!(stale.0, StatusCode::CONFLICT);
    send(
        &router,
        auth_post(
            &format!("/documents/{doc}/ops/field-edit"),
            &token,
            &json!({"node_id":node,"field":"label","value":"Team"}),
        ),
    )
    .await;
    let (_, preview) = send(
        &router,
        auth_post(
            &format!("{draft_uri}/sync"),
            &token,
            &json!({"action":"preview"}),
        ),
    )
    .await;
    assert_eq!(preview["conflicts"].as_array().unwrap().len(), 1);
    let (status,merged)=send(&router,auth_post(&format!("{draft_uri}/sync"),&token,&json!({"action":"share","team_seq":preview["team_seq"],"revision":1,"resolutions":{format!("{node}:label"):"draft"}}))).await;
    assert_eq!(status, StatusCode::OK, "{merged}");
    assert_eq!(merged["completed"], true);
    send(
        &router,
        auth_post(
            &format!("/documents/{doc}/batch"),
            &token,
            &json!({"base_seq":3,"ops":[{"type":"NodeDeleted","node_id":node}]}),
        ),
    )
    .await;
    let (status, restored) = send(
        &router,
        auth_post(
            &format!(
                "/documents/{doc}/versions/{}/restore",
                version["id"].as_str().unwrap()
            ),
            &token,
            &json!({"base_seq":4}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{restored}");
    assert!(restored["events"]
        .as_array()
        .unwrap()
        .iter()
        .any(|e| e["type"] == "NodeRestored"));
    let (_, read) = send(&router, auth_post_get(&format!("/documents/{doc}"), &token)).await;
    assert_eq!(read["state"]["nodes"][node]["deleted"], false);
    assert_eq!(
        read["state"]["nodes"][node]["current_fields"]["label"],
        "Original"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn content_search_never_returns_another_teams_document(pool: PgPool) {
    let router = app(test_state(pool.clone()));
    let owner = seed_identity(&pool, "owner@search.test").await;
    let stranger = seed_identity(&pool, "stranger@search.test").await;
    let token = bearer(owner);
    let doc = create_document(&router, &token, "Fieldwork").await;
    send(&router,auth_post(&format!("/documents/{doc}/ops/node-create"),&token,&json!({"node_type":"form","pos":"a0","fields":{"label":"confidential community interviews"}}))).await;
    let (status, found) = send(&router, auth_post_get("/search?q=community", &token)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(found["items"].as_array().unwrap().len(), 1);
    // The stranger owns matching content too. A missing document/node join must not
    // let their authorized row drag another team's node into the search results.
    let own = create_document(&router, &bearer(stranger), "Community notes").await;
    let (created, body) = send(
        &router,
        auth_post(
            &format!("/documents/{own}/ops/node-create"),
            &bearer(stranger),
            &json!({"node_type":"form","pos":"a0","fields":{"label":"public community notes"}}),
        ),
    )
    .await;
    assert_eq!(created, StatusCode::OK, "{body}");
    let (status, found) = send(
        &router,
        auth_post_get("/search?q=community", &bearer(stranger)),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{found}");
    let items = found["items"].as_array().unwrap();
    assert_eq!(items.len(), 1, "{found}");
    assert!(
        items
            .iter()
            .all(|item| item["document_id"] == own.to_string()),
        "{found}"
    );
    assert!(!found.to_string().contains("confidential"), "{found}");
}

#[sqlx::test(migrations = "../../migrations")]
async fn archive_is_reversible_and_keeps_membership_boundaries(pool: PgPool) {
    let router = app(test_state(pool.clone()));
    let owner = seed_identity(&pool, "owner@archive.test").await;
    let stranger = seed_identity(&pool, "stranger@archive.test").await;
    let token = bearer(owner);
    let doc = create_document(&router, &token, "Archived research").await;
    let uri = format!("/documents/{doc}/metadata");
    let (status, _) = send(&router, auth_post(&uri, &token, &json!({"archived":true}))).await;
    assert_eq!(status, StatusCode::OK);
    let (_, active) = send(&router, auth_post_get("/documents", &token)).await;
    assert!(active["items"].as_array().unwrap().is_empty());
    let (_, archived) = send(&router, auth_post_get("/documents?status=archived", &token)).await;
    assert_eq!(archived["items"][0]["id"], doc.to_string());
    let (_, hidden) = send(
        &router,
        auth_post_get("/documents?status=archived", &bearer(stranger)),
    )
    .await;
    assert!(hidden["items"].as_array().unwrap().is_empty());
    let (status, _) = send(
        &router,
        auth_post(&uri, &bearer(stranger), &json!({"archived":false})),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _) = send(&router, auth_post(&uri, &token, &json!({"archived":false}))).await;
    assert_eq!(status, StatusCode::OK);
    let (_, restored) = send(&router, auth_post_get("/documents", &token)).await;
    assert_eq!(restored["items"][0]["id"], doc.to_string());
    assert_eq!(restored["items"][0]["status"], "draft");
}

#[sqlx::test(migrations = "../../migrations")]
async fn file_import_batches_can_exceed_the_framework_default_limit(pool: PgPool) {
    let router = app(test_state(pool.clone()));
    let owner = seed_identity(&pool, "owner@large-import.test").await;
    let token = bearer(owner);
    let doc = create_document(&router, &token, "Large import").await;
    let text = "a".repeat(2 * 1024 * 1024 + 1);
    let (status, response) = send(&router, auth_post(
        &format!("/documents/{doc}/batch"), &token,
        &json!({"base_seq":0,"ops":[{"type":"NodeCreated","node_id":ulid::Ulid::generate().to_string(),"node_type":"form","pos":"a0","fields":{"label":text}}]})
    )).await;
    assert_eq!(status, StatusCode::OK, "{response}");
    assert_eq!(response["events"].as_array().unwrap().len(), 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn operations_dashboard_rejects_regular_members_and_anonymous_readers(pool: PgPool) {
    let member = seed_identity(&pool, "ordinary-member@example.test").await;
    let router = app(test_state(pool));
    assert_eq!(
        send(&router, get("/admin/overview")).await.0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        send(&router, auth_post_get("/admin/overview", &bearer(member)))
            .await
            .0,
        StatusCode::FORBIDDEN
    );
}
#[sqlx::test(migrations = "../../migrations")]
async fn performance_samples_need_service_auth_and_are_idempotent(pool: PgPool) {
    let mut state = test_state(pool.clone());
    state.auth.service_key = Some("test-service-key".into());
    let router = app(state);
    let body = json!({"page_id":Uuid::new_v4(),"metric":"visit","host":"homepage","page":"home","value":1});
    assert_eq!(
        send(&router, auth_post("/telemetry", "invalid", &body))
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    for _ in 0..2 {
        let req = Request::builder()
            .method("POST")
            .uri("/telemetry")
            .header("content-type", "application/json")
            .header("x-dynodoc-service-key", "test-service-key")
            .body(Body::from(body.to_string()))
            .unwrap();
        assert_eq!(send(&router, req).await.0, StatusCode::OK);
    }
    let count: i64 = sqlx::query_scalar("select count(*) from web_metric")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1);
    let bad = json!({"page_id":Uuid::new_v4(),"metric":"visit","host":"homepage","page":"/documents/private-id","value":1});
    let req = Request::builder()
        .method("POST")
        .uri("/telemetry")
        .header("content-type", "application/json")
        .header("x-dynodoc-service-key", "test-service-key")
        .body(Body::from(bad.to_string()))
        .unwrap();
    assert_eq!(send(&router, req).await.0, StatusCode::BAD_REQUEST);
}

#[sqlx::test(migrations = "../../migrations")]
async fn allowlisted_admin_sees_aggregate_storage_and_no_research_content(pool: PgPool) {
    let admin = seed_identity(&pool, "operator@example.test").await;
    let mut state = test_state(pool);
    state.admin_emails = "operator@example.test".into();
    let router = app(state);
    let (status, result) = send(&router, auth_post_get("/admin/overview", &bearer(admin))).await;
    assert_eq!(status, StatusCode::OK, "{result}");
    assert_eq!(result["counts"]["accounts"], 1);
    assert!(result["counts"]["database_bytes"].as_i64().unwrap() > 0);
    assert!(result["tables"]
        .as_array()
        .unwrap()
        .iter()
        .any(|t| t["name"] == "document_upload"));
    assert!(result.get("documents").is_none());
}

#[sqlx::test(migrations = "../../migrations")]
async fn product_hierarchy_has_inherited_roles_and_manager_boundaries(pool: PgPool) {
    let router = app(test_state(pool.clone()));
    let owner = bearer(seed_identity(&pool, "owner@example.org").await);
    let editor = bearer(seed_identity(&pool, "editor@example.org").await);
    let stranger = bearer(seed_identity(&pool, "stranger@example.org").await);
    let (status, org) = send(
        &router,
        auth_post(
            "/spaces",
            &owner,
            &json!({"name":"University","kind":"organization"}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{org}");
    let org = org["id"].as_str().unwrap();
    let (_, team) = send(
        &router,
        auth_post(
            "/spaces",
            &owner,
            &json!({"name":"Field team","kind":"team","parent_id":org}),
        ),
    )
    .await;
    let team = team["id"].as_str().unwrap();
    let (_, folder) = send(
        &router,
        auth_post(
            "/spaces",
            &owner,
            &json!({"name":"Study","kind":"folder","parent_id":team}),
        ),
    )
    .await;
    let folder = folder["id"].as_str().unwrap();
    let (status, result) = send(
        &router,
        auth_post(
            &format!("/spaces/{org}/members"),
            &owner,
            &json!({"email":"editor@example.org","role":"editor"}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{result}");
    let doc = create_document(&router, &owner, "Protocol").await;
    assert_eq!(
        send(
            &router,
            auth_post(
                &format!("/documents/{doc}/location"),
                &owner,
                &json!({"space_id":folder})
            )
        )
        .await
        .0,
        StatusCode::OK
    );
    let (status, body) = send(
        &router,
        auth_post_get(&format!("/documents/{doc}"), &editor),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["role"], "author");
    assert_eq!(
        send(
            &router,
            auth_post(
                &format!("/documents/{doc}/trash"),
                &editor,
                &json!({"deleted":true})
            )
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        send(
            &router,
            auth_post(
                &format!("/documents/{doc}/members"),
                &editor,
                &json!({"email":"stranger@example.org","role":"author"})
            )
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        send(
            &router,
            auth_post_get(&format!("/documents/{doc}"), &stranger)
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        send(
            &router,
            auth_post(
                &format!("/spaces/{org}/members"),
                &owner,
                &json!({"email":"owner@example.org","role":"viewer"})
            )
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        send(
            &router,
            auth_post(
                &format!("/spaces/{org}/members"),
                &owner,
                &json!({"email":"editor@example.org","role":"remove"})
            )
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(
        send(
            &router,
            auth_post_get(&format!("/documents/{doc}"), &editor)
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn office_review_is_private_and_unresolved_changes_cannot_be_published(pool: PgPool) {
    let router = app(test_state(pool.clone()));
    let owner = bearer(seed_identity(&pool, "office-privacy@example.org").await);
    let doc = create_document(&router, &owner, "Office review").await;
    let root = ulid::Ulid::generate().to_string();
    let node = ulid::Ulid::generate().to_string();
    let content = json!({"type":"paragraph","content":[{"type":"text","text":"Public wording","marks":[{"type":"sourceComment","attrs":{"body":"Private Word comment","author":"Private author"}}]}]});
    let (status, body) = send(&router, auth_post(&format!("/documents/{doc}/batch"), &owner, &json!({"base_seq":0,"ops":[
      {"type":"NodeCreated","node_id":root,"node_type":"form","pos":"a","fields":{"fonts":[{"family":"Private font","uploadId":"secret-file"}]}},
      {"type":"NodeCreated","node_id":node,"node_type":"item","parent_id":root,"pos":"a","fields":{"content":content,"format_0":{"note":"Private spreadsheet note","bold":true}}}
    ]}))).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (status, link) = send(
        &router,
        auth_post(
            &format!("/documents/{doc}/public-links"),
            &owner,
            &json!({"base_seq":2}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{link}");
    let (_, public) = send(
        &router,
        get(&format!(
            "/public/documents/{}",
            link["token"].as_str().unwrap()
        )),
    )
    .await;
    let encoded = public.to_string();
    assert!(
        !encoded.contains("Private") && !encoded.contains("secret-file"),
        "{encoded}"
    );
    assert!(encoded.contains("Public wording"));
    let formatted = public["state"]["nodes"]
        .as_object()
        .unwrap()
        .values()
        .find(|n| n["current_fields"].get("format_0").is_some())
        .unwrap();
    assert_eq!(formatted["current_fields"]["format_0"]["bold"], true);
    let mut tracked = content.clone();
    tracked["content"][0]["marks"] =
        json!([{"type":"trackedChange","attrs":{"kind":"insert","id":"revision"}}]);
    assert_eq!(send(&router, auth_post(&format!("/documents/{doc}/batch"), &owner, &json!({"base_seq":2,"ops":[{"type":"FieldEdited","node_id":node,"field":"content","value":tracked}]}))).await.0, StatusCode::OK);
    let (status, body) = send(
        &router,
        auth_post(
            &format!("/documents/{doc}/public-links"),
            &owner,
            &json!({"base_seq":3}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert!(body.to_string().contains("Accept or reject"));
}

/// A block of a public copy by label; public copies use pseudonymous block ids.
fn public_block<'a>(public: &'a Value, label: &str) -> Option<&'a Value> {
    public["state"]["nodes"]
        .as_object()?
        .values()
        .find(|n| n["current_fields"]["label"] == label)
}

#[sqlx::test(migrations = "../../migrations")]
async fn public_copy_pseudonymizes_structural_ids_without_changing_content(pool: PgPool) {
    let router = app(test_state(pool.clone()));
    let owner = bearer(seed_identity(&pool, "pseudonyms@example.org").await);
    let doc = create_document(&router, &owner, "Pseudonyms").await;
    let [root, row, gone] = std::array::from_fn(|_| ulid::Ulid::generate().to_string());
    let content = json!({"type":"paragraph","attrs":{"blockId":row},"content":[{"type":"text","text":"Row"}]});
    let (status, body) = send(&router, auth_post(&format!("/documents/{doc}/batch"), &owner, &json!({"base_seq":0,"ops":[
      {"type":"NodeCreated","node_id":root,"node_type":"form","pos":"a","fields":{"sheet_layout":{"merges":[{"fromRow":row,"toRow":gone}]}}},
      {"type":"NodeCreated","node_id":row,"node_type":"item","parent_id":root,"pos":"a","fields":{"content":content,"literal":row}},
      {"type":"NodeCreated","node_id":gone,"node_type":"item","parent_id":root,"pos":"b","fields":{"label":"Gone"}},
      {"type":"NodeDeleted","node_id":gone}
    ]}))).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let publish = || async {
        let (status, link) = send(
            &router,
            auth_post(
                &format!("/documents/{doc}/public-links"),
                &owner,
                &json!({"base_seq":4}),
            ),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{link}");
        format!("/public/documents/{}", link["token"].as_str().unwrap())
    };
    let path = publish().await;
    let (status, public) = send(&router, get(&path)).await;
    assert_eq!(status, StatusCode::OK);
    let encoded = public.to_string();
    for id in [&root, &gone] {
        assert!(!encoded.contains(id.as_str()), "{encoded}");
    }
    let nodes = public["state"]["nodes"].as_object().unwrap();
    assert!(nodes.get(&row).is_none());
    let (row_key, row_node) = nodes
        .iter()
        .find(|(_, n)| n["current_fields"]["label"] == "Row")
        .unwrap();
    assert_eq!(
        row_node["current_fields"]["content"]["attrs"]["blockId"],
        json!(row_key)
    );
    // A user's text may happen to equal a real id; it is content, not a reference.
    assert_eq!(row_node["current_fields"]["literal"], json!(row));
    let root_node = nodes.values().find(|n| n["parent_id"].is_null()).unwrap();
    assert_eq!(
        root_node["current_fields"]["sheet_layout"]["merges"][0]["fromRow"],
        json!(row_key)
    );
    // Stable for one link, different for another.
    assert_eq!(send(&router, get(&path)).await.1, public);
    let (_, other) = send(&router, get(&publish().await)).await;
    assert!(other["state"]["nodes"].get(row_key.as_str()).is_none());
}

#[sqlx::test(migrations = "../../migrations")]
async fn product_public_copy_is_frozen_private_and_revocable(pool: PgPool) {
    let mut state = test_state(pool.clone());
    state.named_snapshot_storage = engine_core::snapshot::NamedStorage::SharedV2;
    let router = app(state);
    let owner = bearer(seed_identity(&pool, "publisher@example.org").await);
    let doc = create_document(&router, &owner, "Published protocol").await;
    let root = "01J00000000000000000000000";
    let item = "01J00000000000000000000001";
    let hidden = "01J00000000000000000000002";
    let child = "01J00000000000000000000003";
    let ops = json!({"base_seq":0,"ops":[
      {"type":"NodeCreated","node_id":root,"node_type":"form","pos":"a","fields":{}},
      {"type":"NodeCreated","node_id":item,"node_type":"item","parent_id":root,"pos":"a","fields":{"label":"Original question","research_source":{"token":"private metadata"}}},
      {"type":"NodeCreated","node_id":hidden,"node_type":"section","parent_id":root,"pos":"b","fields":{"label":"Removed private group"}},
      {"type":"NodeCreated","node_id":child,"node_type":"item","parent_id":hidden,"pos":"a","fields":{"label":"Descendant private content"}},
      {"type":"NodeDeleted","node_id":child},
      {"type":"NodeDeleted","node_id":hidden},
      {"type":"CommentAdded","node_id":item,"body":"Private reviewer conversation"}
    ]});
    let (status, body) = send(
        &router,
        auth_post(&format!("/documents/{doc}/batch"), &owner, &ops),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let seq: i64 = sqlx::query_scalar("select max(seq) from event where document_id=$1")
        .bind(doc)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        send(
            &router,
            auth_post(
                &format!("/documents/{doc}/public-links"),
                &owner,
                &json!({"base_seq":0})
            )
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    let (status, link) = send(
        &router,
        auth_post(
            &format!("/documents/{doc}/public-links"),
            &owner,
            &json!({"base_seq":seq,"expires_days":30}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{link}");
    let path = format!("/public/documents/{}", link["token"].as_str().unwrap());
    let (status, public) = send(&router, get(&path)).await;
    assert_eq!(status, StatusCode::OK);
    let original = public_block(&public, "Original question").unwrap();
    assert!(original["current_fields"].get("research_source").is_none());
    assert!(public_block(&public, "Removed private group").is_none());
    assert!(public_block(&public, "Descendant private content").is_none());
    assert_eq!(public["state"]["comments"], json!([]));
    let update = json!({"base_seq":seq,"ops":[{"type":"FieldEdited","node_id":item,"field":"label","value":"Private later edit"}]});
    assert_eq!(
        send(
            &router,
            auth_post(&format!("/documents/{doc}/batch"), &owner, &update)
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(
        send(
            &router,
            auth_post(
                &format!("/documents/{doc}/metadata"),
                &owner,
                &json!({"title":"Private later title"})
            )
        )
        .await
        .0,
        StatusCode::OK
    );
    let (_, public) = send(&router, get(&path)).await;
    assert_eq!(public["title"], "Published protocol");
    assert!(public_block(&public, "Original question").is_some());
    assert_eq!(
        send(
            &router,
            auth_post(
                &format!("/documents/{doc}/trash"),
                &owner,
                &json!({"deleted":true})
            )
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(send(&router, get(&path)).await.0, StatusCode::NOT_FOUND);
    assert_eq!(
        send(&router, auth_post_get(&format!("/documents/{doc}"), &owner))
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        send(
            &router,
            auth_post(
                &format!("/documents/{doc}/trash"),
                &owner,
                &json!({"deleted":false})
            )
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(send(&router, get(&path)).await.0, StatusCode::NOT_FOUND);
    assert_eq!(
        send(&router, auth_post_get(&format!("/documents/{doc}"), &owner))
            .await
            .0,
        StatusCode::OK
    );
    assert!(engine_core::log::verify_chain(&pool, DocumentId(doc))
        .await
        .is_ok());
}

#[sqlx::test(migrations = "../../migrations")]
async fn product_admin_removal_invalidates_existing_tokens_after_restoration(pool: PgPool) {
    let admin_id = seed_identity(&pool, "admin@example.org").await;
    let user = seed_identity(&pool, "person@example.org").await;
    let mut state = test_state(pool.clone());
    state.admin_emails = "admin@example.org".into();
    let router = app(state);
    let admin = bearer(admin_id);
    let old = bearer(user);
    assert_eq!(
        send(&router, auth_post_get("/admin/records?kind=users", &old))
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        send(
            &router,
            auth_post(
                "/admin/actions",
                &admin,
                &json!({"kind":"users","id":user,"removed":true,"confirmation":"wrong"})
            )
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    use engine_api::auth::{sha256, store};
    let expires = chrono::Utc::now() + chrono::Duration::hours(1);
    store::issue_magic_link(
        &pool,
        engine_shared::IdentityId(user),
        &sha256(b"pending-link"),
        expires,
    )
    .await
    .unwrap();
    store::issue_refresh(&pool, user, &sha256(b"pending-refresh"), expires)
        .await
        .unwrap();
    for removed in [true, false] {
        let (status,body)=send(&router,auth_post("/admin/actions",&admin,&json!({"kind":"users","id":user,"removed":removed,"confirmation":"person@example.org"}))).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(
            send(&router, auth_post_get("/auth/me", &old)).await.0,
            StatusCode::UNAUTHORIZED
        );
    }
    assert!(
        store::consume_magic_link(&pool, &sha256(b"pending-link"))
            .await
            .is_err(),
        "A pre-removal email link cannot revive after restoration"
    );
    assert!(
        store::rotate_refresh(
            &pool,
            &sha256(b"pending-refresh"),
            &sha256(b"replacement"),
            expires
        )
        .await
        .is_err(),
        "A pre-removal refresh token cannot revive after restoration"
    );
    let fresh = format!(
        "Bearer {}",
        token::mint_generation(&KEY, user, 3600, 1).unwrap()
    );
    assert_eq!(
        send(&router, auth_post_get("/auth/me", &fresh)).await.0,
        StatusCode::OK
    );
    assert_eq!(send(&router,auth_post("/admin/actions",&admin,&json!({"kind":"users","id":admin_id,"removed":true,"confirmation":"admin@example.org"}))).await.0,StatusCode::BAD_REQUEST);
    let result = sqlx::query("delete from operation_audit")
        .execute(&pool)
        .await;
    assert!(result.is_err(), "Operations audit must be append-only");
}

#[sqlx::test(migrations = "../../migrations")]
async fn corrupt_historical_snapshot_cannot_escape_through_public_restore_or_audit(pool: PgPool) {
    corrupt_snapshot_boundaries(pool, false).await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn corrupt_shared_snapshot_cannot_escape_through_public_restore_or_audit(pool: PgPool) {
    corrupt_snapshot_boundaries(pool, true).await;
}

async fn corrupt_snapshot_boundaries(pool: PgPool, shared: bool) {
    let mut state = test_state(pool.clone());
    if shared {
        state.named_snapshot_storage = engine_core::snapshot::NamedStorage::SharedV2;
    }
    let storage = state.named_snapshot_storage;
    let router = app(state);
    let owner = bearer(seed_identity(&pool, "snapshot-owner@example.test").await);
    let outsider = bearer(seed_identity(&pool, "snapshot-outsider@example.test").await);
    let doc = create_document(&router, &owner, "Verified history").await;
    let node = ulid::Ulid::generate().to_string();
    let (status, body) = send(&router, auth_post(&format!("/documents/{doc}/batch"), &owner,
        &json!({"base_seq":0,"ops":[{"type":"NodeCreated","node_id":node,"node_type":"form","pos":"a0","fields":{"label":"Original"}}]}))).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (status, body) = send(
        &router,
        auth_post(&format!("/documents/{doc}/deploy"), &owner, &json!({})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (status, version) = send(
        &router,
        auth_post(
            &format!("/documents/{doc}/versions"),
            &owner,
            &json!({"name":"Milestone","base_seq":2}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{version}");
    let snapshot = version["snapshot_id"].as_str().unwrap();
    let (status, link) = send(
        &router,
        auth_post(
            &format!("/documents/{doc}/public-links"),
            &owner,
            &json!({"base_seq":2}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{link}");
    let public = format!("/public/documents/{}", link["token"].as_str().unwrap());
    assert_eq!(send(&router, get(&public)).await.0, StatusCode::OK);
    let deployed: Uuid =
        sqlx::query_scalar("select deployed_snapshot_id from document where id=$1")
            .bind(doc)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(deployed.to_string(), snapshot);
    // A newer healthy checkpoint must not mask a corrupt retained historical one.
    let (status, body) = send(&router, auth_post(&format!("/documents/{doc}/batch"), &owner,
        &json!({"base_seq":2,"ops":[{"type":"FieldEdited","node_id":node,"field":"label","value":"Current"}]}))).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    engine_core::snapshot::SnapshotEngine::new(pool.clone())
        .with_named_storage(storage)
        .take(
            DocumentId(doc),
            engine_core::snapshot::SnapshotReason::Periodic,
        )
        .await
        .unwrap();
    sqlx::query("alter table snapshot disable trigger snapshot_no_update")
        .execute(&pool)
        .await
        .unwrap();
    let raw: Value = sqlx::query_scalar("select state from snapshot where id=$1")
        .bind(deployed)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(raw.get("$dynodoc_state").is_some(), shared);
    if shared {
        sqlx::query("update snapshot set state=(select state from snapshot where document_id=$2 and through_seq=3) where id=$1")
            .bind(deployed).bind(doc).execute(&pool).await.unwrap();
    } else {
        sqlx::query("update snapshot set state=jsonb_set(state,'{comments}','[{\"node_id\":null,\"body\":\"forged\"}]') where id=$1")
        .bind(deployed).execute(&pool).await.unwrap();
    }
    sqlx::query("alter table snapshot enable trigger snapshot_no_update")
        .execute(&pool)
        .await
        .unwrap();
    engine_core::log::verify_chain(&pool, DocumentId(doc))
        .await
        .unwrap();
    let (status, current) =
        send(&router, auth_post_get(&format!("/documents/{doc}"), &owner)).await;
    assert_eq!(status, StatusCode::OK, "{current}");
    assert_eq!(
        current["state"]["nodes"][&node]["current_fields"]["label"],
        "Current"
    );
    let path = format!("/documents/{doc}/snapshot/{snapshot}");
    assert_eq!(
        send(&router, auth_post_get(&path, &owner)).await.0,
        StatusCode::INTERNAL_SERVER_ERROR
    );
    assert_eq!(
        send(&router, auth_post_get(&path, &outsider)).await.0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        send(&router, get(&public)).await.0,
        StatusCode::INTERNAL_SERVER_ERROR
    );
    let (status, audit) = send(
        &router,
        auth_post_get(&format!("/documents/{doc}/verify"), &owner),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{audit}");
    assert_eq!(audit["ok"], false);
    assert_eq!(audit["broken_at_seq"], 2);
    let (status, body) = send(
        &router,
        auth_post(
            &format!(
                "/documents/{doc}/versions/{}/restore",
                version["id"].as_str().unwrap()
            ),
            &owner,
            &json!({"base_seq":3}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR, "{body}");
    let seq: i64 = sqlx::query_scalar("select max(seq) from event where document_id=$1")
        .bind(doc)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(seq, 3, "failed restore must append nothing");
}

#[sqlx::test(migrations = "../../migrations")]
async fn search_timeout_stops_blocked_scan_and_recovers(pool: PgPool) {
    let router = app(test_state(pool.clone()));
    let token = bearer(seed_identity(&pool, "bounded-search@example.test").await);
    create_document(&router, &token, "Search fixture").await;
    let mut lock = pool.begin().await.unwrap();
    sqlx::query("lock table node in access exclusive mode")
        .execute(&mut *lock)
        .await
        .unwrap();
    let (status, body) = tokio::time::timeout(
        std::time::Duration::from_secs(8),
        send(&router, auth_post_get("/search?q=fixture", &token)),
    )
    .await
    .unwrap();
    assert_eq!(status, StatusCode::GATEWAY_TIMEOUT, "{body}");
    assert_eq!(body["code"], "deadline_exceeded");
    assert!(!body.to_string().contains("statement"));
    lock.rollback().await.unwrap();
    let (status, body) = send(&router, auth_post_get("/search?q=fixture", &token)).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["items"].as_array().unwrap().len(), 1);
}
#[sqlx::test(migrations = "../../migrations")]
async fn expensive_reads_share_identity_budget_but_not_other_members(pool: PgPool) {
    let router = app(test_state(pool.clone()));
    let actor = seed_identity(&pool, "budget-actor@example.test").await;
    let owner = bearer(actor);
    let other = bearer(seed_identity(&pool, "budget-other@example.test").await);
    let doc = create_document(&router, &owner, "Budget fixture").await;
    // A denied caller cannot debit the owner's project or account budget.
    for _ in 0..125 {
        assert_eq!(
            send(
                &router,
                auth_post_get(&format!("/documents/{doc}/verify"), &other)
            )
            .await
            .0,
            StatusCode::FORBIDDEN
        );
    }
    for _ in 0..120 {
        assert_eq!(
            send(&router, auth_post_get("/search?q=fixture", &owner))
                .await
                .0,
            StatusCode::OK
        );
    }
    let response = router
        .clone()
        .oneshot(auth_post_get(
            &format!("/documents/{doc}/provenance"),
            &bearer(actor),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(response.headers()["retry-after"], "60");
    drop(response);
    assert_eq!(
        send(&router, auth_post_get("/search?q=fixture", &other))
            .await
            .0,
        StatusCode::OK
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn title_and_settings_change_writes_no_event_but_one_audit_row(pool: PgPool) {
    let router = app(test_state(pool.clone()));
    let actor = seed_identity(&pool, "metadata-actor@example.test").await;
    let owner = bearer(actor);
    let doc = create_document(&router, &owner, "Original title").await;
    let (status, seeded) = send(
        &router,
        auth_post(
            &format!("/documents/{doc}/ops/node-create"),
            &owner,
            &json!({"node_type":"form","pos":"a0","fields":{"title":"Content title"}}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{seeded}");
    let before: Vec<(i64, String)> = sqlx::query_as(
        "select seq,encode(chain_hash,'hex') from event where document_id=$1 order by seq",
    )
    .bind(doc)
    .fetch_all(&pool)
    .await
    .unwrap();
    let change = json!({"title":"Renamed document","settings":{"import_warnings":["Synthetic import warning"]}});
    let uri = format!("/documents/{doc}/metadata");
    let (status, response) = send(&router, auth_post(&uri, &owner, &change)).await;
    assert_eq!(status, StatusCode::OK, "{response}");
    let (title, settings): (String, Value) =
        sqlx::query_as("select title,settings from document where id=$1")
            .bind(doc)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(title, "Renamed document");
    assert_eq!(
        settings["import_warnings"],
        json!(["Synthetic import warning"])
    );
    let audit: Vec<(Uuid, Value)> = sqlx::query_as(
        "select actor_id,detail from operation_audit where resource_id=$1 and action='document.metadata_changed'",
    ).bind(doc).fetch_all(&pool).await.unwrap();
    assert_eq!(audit.len(), 1);
    assert_eq!(audit[0].0, actor);
    assert_eq!(audit[0].1["format"], "dynodoc.metadata-audit.v1");
    assert_eq!(audit[0].1["fields"], json!(["title", "settings"]));
    assert_ne!(audit[0].1["before_hash"], audit[0].1["after_hash"]);
    assert!(!audit[0].1.to_string().contains("Synthetic import warning"));
    let (status, response) = send(&router, auth_post(&uri, &owner, &change)).await;
    assert_eq!(status, StatusCode::OK, "{response}");
    let count: i64 = sqlx::query_scalar("select count(*) from operation_audit where resource_id=$1 and action='document.metadata_changed'")
        .bind(doc).fetch_one(&pool).await.unwrap();
    assert_eq!(
        count, 1,
        "repeating unchanged metadata must not add an audit row"
    );
    let after: Vec<(i64, String)> = sqlx::query_as(
        "select seq,encode(chain_hash,'hex') from event where document_id=$1 order by seq",
    )
    .bind(doc)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(after, before);
    assert!(engine_core::log::verify_chain(&pool, DocumentId(doc))
        .await
        .is_ok());
}

#[sqlx::test(migrations = "../../migrations")]
async fn new_pos_keys_are_validated_at_write_boundaries(pool: PgPool) {
    let router = app(test_state(pool.clone()));
    let token = bearer(seed_identity(&pool, "pos-validator@example.test").await);
    let doc = create_document(&router, &token, "Pos Validation").await;
    let node_id = ulid::Ulid::generate().to_string();

    // Batch with empty pos returns 400 Bad Request
    let (status, body) = send(
        &router,
        auth_post(
            &format!("/documents/{doc}/batch"),
            &token,
            &json!({
                "ops": [{
                    "type": "NodeCreated",
                    "node_id": node_id,
                    "node_type": "form",
                    "pos": "",
                    "fields": {}
                }]
            }),
        ),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "expected 400 for empty pos: {body}"
    );

    // Batch with trailing space pos "m " returns 400 Bad Request
    let (status, body) = send(
        &router,
        auth_post(
            &format!("/documents/{doc}/batch"),
            &token,
            &json!({
                "ops": [{
                    "type": "NodeCreated",
                    "node_id": node_id,
                    "node_type": "form",
                    "pos": "m ",
                    "fields": {}
                }]
            }),
        ),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "expected 400 for trailing space pos: {body}"
    );

    // Batch with valid pos succeeds
    let (status, body) = send(
        &router,
        auth_post(
            &format!("/documents/{doc}/batch"),
            &token,
            &json!({
                "ops": [{
                    "type": "NodeCreated",
                    "node_id": node_id,
                    "node_type": "form",
                    "pos": "a0",
                    "fields": {}
                }]
            }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "expected 200 for valid pos: {body}");

    // NodeMoved with invalid new_pos returns 400 Bad Request
    let (status, body) = send(
        &router,
        auth_post(
            &format!("/documents/{doc}/batch"),
            &token,
            &json!({
                "base_seq": 1,
                "ops": [{
                    "type": "NodeMoved",
                    "node_id": node_id,
                    "new_pos": "\n"
                }]
            }),
        ),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "expected 400 for newline new_pos: {body}"
    );
}
