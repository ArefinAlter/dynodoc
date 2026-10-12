//! Read-only project activity: a bounded feed and contributor totals derived from
//! existing history. Nothing here writes; non-members cannot tell a project exists.
//!
//! The feed merges seven bounded sources by `(time, document, kind, id)` so a page
//! boundary never repeats or skips a row: team events, submitted/merged/declined
//! change requests, named versions, file creation and project access changes. One
//! write transaction's events share a timestamp and are shown as a single item.
use crate::{
    auth::{
        device::{ProjectAuth, ProjectRead},
        AuthContext,
    },
    error::ApiError,
    pagination::Cursor,
    projects::require,
    AppState,
};
use axum::{
    extract::{Path, Query, State},
    routing::get,
    Json, Router,
};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::{PgConnection, Row};
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/projects/:id/activity", get(project_activity_feed))
        .route("/projects/:id/contributors", get(project_contributors))
}

const DEFAULT_LIMIT: i64 = 50;
const MAX_LIMIT: i64 = 100;
/// Raw rows examined per page, so same-transaction events can fold into one item.
const RAW_PER_ITEM: i64 = 2;
/// Files and folders considered. Larger projects show their most recently updated files.
const MAX_DOCUMENTS: i64 = 2000;
const MAX_SPACES: i64 = 2000;
const MAX_MEMBERS: i64 = 2000;
/// Index entries read for contributor totals, shared across the files.
const EVENT_BUDGET: i64 = 100_000;
/// Look-back windows tried in turn; the final pass is unbounded.
const WINDOWS_SECONDS: [i64; 3] = [86_400, 2_592_000, 31_536_000];
const KINDS: [&str; 7] = [
    "declined",
    "edit",
    "file_added",
    "member_added",
    "merged",
    "request_opened",
    "version",
];

/// Display name shared by every activity surface: the chosen name, else the part of
/// the email before `@` (members already see emails), and a fixed label once erased.
pub(crate) fn person(alias: &str) -> String {
    format!("(case when {a}.id is null then null when {a}.erased_at is not null then 'Deleted account' else coalesce(nullif(btrim({a}.display_name),''),split_part({a}.email,'@',1)) end)", a = alias)
}
/// Short human label for a team event; the event row must be aliased `e`.
pub(crate) const EVENT_LABEL: &str = "(case e.type when 'NodeCreated' then 'Added a block' when 'NodeMoved' then 'Moved a block' when 'NodeDeleted' then 'Deleted a block' when 'NodeRestored' then 'Restored a block' when 'FieldEdited' then 'Edited content' when 'RichTextPatched' then 'Edited text' when 'ChoiceAdded' then 'Added a choice' when 'ChoiceRemoved' then 'Removed a choice' when 'CommentAdded' then 'Added a comment' when 'CommentReplied' then 'Replied to a comment' when 'CommentResolved' then 'Resolved a comment' when 'SuggestionProposed' then 'Suggested a change' when 'SuggestionAccepted' then 'Accepted a suggestion' when 'SuggestionRejected' then 'Rejected a suggestion' when 'Deployed' then 'Deployed the file' else 'Changed the file' end)";

fn bad(reason: &str) -> ApiError {
    ApiError::BadRequest {
        reason: reason.into(),
    }
}

struct Scope {
    spaces: Vec<Uuid>,
    docs: Vec<Uuid>,
    titles: HashMap<Uuid, String>,
}
/// The caller is already a verified project member: the folders below the project and
/// its live files, each re-checked with the file access helper like the file list.
async fn scope(db: &mut PgConnection, project: Uuid, actor: Uuid) -> Result<Scope, ApiError> {
    let spaces: Vec<Uuid> = sqlx::query_scalar("with recursive tree as (select id from access_space where id=$1 union select s.id from access_space s join tree t on s.parent_id=t.id) select id from tree limit $2")
        .bind(project)
        .bind(MAX_SPACES)
        .fetch_all(&mut *db)
        .await?;
    let rows = sqlx::query("select d.id,d.title from document d where d.space_id=any($1) and d.deleted_at is null and document_member_role(d.id,$2) is not null order by d.updated_at desc,d.id limit $3")
        .bind(&spaces)
        .bind(actor)
        .bind(MAX_DOCUMENTS)
        .fetch_all(&mut *db)
        .await?;
    let mut docs = Vec::with_capacity(rows.len());
    let mut titles = HashMap::with_capacity(rows.len());
    for row in rows {
        let id: Uuid = row.get("id");
        docs.push(id);
        titles.insert(id, row.get::<String, _>("title"));
    }
    Ok(Scope {
        spaces,
        docs,
        titles,
    })
}

#[derive(Deserialize)]
struct ActivityQuery {
    limit: Option<i64>,
    /// Opaque `next` value from the previous page.
    before: Option<String>,
}
struct Position {
    at_micros: i64,
    doc: Uuid,
    kind: String,
    rid: String,
}
fn decode(raw: &str) -> Result<Position, ApiError> {
    let invalid = || bad("Invalid activity cursor");
    let cursor = Cursor::decode(raw).ok_or_else(invalid)?;
    let mut parts = cursor.last_id.split('|');
    let (Some(doc), Some(kind), Some(rid), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return Err(invalid());
    };
    if !KINDS.contains(&kind)
        || rid.is_empty()
        || rid.len() > 64
        || !rid.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
    {
        return Err(invalid());
    }
    Ok(Position {
        at_micros: cursor.last_seq,
        doc: Uuid::parse_str(doc).map_err(|_| invalid())?,
        kind: kind.to_owned(),
        rid: rid.to_owned(),
    })
}

struct Raw {
    at: DateTime<Utc>,
    doc: Uuid,
    actor: Option<Uuid>,
    kind: String,
    rid: String,
    seq: Option<i64>,
    label: Option<String>,
    actor_name: Option<String>,
    subject_name: Option<String>,
}

struct Item {
    at: DateTime<Utc>,
    kind: String,
    doc: Uuid,
    actor: Option<Uuid>,
    actor_name: Option<String>,
    summary: Option<String>,
    seq: Option<i64>,
    count: i64,
}

/// Rows strictly before the cursor in `(at, doc, kind, id)` order (same collation as
/// the final ordering) and inside the time window `(c.floor, c.at]`.
fn before(alias: &str, at: &str, doc: &str, kind: &str, rid: &str) -> String {
    format!("({alias}.{at}<=c.at and {alias}.{at}>c.floor and ({alias}.{at},{doc},'{kind}'::text collate \"C\",{rid} collate \"C\") < (c.at,c.doc,c.kind collate \"C\",c.rid collate \"C\"))")
}

const UUID_TEXT: &str = "'^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'";

/// One bounded top-N over the sources. `$1` files, `$2` folders, `$3..$6` the cursor
/// position, `$7` the row limit and `$8` the window start (null: unbounded). Each
/// source returns at most `$7` rows per file, so nothing here grows with history.
fn feed_sql() -> String {
    let edit = before("x", "created_at", "x.document_id", "edit", "x.id");
    let head = before("h", "created_at", "h.document_id", "edit", "h.id");
    let opened = before(
        "w",
        "submitted_at",
        "w.document_id",
        "request_opened",
        "w.id::text",
    );
    let declined = before(
        "w",
        "reviewed_at",
        "w.document_id",
        "declined",
        "w.id::text",
    );
    let merged = before(
        "a",
        "created_at",
        "a.resource_id",
        "merged",
        "lpad(a.id::text,20,'0')",
    );
    let version = before("v", "created_at", "v.document_id", "version", "v.id::text");
    let file = before("f", "created_at", "f.id", "file_added", "f.id::text");
    let member = before(
        "m",
        "created_at",
        "m.resource_id",
        "member_added",
        "lpad(m.id::text,20,'0')",
    );
    let who_actor = person("i");
    let who_subject = person("s");
    format!(
        "with cur as (select coalesce(timestamptz 'epoch'+$3::bigint*interval '1 microsecond','infinity'::timestamptz) as at,
  coalesce(timestamptz 'epoch'+$8::bigint*interval '1 microsecond','-infinity'::timestamptz) as floor,
  coalesce($4::uuid,'ffffffff-ffff-ffff-ffff-ffffffffffff'::uuid) as doc, coalesce($5::text,'~') as kind, coalesce($6::text,'~') as rid),
feed as (
 (select e.created_at as at,e.document_id as doc,e.actor_id as actor,'edit'::text as kind,e.id as rid,e.seq as seq,{label} as label,null::text as extra
  from cur c
  cross join lateral (select u.id from unnest($1::uuid[]) as u(id)
    cross join lateral (select h.created_at from event h where h.document_id=u.id and {head} order by h.created_at desc,h.id collate \"C\" desc limit 1) newest
    order by newest.created_at desc limit $7) top
  cross join lateral (select x.* from event x where x.document_id=top.id and {edit} order by x.created_at desc,x.id collate \"C\" desc limit $7) e)
 union all
 (select w.submitted_at,w.document_id,w.created_by,'request_opened'::text,w.id::text,null::bigint,w.name,null::text
  from cur c cross join unnest($1::uuid[]) as u(id)
  cross join lateral (select w.* from workspace_draft w where w.document_id=u.id and w.submitted_at is not null and {opened} order by w.submitted_at desc,w.id::text collate \"C\" desc limit $7) w)
 union all
 (select w.reviewed_at,w.document_id,w.reviewed_by,'declined'::text,w.id::text,null::bigint,w.name,null::text
  from cur c cross join unnest($1::uuid[]) as u(id)
  cross join lateral (select w.* from workspace_draft w where w.document_id=u.id and w.submitted_at is not null and w.review_outcome='declined' and w.reviewed_at is not null and {declined} order by w.reviewed_at desc,w.id::text collate \"C\" desc limit $7) w)
 union all
 (select a.created_at,a.resource_id,a.actor_id,'merged'::text,lpad(a.id::text,20,'0'),case when jsonb_typeof(a.detail->'through_seq')='number' then (a.detail->>'through_seq')::bigint end,null::text,case when jsonb_typeof(a.detail->'draft_id')='string' and a.detail->>'draft_id' ~ {uuid} then a.detail->>'draft_id' end
  from cur c cross join unnest($1::uuid[]) as u(id)
  cross join lateral (select a.* from operation_audit a where a.resource_id=u.id and a.action='document.change_request_merged' and {merged} order by a.created_at desc,a.id desc limit $7) a)
 union all
 (select v.created_at,v.document_id,v.created_by,'version'::text,v.id::text,null::bigint,v.name,v.snapshot_id::text
  from cur c cross join unnest($1::uuid[]) as u(id)
  cross join lateral (select v.* from document_version v where v.document_id=u.id and {version} order by v.created_at desc,v.id::text collate \"C\" desc limit $7) v)
 union all
 (select f.created_at,f.id,f.created_by,'file_added'::text,f.id::text,null::bigint,coalesce(f.settings->>'kind',''),null::text
  from cur c
  cross join lateral (select f.* from document f where f.id=any($1::uuid[]) and {file} order by f.created_at desc,f.id desc limit $7) f)
 union all
 (select m.created_at,m.resource_id,m.actor_id,'member_added'::text,lpad(m.id::text,20,'0'),null::bigint,m.detail->>'role',m.detail->>'identity_id'
  from cur c
  cross join lateral (select m.* from operation_audit m where m.resource_id=any($2::uuid[]) and m.action='space.access_changed' and m.detail->>'role' is distinct from 'remove' and {member} order by m.created_at desc,m.id desc limit $7) m)
)
select r.at,r.doc,r.actor,r.kind,r.rid,
  case when r.kind='version' then (select s.through_seq from snapshot s where s.id=r.extra::uuid) else r.seq end as seq,
  case when r.kind='merged' then (select w.name from workspace_draft w where w.id=r.extra::uuid) else r.label end as label,
  {who_actor} as actor_name,{who_subject} as subject_name
from (select * from feed order by at desc,doc desc,kind collate \"C\" desc,rid collate \"C\" desc limit $7) r
left join identity i on i.id=r.actor
left join identity s on s.id=case when r.kind='member_added' and r.extra ~ {uuid} then r.extra::uuid end
order by r.at desc,r.doc desc,r.kind collate \"C\" desc,r.rid collate \"C\" desc",
        label = EVENT_LABEL,
        uuid = UUID_TEXT,
    )
}

fn role_name(role: &str) -> &str {
    match role {
        "approver" => "reviewer",
        "reviewer" => "contributor",
        other => other,
    }
}
fn summary(raw: &Raw) -> Option<String> {
    let label = raw.label.clone().filter(|s| !s.is_empty());
    match raw.kind.as_str() {
        "file_added" => Some(
            match label.as_deref() {
                Some("document") => "Added a document",
                Some("spreadsheet") => "Added a spreadsheet",
                Some("presentation") => "Added a presentation",
                _ => "Added a file",
            }
            .to_owned(),
        ),
        "member_added" => Some(format!(
            "Gave {} {} access",
            raw.subject_name.as_deref().unwrap_or("someone"),
            role_name(label.as_deref().unwrap_or("member"))
        )),
        "request_opened" | "merged" | "declined" => {
            Some(label.unwrap_or_else(|| "Change request".to_owned()))
        }
        _ => label,
    }
}

#[utoipa::path(get, path="/projects/{id}/activity", params(("id"=Uuid,Path),("limit"=Option<i64>,Query,description="Items per page, 1-100 (default 50)"),("before"=Option<String>,Query,description="Opaque cursor from the previous page's next value")), security(("paseto"=[]),("device_session"=["project:read"])), responses((status=200,description="Newest-first project activity: items and an opaque next cursor",body=Value),(status=400,description="Invalid cursor"),(status=403,description="Project membership required"),(status=429,description="Too many activity reads"),(status=504,description="Deadline exceeded")))]
async fn project_activity_feed(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    auth: ProjectAuth<ProjectRead>,
    Query(query): Query<ActivityQuery>,
) -> Result<Json<Value>, ApiError> {
    let limit = query.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
    let position = match query.before.as_deref() {
        None | Some("") => None,
        Some(raw) => Some(decode(raw)?),
    };
    let mut tx = state.pool.begin().await?;
    require(&mut tx, id, auth.identity_id).await?;
    let _budget = state.workloads.enter(auth.identity_id, Some(id))?;
    sqlx::query("set local statement_timeout='3s'")
        .execute(&mut *tx)
        .await?;
    let scope = scope(&mut tx, id, auth.identity_id).await?;
    let window = limit * RAW_PER_ITEM + 1;
    // Widen the time window until it holds a page. Each pass is a complete, ordered
    // top-N of everything newer than its floor, so the last pass is always exact.
    let start = position
        .as_ref()
        .map(|p| p.at_micros)
        .unwrap_or_else(|| Utc::now().timestamp_micros());
    let sql = feed_sql();
    let mut rows = Vec::new();
    for span in WINDOWS_SECONDS.iter().map(|s| Some(*s)).chain([None]) {
        rows = sqlx::query(&sql)
            .bind(&scope.docs)
            .bind(&scope.spaces)
            .bind(position.as_ref().map(|p| p.at_micros))
            .bind(position.as_ref().map(|p| p.doc))
            .bind(position.as_ref().map(|p| p.kind.clone()))
            .bind(position.as_ref().map(|p| p.rid.clone()))
            .bind(window)
            .bind(span.map(|s| start - s * 1_000_000))
            .fetch_all(&mut *tx)
            .await?;
        if rows.len() as i64 >= window {
            break;
        }
    }
    tx.commit().await?;
    let raw: Vec<Raw> = rows
        .iter()
        .map(|r| Raw {
            at: r.get("at"),
            doc: r.get("doc"),
            actor: r.get("actor"),
            kind: r.get("kind"),
            rid: r.get("rid"),
            seq: r.get("seq"),
            label: r.get("label"),
            actor_name: r.get("actor_name"),
            subject_name: r.get("subject_name"),
        })
        .collect();

    let mut items: Vec<Item> = Vec::new();
    let mut consumed = 0;
    let mut more = false;
    for row in &raw {
        let folds = row.kind == "edit"
            && items.last().is_some_and(|last| {
                matches!(last.kind.as_str(), "edit" | "merged")
                    && last.at == row.at
                    && last.doc == row.doc
                    && last.actor == row.actor
            });
        if folds {
            let last = items.last_mut().expect("checked above");
            last.count += 1;
            last.seq = last.seq.max(row.seq);
            if last.kind == "edit" {
                last.summary = Some(format!("Made {} changes", last.count));
            }
        } else {
            if items.len() as i64 == limit {
                more = true;
                break;
            }
            items.push(Item {
                at: row.at,
                kind: row.kind.clone(),
                doc: row.doc,
                actor: row.actor,
                actor_name: row.actor_name.clone(),
                summary: summary(row),
                seq: row.seq,
                count: i64::from(row.kind == "edit"),
            });
        }
        consumed += 1;
    }
    more = more || raw.len() as i64 == window;
    let next = if more && consumed > 0 {
        let last = &raw[consumed - 1];
        Some(
            Cursor {
                last_seq: last.at.timestamp_micros(),
                last_id: format!("{}|{}|{}", last.doc, last.kind, last.rid),
            }
            .encode(),
        )
    } else {
        None
    };
    let items: Vec<Value> = items
        .into_iter()
        .map(|item| {
            let space_level = item.kind == "member_added";
            let mut value = json!({
                "at": item.at.to_rfc3339(),
                "kind": item.kind,
                "actor_id": item.actor,
                "actor_name": item.actor_name,
                "document_id": if space_level { None } else { Some(item.doc) },
                "document_title": if space_level { None } else { scope.titles.get(&item.doc) },
                "summary": item.summary,
                "seq": item.seq,
            });
            if item.count > 0 {
                value["count"] = json!(item.count);
            }
            value
        })
        .collect();
    Ok(Json(json!({"items": items, "next": next})))
}

#[utoipa::path(get, path="/projects/{id}/contributors", params(("id"=Uuid,Path)), security(("paseto"=[])), responses((status=200,description="Current project members with merged requests and authored events; capped is true when a very active file's older events were not counted",body=Value),(status=403,description="Project membership required"),(status=429,description="Too many activity reads"),(status=504,description="Deadline exceeded")))]
async fn project_contributors(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    auth: AuthContext,
) -> Result<Json<Value>, ApiError> {
    let mut tx = state.pool.begin().await?;
    require(&mut tx, id, auth.identity_id).await?;
    let _budget = state.workloads.enter(auth.identity_id, Some(id))?;
    sqlx::query("set local statement_timeout='3s'")
        .execute(&mut *tx)
        .await?;
    let scope = scope(&mut tx, id, auth.identity_id).await?;
    let who = person("i");
    let members = sqlx::query(&format!("select i.id,{who} as name from space_member sm join identity i on i.id=sm.identity_id and i.disabled_at is null and i.erased_at is null where sm.space_id=$1 order by i.id limit $2"))
        .bind(id)
        .bind(MAX_MEMBERS)
        .fetch_all(&mut *tx)
        .await?;
    let ids: Vec<Uuid> = members.iter().map(|m| m.get("id")).collect();
    // Every file contributes at most `per_file` of its newest events, so the work is
    // bounded by files x per_file index entries whatever the history length.
    let per_file = (EVENT_BUDGET / (scope.docs.len() as i64).max(1)).clamp(20, 20_000);
    let events = sqlx::query("select u.id as doc,e.actor_id,count(*)::bigint as events,max(e.created_at) as last_at from unnest($1::uuid[]) as u(id) cross join lateral (select x.actor_id,x.created_at from event x where x.document_id=u.id order by x.created_at desc,x.id collate \"C\" desc limit $2) e group by u.id,e.actor_id")
        .bind(&scope.docs)
        .bind(per_file)
        .fetch_all(&mut *tx)
        .await?;
    let merges = sqlx::query("select w.created_by,count(*)::bigint as merges,max(w.merged_at) as last_at from workspace_draft w where w.document_id=any($1::uuid[]) and w.merged_at is not null and w.created_by=any($2::uuid[]) group by w.created_by")
        .bind(&scope.docs)
        .bind(&ids)
        .fetch_all(&mut *tx)
        .await?;
    tx.commit().await?;
    type Totals = (i64, i64, Option<DateTime<Utc>>);
    let mut totals: HashMap<Uuid, Totals> = HashMap::new();
    let mut per_doc: HashMap<Uuid, i64> = HashMap::new();
    let member_set: HashSet<Uuid> = ids.iter().copied().collect();
    for row in &events {
        let events: i64 = row.get("events");
        *per_doc.entry(row.get("doc")).or_default() += events;
        let actor: Uuid = row.get("actor_id");
        if member_set.contains(&actor) {
            let entry = totals.entry(actor).or_default();
            entry.1 += events;
            entry.2 = entry.2.max(row.get::<Option<DateTime<Utc>>, _>("last_at"));
        }
    }
    // A file that filled its allowance may have older events that were not counted.
    let capped = per_doc.values().any(|n| *n >= per_file);
    for row in &merges {
        let entry = totals.entry(row.get("created_by")).or_default();
        entry.0 += row.get::<i64, _>("merges");
        entry.2 = entry.2.max(row.get::<Option<DateTime<Utc>>, _>("last_at"));
    }
    let mut people: Vec<(Uuid, Option<String>, Totals)> = members
        .iter()
        .map(|m| {
            let id: Uuid = m.get("id");
            (
                id,
                m.get("name"),
                totals.get(&id).copied().unwrap_or_default(),
            )
        })
        .collect();
    people.sort_by(|a, b| {
        b.2 .0
            .cmp(&a.2 .0)
            .then(b.2 .1.cmp(&a.2 .1))
            .then(b.2 .2.cmp(&a.2 .2))
            .then(a.0.cmp(&b.0))
    });
    people.truncate(200);
    let items: Vec<Value> = people
        .into_iter()
        .map(|(identity_id, name, (merges, edits, last_at))| {
            json!({"identity_id":identity_id,"name":name,"merges":merges,"edits":edits,"last_at":last_at.map(|t| t.to_rfc3339())})
        })
        .collect();
    Ok(Json(json!({"contributors": items, "capped": capped})))
}

#[derive(utoipa::OpenApi)]
#[openapi(paths(project_activity_feed, project_contributors))]
pub struct ProjectActivityApi;
