//! Real-router regressions for drafts that used to get stuck on structural merges.
use axum::{
    body::{to_bytes, Body},
    http::{header, Request, StatusCode},
    Router,
};
use engine_api::{
    app,
    auth::{store, token, AuthConfig},
    AppState,
};
use engine_core::materializer::{DocumentState, Materializer};
use serde_json::{json, Value};
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

mod common;

const KEY: [u8; 32] = [89; 32];

struct Fixture {
    db: common::RuntimeDatabase,
    router: Router,
    token: String,
    doc: Uuid,
    root: String,
    a: String,
    b: String,
    child: String,
}

impl Fixture {
    async fn new(owner: PgPool) -> Self {
        let db = common::RuntimeDatabase::new(owner).await;
        let actor = store::upsert_identity(&db.pool, "merge-flow@example.test", None)
            .await
            .unwrap()
            .id
            .0;
        let token = format!("Bearer {}", token::mint(&KEY, actor, 3600).unwrap());
        let router = app(AppState::new(
            db.pool.clone(),
            AuthConfig {
                paseto_key: KEY,
                service_key: None,
                token_ttl_seconds: 3600,
                public_base_url: "http://test.local".into(),
            },
        ));
        let mut fixture = Self {
            db,
            router,
            token,
            doc: Uuid::nil(),
            root: ulid::Ulid::generate().to_string(),
            a: ulid::Ulid::generate().to_string(),
            b: ulid::Ulid::generate().to_string(),
            child: ulid::Ulid::generate().to_string(),
        };
        let (status, body) = fixture
            .post("/documents", json!({"title":"Merge flow"}))
            .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        fixture.doc = Uuid::parse_str(body["id"].as_str().unwrap()).unwrap();
        fixture.batch(0, json!([
            {"type":"NodeCreated","node_id":fixture.root,"node_type":"form","pos":"a0","fields":{}},
            {"type":"NodeCreated","node_id":fixture.a,"node_type":"section","parent_id":fixture.root,"pos":"a0","fields":{}},
            {"type":"NodeCreated","node_id":fixture.b,"node_type":"section","parent_id":fixture.root,"pos":"b0","fields":{}},
            {"type":"NodeCreated","node_id":fixture.child,"node_type":"item","parent_id":fixture.a,"pos":"a0","fields":{"label":"Original"}}
        ])).await;
        fixture
    }

    async fn post(&self, uri: &str, body: Value) -> (StatusCode, Value) {
        let response = self
            .router
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(uri)
                    .header(header::AUTHORIZATION, &self.token)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(serde_json::to_vec(&body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = response.status();
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        (status, serde_json::from_slice(&bytes).unwrap())
    }

    async fn batch(&self, seq: i64, ops: Value) {
        let (status, body) = self
            .post(
                &format!("/documents/{}/batch", self.doc),
                json!({"base_seq":seq,"ops":ops}),
            )
            .await;
        assert_eq!(status, StatusCode::OK, "{body}");
    }

    async fn draft(&self, ops: Value) -> String {
        let (status, body) = self
            .post(
                &format!("/documents/{}/drafts", self.doc),
                json!({"name":"Private draft"}),
            )
            .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let uri = format!(
            "/documents/{}/drafts/{}",
            self.doc,
            body["id"].as_str().unwrap()
        );
        let (status, body) = self.post(&uri, json!({"revision":0,"ops":ops})).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        format!("{uri}/sync")
    }

    async fn preview(&self, uri: &str) -> Value {
        let (status, body) = self.post(uri, json!({"action":"preview"})).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        body
    }

    async fn rejected_share_preserves_history(&self, uri: &str, body: Value) {
        let before: Vec<(i64, Vec<u8>)> =
            sqlx::query_as("select seq,chain_hash from event where document_id=$1 order by seq")
                .bind(self.doc)
                .fetch_all(&self.db.pool)
                .await
                .unwrap();
        let (status, response) = self.post(uri, body).await;
        assert_eq!(status, StatusCode::CONFLICT, "{response}");
        let after: Vec<(i64, Vec<u8>)> =
            sqlx::query_as("select seq,chain_hash from event where document_id=$1 order by seq")
                .bind(self.doc)
                .fetch_all(&self.db.pool)
                .await
                .unwrap();
        assert_eq!(
            after, before,
            "a failed share must not append partial history"
        );
    }

    async fn share(&self, uri: &str, preview: &Value, resolutions: Value) -> Value {
        let (status, body) = self.post(uri, json!({"action":"share","team_seq":preview["team_seq"],"revision":preview["revision"],"resolutions":resolutions})).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(
            body["completed"], true,
            "the resolved draft must not stay stuck"
        );
        body
    }

    async fn verify(&self) -> DocumentState {
        let events: Vec<engine_shared::Event> =
            sqlx::query_as("select * from event where document_id=$1 order by seq")
                .bind(self.doc)
                .fetch_all(&self.db.pool)
                .await
                .unwrap();
        let replay = Materializer::fold(&events).unwrap();
        let response = self
            .router
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/documents/{}", self.doc))
                    .header(header::AUTHORIZATION, &self.token)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body: Value =
            serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap())
                .unwrap();
        let state: DocumentState = serde_json::from_value(body["state"].clone()).unwrap();
        assert_eq!(state, replay);
        engine_core::log::verify_chain(&self.db.pool, engine_shared::DocumentId(self.doc))
            .await
            .unwrap();
        state
    }
}

fn choose(preview: &Value, value: &str) -> Value {
    Value::Object(
        preview["conflicts"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| (c["key"].as_str().unwrap().to_string(), json!(value)))
            .collect(),
    )
}

#[sqlx::test(migrations = "../../migrations")]
async fn stuck_draft_cycle_can_be_resolved_and_shared(pool: PgPool) {
    let f = Fixture::new(pool).await;
    let uri = f
        .draft(json!([{"type":"NodeMoved","node_id":f.b,"new_parent_id":f.a,"new_pos":"b0"}]))
        .await;
    f.batch(
        4,
        json!([{"type":"NodeMoved","node_id":f.a,"new_parent_id":f.b,"new_pos":"a0"}]),
    )
    .await;
    let preview = f.preview(&uri).await;
    let conflicts = preview["conflicts"].as_array().unwrap();
    assert_eq!(conflicts.len(), 1, "{preview}");
    assert_eq!(conflicts[0]["field"], "$location");
    f.rejected_share_preserves_history(
        &uri,
        json!({"action":"share","team_seq":preview["team_seq"],"revision":preview["revision"]}),
    )
    .await;
    let resolutions = choose(&preview, "draft");
    f.rejected_share_preserves_history(&uri, json!({"action":"share","team_seq":preview["team_seq"],"revision":0,"resolutions":resolutions})).await;
    f.share(&uri, &preview, resolutions).await;
    let state = f.verify().await;
    assert_eq!(
        state.nodes[&engine_shared::NodeId(f.a.clone())].parent_id,
        Some(engine_shared::NodeId(f.root.clone()))
    );
    assert_eq!(
        state.nodes[&engine_shared::NodeId(f.b.clone())].parent_id,
        Some(engine_shared::NodeId(f.a.clone()))
    );
    f.db.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn stuck_draft_orphan_restores_its_parent_after_review(pool: PgPool) {
    let f = Fixture::new(pool).await;
    let uri = f
        .draft(json!([{"type":"NodeMoved","node_id":f.child,"new_parent_id":f.b,"new_pos":"a0"}]))
        .await;
    f.batch(4, json!([{"type":"NodeDeleted","node_id":f.b}]))
        .await;
    let preview = f.preview(&uri).await;
    let conflicts = preview["conflicts"].as_array().unwrap();
    assert_eq!(conflicts.len(), 1, "{preview}");
    assert_eq!(conflicts[0]["node_id"], f.b);
    f.rejected_share_preserves_history(
        &uri,
        json!({"action":"share","team_seq":preview["team_seq"],"revision":preview["revision"]}),
    )
    .await;
    f.share(&uri, &preview, choose(&preview, "draft")).await;
    let state = f.verify().await;
    assert!(!state.nodes[&engine_shared::NodeId(f.b.clone())].deleted);
    assert_eq!(
        state.nodes[&engine_shared::NodeId(f.child.clone())].parent_id,
        Some(engine_shared::NodeId(f.b.clone()))
    );
    f.db.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn selective_share_brings_nested_parents_and_keeps_unselected_work_private(pool: PgPool) {
    let f = Fixture::new(pool).await;
    let outer = ulid::Ulid::generate().to_string();
    let inner = ulid::Ulid::generate().to_string();
    let leaf = ulid::Ulid::generate().to_string();
    let private = ulid::Ulid::generate().to_string();
    let uri = f.draft(json!([
        {"type":"NodeCreated","node_id":outer,"node_type":"section","parent_id":f.root,"pos":"c0","fields":{}},
        {"type":"NodeCreated","node_id":inner,"node_type":"section","parent_id":outer,"pos":"a0","fields":{}},
        {"type":"NodeCreated","node_id":leaf,"node_type":"item","parent_id":inner,"pos":"a0","fields":{"label":"Shared"}},
        {"type":"NodeCreated","node_id":private,"node_type":"item","parent_id":f.root,"pos":"d0","fields":{"label":"Private"}}
    ])).await;
    let preview = f.preview(&uri).await;
    assert!(preview["conflicts"].as_array().unwrap().is_empty());
    let (status, body) = f.post(&uri, json!({"action":"share","team_seq":preview["team_seq"],"revision":preview["revision"],"included_nodes":[leaf]})).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["completed"], false);
    let shared: Vec<_> = body["events"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["payload"]["node_id"].as_str().unwrap())
        .collect();
    assert_eq!(shared, vec![outer.as_str(), inner.as_str(), leaf.as_str()]);
    let state = f.verify().await;
    assert!(!state
        .nodes
        .contains_key(&engine_shared::NodeId(private.clone())));
    let remaining = f.preview(&uri).await;
    let ops = remaining["ops"].as_array().unwrap();
    assert_eq!(ops.len(), 1);
    assert_eq!(ops[0]["node_id"], private);
    f.share(&uri, &remaining, json!({})).await;
    assert!(f
        .verify()
        .await
        .nodes
        .contains_key(&engine_shared::NodeId(private)));
    f.db.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn duplicate_var_name_from_two_drafts_can_be_renamed_and_shared(pool: PgPool) {
    let f = Fixture::new(pool).await;
    let first = ulid::Ulid::generate().to_string();
    let second = ulid::Ulid::generate().to_string();
    let make = |id: &str| json!([{"type":"NodeCreated","node_id":id,"node_type":"item","parent_id":f.a,"pos":id,"var_name":"question","fields":{"label":id}}]);
    let first_uri = f.draft(make(&first)).await;
    let second_uri = f.draft(make(&second)).await;
    let preview = f.preview(&first_uri).await;
    f.share(&first_uri, &preview, json!({})).await;
    let preview = f.preview(&second_uri).await;
    let conflicts = preview["conflicts"].as_array().unwrap();
    assert_eq!(conflicts.len(), 1, "{preview}");
    assert_eq!(conflicts[0]["field"], "var_name");
    f.rejected_share_preserves_history(
        &second_uri,
        json!({"action":"share","team_seq":preview["team_seq"],"revision":preview["revision"]}),
    )
    .await;
    f.share(&second_uri, &preview, choose(&preview, "question_copy"))
        .await;
    let state = f.verify().await;
    assert_eq!(
        state.nodes[&engine_shared::NodeId(first)]
            .var_name
            .as_deref(),
        Some("question")
    );
    assert_eq!(
        state.nodes[&engine_shared::NodeId(second)]
            .var_name
            .as_deref(),
        Some("question_copy")
    );
    f.db.close().await;
}
