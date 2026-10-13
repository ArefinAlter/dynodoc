//! Project issues, labels and stars through the real router and the restricted
//! runtime login. Requires a reachable Postgres (DATABASE_URL), like the other API
//! integration tests. See docs/PROJECT-ISSUES-API.md.

use axum::body::{to_bytes, Body};
use axum::http::{header, Request, StatusCode};
use axum::Router;
use engine_api::auth::{
    store as auth_store,
    token::{self, DeviceClaims},
    AuthConfig,
};
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
async fn call(
    router: &Router,
    method: &str,
    uri: &str,
    token: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut req = Request::builder().method(method).uri(uri);
    if !token.is_empty() {
        req = req.header(header::AUTHORIZATION, token);
    }
    let req = match body {
        Some(body) => req
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap())),
        None => req.body(Body::empty()),
    }
    .unwrap();
    let resp = router.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}
async fn get(router: &Router, uri: &str, token: &str) -> (StatusCode, Value) {
    call(router, "GET", uri, token, None).await
}
async fn post(router: &Router, uri: &str, token: &str, body: Value) -> (StatusCode, Value) {
    call(router, "POST", uri, token, Some(body)).await
}
async fn patch(router: &Router, uri: &str, token: &str, body: Value) -> (StatusCode, Value) {
    call(router, "PATCH", uri, token, Some(body)).await
}
fn items(value: &Value) -> Vec<Value> {
    value["items"].as_array().unwrap().clone()
}
fn numbers(value: &Value) -> Vec<i64> {
    items(value)
        .iter()
        .map(|i| i["number"].as_i64().unwrap())
        .collect()
}

struct Team {
    pid: String,
    owner: (String, Uuid),
    manager: (String, Uuid),
    editor: (String, Uuid),
    /// Role `approver`, shown as "reviewer".
    approver: (String, Uuid),
    /// Role `reviewer`, shown as "contributor".
    contributor: (String, Uuid),
    viewer: (String, Uuid),
    stranger: (String, Uuid),
}

async fn team(pool: &PgPool, router: &Router) -> Team {
    let owner = person(pool, "maya@example.test", "Maya Chen").await;
    let manager = person(pool, "sam@example.test", "Sam Okoye").await;
    let editor = person(pool, "priya@example.test", "Priya Nair").await;
    let approver = person(pool, "noah@example.test", "Noah Berg").await;
    let contributor = person(pool, "lena@example.test", "Lena Voss").await;
    let viewer = person(pool, "vik@example.test", "Vik Rao").await;
    let stranger = person(pool, "ann@example.test", "Ann Outsider").await;
    let (status, created) =
        post(router, "/projects", &owner.0, json!({"name":"Launch plan"})).await;
    assert_eq!(status, StatusCode::OK, "{created}");
    let pid = created["id"].as_str().unwrap().to_owned();
    for (email, role) in [
        ("sam@example.test", "manager"),
        ("priya@example.test", "editor"),
        ("noah@example.test", "approver"),
        ("lena@example.test", "reviewer"),
        ("vik@example.test", "viewer"),
    ] {
        let (status, body) = post(
            router,
            &format!("/spaces/{pid}/members"),
            &owner.0,
            json!({"email":email,"role":role}),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
    }
    Team {
        pid,
        owner,
        manager,
        editor,
        approver,
        contributor,
        viewer,
        stranger,
    }
}

async fn open_issue(router: &Router, pid: &str, token: &str, body: Value) -> Value {
    let (status, issue) = post(router, &format!("/projects/{pid}/issues"), token, body).await;
    assert_eq!(status, StatusCode::OK, "{issue}");
    issue
}
async fn nums(router: &Router, issues: &str, token: &str, query: &str) -> Vec<i64> {
    let (status, page) = get(router, &format!("{issues}?state=all&{query}"), token).await;
    assert_eq!(status, StatusCode::OK, "{page}");
    numbers(&page)
}
async fn audit_actions(pool: &PgPool, pid: &str) -> Vec<String> {
    sqlx::query_scalar(
        "select action from operation_audit where resource_id=$1::uuid and (action like 'issue.%' or action like 'label.%' or action in ('project.starred','project.unstarred')) order by id",
    )
    .bind(pid)
    .fetch_all(pool)
    .await
    .unwrap()
}

#[sqlx::test(migrations = "../../migrations")]
async fn authority_lifecycle_filters_paging_labels_stars_and_audit(pool: PgPool) {
    let runtime = common::RuntimeDatabase::new(pool.clone()).await;
    let router = app(state(runtime.pool.clone(), ""));
    let t = team(&pool, &router).await;
    let pid = &t.pid;
    let issues = format!("/projects/{pid}/issues");
    let labels = format!("/projects/{pid}/labels");

    // Every current member reads, viewers included; nothing exists yet.
    for member in [
        &t.owner,
        &t.manager,
        &t.editor,
        &t.approver,
        &t.contributor,
        &t.viewer,
    ] {
        let (status, page) = get(&router, &issues, &member.0).await;
        assert_eq!(status, StatusCode::OK, "{page}");
        assert_eq!(page["items"], json!([]));
        assert_eq!(page["next_before"], Value::Null);
        assert_eq!(page["counts"], json!({"open":0,"closed":0}));
        assert_eq!(get(&router, &labels, &member.0).await.0, StatusCode::OK);
    }
    // Non-members and unknown projects are indistinguishable and see nothing.
    let unknown = Uuid::new_v4();
    for base in [format!("/projects/{pid}"), format!("/projects/{unknown}")] {
        for (method, uri, body) in [
            ("GET", format!("{base}/issues"), None),
            (
                "POST",
                format!("{base}/issues"),
                Some(json!({"title":"Hi"})),
            ),
            ("GET", format!("{base}/issues/1"), None),
            (
                "PATCH",
                format!("{base}/issues/1"),
                Some(json!({"state":"closed"})),
            ),
            (
                "POST",
                format!("{base}/issues/1/comments"),
                Some(json!({"body":"Hi"})),
            ),
            ("GET", format!("{base}/labels"), None),
            (
                "POST",
                format!("{base}/labels"),
                Some(json!({"name":"x","color":"aaaaaa"})),
            ),
            ("PUT", format!("{base}/star"), None),
            ("DELETE", format!("{base}/star"), None),
        ] {
            let (status, body) = call(&router, method, &uri, &t.stranger.0, body).await;
            assert_eq!(status, StatusCode::FORBIDDEN, "{method} {uri}: {body}");
        }
    }
    // Unauthenticated and project device tokens are refused by every route.
    let generation: i64 = sqlx::query_scalar("select session_generation from identity where id=$1")
        .bind(t.owner.1)
        .fetch_one(&pool)
        .await
        .unwrap();
    let device = format!(
        "Bearer {}",
        token::mint_project_device(
            &KEY,
            &DeviceClaims {
                identity: t.owner.1,
                key: Uuid::new_v4(),
                project: Uuid::parse_str(pid).unwrap(),
                session: Uuid::new_v4(),
                scopes: vec!["project:read".into(), "history:read".into()],
                generation,
            },
            3600,
        )
        .unwrap()
    );
    for bearer in ["", device.as_str()] {
        for (method, uri, body) in [
            ("GET", issues.clone(), None),
            ("POST", issues.clone(), Some(json!({"title":"Hi"}))),
            ("GET", labels.clone(), None),
            ("PUT", format!("/projects/{pid}/star"), None),
        ] {
            let (status, _) = call(&router, method, &uri, bearer, body).await;
            assert_eq!(status, StatusCode::UNAUTHORIZED, "{method} {uri}");
        }
    }

    // Labels belong to managers and owners.
    for who in [&t.editor, &t.approver, &t.contributor, &t.viewer] {
        let (status, body) = post(
            &router,
            &labels,
            &who.0,
            json!({"name":"Nope","color":"aaaaaa"}),
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
        assert!(body["details"]["reason"]
            .as_str()
            .unwrap()
            .contains("managers"));
    }
    let (status, evidence) = post(
        &router,
        &labels,
        &t.manager.0,
        json!({"name":"Evidence","color":"#D73A4A","description":"A source is missing"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{evidence}");
    assert_eq!(evidence["color"], "d73a4a");
    let (status, question) = post(
        &router,
        &labels,
        &t.owner.0,
        json!({"name":"Question","color":"0969da"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{question}");
    // Names are unique ignoring case; colours must be six hex digits.
    for body in [
        json!({"name":"EVIDENCE","color":"000000"}),
        json!({"name":" evidence ","color":"000000"}),
    ] {
        let (status, body) = post(&router, &labels, &t.manager.0, body).await;
        assert_eq!(status, StatusCode::CONFLICT, "{body}");
    }
    for body in [
        json!({"name":"Bad","color":"fff"}),
        json!({"name":"Bad","color":"gggggg"}),
        json!({"name":"","color":"aaaaaa"}),
        json!({"name":"x".repeat(51),"color":"aaaaaa"}),
        json!({"name":"Two\nlines","color":"aaaaaa"}),
        json!({"name":"Bad","color":"aaaaaa","description":"d".repeat(201)}),
    ] {
        let (status, body) = post(&router, &labels, &t.manager.0, body).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    }
    // A body that is not the documented shape (unknown or missing field) is a 422.
    for body in [
        json!({"name":"Bad","color":"aaaaaa","unknown":1}),
        json!({"color":"aaaaaa"}),
    ] {
        let (status, _) = post(&router, &labels, &t.manager.0, body).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    }
    let evidence_id = evidence["id"].as_str().unwrap().to_owned();
    let question_id = question["id"].as_str().unwrap().to_owned();
    let (status, renamed) = patch(
        &router,
        &format!("{labels}/{evidence_id}"),
        &t.owner.0,
        json!({"name":"Needs evidence","color":"1A7F37"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{renamed}");
    assert_eq!(renamed["name"], "Needs evidence");
    assert_eq!(renamed["color"], "1a7f37");
    assert_eq!(renamed["description"], "A source is missing");
    assert_eq!(
        patch(
            &router,
            &format!("{labels}/{question_id}"),
            &t.manager.0,
            json!({"name":"needs EVIDENCE"})
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    assert_eq!(
        patch(
            &router,
            &format!("{labels}/{question_id}"),
            &t.editor.0,
            json!({"color":"000000"})
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        patch(
            &router,
            &format!("{labels}/{}", Uuid::new_v4()),
            &t.manager.0,
            json!({"color":"000000"})
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    let (_, listed) = get(&router, &labels, &t.editor.0).await;
    assert_eq!(listed["can_manage"], false);
    assert_eq!(
        items(&listed)
            .iter()
            .map(|l| l["name"].as_str().unwrap().to_owned())
            .collect::<Vec<_>>(),
        ["Needs evidence", "Question"]
    );
    assert_eq!(
        get(&router, &labels, &t.manager.0).await.1["can_manage"],
        true
    );

    // Opening issues: everyone but viewers; text is trimmed, assignee must be a member.
    let (status, body) = post(
        &router,
        &issues,
        &t.viewer.0,
        json!({"title":"Viewer issue"}),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert!(body["details"]["reason"]
        .as_str()
        .unwrap()
        .contains("Viewers"));
    let (status, body) = post(
        &router,
        &issues,
        &t.editor.0,
        json!({"title":"X","assignee_id":t.stranger.1}),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    let (status, body) = post(
        &router,
        &issues,
        &t.editor.0,
        json!({"title":"X","labels":[Uuid::new_v4()]}),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    let first = open_issue(
        &router,
        pid,
        &t.editor.0,
        json!({"title":"  Missing source for the Q3 figure  ","body":"Where does 4.2M come from?","labels":[evidence_id,question_id,evidence_id],"assignee_id":t.viewer.1}),
    )
    .await;
    assert_eq!(first["number"], 1);
    assert_eq!(first["title"], "Missing source for the Q3 figure");
    assert_eq!(first["state"], "open");
    assert_eq!(
        first["author"],
        json!({"id":t.editor.1,"name":"Priya Nair"})
    );
    assert_eq!(first["assignee"]["id"], json!(t.viewer.1));
    assert_eq!(first["assignee"]["member"], true);
    assert_eq!(first["labels"].as_array().unwrap().len(), 2);
    assert_eq!(first["labels"][0]["name"], "Needs evidence");
    assert_eq!(first["revision"], 1);
    assert_eq!(first["comment_count"], 0);
    assert_eq!(first["closed_at"], Value::Null);
    assert_eq!(
        first["viewer"],
        json!({"role":"editor","can_comment":true,"can_edit":true})
    );
    let second = open_issue(
        &router,
        pid,
        &t.contributor.0,
        json!({"title":"Who owns the rollout checklist?"}),
    )
    .await;
    let third = open_issue(
        &router,
        pid,
        &t.approver.0,
        json!({"title":"Deck needs a legal read"}),
    )
    .await;
    assert_eq!(
        (second["number"].clone(), third["number"].clone()),
        (json!(2), json!(3))
    );

    // Editing: the author, a manager or an owner, with the current revision.
    let uri2 = format!("{issues}/2");
    let (status, body) = patch(
        &router,
        &uri2,
        &t.editor.0,
        json!({"title":"Hijacked","revision":1}),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert!(body["details"]["reason"]
        .as_str()
        .unwrap()
        .contains("author"));
    let (status, body) = patch(&router, &uri2, &t.viewer.0, json!({"state":"closed"})).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    let (status, body) = patch(
        &router,
        &uri2,
        &t.contributor.0,
        json!({"title":"No revision"}),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    let (status, body) = patch(&router, &uri2, &t.contributor.0, json!({})).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    let (status, body) = patch(&router, &uri2, &t.contributor.0, json!({"state":"merged"})).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    let (status, edited) = patch(
        &router,
        &uri2,
        &t.contributor.0,
        json!({"title":"Who owns the rollout checklist for launch?","body":"Needs an owner by Friday.","revision":1}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{edited}");
    assert_eq!(edited["revision"], 2);
    let (status, body) = patch(
        &router,
        &uri2,
        &t.contributor.0,
        json!({"body":"stale","revision":1}),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    // A manager edits anyone's issue; an unchanged patch writes nothing.
    let (status, managed) = patch(
        &router,
        &uri2,
        &t.manager.0,
        json!({"title":"Rollout checklist owner","revision":2,"assignee_id":t.editor.1}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{managed}");
    assert_eq!(managed["viewer"]["can_edit"], true);
    assert_eq!(managed["assignee"]["name"], "Priya Nair");
    let before = audit_actions(&pool, pid).await.len();
    let (status, same) = patch(
        &router,
        &uri2,
        &t.manager.0,
        json!({"title":"Rollout checklist owner","revision":3}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{same}");
    assert_eq!(same["revision"], 3);
    assert_eq!(audit_actions(&pool, pid).await.len(), before);
    // The assignee must be a member; null unassigns.
    let (status, body) = patch(
        &router,
        &uri2,
        &t.manager.0,
        json!({"assignee_id":t.stranger.1}),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    let (status, cleared) = patch(&router, &uri2, &t.manager.0, json!({"assignee_id":null})).await;
    assert_eq!(status, StatusCode::OK, "{cleared}");
    assert_eq!(cleared["assignee"], Value::Null);
    // Replace the label set; labels are checked against this project.
    let (status, relabelled) = patch(
        &router,
        &uri2,
        &t.manager.0,
        json!({"labels":[question_id]}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{relabelled}");
    assert_eq!(relabelled["labels"].as_array().unwrap().len(), 1);
    let (status, body) = patch(
        &router,
        &uri2,
        &t.manager.0,
        json!({"labels":[Uuid::new_v4()]}),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    // The prior text is kept in the audit detail, bounded.
    let long = "b".repeat(3000);
    let revision = relabelled["revision"].as_i64().unwrap();
    let (status, edited) = patch(
        &router,
        &uri2,
        &t.owner.0,
        json!({"body":long,"revision":revision}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{edited}");
    let (status, edited) = patch(
        &router,
        &uri2,
        &t.owner.0,
        json!({"body":"short","revision":edited["revision"]}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{edited}");
    let (prior, chars, previous_title): (String, i64, Option<String>) = sqlx::query_as(
        "select detail->>'previous_body',(detail->>'previous_body_chars')::bigint,detail->>'previous_title' from operation_audit where resource_id=$1::uuid and action='issue.edited' order by id desc limit 1",
    )
    .bind(pid)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!((prior.len(), chars, previous_title), (2000, 3000, None));
    let first_edit: Value = sqlx::query_scalar(
        "select detail from operation_audit where resource_id=$1::uuid and action='issue.edited' order by id limit 1",
    )
    .bind(pid)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        first_edit["previous_title"],
        "Who owns the rollout checklist?"
    );
    assert_eq!(first_edit["number"], 2);

    // Close and reopen (author or manager); closed issues still take comments.
    let uri1 = format!("{issues}/1");
    let (status, body) = patch(&router, &uri1, &t.approver.0, json!({"state":"closed"})).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    let (status, closed) = patch(&router, &uri1, &t.editor.0, json!({"state":"closed"})).await;
    assert_eq!(status, StatusCode::OK, "{closed}");
    assert_eq!(closed["state"], "closed");
    assert_eq!(closed["closed_by"]["name"], "Priya Nair");
    assert!(closed["closed_at"].is_string());
    let (_, open_only) = get(&router, &issues, &t.viewer.0).await;
    assert_eq!(numbers(&open_only), [3, 2]);
    assert_eq!(open_only["counts"], json!({"open":2,"closed":1}));
    assert_eq!(
        numbers(
            &get(&router, &format!("{issues}?state=closed"), &t.viewer.0)
                .await
                .1
        ),
        [1]
    );
    assert_eq!(
        numbers(
            &get(&router, &format!("{issues}?state=all"), &t.viewer.0)
                .await
                .1
        ),
        [3, 2, 1]
    );
    let (status, comment) = post(
        &router,
        &format!("{uri1}/comments"),
        &t.approver.0,
        json!({"body":"  Checking the ledger.  "}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{comment}");
    assert_eq!(comment["comment"]["seq"], 1);
    assert_eq!(comment["comment"]["body"], "Checking the ledger.");
    assert_eq!(comment["comment"]["author"]["name"], "Noah Berg");
    assert_eq!(comment["comment_count"], 1);
    let (status, _) = post(
        &router,
        &format!("{uri1}/comments"),
        &t.manager.0,
        json!({"body":"Source added."}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    for (who, expected) in [
        (&t.viewer, StatusCode::FORBIDDEN),
        (&t.stranger, StatusCode::FORBIDDEN),
    ] {
        let (status, body) = post(
            &router,
            &format!("{uri1}/comments"),
            &who.0,
            json!({"body":"Hi"}),
        )
        .await;
        assert_eq!(status, expected, "{body}");
    }
    assert_eq!(
        post(
            &router,
            &format!("{issues}/99/comments"),
            &t.manager.0,
            json!({"body":"Hi"})
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    // Comments page oldest first with an explicit marker.
    let (status, page) = get(&router, &format!("{uri1}?limit=1"), &t.viewer.0).await;
    assert_eq!(status, StatusCode::OK, "{page}");
    assert_eq!(
        page["viewer"],
        json!({"role":"viewer","can_comment":false,"can_edit":false})
    );
    assert_eq!(page["comments"]["items"].as_array().unwrap().len(), 1);
    assert_eq!(page["comments"]["items"][0]["seq"], 1);
    assert_eq!(page["comments"]["next_after"], 1);
    let (_, page) = get(&router, &format!("{uri1}?limit=1&after=1"), &t.viewer.0).await;
    assert_eq!(page["comments"]["items"][0]["seq"], 2);
    assert_eq!(page["comments"]["next_after"], Value::Null);
    assert_eq!(page["comment_count"], 2);
    assert_eq!(
        get(&router, &format!("{issues}/42"), &t.viewer.0).await.0,
        StatusCode::NOT_FOUND
    );
    let (status, reopened) = patch(&router, &uri1, &t.manager.0, json!({"state":"open"})).await;
    assert_eq!(status, StatusCode::OK, "{reopened}");
    assert_eq!(
        (reopened["state"].clone(), reopened["closed_by"].clone()),
        (json!("open"), Value::Null)
    );

    // Filters.
    assert_eq!(
        nums(
            &router,
            &issues,
            &t.owner.0,
            &format!("label={question_id}")
        )
        .await,
        [2, 1]
    );
    assert_eq!(
        nums(&router, &issues, &t.owner.0, "label=NEEDS%20EVIDENCE").await,
        [1]
    );
    assert_eq!(
        nums(
            &router,
            &issues,
            &t.owner.0,
            &format!("assignee={}", t.viewer.1)
        )
        .await,
        [1]
    );
    assert_eq!(
        nums(&router, &issues, &t.viewer.0, "assignee=me").await,
        [1]
    );
    assert_eq!(
        nums(&router, &issues, &t.owner.0, "assignee=none").await,
        [3, 2]
    );
    assert_eq!(
        nums(&router, &issues, &t.contributor.0, "author=me").await,
        [2]
    );
    assert_eq!(
        nums(
            &router,
            &issues,
            &t.owner.0,
            &format!("author={}", t.approver.1)
        )
        .await,
        [3]
    );
    assert_eq!(nums(&router, &issues, &t.owner.0, "q=CHECKLIST").await, [2]);
    assert_eq!(
        nums(&router, &issues, &t.owner.0, "q=zzz").await,
        Vec::<i64>::new()
    );
    assert_eq!(
        nums(&router, &issues, &t.owner.0, "q=%25").await,
        Vec::<i64>::new()
    );
    for bad_query in [
        "state=bogus".to_string(),
        "assignee=nobody".into(),
        "author=none".into(),
        "before=0".into(),
        format!("q={}", "x".repeat(101)),
    ] {
        let (status, _) = get(&router, &format!("{issues}?{bad_query}"), &t.owner.0).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{bad_query}");
    }

    // Keyset paging stays correct while the list grows.
    for n in 4..=7 {
        open_issue(
            &router,
            pid,
            &t.manager.0,
            json!({"title":format!("Follow-up {n}")}),
        )
        .await;
    }
    let (_, page) = get(&router, &format!("{issues}?state=all&limit=3"), &t.viewer.0).await;
    assert_eq!(numbers(&page), [7, 6, 5]);
    assert_eq!(page["next_before"], 5);
    open_issue(
        &router,
        pid,
        &t.manager.0,
        json!({"title":"Arrived mid-paging"}),
    )
    .await;
    let (_, page) = get(
        &router,
        &format!("{issues}?state=all&limit=3&before=5"),
        &t.viewer.0,
    )
    .await;
    assert_eq!(numbers(&page), [4, 3, 2]);
    let (_, page) = get(
        &router,
        &format!("{issues}?state=all&limit=3&before=2"),
        &t.viewer.0,
    )
    .await;
    assert_eq!(numbers(&page), [1]);
    assert_eq!(page["next_before"], Value::Null);
    assert_eq!(page["counts"], json!({"open":8,"closed":0}));
    assert_eq!(
        numbers(
            &get(&router, &format!("{issues}?limit=1000"), &t.viewer.0)
                .await
                .1
        )
        .len(),
        8
    );

    // Deleting a label removes it from every issue.
    let (status, gone) = call(
        &router,
        "DELETE",
        &format!("{labels}/{question_id}"),
        &t.editor.0,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{gone}");
    let (status, gone) = call(
        &router,
        "DELETE",
        &format!("{labels}/{question_id}"),
        &t.manager.0,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{gone}");
    assert_eq!(gone["issues_updated"], 2);
    assert_eq!(
        call(
            &router,
            "DELETE",
            &format!("{labels}/{question_id}"),
            &t.manager.0,
            None
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    let (_, one) = get(&router, &uri1, &t.viewer.0).await;
    assert_eq!(one["labels"].as_array().unwrap().len(), 1);
    assert_eq!(one["labels"][0]["name"], "Needs evidence");

    // Stars: any member, own star only, idempotent, counted among current members.
    let star = format!("/projects/{pid}/star");
    let (status, starred) = call(&router, "PUT", &star, &t.viewer.0, None).await;
    assert_eq!(status, StatusCode::OK, "{starred}");
    assert_eq!(starred, json!({"starred":true,"star_count":1}));
    assert_eq!(
        call(&router, "PUT", &star, &t.viewer.0, None).await.1["star_count"],
        1
    );
    assert_eq!(
        call(&router, "PUT", &star, &t.owner.0, None).await.1["star_count"],
        2
    );
    let (_, detail) = get(&router, &format!("/projects/{pid}"), &t.viewer.0).await;
    assert_eq!(
        (
            detail["starred"].clone(),
            detail["star_count"].clone(),
            detail["open_issue_count"].clone()
        ),
        (json!(true), json!(2), json!(8))
    );
    let (_, detail) = get(&router, &format!("/projects/{pid}"), &t.editor.0).await;
    assert_eq!(detail["starred"], false);
    for key in [
        "id",
        "name",
        "description",
        "revision",
        "role",
        "policy",
        "watch",
    ] {
        assert!(
            detail.get(key).is_some(),
            "existing project key {key} kept: {detail}"
        );
    }
    assert_eq!(detail["policy"]["merge_roles"], "editors");
    let (_, list) = get(&router, "/projects", &t.owner.0).await;
    let row = &items(&list)[0];
    assert_eq!(
        (
            row["starred"].clone(),
            row["star_count"].clone(),
            row["open_issue_count"].clone()
        ),
        (json!(true), json!(2), json!(8))
    );
    assert!(row.get("policy").is_some() && row.get("watch").is_some());
    let (_, unstarred) = call(&router, "DELETE", &star, &t.owner.0, None).await;
    assert_eq!(unstarred, json!({"starred":false,"star_count":1}));
    assert_eq!(
        call(&router, "DELETE", &star, &t.owner.0, None).await.1["star_count"],
        1
    );
    // A star no longer counts once the person is removed from the project.
    let (status, _) = post(
        &router,
        &format!("/spaces/{pid}/members"),
        &t.owner.0,
        json!({"email":"vik@example.test","role":"remove"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        get(&router, &format!("/projects/{pid}"), &t.owner.0)
            .await
            .1["star_count"],
        0
    );
    assert_eq!(
        get(&router, &uri1, &t.viewer.0).await.0,
        StatusCode::FORBIDDEN
    );
    // Their assignment is kept but marked stale.
    let (_, stale) = get(&router, &uri1, &t.owner.0).await;
    assert_eq!(stale["assignee"]["member"], false);

    let actions = audit_actions(&pool, pid).await;
    for expected in [
        "label.created",
        "label.edited",
        "label.deleted",
        "issue.created",
        "issue.edited",
        "issue.assigned",
        "issue.unassigned",
        "issue.labels_changed",
        "issue.closed",
        "issue.reopened",
        "issue.commented",
        "project.starred",
        "project.unstarred",
    ] {
        assert!(
            actions.iter().any(|a| a == expected),
            "{expected} in {actions:?}"
        );
    }
    // Idempotent repeats write one row each; refused requests write none.
    assert_eq!(
        actions.iter().filter(|a| *a == "project.starred").count(),
        2
    );
    assert_eq!(
        actions.iter().filter(|a| *a == "project.unstarred").count(),
        1
    );
    assert_eq!(actions.iter().filter(|a| *a == "label.created").count(), 2);
    runtime.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn limits_text_bounds_and_caps_are_enforced(pool: PgPool) {
    let runtime = common::RuntimeDatabase::new(pool.clone()).await;
    let router = app(state(runtime.pool.clone(), ""));
    let t = team(&pool, &router).await;
    let pid = &t.pid;
    let issues = format!("/projects/{pid}/issues");
    let labels = format!("/projects/{pid}/labels");
    let uuid = Uuid::parse_str(pid).unwrap();

    // Text bounds count characters; control characters (including NUL) are 400, never 500.
    for (title, ok) in [
        ("é".repeat(200), true),
        ("é".repeat(201), false),
        ("   ".into(), false),
        ("two\nlines".into(), false),
        ("nul\u{0}".into(), false),
        ("bell\u{7}".into(), false),
    ] {
        let (status, body) = post(&router, &issues, &t.editor.0, json!({"title":title})).await;
        assert_eq!(status == StatusCode::OK, ok, "{title:?}: {body}");
        if !ok {
            assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
        }
    }
    for (body_text, ok) in [
        ("é".repeat(20_000), true),
        ("é".repeat(20_001), false),
        ("nul\u{0}".into(), false),
        ("fine\nmultiline\ttext".into(), true),
    ] {
        let (status, body) = post(
            &router,
            &issues,
            &t.editor.0,
            json!({"title":"Body check","body":body_text}),
        )
        .await;
        assert_eq!(status == StatusCode::OK, ok, "{body}");
    }
    // Rejected requests did not consume issue numbers: one title and two bodies passed.
    let (_, page) = get(&router, &format!("{issues}?state=all"), &t.owner.0).await;
    assert_eq!(numbers(&page), [3, 2, 1]);
    let number: i32 =
        sqlx::query_scalar("select next_number from project_issue_counter where project_id=$1")
            .bind(uuid)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(number, 4);
    let comments = format!("{issues}/1/comments");
    for (text, ok) in [
        ("c".repeat(10_000), true),
        ("c".repeat(10_001), false),
        (String::new(), false),
        ("  \n ".into(), false),
        ("nul\u{0}".into(), false),
    ] {
        let (status, body) = post(&router, &comments, &t.editor.0, json!({"body":text})).await;
        assert_eq!(status == StatusCode::OK, ok, "{body}");
    }

    // 20 labels per issue.
    sqlx::query("insert into project_label(project_id,name,color,created_by) select $1,'L'||g,'aaaaaa',$2 from generate_series(1,21) g")
        .bind(uuid)
        .bind(t.owner.1)
        .execute(&pool)
        .await
        .unwrap();
    let ids: Vec<Uuid> =
        sqlx::query_scalar("select id from project_label where project_id=$1 order by name")
            .bind(uuid)
            .fetch_all(&pool)
            .await
            .unwrap();
    let (status, body) = post(
        &router,
        &issues,
        &t.editor.0,
        json!({"title":"Too many","labels":ids}),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    let (status, issue) = post(
        &router,
        &issues,
        &t.editor.0,
        json!({"title":"Twenty","labels":&ids[..20]}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{issue}");
    assert_eq!(issue["labels"].as_array().unwrap().len(), 20);
    let (status, body) = patch(
        &router,
        &format!("{issues}/{}", issue["number"]),
        &t.editor.0,
        json!({"labels":ids}),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    let (status, fewer) = patch(
        &router,
        &format!("{issues}/{}", issue["number"]),
        &t.editor.0,
        json!({"labels":&ids[10..21]}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{fewer}");
    assert_eq!(fewer["labels"].as_array().unwrap().len(), 11);

    // 100 labels per project: 21 exist, add 78 directly, then the API fills and refuses.
    sqlx::query("insert into project_label(project_id,name,color,created_by) select $1,'M'||g,'bbbbbb',$2 from generate_series(1,78) g")
        .bind(uuid)
        .bind(t.owner.1)
        .execute(&pool)
        .await
        .unwrap();
    let (status, body) = post(
        &router,
        &labels,
        &t.manager.0,
        json!({"name":"Hundredth","color":"cccccc"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (status, body) = post(
        &router,
        &labels,
        &t.manager.0,
        json!({"name":"Hundred and first","color":"cccccc"}),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(
        items(&get(&router, &labels, &t.manager.0).await.1).len(),
        100
    );

    // 1,000 comments per issue.
    let issue_id: Uuid =
        sqlx::query_scalar("select id from project_issue where project_id=$1 and number=1")
            .bind(uuid)
            .fetch_one(&pool)
            .await
            .unwrap();
    sqlx::query("update project_issue set comment_count=998 where id=$1")
        .bind(issue_id)
        .execute(&pool)
        .await
        .unwrap();
    for expected in [999, 1000] {
        let (status, body) = post(&router, &comments, &t.editor.0, json!({"body":"More"})).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body["comment"]["seq"], expected);
    }
    let (status, body) = post(
        &router,
        &comments,
        &t.editor.0,
        json!({"body":"One too many"}),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(
        get(
            &router,
            &format!("{issues}/1?limit=100&after=1000"),
            &t.owner.0
        )
        .await
        .1["comments"]["items"],
        json!([])
    );

    // 5,000 issues per project; numbers are never reused.
    sqlx::query("update project_issue_counter set next_number=4999 where project_id=$1")
        .bind(uuid)
        .execute(&pool)
        .await
        .unwrap();
    for expected in [4999, 5000] {
        let issue = open_issue(&router, pid, &t.editor.0, json!({"title":"Near the cap"})).await;
        assert_eq!(issue["number"], expected);
    }
    let (status, body) = post(
        &router,
        &issues,
        &t.editor.0,
        json!({"title":"Over the cap"}),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert!(body["details"]["reason"]
        .as_str()
        .unwrap()
        .contains("5,000"));
    let (_, page) = get(&router, &format!("{issues}?state=all&limit=2"), &t.owner.0).await;
    assert_eq!(numbers(&page), [5000, 4999]);
    runtime.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn concurrent_numbering_comments_and_label_cap_hold(pool: PgPool) {
    let runtime = common::RuntimeDatabase::new(pool.clone()).await;
    let router = app(state(runtime.pool.clone(), ""));
    let t = team(&pool, &router).await;
    let pid = t.pid.clone();
    let uuid = Uuid::parse_str(&pid).unwrap();
    let writers = [&t.owner, &t.manager, &t.editor, &t.approver];
    let issues = format!("/projects/{pid}/issues");

    // Four writers at a time (the per-project parallel allowance), eight rounds.
    let mut seen = Vec::new();
    for round in 0..8 {
        let calls = writers.iter().enumerate().map(|(i, who)| {
            let router = router.clone();
            let uri = issues.clone();
            let token = who.0.clone();
            async move {
                post(
                    &router,
                    &uri,
                    &token,
                    json!({"title":format!("Round {round} writer {i}")}),
                )
                .await
            }
        });
        for (status, body) in futures::future::join_all(calls).await {
            assert_eq!(status, StatusCode::OK, "{body}");
            seen.push(body["number"].as_i64().unwrap());
        }
    }
    seen.sort_unstable();
    assert_eq!(seen, (1..=32).collect::<Vec<_>>(), "no duplicates, no gaps");
    let (next, count): (i32, i64) = sqlx::query_as("select (select next_number from project_issue_counter where project_id=$1),(select count(*) from project_issue where project_id=$1)")
        .bind(uuid)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!((next, count), (33, 32));

    // Concurrent comments on one issue get distinct sequence numbers.
    let comments = format!("{issues}/1/comments");
    let calls = writers.iter().map(|who| {
        let router = router.clone();
        let uri = comments.clone();
        let token = who.0.clone();
        async move { post(&router, &uri, &token, json!({"body":"Same moment"})).await }
    });
    let mut seqs = Vec::new();
    for (status, body) in futures::future::join_all(calls).await {
        assert_eq!(status, StatusCode::OK, "{body}");
        seqs.push(body["comment"]["seq"].as_i64().unwrap());
    }
    seqs.sort_unstable();
    assert_eq!(seqs, [1, 2, 3, 4]);
    assert_eq!(
        get(&router, &format!("{issues}/1"), &t.owner.0).await.1["comment_count"],
        4
    );

    // The label cap holds when creators race: 98 exist, four try, two win.
    sqlx::query("insert into project_label(project_id,name,color,created_by) select $1,'L'||g,'aaaaaa',$2 from generate_series(1,98) g")
        .bind(uuid)
        .bind(t.owner.1)
        .execute(&pool)
        .await
        .unwrap();
    let labels = format!("/projects/{pid}/labels");
    let managers = [&t.owner, &t.manager, &t.owner, &t.manager];
    let calls = managers.iter().enumerate().map(|(i, who)| {
        let router = router.clone();
        let uri = labels.clone();
        let token = who.0.clone();
        async move {
            post(
                &router,
                &uri,
                &token,
                json!({"name":format!("Racer {i}"),"color":"123456"}),
            )
            .await
            .0
        }
    });
    let statuses = futures::future::join_all(calls).await;
    assert_eq!(
        statuses.iter().filter(|s| **s == StatusCode::OK).count(),
        2,
        "{statuses:?}"
    );
    assert_eq!(
        statuses
            .iter()
            .filter(|s| **s == StatusCode::CONFLICT)
            .count(),
        2,
        "{statuses:?}"
    );
    let total: i64 = sqlx::query_scalar("select count(*) from project_label where project_id=$1")
        .bind(uuid)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(total, 100);
    runtime.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn runtime_role_cannot_rewrite_or_delete_issue_history(pool: PgPool) {
    let runtime = common::RuntimeDatabase::new(pool.clone()).await;
    let router = app(state(runtime.pool.clone(), ""));
    let t = team(&pool, &router).await;
    let uuid = Uuid::parse_str(&t.pid).unwrap();
    let (_, label) = post(
        &router,
        &format!("/projects/{}/labels", t.pid),
        &t.manager.0,
        json!({"name":"Evidence","color":"d73a4a"}),
    )
    .await;
    let issue = open_issue(
        &router,
        &t.pid,
        &t.editor.0,
        json!({"title":"Immutable","labels":[label["id"]]}),
    )
    .await;
    post(
        &router,
        &format!("/projects/{}/issues/1/comments", t.pid),
        &t.editor.0,
        json!({"body":"Written once"}),
    )
    .await;
    call(
        &router,
        "PUT",
        &format!("/projects/{}/star", t.pid),
        &t.editor.0,
        None,
    )
    .await;
    assert_eq!(issue["number"], 1);

    // Runtime privileges are exactly the allowlist: comments insert-only, issues
    // never deleted, join rows and stars created or removed but not edited.
    for (table, column, update, delete) in [
        ("project_issue_comment", "body", false, false),
        ("project_issue", "title", true, false),
        ("project_issue_counter", "next_number", true, false),
        ("project_issue_label", "label_id", false, true),
        ("project_star", "created_at", false, true),
        ("project_label", "name", true, true),
    ] {
        let (can_update, can_delete, can_truncate): (bool, bool, bool) = sqlx::query_as(
            "select has_table_privilege(current_user,$1,'UPDATE'),has_table_privilege(current_user,$1,'DELETE'),has_table_privilege(current_user,$1,'TRUNCATE')",
        )
        .bind(table)
        .fetch_one(&runtime.pool)
        .await
        .unwrap();
        assert_eq!(
            (can_update, can_delete, can_truncate),
            (update, delete, false),
            "{table}"
        );
        for (allowed, statement) in [
            (
                update,
                format!("update {table} set {column}={column} where false"),
            ),
            (delete, format!("delete from {table} where false")),
            (false, format!("truncate {table} cascade")),
        ] {
            let result = sqlx::raw_sql(&statement).execute(&runtime.pool).await;
            if allowed {
                result.unwrap();
            } else {
                let error = result.unwrap_err();
                assert_eq!(
                    error.as_database_error().and_then(|e| e.code()).as_deref(),
                    Some("42501"),
                    "{statement}: {error}"
                );
            }
        }
    }
    // Even the table owner cannot edit a comment; the trigger refuses it.
    let error = sqlx::query("update project_issue_comment set body='Rewritten'")
        .execute(&pool)
        .await
        .unwrap_err();
    assert!(error.to_string().contains("append-only"), "{error}");
    // A label cannot be attached to another project's issue, whatever the API does.
    let other: Uuid = sqlx::query_scalar(
        "insert into access_space(name,kind,created_by) values('Other','project',$1) returning id",
    )
    .bind(t.owner.1)
    .fetch_one(&pool)
    .await
    .unwrap();
    sqlx::query("insert into project_settings(space_id) values($1)")
        .bind(other)
        .execute(&pool)
        .await
        .unwrap();
    let foreign: Uuid = sqlx::query_scalar("insert into project_label(project_id,name,color,created_by) values($1,'Foreign','aaaaaa',$2) returning id")
        .bind(other)
        .bind(t.owner.1)
        .fetch_one(&pool)
        .await
        .unwrap();
    let issue_id: Uuid = sqlx::query_scalar("select id from project_issue where project_id=$1")
        .bind(uuid)
        .fetch_one(&pool)
        .await
        .unwrap();
    for project in [uuid, other] {
        let error = sqlx::query(
            "insert into project_issue_label(issue_id,project_id,label_id) values($1,$2,$3)",
        )
        .bind(issue_id)
        .bind(project)
        .bind(foreign)
        .execute(&pool)
        .await
        .unwrap_err();
        assert_eq!(
            error.as_database_error().and_then(|e| e.code()).as_deref(),
            Some("23503"),
            "{error}"
        );
    }
    // Re-provisioning is idempotent and the startup gate still passes.
    engine_core::database::grant_runtime(&runtime.owner, &runtime.role)
        .await
        .unwrap();
    engine_core::database::check_runtime(&runtime.pool)
        .await
        .unwrap();
    // The comment and its sequence number are unchanged.
    let (body, seq): (String, i32) = sqlx::query_as("select body,seq from project_issue_comment")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!((body.as_str(), seq), ("Written once", 1));
    runtime.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn account_and_project_erasure_keep_only_anonymised_authorship(pool: PgPool) {
    let runtime = common::RuntimeDatabase::new(pool.clone()).await;
    let router = app(state(runtime.pool.clone(), "admin@example.test"));
    let t = team(&pool, &router).await;
    let (admin, _) = person(&pool, "admin@example.test", "Ada Admin").await;
    let pid = &t.pid;
    let issues = format!("/projects/{pid}/issues");
    // Priya writes an issue and a comment, stars the project, and is assigned
    // an issue by a manager.
    let (_, label) = post(
        &router,
        &format!("/projects/{pid}/labels"),
        &t.manager.0,
        json!({"name":"Evidence","color":"d73a4a"}),
    )
    .await;
    let own = open_issue(&router, pid, &t.editor.0, json!({"title":"Priya's private-sounding title","body":"Details only members may read","labels":[label["id"]]})).await;
    assert_eq!(own["number"], 1);
    let assigned = open_issue(
        &router,
        pid,
        &t.manager.0,
        json!({"title":"Needs Priya","assignee_id":t.editor.1}),
    )
    .await;
    assert_eq!(assigned["assignee"]["name"], "Priya Nair");
    let (status, _) = post(
        &router,
        &format!("{issues}/2/comments"),
        &t.editor.0,
        json!({"body":"On it"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = patch(
        &router,
        &format!("{issues}/1"),
        &t.editor.0,
        json!({"title":"Edited title","revision":1}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        call(
            &router,
            "PUT",
            &format!("/projects/{pid}/star"),
            &t.editor.0,
            None
        )
        .await
        .1["star_count"],
        1
    );

    // The audited administrator entrypoint: disable, preview, erase.
    let removed = post(
        &router,
        "/admin/actions",
        &admin,
        json!({"kind":"users","id":t.editor.1,"removed":true,"confirmation":"priya@example.test"}),
    )
    .await;
    assert_eq!(removed.0, StatusCode::OK, "{}", removed.1);
    let impact_uri = format!("/admin/impact?kind=users&id={}", t.editor.1);
    let (_, preview) = get(&router, &impact_uri, &admin).await;
    assert_eq!(preview["blockers"], json!([]), "{preview}");
    assert_eq!(preview["counts"]["issue_contributions_retained"], 2);
    let (status, erased) = post(
        &router,
        "/admin/erase",
        &admin,
        json!({"kind":"users","id":t.editor.1,"confirmation":"priya@example.test","revision":preview["revision"],"reason":"owner_request","acknowledge_backups":true}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{erased}");

    // Authorship stays under the anonymised form; stars and assignments are gone.
    let (_, kept) = get(&router, &format!("{issues}/1"), &t.owner.0).await;
    assert_eq!(
        kept["author"],
        json!({"id":t.editor.1,"name":"Deleted account"})
    );
    assert_eq!(kept["title"], "Edited title");
    let (_, other) = get(&router, &format!("{issues}/2"), &t.owner.0).await;
    assert_eq!(other["assignee"], Value::Null);
    assert_eq!(
        other["comments"]["items"][0]["author"]["name"],
        "Deleted account"
    );
    assert_eq!(other["comments"]["items"][0]["body"], "On it");
    let stars: i64 = sqlx::query_scalar("select count(*) from project_star where identity_id=$1")
        .bind(t.editor.1)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(stars, 0);
    assert_eq!(
        get(&router, &format!("/projects/{pid}"), &t.owner.0)
            .await
            .1["star_count"],
        0
    );
    // The receipt keeps opaque counts only.
    let counts: Value =
        sqlx::query_scalar("select counts from erasure_receipt where resource_id=$1")
            .bind(t.editor.1)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(
        counts.as_object().unwrap().values().all(Value::is_number),
        "{counts}"
    );
    assert_eq!(counts["issue_contributions_retained"], 2);
    assert!(!counts.to_string().contains("Edited title"));
    // The erased account can no longer act.
    assert_eq!(
        get(&router, &issues, &t.editor.0).await.0,
        StatusCode::UNAUTHORIZED
    );

    // Removing the project removes its issues, comments, labels, links and stars
    // by cascade; append-only audit rows remain.
    let audit_before: i64 =
        sqlx::query_scalar("select count(*) from operation_audit where resource_id=$1::uuid")
            .bind(pid)
            .fetch_one(&pool)
            .await
            .unwrap();
    call(
        &router,
        "PUT",
        &format!("/projects/{pid}/star"),
        &t.owner.0,
        None,
    )
    .await;
    sqlx::query("delete from project_settings where space_id=$1::uuid")
        .bind(pid)
        .execute(&pool)
        .await
        .unwrap();
    for table in [
        "project_issue",
        "project_issue_comment",
        "project_label",
        "project_issue_label",
        "project_star",
        "project_issue_counter",
    ] {
        let rows: i64 = sqlx::query_scalar(&format!("select count(*) from {table}"))
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(rows, 0, "{table}");
    }
    let audit_after: i64 =
        sqlx::query_scalar("select count(*) from operation_audit where resource_id=$1::uuid")
            .bind(pid)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(audit_after >= audit_before);
    runtime.close().await;
}
