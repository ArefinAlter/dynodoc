//! The reviewer's side of AI-agent proposals (docs/AI-AGENT-CONNECTORS.md 6.2, migration
//! 0045) through the real router and the restricted runtime login:
//!
//! * an approval or a request for changes names the revision of the change request it was
//!   given for, is refused with `409 request_changed` when the request has moved, and counts
//!   only while the content is as it was at that revision (person requests and agent requests
//!   alike), including under real concurrency;
//! * the reviewers' list of agent proposals, the agent's own list, and adopting an agent's
//!   draft.
//!
//! Requires a reachable Postgres (DATABASE_URL).

use axum::http::StatusCode;
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

mod common;

mod agent_world;
use agent_world::*;

const WRITE: [&str; 3] = ["content:read", "content:propose", "content:submit"];

// ----------------------------------------------------------------------- helpers

async fn paper(w: &World, title: &str, texts: &[&str]) -> Uuid {
    let mut ops = Ops::default();
    let root = ops.node("form", None, 0, json!({"label":title}));
    for (i, text) in texts.iter().enumerate() {
        ops.block(&root, (i as u32 + 1) * 10, para(text));
    }
    w.file(title, "document", ops).await
}

async fn standard(w: &World) -> Uuid {
    paper(
        w,
        "Plan",
        &[
            "Alpha policy paragraph.",
            "Beta budget paragraph.",
            "Gamma schedule paragraph.",
            "Delta review paragraph.",
        ],
    )
    .await
}

async fn node_ids(w: &World, doc: Uuid) -> Vec<String> {
    sqlx::query_scalar("select id from node where document_id=$1 and parent_id is not null and not deleted order by pos,id")
        .bind(doc)
        .fetch_all(w.pool())
        .await
        .unwrap()
}

async fn count_where(w: &World, sql: &str, bind: Uuid) -> i64 {
    sqlx::query_scalar(sql)
        .bind(bind)
        .fetch_one(w.pool())
        .await
        .unwrap()
}

// --------------------------------------------------- a world of people and one document

/// A project with a document, a contributor (the author of requests), two reviewers, an
/// editor, and the owner (also the person who merges).
struct People {
    w: World,
    doc: Uuid,
    nodes: Vec<String>,
    author_login: String,
    rita_login: String,
    sam_login: String,
    editor: Uuid,
    editor_login: String,
}

async fn people(pool: PgPool, approvals: i16) -> People {
    let w = World::new(pool, true).await;
    let doc = standard(&w).await;
    let (author, author_login) = person(w.pool(), "author@example.test").await;
    w.member(w.project, author, "reviewer").await; // a contributor
    let (rita, rita_login) = person(w.pool(), "rita@example.test").await;
    w.member(w.project, rita, "approver").await;
    let (sam, sam_login) = person(w.pool(), "sam@example.test").await;
    w.member(w.project, sam, "approver").await;
    let (editor, editor_login) = person(w.pool(), "ed@example.test").await;
    w.member(w.project, editor, "editor").await;
    sqlx::query("insert into document_policy(document_id,protect_team_version,required_approvals,merge_roles) values($1,false,$2,'editors')")
        .bind(doc)
        .bind(approvals)
        .execute(&w.runtime.owner)
        .await
        .unwrap();
    let nodes = node_ids(&w, doc).await;
    People {
        w,
        doc,
        nodes,
        author_login,
        rita_login,
        sam_login,
        editor,
        editor_login,
    }
}

impl People {
    /// The two edits that change a paragraph's text (a label alone is redundant beside the
    /// rich content and is dropped).
    fn edit_ops(&self, node: usize, text: &str) -> Vec<Value> {
        vec![
            json!({"type":"FieldEdited","node_id":self.nodes[node],"field":"content","value":para(text)}),
            json!({"type":"FieldEdited","node_id":self.nodes[node],"field":"label","value":text}),
        ]
    }

    /// A submitted change request by `login`, changing the labels of `nodes`.
    async fn request_by(&self, login: &str, nodes: &[(usize, &str)]) -> Uuid {
        let head = self.w.head(self.doc).await;
        let ops: Vec<Value> = nodes
            .iter()
            .flat_map(|(i, t)| self.edit_ops(*i, t))
            .collect();
        let (status, body) = call(
            &self.w.app,
            "POST",
            &format!("/documents/{}/change-requests", self.doc),
            Some(login),
            json!({"name":"Wording","base_seq":head,"ops":ops,"submit":true}),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        Uuid::parse_str(body["id"].as_str().unwrap()).unwrap()
    }

    async fn request(&self, nodes: &[(usize, &str)]) -> Uuid {
        let login = self.author_login.clone();
        self.request_by(&login, nodes).await
    }

    fn uri(&self, draft: Uuid) -> String {
        format!("/documents/{}/change-requests/{draft}", self.doc)
    }

    async fn detail(&self, login: &str, draft: Uuid) -> Value {
        let (status, body) = call(
            &self.w.app,
            "GET",
            &self.uri(draft),
            Some(login),
            Value::Null,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        body
    }

    /// The contributor saves another label into the draft at `revision`.
    async fn change(
        &self,
        draft: Uuid,
        revision: i64,
        node: usize,
        text: &str,
    ) -> (StatusCode, Value) {
        call(
            &self.w.app,
            "POST",
            &format!("/documents/{}/drafts/{draft}", self.doc),
            Some(&self.author_login),
            json!({"revision":revision,"ops":self.edit_ops(node, text)}),
        )
        .await
    }

    async fn review(&self, login: &str, draft: Uuid, body: Value) -> (StatusCode, Value) {
        call(
            &self.w.app,
            "POST",
            &format!("{}/reviews", self.uri(draft)),
            Some(login),
            body,
        )
        .await
    }

    async fn approve(&self, login: &str, draft: Uuid, revision: Value) -> (StatusCode, Value) {
        self.review(
            login,
            draft,
            json!({"verdict":"approved","revision":revision}),
        )
        .await
    }

    async fn merge(&self, login: &str, draft: Uuid, seen: &Value) -> (StatusCode, Value) {
        call(
            &self.w.app,
            "POST",
            &format!("{}/merge", self.uri(draft)),
            Some(login),
            json!({"team_seq":seen["team_seq"],"revision":seen["revision"]}),
        )
        .await
    }

    async fn rows(&self, draft: Uuid) -> i64 {
        count_where(
            &self.w,
            "select count(*) from change_request_review where draft_id=$1",
            draft,
        )
        .await
    }
}

// ============================================================ 1. the binding, person requests

/// A reviewer who loaded revision N while the contributor saved N+1 gets a 409 with the code
/// `request_changed` and the current revision; nothing is written; reading the new version
/// and approving it counts.
#[sqlx::test(migrations = "../../migrations")]
async fn a_stale_approval_is_refused_with_409_and_a_current_one_counts(pool: PgPool) {
    let p = people(pool, 1).await;
    let id = p.request(&[(0, "Alpha one.")]).await;
    let seen = p.detail(&p.rita_login, id).await;
    let revision = seen["revision"].as_i64().unwrap();
    assert!(seen.get("agent_statement").is_none(), "only an agent's request has one");

    let (status, edited) = p.change(id, revision, 1, "Beta two.").await;
    assert_eq!(status, StatusCode::OK, "{edited}");
    assert_eq!(edited["revision"], json!(revision + 1));

    let (status, body) = p.approve(&p.rita_login, id, json!(revision)).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["code"], "request_changed", "{body}");
    assert_eq!(body["details"]["current_revision"], json!(revision + 1));
    assert_eq!(body["details"]["revision"], json!(revision));
    assert!(body["details"]["reason"]
        .as_str()
        .unwrap()
        .contains("changed"));
    assert_eq!(p.rows(id).await, 0, "a refused approval writes nothing");

    let fresh = p.detail(&p.rita_login, id).await;
    assert_eq!(fresh["revision"], json!(revision + 1));
    let (status, body) = p
        .approve(&p.rita_login, id, fresh["revision"].clone())
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["revision"], json!(revision + 1));
    assert_eq!(body["review"]["satisfied"], true, "{body}");
    assert_eq!(p.rows(id).await, 1);
    let stored: Option<i64> =
        sqlx::query_scalar("select draft_revision from change_request_review where draft_id=$1")
            .bind(id)
            .fetch_one(p.w.pool())
            .await
            .unwrap();
    assert_eq!(stored, Some(revision + 1));
    p.w.close().await;
}

/// The field is required for the two verdicts that decide, ignored for none: an older client
/// gets a clear 400 with its own code, not a silent success. A comment needs none.
#[sqlx::test(migrations = "../../migrations")]
async fn a_deciding_review_must_name_its_revision(pool: PgPool) {
    let p = people(pool, 1).await;
    let id = p.request(&[(0, "Alpha one.")]).await;
    for body in [
        json!({"verdict":"approved"}),
        json!({"verdict":"approved","note":"fine","revision":null}),
        json!({"verdict":"changes_requested","note":"Cite it."}),
    ] {
        let (status, got) = p.review(&p.rita_login, id, body.clone()).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body}: {got}");
        assert_eq!(got["code"], "revision_required", "{got}");
    }
    let (status, got) = p
        .review(
            &p.rita_login,
            id,
            json!({"verdict":"approved","revision":-1}),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{got}");
    let (status, got) = p
        .review(
            &p.rita_login,
            id,
            json!({"verdict":"approved","revision":"3"}),
        )
        .await;
    assert!(status.is_client_error(), "{got}");
    assert_eq!(p.rows(id).await, 0);
    // A comment is not a decision.
    let (status, got) = p
        .review(
            &p.rita_login,
            id,
            json!({"verdict":"commented","note":"Looks fine."}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{got}");
    assert_eq!(p.rows(id).await, 1);
    p.w.close().await;
}

/// Approve, then the contributor changes the request, then someone tries to merge: the old
/// approval is still in the table (history) and in the audit trail with the change that made
/// it lapse, but it does not count, so the merge is refused until a reviewer approves the new
/// version.
#[sqlx::test(migrations = "../../migrations")]
async fn an_approval_lapses_when_the_request_changes_and_stays_as_history(pool: PgPool) {
    let p = people(pool, 1).await;
    let id = p.request(&[(0, "Alpha one.")]).await;
    let seen = p.detail(&p.editor_login, id).await;
    let revision = seen["revision"].as_i64().unwrap();
    let (status, given) = p.approve(&p.rita_login, id, json!(revision)).await;
    assert_eq!(status, StatusCode::OK, "{given}");
    let review_id = given["id"].as_str().unwrap().to_owned();
    let counted = p.detail(&p.editor_login, id).await;
    assert_eq!(counted["review"]["approvals"], 1);
    assert_eq!(counted["review"]["satisfied"], true);

    let (status, edited) = p.change(id, revision, 1, "Beta two.").await;
    assert_eq!(status, StatusCode::OK, "{edited}");

    let after = p.detail(&p.editor_login, id).await;
    assert_eq!(after["review"]["approvals"], 0, "{after}");
    assert_eq!(after["review"]["satisfied"], false);
    let listed = after["review"]["reviews"].as_array().unwrap();
    assert_eq!(listed.len(), 1, "the approval is kept");
    assert_eq!(listed[0]["stale"], true);
    assert_eq!(listed[0]["revision"], json!(revision));
    assert_eq!(p.rows(id).await, 1);
    let head = p.w.head(p.doc).await;
    for stale_view in [&seen, &after] {
        let (status, body) = p.merge(&p.editor_login, id, stale_view).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    }
    assert_eq!(p.w.head(p.doc).await, head, "nothing was merged");

    // The trail: the approval row, and one audit row naming what lapsed and the change.
    let changed: Vec<Value> = sqlx::query_scalar("select detail from operation_audit where action='document.change_request_changed' and resource_id=$1")
        .bind(p.doc)
        .fetch_all(p.w.pool())
        .await
        .unwrap();
    assert_eq!(changed.len(), 1, "{changed:?}");
    assert_eq!(changed[0]["draft_id"], json!(id));
    assert_eq!(changed[0]["from_revision"], json!(revision));
    assert_eq!(changed[0]["to_revision"], json!(revision + 1));
    assert_eq!(changed[0]["lapsed"][0]["review"], json!(review_id));
    assert_eq!(changed[0]["lapsed"][0]["revision"], json!(revision));

    // A second change while nothing counts writes no second row.
    let (status, _) = p.change(id, revision + 1, 2, "Gamma three.").await;
    assert_eq!(status, StatusCode::OK);
    let rows: i64 = sqlx::query_scalar(
        "select count(*) from operation_audit where action='document.change_request_changed'",
    )
    .fetch_one(p.w.pool())
    .await
    .unwrap();
    assert_eq!(rows, 1);

    // Re-approval of what is there now lets it merge.
    let now = p.detail(&p.sam_login, id).await;
    let (status, body) = p.approve(&p.sam_login, id, now["revision"].clone()).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let now = p.detail(&p.editor_login, id).await;
    assert_eq!(now["review"]["satisfied"], true);
    let (status, merged) = p.merge(&p.editor_login, id, &now).await;
    assert_eq!(status, StatusCode::OK, "{merged}");
    p.w.close().await;
}

/// Merging only some of the changes moves the revision and not the content: the approvals
/// given for the content still count for the rest.
#[sqlx::test(migrations = "../../migrations")]
async fn merging_part_of_a_request_does_not_lapse_its_approvals(pool: PgPool) {
    let p = people(pool, 1).await;
    let id = p.request(&[(0, "Alpha one."), (1, "Beta two.")]).await;
    let seen = p.detail(&p.rita_login, id).await;
    let (status, body) = p.approve(&p.rita_login, id, seen["revision"].clone()).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let seen = p.detail(&p.editor_login, id).await;
    let (status, merged) = call(
        &p.w.app,
        "POST",
        &format!("{}/merge", p.uri(id)),
        Some(&p.editor_login),
        json!({"team_seq":seen["team_seq"],"revision":seen["revision"],"included_nodes":[p.nodes[0]]}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{merged}");
    assert_eq!(merged["completed"], false, "{merged}");
    let after = p.detail(&p.editor_login, id).await;
    assert!(after["revision"].as_i64() > seen["revision"].as_i64());
    assert_eq!(after["review"]["approvals"], 1, "{after}");
    assert_eq!(after["review"]["satisfied"], true);
    // The reviewer who read the page before the merge is told it moved.
    let (status, body) = p.approve(&p.sam_login, id, seen["revision"].clone()).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["code"], "request_changed");
    p.w.close().await;
}

/// Two reviewers approving the same revision at the same moment are both counted.
#[sqlx::test(migrations = "../../migrations")]
async fn two_reviewers_racing_on_one_revision_are_both_counted(pool: PgPool) {
    let p = people(pool, 2).await;
    let id = p.request(&[(0, "Alpha one.")]).await;
    let seen = p.detail(&p.rita_login, id).await;
    let (a, b) = tokio::join!(
        p.approve(&p.rita_login, id, seen["revision"].clone()),
        p.approve(&p.sam_login, id, seen["revision"].clone()),
    );
    assert_eq!(a.0, StatusCode::OK, "{}", a.1);
    assert_eq!(b.0, StatusCode::OK, "{}", b.1);
    let after = p.detail(&p.editor_login, id).await;
    assert_eq!(after["review"]["approvals"], 2, "{after}");
    assert_eq!(after["review"]["satisfied"], true);
    let (status, merged) = p.merge(&p.editor_login, id, &after).await;
    assert_eq!(status, StatusCode::OK, "{merged}");
    p.w.close().await;
}

/// An approval racing the contributor's change: whichever commits first, the approval never
/// counts for the new content. Either the approval lands and lapses with the change, or it
/// is refused as stale. Repeated, with real concurrent requests.
#[sqlx::test(migrations = "../../migrations")]
async fn an_approval_racing_a_change_never_counts_for_the_new_content(pool: PgPool) {
    let p = people(pool, 1).await;
    let id = p.request(&[(0, "Alpha one.")]).await;
    let (mut landed, mut refused) = (0, 0);
    for trial in 0..14 {
        let seen = p.detail(&p.rita_login, id).await;
        let revision = seen["revision"].as_i64().unwrap();
        let text = format!("Beta {trial}.");
        let (approval, edit) = tokio::join!(
            p.approve(&p.rita_login, id, json!(revision)),
            p.change(id, revision, 1, &text),
        );
        assert_eq!(edit.0, StatusCode::OK, "trial {trial}: {}", edit.1);
        match approval.0 {
            StatusCode::OK => landed += 1,
            StatusCode::CONFLICT => {
                refused += 1;
                assert_eq!(approval.1["code"], "request_changed");
            }
            other => panic!("trial {trial}: {other}: {}", approval.1),
        }
        let after = p.detail(&p.editor_login, id).await;
        assert_eq!(after["review"]["approvals"], 0, "trial {trial}: {after}");
        assert_eq!(after["review"]["satisfied"], false, "trial {trial}");
    }
    println!("landed then lapsed: {landed}, refused as stale: {refused}");
    // The database agrees: no counted row names the revision before the last change.
    let counted: i64 = sqlx::query_scalar("select count(*) from change_request_review r join workspace_draft w on w.id=r.draft_id where r.draft_id=$1 and r.verdict='approved' and r.draft_revision>=w.content_revision")
        .bind(id)
        .fetch_one(p.w.pool())
        .await
        .unwrap();
    assert_eq!(counted, 0);
    let now = p.detail(&p.sam_login, id).await;
    let (status, body) = p.approve(&p.sam_login, id, now["revision"].clone()).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["review"]["satisfied"], true);
    p.w.close().await;
}

/// The author cannot approve their own request, with or without the revision; nothing is
/// written.
#[sqlx::test(migrations = "../../migrations")]
async fn the_author_cannot_approve_their_own_request(pool: PgPool) {
    let p = people(pool, 1).await;
    // An editor can approve other people's requests, never their own.
    let id = p
        .request_by(&p.editor_login.clone(), &[(0, "Alpha one.")])
        .await;
    let seen = p.detail(&p.editor_login, id).await;
    assert_eq!(seen["request"]["author"]["id"], json!(p.editor));
    assert_eq!(seen["review"]["can_approve"], false);
    let (status, body) = p
        .approve(&p.editor_login, id, seen["revision"].clone())
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert!(body["details"]["reason"]
        .as_str()
        .unwrap()
        .contains("your own"));
    let (status, body) = p
        .review(&p.editor_login, id, json!({"verdict":"approved"}))
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(p.rows(id).await, 0);
    // And a contributor who cannot approve anything is refused as before.
    let other = p.request(&[(1, "Beta two.")]).await;
    let seen = p.detail(&p.author_login, other).await;
    let (status, _) = p
        .approve(&p.author_login, other, seen["revision"].clone())
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(p.rows(other).await, 0);
    p.w.close().await;
}

/// An approval given for one request, sent again to another request, is refused when that
/// one is at another revision; and the database refuses a row for any revision but the
/// draft's current one, whoever writes it.
#[sqlx::test(migrations = "../../migrations")]
async fn an_approval_replayed_on_another_request_is_refused(pool: PgPool) {
    let p = people(pool, 1).await;
    let a = p.request(&[(0, "Alpha one.")]).await;
    let b = p.request(&[(1, "Beta two.")]).await;
    // Move B on by two changes, so its revision is not A's.
    let seen_b = p.detail(&p.rita_login, b).await;
    let rb = seen_b["revision"].as_i64().unwrap();
    assert_eq!(p.change(b, rb, 2, "Gamma three.").await.0, StatusCode::OK);
    assert_eq!(
        p.change(b, rb + 1, 3, "Delta four.").await.0,
        StatusCode::OK
    );
    let seen_a = p.detail(&p.rita_login, a).await;
    let body = json!({"verdict":"approved","revision":seen_a["revision"]});
    let (status, got) = p.review(&p.rita_login, a, body.clone()).await;
    assert_eq!(status, StatusCode::OK, "{got}");
    let (status, got) = p.review(&p.rita_login, b, body).await;
    assert_eq!(status, StatusCode::CONFLICT, "{got}");
    assert_eq!(got["code"], "request_changed");
    assert_eq!(p.rows(b).await, 0);

    // Straight into the table, as the owner of the database.
    let owner = &p.w.runtime.owner;
    let rita: Uuid = sqlx::query_scalar("select id from identity where email='rita@example.test'")
        .fetch_one(owner)
        .await
        .unwrap();
    let insert = |revision: Option<i64>, verdict: &'static str| {
        let (doc, draft) = (p.doc, b);
        async move {
            sqlx::query("insert into change_request_review(document_id,draft_id,reviewer_id,verdict,note,draft_revision) values($1,$2,$3,$4,'',$5)")
                .bind(doc)
                .bind(draft)
                .bind(rita)
                .bind(verdict)
                .bind(revision)
                .execute(owner)
                .await
        }
    };
    let current: i64 = sqlx::query_scalar("select revision from workspace_draft where id=$1")
        .bind(b)
        .fetch_one(owner)
        .await
        .unwrap();
    assert!(
        insert(Some(current - 1), "approved").await.is_err(),
        "an older revision"
    );
    assert!(
        insert(Some(current + 1), "approved").await.is_err(),
        "a revision not reached"
    );
    assert!(insert(None, "approved").await.is_err(), "no revision");
    assert!(
        insert(None, "changes_requested").await.is_err(),
        "no revision"
    );
    assert!(insert(Some(current), "approved").await.is_ok());
    assert!(
        insert(None, "commented").await.is_ok(),
        "a comment names none"
    );
    // The counter only moves forward.
    for statement in [
        "update workspace_draft set revision=revision-1 where id=$1",
        "update workspace_draft set content_revision=revision+1 where id=$1",
        "update workspace_draft set content_revision=content_revision-1 where id=$1 and content_revision>0",
    ] {
        let result = sqlx::query(statement).bind(b).execute(owner).await;
        assert!(result.is_err(), "{statement}");
    }
    p.w.close().await;
}

/// Reviews written before migration 0045 name no revision and keep the rule they were
/// written under: they count when written after the last content change. Reproduced here by
/// turning the new guards off for the test database.
#[sqlx::test(migrations = "../../migrations")]
async fn a_review_without_a_revision_keeps_the_older_rule(pool: PgPool) {
    let p = people(pool, 1).await;
    let id = p.request(&[(0, "Alpha one.")]).await;
    let owner = &p.w.runtime.owner;
    sqlx::raw_sql("alter table change_request_review drop constraint change_request_review_decision_revision; alter table change_request_review disable trigger change_request_review_revision;")
        .execute(owner)
        .await
        .unwrap();
    let rita: Uuid = sqlx::query_scalar("select id from identity where email='rita@example.test'")
        .fetch_one(owner)
        .await
        .unwrap();
    sqlx::query("insert into change_request_review(document_id,draft_id,reviewer_id,verdict,note,created_at) values($1,$2,$3,'approved','',now()+interval '1 minute')")
        .bind(p.doc)
        .bind(id)
        .bind(rita)
        .execute(owner)
        .await
        .unwrap();
    let after = p.detail(&p.editor_login, id).await;
    assert_eq!(
        after["review"]["approvals"], 1,
        "written after the last change: {after}"
    );
    assert!(after["review"]["reviews"][0]["revision"].is_null());
    // The contributor changes the request: the older rule lapses it as it always did.
    let revision = after["revision"].as_i64().unwrap();
    assert_eq!(
        p.change(id, revision, 1, "Beta two.").await.0,
        StatusCode::OK
    );
    sqlx::query(
        "update workspace_draft set content_changed_at=now()+interval '2 minutes' where id=$1",
    )
    .bind(id)
    .execute(owner)
    .await
    .unwrap();
    let later = p.detail(&p.editor_login, id).await;
    assert_eq!(later["review"]["approvals"], 0, "{later}");
    assert_eq!(later["review"]["reviews"][0]["stale"], true);
    p.w.close().await;
}

// ============================================================ 2. agent requests

/// Writing on, and the agent may submit by itself.
fn write_full() -> Value {
    json!({"write_mode":"change_request","submit_requires_person":false,"proposals_per_day":1000,"open_proposals":100})
}

async fn policy_revision(w: &World) -> i64 {
    let view = get(
        &w.app,
        &format!("/projects/{}/agent-policy", w.project),
        &w.owner_login,
    )
    .await;
    assert_eq!(view.code(), 200, "{}", view.text());
    view.json()["revision"].as_i64().unwrap()
}

async fn set_policy(w: &World, extra: Value) -> Reply {
    let mut body = json!({"enabled": true});
    for (name, value) in extra.as_object().unwrap() {
        body[name] = value.clone();
    }
    let revision = policy_revision(w).await.to_string();
    raw(
        &w.app,
        "PUT",
        &format!("/projects/{}/agent-policy", w.project),
        Some(&w.owner_login),
        &[("if-match", &revision)],
        Some(body),
    )
    .await
}

async fn enroll_claude(w: &World, login: &str, seed: u8) -> Agent {
    let (key, key_id, _, status, body) = w
        .enroll(
            login,
            seed,
            json!({"class":"agent","label":"Claude Desktop","client_kind":"claude"}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    Agent {
        login: login.to_owned(),
        key,
        key_id,
    }
}

/// Olivia (an editor, the principal) with a Claude key granted the three content scopes.
struct Setup {
    w: World,
    olivia: Uuid,
    login: String,
    token: String,
}

async fn setup(pool: PgPool, policy: Value) -> Setup {
    let w = World::new(pool, true).await;
    let saved = set_policy(&w, policy).await;
    assert_eq!(saved.code(), 200, "{}", saved.text());
    let (olivia, login) = person(w.pool(), "olivia@example.test").await;
    w.member(w.project, olivia, "editor").await;
    let agent = enroll_claude(&w, &login, 7).await;
    let (status, body) = w.grant(&agent, w.project, &WRITE).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let token = w.token(&agent, w.project, &WRITE).await;
    Setup {
        w,
        olivia,
        login,
        token,
    }
}

async fn outline(w: &World, token: &str, doc: Uuid) -> Value {
    let reply = get(&w.app, &w.url(&format!("/files/{doc}/outline")), token).await;
    assert_eq!(reply.code(), 200, "{}", reply.text());
    reply.json()
}

fn block(outline: &Value, i: usize) -> (String, String) {
    let b = &outline["blocks"][i];
    (
        b["ref"].as_str().unwrap().to_owned(),
        b["h"].as_str().unwrap().to_owned(),
    )
}

async fn propose(w: &World, token: &str, doc: Uuid, body: Value) -> Reply {
    raw(
        &w.app,
        "POST",
        &w.url(&format!("/files/{doc}/propose")),
        Some(token),
        &[],
        Some(body),
    )
    .await
}

async fn post_to(w: &World, token: &str, path: &str, body: Value) -> Reply {
    raw(&w.app, "POST", &w.url(path), Some(token), &[], Some(body)).await
}

/// A fresh proposal that sets paragraph `i` to `text`. Returns the proposal id.
async fn open_at(s: &Setup, doc: Uuid, i: usize, text: &str, name: Option<&str>) -> Uuid {
    let o = outline(&s.w, &s.token, doc).await;
    let (r, h) = block(&o, i);
    let mut body = json!({"rev":o["rev"],"request_id":Uuid::new_v4(),"ops":[{"op":"set_text","ref":r,"text":text,"base":h,"why":"clearer"}]});
    if let Some(name) = name {
        body["name"] = json!(name);
    }
    let reply = propose(&s.w, &s.token, doc, body).await;
    assert_eq!(reply.code(), 200, "{}", reply.text());
    Uuid::parse_str(reply.json()["proposal"].as_str().unwrap()).unwrap()
}

/// The agent adds another edit to its own proposal.
async fn extend(s: &Setup, doc: Uuid, proposal: Uuid, i: usize, text: &str) -> Reply {
    let o = outline(&s.w, &s.token, doc).await;
    let (r, h) = block(&o, i);
    propose(
        &s.w,
        &s.token,
        doc,
        json!({"rev":o["rev"],"request_id":Uuid::new_v4(),"proposal":proposal,"ops":[{"op":"set_text","ref":r,"text":text,"base":h}]}),
    )
    .await
}

async fn draft_revision(w: &World, draft: Uuid) -> i64 {
    sqlx::query_scalar("select revision from workspace_draft where id=$1")
        .bind(draft)
        .fetch_one(w.pool())
        .await
        .unwrap()
}

async fn agent_identity_of(w: &World, principal: Uuid) -> Uuid {
    sqlx::query_scalar(
        "select identity_id from agent_identity where principal_id=$1 and revoked_at is null",
    )
    .bind(principal)
    .fetch_one(w.pool())
    .await
    .unwrap()
}

/// A person submits (adopts) the agent's draft with the revision they read.
async fn adopt(s: &Setup, doc: Uuid, draft: Uuid, body: Value) -> (StatusCode, Value) {
    call(
        &s.w.app,
        "POST",
        &format!("/documents/{doc}/drafts/{draft}/submit"),
        Some(&s.login),
        body,
    )
    .await
}

async fn request_as(w: &World, login: &str, doc: Uuid, draft: Uuid) -> Value {
    let (status, body) = call(
        &w.app,
        "GET",
        &format!("/documents/{doc}/change-requests/{draft}"),
        Some(login),
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    body
}

async fn review_as(
    w: &World,
    login: &str,
    doc: Uuid,
    draft: Uuid,
    body: Value,
) -> (StatusCode, Value) {
    call(
        &w.app,
        "POST",
        &format!("/documents/{doc}/change-requests/{draft}/reviews"),
        Some(login),
        body,
    )
    .await
}

/// An agent's request needs the approvals the file asks for; the person it acts for cannot
/// give one; an approval of a reviewer lapses when the agent adds to the proposal, and a
/// click on the older revision is refused. An agent identity never counts, even with every
/// other lock off.
#[sqlx::test(migrations = "../../migrations")]
async fn an_agents_request_needs_current_approvals_and_an_agent_never_counts(pool: PgPool) {
    let s = setup(pool, write_full()).await;
    let w = &s.w;
    let doc = standard(w).await;
    sqlx::query("insert into document_policy(document_id,protect_team_version,required_approvals,merge_roles) values($1,false,1,'editors')")
        .bind(doc)
        .execute(&w.runtime.owner)
        .await
        .unwrap();
    let (rita, rita_login) = person(w.pool(), "rita@example.test").await;
    w.member(w.project, rita, "approver").await;
    let p = open_at(&s, doc, 0, "Alpha procedure paragraph.", None).await;
    let sent = post_to(w, &s.token, &format!("/proposals/{p}/submit"), json!({})).await;
    assert_eq!(sent.json()["state"], "open", "{}", sent.text());

    let seen = request_as(w, &rita_login, doc, p).await;
    assert!(seen["request"]["agent"].is_object());
    let revision = seen["revision"].clone();
    // The principal cannot approve it, revision or not.
    let (status, body) = review_as(
        w,
        &s.login,
        doc,
        p,
        json!({"verdict":"approved","revision":revision}),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    // The reviewer's approval counts.
    let (status, body) = review_as(
        w,
        &rita_login,
        doc,
        p,
        json!({"verdict":"approved","revision":revision}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["review"]["satisfied"], true);
    // The agent adds an edit: the approval no longer counts, the old click is refused.
    let more = extend(&s, doc, p, 1, "Beta budget revised.").await;
    assert_eq!(more.code(), 200, "{}", more.text());
    let after = request_as(w, &rita_login, doc, p).await;
    assert_eq!(after["review"]["approvals"], 0, "{after}");
    assert_eq!(after["review"]["satisfied"], false);
    let (status, body) = review_as(
        w,
        &rita_login,
        doc,
        p,
        json!({"verdict":"approved","revision":revision}),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["code"], "request_changed");
    // What the agent sees agrees.
    let status = get(&w.app, &w.url(&format!("/proposals/{p}")), &s.token)
        .await
        .json();
    assert_eq!(status["approvals"], 0);
    assert_eq!(status["satisfied"], false);
    // The audit row of the agent's extension names the approval it lapsed.
    let changed: Vec<Value> = sqlx::query_scalar("select detail from operation_audit where action='document.change_request_changed' and resource_id=$1")
        .bind(doc)
        .fetch_all(w.pool())
        .await
        .unwrap();
    assert_eq!(changed.len(), 1, "{changed:?}");
    assert_eq!(changed[0]["lapsed"][0]["reviewer"], json!(rita));

    // The agent identity: no account session at all...
    let agent = agent_identity_of(w, s.olivia).await;
    let minted = engine_api::auth::token::mint(&KEY, agent, 3600).unwrap();
    let now = request_as(w, &rita_login, doc, p).await;
    let (status, body) = review_as(
        w,
        &minted,
        doc,
        p,
        json!({"verdict":"approved","revision":now["revision"]}),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "{body}");
    // ... and even with the triggers that keep an agent out of the tables turned off and a
    // membership forced in, the approval row of another person's agent identity (neither the
    // author nor the principal of this request, so no other rule excludes it) is not counted.
    let (pat, pat_login) = person(w.pool(), "pat@example.test").await;
    w.member(w.project, pat, "editor").await;
    let pats = enroll_claude(w, &pat_login, 8).await;
    assert_eq!(w.grant(&pats, w.project, &WRITE).await.0, StatusCode::OK);
    let pats_agent = agent_identity_of(w, pat).await;
    assert_ne!(pats_agent, agent);
    let owner = &w.runtime.owner;
    sqlx::raw_sql("alter table space_member disable trigger space_member_no_agent; alter table change_request_review disable trigger change_request_review_no_agent; alter table identity disable trigger identity_agent_account;")
        .execute(owner)
        .await
        .unwrap();
    // An enabled account with a membership: only the agent rule is left to stop it.
    sqlx::query("update identity set disabled_at=null where id=$1")
        .bind(pats_agent)
        .execute(owner)
        .await
        .unwrap();
    sqlx::query("insert into space_member(space_id,identity_id,role) values($1,$2,'approver')")
        .bind(w.project)
        .bind(pats_agent)
        .execute(owner)
        .await
        .unwrap();
    sqlx::query("insert into change_request_review(document_id,draft_id,reviewer_id,verdict,note,draft_revision) values($1,$2,$3,'approved','',$4)")
        .bind(doc)
        .bind(p)
        .bind(pats_agent)
        .bind(draft_revision(w, p).await)
        .execute(owner)
        .await
        .unwrap();
    let forced = request_as(w, &rita_login, doc, p).await;
    assert_eq!(
        forced["review"]["reviews"].as_array().unwrap().len(),
        2,
        "{forced}"
    );
    assert_eq!(forced["review"]["approvals"], 0, "{forced}");
    assert_eq!(forced["review"]["satisfied"], false);
    s.w.close().await;
}

// ============================================================ 3. adopting an agent's draft

/// The person an agent acts for submits its draft as themselves ("adopt"): they must send the
/// revision they read, a draft the agent has added to since is refused, and the audit row
/// shows both the person who adopted it and the agent that proposed it.
#[sqlx::test(migrations = "../../migrations")]
async fn adopting_an_agents_draft_binds_the_revision_and_names_both(pool: PgPool) {
    let s = setup(pool, json!({"write_mode":"draft"})).await;
    let w = &s.w;
    let doc = standard(w).await;
    let p = open_at(&s, doc, 0, "Alpha adopted paragraph.", None).await;
    let seen = draft_revision(w, p).await;

    let (status, body) = adopt(&s, doc, p, json!({"note":"Mine."})).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(body["code"], "revision_required");

    // The agent adds to the draft after she read it.
    assert_eq!(extend(&s, doc, p, 1, "Beta added later.").await.code(), 200);
    let (status, body) = adopt(&s, doc, p, json!({"note":"Mine.","revision":seen})).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["code"], "request_changed");
    assert_eq!(
        body["details"]["current_revision"],
        json!(draft_revision(w, p).await)
    );
    let submitted: bool =
        sqlx::query_scalar("select submitted_at is not null from workspace_draft where id=$1")
            .bind(p)
            .fetch_one(w.pool())
            .await
            .unwrap();
    assert!(!submitted, "nothing was submitted");

    let now = draft_revision(w, p).await;
    let (status, body) = adopt(&s, doc, p, json!({"note":"Mine.","revision":now})).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let agent = agent_identity_of(w, s.olivia).await;
    let (actor, detail): (Uuid, Value) = sqlx::query_as("select actor_id,detail from operation_audit where action='document.change_request_submitted' and detail->>'draft_id'=$1")
        .bind(p.to_string())
        .fetch_one(w.pool())
        .await
        .unwrap();
    assert_eq!(actor, s.olivia, "who adopted");
    assert_eq!(detail["contributor"], json!(agent), "who proposed");
    assert_eq!(detail["contributor_kind"], "agent");
    assert_eq!(detail["adopted"], true);
    assert_eq!(detail["agent"]["principal"], json!(s.olivia));
    let created: Uuid = sqlx::query_scalar("select actor_id from operation_audit where action='agent.proposal.created' and resource_id=$1")
        .bind(p)
        .fetch_one(w.pool())
        .await
        .unwrap();
    assert_eq!(created, agent);
    // The request is still the agent's: its principal cannot approve it.
    let seen = request_as(w, &s.login, doc, p).await;
    assert_eq!(seen["request"]["mine"], true);
    assert_eq!(seen["review"]["can_approve"], false);
    // The reviewer is shown what the agent said, as untrusted text, and who and what it was.
    let said = &seen["agent_statement"];
    assert_eq!(said["trust"], "untrusted_text", "{seen}");
    assert_eq!(said["calls"], 2);
    assert_eq!(said["ops_in"], 2);
    assert_eq!(said["reasons"][0]["why"], "clearer");
    assert_eq!(said["reasons"][0]["op"], "set_text");
    assert_eq!(seen["request"]["agent"]["client_kind"], "claude");
    assert_eq!(seen["request"]["agent"]["display_name"], "Claude for Tester");

    // A draft of the person's own needs no revision, but one that is sent must be right.
    let own: Value = call(
        &w.app,
        "POST",
        &format!("/documents/{doc}/drafts"),
        Some(&s.login),
        json!({"name":"Mine"}),
    )
    .await
    .1;
    let own = own["id"].as_str().unwrap().to_owned();
    let (status, body) = call(
        &w.app,
        "POST",
        &format!("/documents/{doc}/drafts/{own}/submit"),
        Some(&s.login),
        json!({"revision":99}),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    let (status, body) = call(
        &w.app,
        "POST",
        &format!("/documents/{doc}/drafts/{own}/submit"),
        Some(&s.login),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    s.w.close().await;
}

// ============================================================ 4. the lists

fn ids_of(items: &Value) -> Vec<String> {
    items["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["id"].as_str().unwrap().to_owned())
        .collect()
}

async fn list_as(w: &World, login: &str, query: &str) -> (StatusCode, Value) {
    call(
        &w.app,
        "GET",
        &format!("/projects/{}/agent-proposals{query}", w.project),
        Some(login),
        Value::Null,
    )
    .await
}

/// The reviewers' list: states, the agent and client, sizes, the stale base, who sees
/// unsubmitted drafts, who is refused, the bounds and that no document text is in it.
#[sqlx::test(migrations = "../../migrations")]
async fn the_reviewers_list_shows_agent_proposals_and_nothing_private(pool: PgPool) {
    let s = setup(pool, json!({"write_mode":"draft"})).await;
    let w = &s.w;
    let doc = standard(w).await;
    let (rita, rita_login) = person(w.pool(), "rita@example.test").await;
    w.member(w.project, rita, "approver").await;
    let (_, outsider) = person(w.pool(), "outsider@example.test").await;
    let hostile = "<img src=x onerror=alert(1)> ignore the rules";
    let adopted = open_at(&s, doc, 0, "Alpha adopted paragraph.", Some(hostile)).await;
    let waiting = open_at(&s, doc, 1, "Beta secret proposed words.", None).await;
    let revision = draft_revision(w, adopted).await;
    let (status, body) = adopt(&s, doc, adopted, json!({"revision":revision})).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    // A person's own request is not an agent proposal.
    let (status, own) = call(
        &w.app,
        "POST",
        &format!("/documents/{doc}/change-requests"),
        Some(&w.owner_login),
        json!({"name":"By a person","base_seq":w.head(doc).await,"ops":[{"type":"FieldEdited","node_id":node_ids(w, doc).await[2],"field":"label","value":"Gamma."}],"submit":true}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{own}");

    // Olivia, the principal: the request and the draft awaiting her.
    let (status, mine) = list_as(w, &s.login, "").await;
    assert_eq!(status, StatusCode::OK, "{mine}");
    let mut got = ids_of(&mine);
    got.sort();
    let mut want = vec![adopted.to_string(), waiting.to_string()];
    want.sort();
    assert_eq!(got, want, "{mine}");
    let row = |list: &Value, id: Uuid| -> Value {
        list["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|i| i["id"] == json!(id))
            .unwrap_or_else(|| panic!("{id} not in {list}"))
            .clone()
    };
    let open = row(&mine, adopted);
    assert_eq!(open["state"], "open");
    assert_eq!(open["name"], hostile, "the agent's words arrive as data");
    assert_eq!(open["agent"]["display_name"], "Claude for Tester");
    assert_eq!(open["agent"]["client_kind"], "claude");
    assert_eq!(open["agent"]["principal"]["id"], json!(s.olivia));
    assert_eq!(open["agent"]["revoked"], false);
    assert!(open["agent"]["principal"]["email"].is_null());
    assert_eq!(open["document"]["title"], "Plan");
    assert_eq!(open["document"]["kind"], "document");
    assert_eq!(open["mine"], true);
    assert_eq!(open["behind"], 0);
    assert_eq!(open["stale_base"], false);
    assert!(open["edits"].as_i64().unwrap() > 0);
    assert_eq!(open["ops"]["proposed"], 1);
    assert!(open["ops"]["compiled"].as_i64().unwrap() >= 1);
    assert_eq!(row(&mine, waiting)["state"], "draft");
    assert_eq!(mine["state"], "active");
    assert_eq!(mine["more"], false);
    // Budgets of her own agents: two opened today, two open.
    assert_eq!(mine["budgets"]["proposals_today"]["used"], 2);
    assert_eq!(mine["budgets"]["open_proposals"]["used"], 2);
    assert_eq!(mine["budgets"]["ops_per_proposal"], 500);
    // No document text, no reasons, no placeholder address.
    let text = mine.to_string();
    for private in [
        "secret proposed words",
        "adopted paragraph",
        "clearer",
        "agents.invalid",
    ] {
        assert!(!text.contains(private), "{private} leaked: {text}");
    }

    // The owner and Rita: the submitted request only, no draft; and no budgets.
    for login in [&w.owner_login, &rita_login] {
        let (status, theirs) = list_as(w, login, "?state=all").await;
        assert_eq!(status, StatusCode::OK, "{theirs}");
        assert_eq!(ids_of(&theirs), vec![adopted.to_string()], "{theirs}");
        assert_eq!(row(&theirs, adopted)["mine"], false);
        assert!(theirs["budgets"].is_null());
    }
    // Refusals: a stranger, no token, a device token.
    let (status, _) = list_as(w, &outsider, "").await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _) = call(
        &w.app,
        "GET",
        &format!("/projects/{}/agent-proposals", w.project),
        None,
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let device = get(
        &w.app,
        &format!("/projects/{}/agent-proposals", w.project),
        &s.token,
    )
    .await;
    assert_eq!(device.code(), 401, "{}", device.text());

    // Filters and bounds.
    let (_, only) = list_as(w, &s.login, "?state=draft").await;
    assert_eq!(ids_of(&only), vec![waiting.to_string()]);
    let (_, only) = list_as(w, &s.login, "?state=open").await;
    assert_eq!(ids_of(&only), vec![adopted.to_string()]);
    let (_, none) = list_as(w, &s.login, "?state=merged").await;
    assert!(ids_of(&none).is_empty());
    for bad in [
        "?state=nope",
        "?limit=0",
        "?limit=51",
        "?offset=-1",
        "?offset=1001",
        "?who=me",
        "?limit=x",
    ] {
        let (status, body) = list_as(w, &s.login, bad).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{bad}: {body}");
    }
    let (_, page) = list_as(w, &s.login, "?limit=1").await;
    assert_eq!(page["items"].as_array().unwrap().len(), 1);
    assert_eq!(page["more"], true);
    let (_, next) = list_as(w, &s.login, "?limit=1&offset=1").await;
    assert_eq!(next["items"].as_array().unwrap().len(), 1);
    assert_eq!(next["more"], false);
    assert_ne!(ids_of(&page), ids_of(&next));

    // The team moves on: the base is behind, and the warning is on the active ones.
    let node = node_ids(w, doc).await.remove(3);
    let mut ops = Ops::default();
    ops.edit(&node, "label", json!("Delta changed by the team."));
    w.edit(doc, ops).await;
    let (_, behind) = list_as(w, &s.login, "").await;
    for id in [adopted, waiting] {
        let r = row(&behind, id);
        assert_eq!(r["behind"], 1, "{r}");
        assert_eq!(r["stale_base"], true);
    }

    // A counted reviewer asks for changes: the state says so.
    let seen = request_as(w, &rita_login, doc, adopted).await;
    let (status, body) = review_as(
        w,
        &rita_login,
        doc,
        adopted,
        json!({"verdict":"changes_requested","note":"Cite it.","revision":seen["revision"]}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (_, asked) = list_as(w, &s.login, "?state=changes_requested").await;
    assert_eq!(ids_of(&asked), vec![adopted.to_string()]);
    // Declined: out of the active list, in "declined" and "all", no stale warning.
    let (status, body) = call(
        &w.app,
        "POST",
        &format!("/documents/{doc}/change-requests/{adopted}/decline"),
        Some(&w.owner_login),
        json!({"note":"No."}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (_, active) = list_as(w, &s.login, "").await;
    assert_eq!(ids_of(&active), vec![waiting.to_string()]);
    let (_, declined) = list_as(w, &s.login, "?state=declined").await;
    assert_eq!(row(&declined, adopted)["stale_base"], false);
    let (_, all) = list_as(w, &s.login, "?state=all").await;
    assert_eq!(ids_of(&all).len(), 2);
    assert_eq!(ids_of(&all)[0], waiting.to_string(), "open before closed");
    s.w.close().await;
}

/// An agent of another person stays private to that person until it is submitted.
#[sqlx::test(migrations = "../../migrations")]
async fn the_reviewers_list_keeps_another_persons_agent_drafts_private(pool: PgPool) {
    let s = setup(pool, json!({"write_mode":"draft"})).await;
    let w = &s.w;
    let doc = standard(w).await;
    let drafted = open_at(&s, doc, 0, "Alpha drafted.", None).await;
    let revision = draft_revision(w, drafted).await;
    assert_eq!(
        adopt(&s, doc, drafted, json!({"revision":revision}))
            .await
            .0,
        StatusCode::OK
    );
    // A second person with an agent of her own.
    let (pat, pat_login) = person(w.pool(), "pat@example.test").await;
    w.member(w.project, pat, "editor").await;
    let pats = enroll_claude(w, &pat_login, 8).await;
    assert_eq!(w.grant(&pats, w.project, &WRITE).await.0, StatusCode::OK);
    let pat_token = w.token(&pats, w.project, &WRITE).await;
    let o = outline(w, &pat_token, doc).await;
    let (r, h) = block(&o, 2);
    let theirs = propose(w, &pat_token, doc, json!({"rev":o["rev"],"request_id":Uuid::new_v4(),"ops":[{"op":"set_text","ref":r,"text":"Gamma by Pat's agent.","base":h}]})).await;
    assert_eq!(theirs.code(), 200, "{}", theirs.text());
    let pats_draft = theirs.json()["proposal"].as_str().unwrap().to_owned();
    // Olivia does not see Pat's draft; Pat sees it, and Olivia's request.
    let (_, olivia) = list_as(w, &s.login, "?state=all").await;
    assert_eq!(ids_of(&olivia), vec![drafted.to_string()]);
    let (_, pat_sees) = list_as(w, &pat_login, "?state=all").await;
    let mut got = ids_of(&pat_sees);
    got.sort();
    let mut want = vec![drafted.to_string(), pats_draft.clone()];
    want.sort();
    assert_eq!(got, want, "{pat_sees}");
    // Each agent lists only its own proposals.
    let mine = get(&w.app, &w.url("/proposals?state=all"), &pat_token)
        .await
        .json();
    assert_eq!(mine["n"], 1);
    assert_eq!(mine["proposals"][0]["proposal"], json!(pats_draft));
    s.w.close().await;
}

/// The agent's own list: its proposals only, filters, bounds, the DCT form, the budgets, a
/// key with no agent identity, and the OpenAPI description.
#[sqlx::test(migrations = "../../migrations")]
async fn an_agent_can_list_its_own_proposals(pool: PgPool) {
    let s = setup(pool, json!({"write_mode":"draft"})).await;
    let w = &s.w;
    let doc = standard(w).await;
    let a = open_at(&s, doc, 0, "Alpha one.", None).await;
    let b = open_at(&s, doc, 1, "Beta two.", None).await;
    let revision = draft_revision(w, b).await;
    assert_eq!(
        adopt(&s, doc, b, json!({"revision":revision})).await.0,
        StatusCode::OK
    );

    let listed = get(&w.app, &w.url("/proposals"), &s.token).await;
    assert_eq!(listed.code(), 200, "{}", listed.text());
    let body = listed.json();
    assert_eq!(body["tool"], "status");
    assert_eq!(body["what"], "proposals");
    assert_eq!(body["trust"], "data");
    assert_eq!(body["n"], 2);
    assert_eq!(body["more"], false);
    assert_eq!(body["opened_today"], 2);
    assert_eq!(body["open"], 2);
    assert_eq!(body["opened_limit"], 20);
    assert_eq!(body["open_limit"], 5);
    assert_eq!(body["write_mode"], "draft");
    let rows = body["proposals"].as_array().unwrap();
    let find = |id: Uuid| {
        rows.iter()
            .find(|r| r["proposal"] == json!(id))
            .unwrap()
            .clone()
    };
    assert_eq!(find(a)["state"], "draft");
    assert_eq!(find(b)["state"], "open");
    assert_eq!(find(a)["file"], json!(doc));
    assert_eq!(find(a)["behind"], 0);
    assert!(find(a)["edits"].as_i64().unwrap() > 0);
    assert!(
        !body.to_string().contains("Alpha one"),
        "ids and numbers only"
    );

    let drafts = get(&w.app, &w.url("/proposals?state=draft"), &s.token)
        .await
        .json();
    assert_eq!(drafts["n"], 1);
    assert_eq!(drafts["proposals"][0]["proposal"], json!(a));
    for bad in [
        "?state=zzz",
        "?limit=0",
        "?limit=51",
        "?offset=1001",
        "?q=1",
    ] {
        let reply = get(&w.app, &w.url(&format!("/proposals{bad}")), &s.token).await;
        assert_eq!(reply.code(), 400, "{bad}: {}", reply.text());
        assert_eq!(reply.json()["details"]["agent_code"], "bad_request");
    }
    let paged = get(&w.app, &w.url("/proposals?limit=1"), &s.token)
        .await
        .json();
    assert_eq!(paged["more"], true);
    assert_eq!(paged["n"], 1);

    // The DCT form: a header and one line per proposal, ids and numbers only.
    let dct = get_dct(&w.app, &w.url("/proposals?state=all"), &s.token).await;
    assert_eq!(dct.code(), 200, "{}", dct.text());
    let text = dct.text();
    assert_eq!(
        text.lines().filter(|l| l.starts_with("proposal ")).count(),
        2,
        "{text}"
    );
    assert!(
        text.contains(&a.to_string()) && text.contains(&b.to_string()),
        "{text}"
    );

    // It is one ledger row of the `status` tool, and not another route's.
    let rows: i64 =
        sqlx::query_scalar("select count(*) from agent_call where tool='status' and outcome='ok'")
            .fetch_one(w.pool())
            .await
            .unwrap();
    assert!(rows >= 3);

    // A file whose policy is `none` drops out of the list.
    let (status, set) = call(
        &w.app,
        "PUT",
        &format!("/documents/{doc}/agent-policy"),
        Some(&w.owner_login),
        json!({"access":"none"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{set}");
    let hidden = get(&w.app, &w.url("/proposals?state=all"), &s.token)
        .await
        .json();
    assert_eq!(hidden["n"], 0, "{hidden}");

    // A key with only the read scope has no agent identity yet: an empty list, not an error.
    let (_, token) = w.reader(&s.login, 11).await;
    let empty = get(&w.app, &w.url("/proposals"), &token).await;
    assert_eq!(empty.code(), 200, "{}", empty.text());
    assert_eq!(empty.json()["n"], 0);

    // Another person's token sees none of these, and a login is refused.
    let login = get(&w.app, &w.url("/proposals"), &s.login).await;
    assert_eq!(login.code(), 401);

    // The OpenAPI document describes the answer.
    let spec = serde_json::to_value(<engine_api::ApiDoc as utoipa::OpenApi>::openapi()).unwrap();
    let schema = &spec["components"]["schemas"]["AgentProposalList"];
    assert!(schema.is_object(), "AgentProposalList is documented");
    let mut problems = Vec::new();
    conforms(&spec, schema, &body, "$", &mut problems);
    assert!(problems.is_empty(), "{problems:?}");
    s.w.close().await;
}
