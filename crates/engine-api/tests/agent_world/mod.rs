//! The harness of the agent route tests: a real router over the restricted runtime
//! login, a project with an owner, real engine operations to build files, and the account
//! flow that enrols an agent key, grants it and opens a device session. Shared by
//! `agent_read_test` and `agent_policy_test`; each declares `mod common;` first.
#![allow(dead_code)]

use axum::{
    body::{to_bytes, Body},
    http::{HeaderMap, Request, StatusCode},
    Router,
};
use engine_agent::model::FileKind;
use engine_api::{
    agent::StateCache,
    app,
    auth::{
        store,
        token::{self},
        AuthConfig,
    },
    AppState,
};
use engine_core::{
    materializer::DocumentState,
    repository::{
        device::{Challenge, Proof},
        device_session::{SessionChallenge, SessionProof},
    },
    snapshot::SnapshotEngine,
};
use engine_shared::DocumentId;
use ring::signature::{Ed25519KeyPair, KeyPair};
use serde_json::{json, Value};
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

pub const KEY: [u8; 32] = [9; 32];
pub const DCT: &str = "text/vnd.dynodoc.dct; v=1";

// ----------------------------------------------------------------------- plumbing

pub struct Reply {
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub bytes: Vec<u8>,
}
impl Reply {
    pub fn json(&self) -> Value {
        serde_json::from_slice(&self.bytes).unwrap_or(Value::Null)
    }
    pub fn text(&self) -> String {
        String::from_utf8(self.bytes.clone()).unwrap()
    }
    pub fn header(&self, name: &str) -> Option<String> {
        self.headers
            .get(name)
            .map(|v| v.to_str().unwrap().to_owned())
    }
    pub fn code(&self) -> u16 {
        self.status.as_u16()
    }
}

pub async fn raw(
    app: &Router,
    method: &str,
    uri: &str,
    token: Option<&str>,
    headers: &[(&str, &str)],
    body: Option<Value>,
) -> Reply {
    let mut request = Request::builder().method(method).uri(uri);
    if let Some(token) = token {
        request = request.header("authorization", format!("Bearer {token}"));
    }
    for (name, value) in headers {
        request = request.header(*name, *value);
    }
    let body = match body {
        Some(body) => {
            request = request.header("content-type", "application/json");
            Body::from(serde_json::to_vec(&body).unwrap())
        }
        None => Body::empty(),
    };
    let response = app
        .clone()
        .oneshot(request.body(body).unwrap())
        .await
        .unwrap();
    let (parts, body) = response.into_parts();
    Reply {
        status: parts.status,
        headers: parts.headers,
        bytes: to_bytes(body, 16 * 1024 * 1024).await.unwrap().to_vec(),
    }
}
pub async fn call(
    app: &Router,
    method: &str,
    uri: &str,
    token: Option<&str>,
    body: Value,
) -> (StatusCode, Value) {
    let body = (!body.is_null()).then_some(body);
    let reply = raw(app, method, uri, token, &[], body).await;
    (reply.status, reply.json())
}
pub async fn get(app: &Router, uri: &str, token: &str) -> Reply {
    raw(app, "GET", uri, Some(token), &[], None).await
}
pub async fn get_dct(app: &Router, uri: &str, token: &str) -> Reply {
    raw(app, "GET", uri, Some(token), &[("accept", DCT)], None).await
}

pub fn key(seed: u8) -> Ed25519KeyPair {
    Ed25519KeyPair::from_seed_unchecked(&[seed; 32]).unwrap()
}
pub fn ulid() -> String {
    ulid::Ulid::generate().to_string()
}
pub fn para(text: &str) -> Value {
    if text.is_empty() {
        return json!({"type":"paragraph"});
    }
    json!({"type":"paragraph","content":[{"type":"text","text":text}]})
}
pub fn heading(level: u8, text: &str) -> Value {
    json!({"type":"heading","attrs":{"level":level},"content":[{"type":"text","text":text}]})
}

/// Node creation and edits, to be sent as one batch of real engine operations.
#[derive(Default)]
pub struct Ops(pub Vec<Value>);
impl Ops {
    pub fn node(
        &mut self,
        node_type: &str,
        parent: Option<&str>,
        pos: u32,
        fields: Value,
    ) -> String {
        let id = ulid();
        let mut op = json!({"type":"NodeCreated","node_id":id,"node_type":node_type,
            "pos":format!("{pos:08}"),"fields":fields});
        if let Some(parent) = parent {
            op["parent_id"] = json!(parent);
        }
        self.0.push(op);
        id
    }
    pub fn block(&mut self, root: &str, pos: u32, content: Value) -> String {
        let label = engine_core::richtext::plain_text(&content);
        self.node(
            "item",
            Some(root),
            pos,
            json!({"label":label,"content":content}),
        )
    }
    pub fn edit(&mut self, node: &str, field: &str, value: Value) {
        self.0
            .push(json!({"type":"FieldEdited","node_id":node,"field":field,"value":value}));
    }
}

/// A person with an account, an agent key and (once granted) a device session token.
pub struct Agent {
    pub login: String,
    pub key: Ed25519KeyPair,
    pub key_id: Uuid,
}

pub struct World {
    pub app: Router,
    pub runtime: crate::common::RuntimeDatabase,
    pub cache: StateCache,
    pub owner_login: String,
    pub owner: Uuid,
    pub project: Uuid,
}

impl World {
    /// A world whose project has agents enabled (the owner saved the policy through the
    /// route), with a call budget the admission limits of the tests cannot reach first.
    pub async fn new(pool: PgPool, gateway: bool) -> Self {
        let w = Self::bare(pool, gateway).await;
        w.enable_agents(w.project).await;
        w
    }

    /// Enable agents for `project` as its owner (a project with no policy row is disabled).
    pub async fn enable_agents(&self, project: Uuid) {
        let reply = raw(
            &self.app,
            "PUT",
            &format!("/projects/{project}/agent-policy"),
            Some(&self.owner_login),
            &[("if-match", "0")],
            Some(json!({"enabled":true,"calls_per_minute":600})),
        )
        .await;
        assert_eq!(reply.code(), 200, "{}", reply.text());
    }

    /// A world with no agent policy: the default, in which agents are disabled.
    pub async fn bare(pool: PgPool, gateway: bool) -> Self {
        let runtime = crate::common::RuntimeDatabase::new(pool).await;
        let mut state = AppState::new(
            runtime.pool.clone(),
            AuthConfig {
                paseto_key: KEY,
                service_key: None,
                token_ttl_seconds: 3600,
                public_base_url: "http://test.local".into(),
            },
        );
        state.admin_emails = "admin@example.test".into();
        state.agent_gateway = gateway;
        let cache = state.agent_cache.clone();
        let app = app(state);
        let (owner, owner_login) = person(&runtime.pool, "owner@example.test").await;
        let (status, body) = call(
            &app,
            "POST",
            "/projects",
            Some(&owner_login),
            json!({"name":"Policy 2026"}),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let project = serde_json::from_value(body["id"].clone()).unwrap();
        Self {
            app,
            runtime,
            cache,
            owner_login,
            owner,
            project,
        }
    }

    pub fn pool(&self) -> &PgPool {
        &self.runtime.pool
    }

    pub async fn project(&self, name: &str) -> Uuid {
        let (status, body) = call(
            &self.app,
            "POST",
            "/projects",
            Some(&self.owner_login),
            json!({"name":name}),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        serde_json::from_value(body["id"].clone()).unwrap()
    }

    pub async fn member(&self, project: Uuid, who: Uuid, role: &str) {
        sqlx::query("insert into space_member(space_id,identity_id,role) values($1,$2,$3)")
            .bind(project)
            .bind(who)
            .bind(role)
            .execute(self.pool())
            .await
            .unwrap();
    }

    /// Create a file of `kind` in `project` from real engine operations.
    pub async fn file_in(&self, project: Uuid, title: &str, kind: &str, ops: Ops) -> Uuid {
        let owner = Some(self.owner_login.as_str());
        let (status, body) = call(
            &self.app,
            "POST",
            "/documents",
            owner,
            json!({"title":title}),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let doc = Uuid::parse_str(body["id"].as_str().unwrap()).unwrap();
        for (path, payload) in [
            (
                format!("/documents/{doc}/metadata"),
                json!({"settings":{"kind":kind}}),
            ),
            (
                format!("/documents/{doc}/location"),
                json!({"space_id":project}),
            ),
            (
                format!("/documents/{doc}/batch"),
                json!({"base_seq":0,"ops":ops.0}),
            ),
        ] {
            let (status, body) = call(&self.app, "POST", &path, owner, payload).await;
            assert_eq!(status, StatusCode::OK, "{path}: {body}");
        }
        doc
    }
    pub async fn file(&self, title: &str, kind: &str, ops: Ops) -> Uuid {
        self.file_in(self.project, title, kind, ops).await
    }

    /// Append real edits to an existing file.
    pub async fn edit(&self, doc: Uuid, ops: Ops) {
        let head: i64 =
            sqlx::query_scalar("select coalesce(max(seq),0) from event where document_id=$1")
                .bind(doc)
                .fetch_one(self.pool())
                .await
                .unwrap();
        let (status, body) = call(
            &self.app,
            "POST",
            &format!("/documents/{doc}/batch"),
            Some(&self.owner_login),
            json!({"base_seq":head,"ops":ops.0}),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
    }

    /// Enroll a key through the real account flow with `extra` merged into the request.
    /// Returns the key, its id, the exact body sent and the answer.
    pub async fn enroll(
        &self,
        login: &str,
        seed: u8,
        extra: Value,
    ) -> (Ed25519KeyPair, Uuid, Value, StatusCode, Value) {
        let key = key(seed);
        let (status, challenge) = call(
            &self.app,
            "POST",
            "/devices/challenge",
            Some(login),
            json!({"public_key":hex::encode(key.public_key().as_ref())}),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{challenge}");
        let challenge: Challenge = serde_json::from_value(challenge).unwrap();
        let proof = Proof::sign(
            challenge.clone(),
            challenge.authority,
            challenge.identity,
            &key,
        )
        .unwrap();
        let mut sent = json!(proof);
        for (name, value) in extra.as_object().unwrap() {
            sent[name] = value.clone();
        }
        let (status, answer) = call(&self.app, "POST", "/devices", Some(login), sent.clone()).await;
        (key, challenge.id, sent, status, answer)
    }

    /// Enroll an agent key (class agent) through the real account flow.
    pub async fn agent_key(&self, login: &str, seed: u8, mode: Option<&str>) -> Agent {
        let mut extra = json!({"class":"agent","label":"Claude Desktop on laptop"});
        if let Some(mode) = mode {
            extra["project_access"] = json!(mode);
        }
        let (key, key_id, _, status, body) = self.enroll(login, seed, extra).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body["class"], "agent");
        Agent {
            login: login.to_owned(),
            key,
            key_id,
        }
    }

    pub async fn empty_file(&self, title: &str, kind: &str) -> Uuid {
        let owner = Some(self.owner_login.as_str());
        let (status, body) = call(
            &self.app,
            "POST",
            "/documents",
            owner,
            json!({"title":title}),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let doc = Uuid::parse_str(body["id"].as_str().unwrap()).unwrap();
        for (path, payload) in [
            (
                format!("/documents/{doc}/metadata"),
                json!({"settings":{"kind":kind}}),
            ),
            (
                format!("/documents/{doc}/location"),
                json!({"space_id":self.project}),
            ),
        ] {
            let (status, body) = call(&self.app, "POST", &path, owner, payload).await;
            assert_eq!(status, StatusCode::OK, "{path}: {body}");
        }
        doc
    }

    pub async fn grant(
        &self,
        agent: &Agent,
        project: Uuid,
        scopes: &[&str],
    ) -> (StatusCode, Value) {
        call(
            &self.app,
            "PUT",
            &format!("/devices/{}/projects/{project}", agent.key_id),
            Some(&agent.login),
            json!({"scopes":scopes}),
        )
        .await
    }
    pub async fn ungrant(&self, agent: &Agent, project: Uuid) {
        let (status, body) = call(
            &self.app,
            "DELETE",
            &format!("/devices/{}/projects/{project}", agent.key_id),
            Some(&agent.login),
            Value::Null,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
    }

    pub async fn proof(&self, agent: &Agent, project: Uuid, scopes: &[&str]) -> SessionProof {
        let (status, body) = call(
            &self.app,
            "POST",
            &format!("/projects/{project}/device-sessions/challenge"),
            None,
            json!({"public_key":hex::encode(agent.key.public_key().as_ref()),"scopes":scopes}),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let challenge: SessionChallenge =
            serde_json::from_value(body["challenge"].clone()).unwrap();
        let scopes: Vec<String> = scopes.iter().map(|s| s.to_string()).collect();
        SessionProof::sign(
            challenge.clone(),
            body["mac"].as_str().unwrap().to_owned(),
            challenge.authority,
            project,
            &scopes,
            &agent.key,
        )
        .unwrap()
    }
    pub async fn open(&self, agent: &Agent, project: Uuid, scopes: &[&str]) -> (StatusCode, Value) {
        let proof = self.proof(agent, project, scopes).await;
        call(
            &self.app,
            "POST",
            &format!("/projects/{project}/device-sessions"),
            None,
            json!(proof),
        )
        .await
    }
    pub async fn token(&self, agent: &Agent, project: Uuid, scopes: &[&str]) -> String {
        let (status, body) = self.open(agent, project, scopes).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        body["access_token"].as_str().unwrap().to_owned()
    }

    /// An agent key for `identity`, granted `content:read` on the world's project, and a
    /// session token for it.
    pub async fn reader(&self, login: &str, seed: u8) -> (Agent, String) {
        let agent = self.agent_key(login, seed, None).await;
        let (status, body) = self.grant(&agent, self.project, &["content:read"]).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let token = self.token(&agent, self.project, &["content:read"]).await;
        (agent, token)
    }

    /// The owner's reader.
    pub async fn owner_reader(&self) -> (Agent, String) {
        let login = self.owner_login.clone();
        self.reader(&login, 7).await
    }

    pub fn url(&self, tail: &str) -> String {
        format!("/projects/{}/agent{tail}", self.project)
    }

    /// What the tools should return for `doc` at its head, computed with the library
    /// from the same database.
    pub async fn expected(
        &self,
        doc: Uuid,
        kind: FileKind,
    ) -> (engine_agent::model::FileMeta, DocumentState) {
        let (state, seq) = SnapshotEngine::new(self.pool().clone())
            .read_current_state_with_seq(DocumentId(doc))
            .await
            .unwrap();
        let chain: Option<Vec<u8>> =
            sqlx::query_scalar("select chain_hash from event where document_id=$1 and seq=$2")
                .bind(doc)
                .bind(seq)
                .fetch_optional(self.pool())
                .await
                .unwrap();
        let meta = engine_agent::model::FileMeta {
            id: DocumentId(doc),
            kind,
            rev: seq as u64,
            chain: hex::encode(chain.unwrap_or_else(|| vec![0; 32])),
        };
        (meta, state)
    }

    pub async fn head(&self, doc: Uuid) -> i64 {
        sqlx::query_scalar("select coalesce(max(seq),0) from event where document_id=$1")
            .bind(doc)
            .fetch_one(self.pool())
            .await
            .unwrap()
    }
    pub async fn etag(&self, doc: Uuid) -> String {
        let head = self.head(doc).await;
        let chain: Option<Vec<u8>> =
            sqlx::query_scalar("select chain_hash from event where document_id=$1 and seq=$2")
                .bind(doc)
                .bind(head)
                .fetch_optional(self.pool())
                .await
                .unwrap();
        format!(
            "\"{head}:{}\"",
            hex::encode(chain.unwrap_or_else(|| vec![0; 32]))[..8].to_owned()
        )
    }

    pub async fn close(self) {
        self.runtime.close().await;
    }
}

pub async fn person(pool: &PgPool, email: &str) -> (Uuid, String) {
    let id = store::upsert_identity(pool, email, Some("Tester"))
        .await
        .unwrap()
        .id
        .0;
    (id, token::mint(&KEY, id, 3600).unwrap())
}
