//! Copy detection across documents.
//!
//! Every document's content is fingerprinted ([`engine_core::similarity`]) shortly
//! after it changes, by a background indexer outside the edit path. When newly
//! arrived content (an uploaded file, a pasted document) closely resembles another
//! person's document, that document's owner is notified — even if the file was
//! renamed and even if the uploader cannot see the original. The uploader is told
//! this happens before uploading; the owner learns who uploaded what file and how
//! similar it is, never the file's content.
//!
//! Before uploading, `POST /similarity/check` compares a file with the documents the
//! uploader can already see, so an edited copy can be sent as changes instead of
//! becoming an unrelated document.
use crate::{
    access::{self, MemberRole},
    auth::AuthContext,
    error::ApiError,
    notifications, AppState,
};
use axum::{
    extract::{Path, State},
    routing::{get, post},
    Json, Router,
};
use engine_core::similarity::{self, Comparison, Fingerprint, Relation, Summary};
use engine_shared::DocumentId;
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::{PgConnection, PgPool, Row};
use std::sync::Arc;
use std::time::Duration;
use uuid::Uuid;

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/similarity/check",
            post(check).layer(axum::extract::DefaultBodyLimit::max(8 * 1024 * 1024)),
        )
        .route("/documents/:id/copies", get(copies))
        .route("/copies/:match_id", post(respond))
}

/// Wakes the background indexer early, e.g. right after a file import.
#[derive(Clone, Default)]
pub struct CopyIndex {
    wake: Arc<tokio::sync::Notify>,
}
impl CopyIndex {
    pub fn wake(&self) {
        self.wake.notify_one();
    }
}

/// Run the indexer until the process exits: fingerprint changed documents, then look
/// for copies. Failures are logged and retried on the next pass.
pub fn spawn(state: AppState) {
    tokio::spawn(async move {
        loop {
            match run_once(&state.pool).await {
                Ok(n) if n > 0 => tracing::debug!(documents = n, "copy index refreshed"),
                Ok(_) => {}
                Err(e) => tracing::warn!(error = %e, "copy index pass failed"),
            }
            tokio::select! {
                _ = state.copy_index.wake.notified() => {},
                _ = tokio::time::sleep(Duration::from_secs(5)) => {},
            }
        }
    });
}

/// One indexing pass over up to 25 documents whose content or title changed. Brand
/// new documents go first; documents being edited are refreshed at most every 15 s.
pub async fn run_once(pool: &PgPool) -> Result<usize, ApiError> {
    let stale: Vec<Uuid> = sqlx::query_scalar(
        "select d.id from document d left join document_fingerprint f on f.document_id=d.id
         where d.deleted_at is null and exists(select 1 from event e where e.document_id=d.id)
           and (f.document_id is null or f.version<>$1 or (f.updated_at<now()-interval '15 seconds'
             and (f.title<>d.title or f.through_seq<(select max(seq) from event e where e.document_id=d.id))))
         order by f.updated_at nulls first, d.updated_at desc limit 25",
    )
    .bind(similarity::VERSION)
    .fetch_all(pool)
    .await?;
    for id in &stale {
        index_and_detect(pool, *id).await?;
    }
    Ok(stale.len())
}

/// Fingerprint one document now and report copies it resembles.
pub async fn index_and_detect(pool: &PgPool, id: Uuid) -> Result<Vec<Value>, ApiError> {
    match index(pool, id).await? {
        Some(fingerprint) => detect(pool, id, &fingerprint).await,
        None => Ok(Vec::new()),
    }
}

async fn index(pool: &PgPool, id: Uuid) -> Result<Option<Fingerprint>, ApiError> {
    let Some(title) = sqlx::query_scalar::<_, String>(
        "select title from document where id=$1 and deleted_at is null",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?
    else {
        return Ok(None);
    };
    let (state, through_seq) = engine_core::snapshot::SnapshotEngine::new(pool.clone())
        .read_current_state_with_seq(DocumentId(id))
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    let fingerprint = similarity::fingerprint_state(&state);
    let mut tx = pool.begin().await?;
    sqlx::query(
        "insert into document_fingerprint(document_id,version,through_seq,title,title_key,content_hash,signature,block_count,substantive_count,word_count,shingle_count,updated_at)
         values($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,now())
         on conflict(document_id) do update set version=excluded.version,through_seq=excluded.through_seq,title=excluded.title,title_key=excluded.title_key,content_hash=excluded.content_hash,signature=excluded.signature,block_count=excluded.block_count,substantive_count=excluded.substantive_count,word_count=excluded.word_count,shingle_count=excluded.shingle_count,updated_at=now()",
    )
    .bind(id)
    .bind(similarity::VERSION)
    .bind(through_seq)
    .bind(&title)
    .bind(similarity::title_key(&title))
    .bind(fingerprint.content_hash)
    .bind(fingerprint.signature_bytes())
    .bind(fingerprint.block_count as i32)
    .bind(fingerprint.blocks.len() as i32)
    .bind(fingerprint.word_count as i32)
    .bind(fingerprint.shingle_count as i32)
    .execute(&mut *tx)
    .await?;
    sqlx::query("delete from document_fingerprint_band where document_id=$1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    let bands: Vec<i16> = (0..fingerprint.bands.len() as i16).collect();
    sqlx::query("insert into document_fingerprint_band(document_id,band,key) select $1,b,k from unnest($2::smallint[],$3::bigint[]) as t(b,k)")
        .bind(id)
        .bind(&bands)
        .bind(&fingerprint.bands)
        .execute(&mut *tx)
        .await?;
    sqlx::query("delete from document_block_hash where document_id=$1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("insert into document_block_hash(document_id,hash) select $1,h from unnest($2::bigint[]) as t(h)")
        .bind(id)
        .bind(&fingerprint.blocks)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(Some(fingerprint))
}

/// A stored candidate and its comparison with the new content.
struct Candidate {
    id: Uuid,
    title: String,
    kind: String,
    owner: Uuid,
    created_at: chrono::DateTime<chrono::Utc>,
    uploaded: bool,
    copied_from: Option<Uuid>,
    comparison: Comparison,
}

/// One side of a detected pair.
#[derive(Clone, Copy)]
struct Side {
    id: Uuid,
    owner: Uuid,
    created_at: chrono::DateTime<chrono::Utc>,
    uploaded: bool,
    copied_from: Option<Uuid>,
}

/// Which of two resembling documents is the copy: the one created later. Indexing
/// order is not used, because an original may still be being typed when a copy is
/// uploaded. Returns (copy, original).
fn orient(a: Side, b: Side) -> (Side, Side) {
    if (a.created_at, a.id) >= (b.created_at, b.id) {
        (a, b)
    } else {
        (b, a)
    }
}

/// The comparison seen from the other document.
fn flipped(c: Comparison) -> Comparison {
    Comparison {
        relation: match c.relation {
            Relation::Excerpt => Relation::Extended,
            Relation::Extended => Relation::Excerpt,
            other => other,
        },
        coverage_new: c.coverage_old,
        coverage_old: c.coverage_new,
        ..c
    }
}

/// Documents sharing a locality-sensitive key, several substantive blocks, the exact
/// content or the same name, compared with `fingerprint`. `viewer` limits the search
/// to documents that person can open. At most 100 candidates per signal.
async fn candidates(
    db: &mut PgConnection,
    exclude: Option<Uuid>,
    fingerprint: &Fingerprint,
    title_key: &str,
    viewer: Option<Uuid>,
    also: Option<Uuid>,
) -> Result<Vec<Candidate>, ApiError> {
    let bands: Vec<i16> = (0..fingerprint.bands.len() as i16).collect();
    let min_shared = fingerprint.blocks.len().clamp(1, 2) as i64;
    let rows = sqlx::query(
        "with bands as (select * from unnest($2::smallint[],$3::bigint[]) as b(band,key)),
         found as (
           (select f.document_id from document_fingerprint_band f join bands b on b.band=f.band and b.key=f.key
             where f.document_id<>coalesce($1,'00000000-0000-0000-0000-000000000000'::uuid) group by f.document_id order by count(*) desc limit 100)
           union (select h.document_id from document_block_hash h where h.hash=any($4) and h.document_id<>coalesce($1,'00000000-0000-0000-0000-000000000000'::uuid)
             group by h.document_id having count(*)>=$8 order by count(*) desc limit 100)
           union (select document_id from document_fingerprint where $5<>0 and content_hash=$5 limit 100)
           union (select document_id from document_fingerprint where $6<>'' and title_key=$6 limit 100)
           union (select $9::uuid where $9::uuid is not null))
         select d.id,d.title,d.created_by,d.created_at,d.settings->>'copied_from' as copied_from,
           exists(select 1 from document_upload u where u.document_id=d.id and coalesce(u.report->>'kind','')<>'font') as uploaded,
           coalesce(d.settings->>'kind','questionnaire') as kind,
           f.signature,f.shingle_count,f.substantive_count,f.content_hash,f.title_key,
           (select count(*) from document_block_hash h where h.document_id=d.id and h.hash=any($4)) as shared
         from found c join document d on d.id=c.document_id and d.deleted_at is null
         join document_fingerprint f on f.document_id=d.id
         where d.id<>coalesce($1,'00000000-0000-0000-0000-000000000000'::uuid)
           and ($7::uuid is null or document_member_role(d.id,$7) is not null)",
    )
    .bind(exclude)
    .bind(&bands)
    .bind(&fingerprint.bands)
    .bind(&fingerprint.blocks)
    .bind(fingerprint.content_hash)
    .bind(title_key)
    .bind(viewer)
    .bind(min_shared)
    .bind(also)
    .fetch_all(&mut *db)
    .await?;
    let new = fingerprint.summary();
    Ok(rows
        .into_iter()
        .map(|row| {
            let signature = Fingerprint::signature_from_bytes(&row.get::<Vec<u8>, _>("signature"));
            let old = Summary {
                signature: &signature,
                shingle_count: row.get::<i32, _>("shingle_count") as u32,
                substantive_count: row.get::<i32, _>("substantive_count") as u32,
                content_hash: row.get("content_hash"),
            };
            let key: String = row.get("title_key");
            let comparison = similarity::compare_summaries(
                &new,
                &old,
                row.get::<i64, _>("shared") as u32,
                !title_key.is_empty() && key == title_key,
            );
            Candidate {
                id: row.get("id"),
                title: row.get("title"),
                kind: row.get("kind"),
                owner: row.get("created_by"),
                created_at: row.get("created_at"),
                uploaded: row.get("uploaded"),
                copied_from: row
                    .get::<Option<String>, _>("copied_from")
                    .and_then(|v| v.parse().ok()),
                comparison,
            }
        })
        .collect())
}

fn comparison_json(c: &Comparison) -> Value {
    json!({"relation":c.relation.as_str(),"score":c.score,"jaccard":c.jaccard,"coverage_new":c.coverage_new,"coverage_old":c.coverage_old,"shared_blocks":c.shared_blocks,"title_match":c.title_match})
}

/// Record and announce copies involving newly indexed content. Each pair is recorded
/// once, in either direction, and the owner of the original is notified.
async fn detect(
    pool: &PgPool,
    id: Uuid,
    fingerprint: &Fingerprint,
) -> Result<Vec<Value>, ApiError> {
    let mut db = pool.acquire().await?;
    let Some(row) = sqlx::query(
        "select d.title,d.created_by,d.created_at,d.settings->>'copied_from' as copied_from,
           exists(select 1 from document_upload u where u.document_id=d.id and coalesce(u.report->>'kind','')<>'font') as uploaded
         from document d where d.id=$1 and d.deleted_at is null",
    )
    .bind(id)
    .fetch_optional(&mut *db)
    .await?
    else {
        return Ok(Vec::new());
    };
    let me = Side {
        id,
        owner: row.get("created_by"),
        created_at: row.get("created_at"),
        uploaded: row.get("uploaded"),
        copied_from: row
            .get::<Option<String>, _>("copied_from")
            .and_then(|v| v.parse().ok()),
    };
    let title_key = similarity::title_key(&row.get::<String, _>("title"));
    if fingerprint.is_trivial() && !me.uploaded {
        return Ok(Vec::new());
    }
    let mut found = candidates(&mut db, Some(id), fingerprint, &title_key, None, None).await?;
    found.sort_by(|a, b| b.comparison.score.total_cmp(&a.comparison.score));
    let baseline: Option<chrono::DateTime<chrono::Utc>> =
        sqlx::query_scalar("select started_at from copy_detection_baseline")
            .fetch_optional(&mut *db)
            .await?;
    let mut recorded = Vec::new();
    // Widely copied content (a shared template, say) can match many documents: record
    // every pair, but tell each owner once per pass.
    let mut told = std::collections::HashSet::new();
    for candidate in found {
        let other = Side {
            id: candidate.id,
            owner: candidate.owner,
            created_at: candidate.created_at,
            uploaded: candidate.uploaded,
            copied_from: candidate.copied_from,
        };
        let (copy_side, original) = orient(me, other);
        // Comparisons are computed with this document as the new content.
        let c = if copy_side.id == me.id {
            candidate.comparison
        } else {
            flipped(candidate.comparison)
        };
        let copy = c.relation.is_copy();
        // A file uploaded under the name of another document is worth a note too,
        // but only among collaborators: unrelated people reuse ordinary names.
        let same_name = c.title_match && copy_side.uploaded && !copy;
        if (!copy && !same_name) || copy_side.owner == original.owner {
            continue;
        }
        let collaborator = access::member_role(&mut db, original.id, copy_side.owner)
            .await?
            .is_some();
        if same_name && !collaborator {
            continue;
        }
        // "Make a copy" inside Dynodoc by someone with access is recorded, not announced.
        let in_app_copy = copy_side.copied_from == Some(original.id) && collaborator;
        let method = if in_app_copy {
            "in_app_copy"
        } else if c.relation == Relation::Identical {
            "exact"
        } else if copy {
            "content"
        } else {
            "title"
        };
        let inserted: Option<Uuid> = sqlx::query_scalar(
            "insert into document_match(source_document_id,target_document_id,relation,method,score,jaccard,coverage_source,coverage_target,shared_blocks,title_match,status)
             values($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11) on conflict do nothing returning id",
        )
        .bind(copy_side.id)
        .bind(original.id)
        .bind(c.relation.as_str())
        .bind(method)
        .bind(c.score)
        .bind(c.jaccard)
        .bind(c.coverage_new)
        .bind(c.coverage_old)
        .bind(c.shared_blocks as i32)
        .bind(c.title_match)
        .bind(if in_app_copy { "linked" } else { "open" })
        .fetch_optional(&mut *db)
        .await?;
        let Some(match_id) = inserted else { continue };
        // Both documents predate copy detection: list the copy, don't announce it.
        let historical =
            baseline.is_some_and(|t| copy_side.created_at < t && original.created_at < t);
        if !in_app_copy && !historical && told.len() < 25 && told.insert(original.owner) {
            notifications::notify(
                &mut db,
                original.owner,
                if copy { "copy_detected" } else { "same_name_detected" },
                Some(original.id),
                Some(copy_side.owner),
                Some(match_id),
                json!({"source_document_id":copy_side.id,"relation":c.relation.as_str(),"score":c.score,"coverage_source":c.coverage_new,"coverage_target":c.coverage_old,"method":method,"uploader_has_access":collaborator}),
            )
            .await?;
        }
        recorded.push(json!({"match_id":match_id,"copy":copy_side.id,"original":original.id,"method":method,"comparison":comparison_json(&c)}));
        if recorded.len() >= 50 {
            break;
        }
    }
    Ok(recorded)
}

#[derive(Deserialize)]
struct CheckRequest {
    /// File name or intended title.
    #[serde(default)]
    title: String,
    /// The file's blocks as plain text, in any order.
    blocks: Vec<String>,
    /// Document id recorded in a file downloaded from Dynodoc, if any.
    origin_document_id: Option<Uuid>,
}
/// Compare a file with the documents the caller can open. Never reveals others.
#[utoipa::path(post, path="/similarity/check", tag="copies", request_body=Value, security(("paseto" = [])), responses((status=200, description="Visible documents that resemble the file, strongest first", body=Value),(status=400, description="Too many blocks"),(status=401, description="Sign in required")))]
async fn check(
    State(state): State<AppState>,
    auth: AuthContext,
    Json(req): Json<CheckRequest>,
) -> Result<Json<Value>, ApiError> {
    if req.blocks.len() > 50_000 || req.title.len() > 500 {
        return Err(ApiError::BadRequest {
            reason: "Files are compared on up to 50,000 blocks".into(),
        });
    }
    let fingerprint = Fingerprint::from_blocks(req.blocks.iter().map(String::as_str));
    let title_key = similarity::title_key(&req.title);
    let mut db = state.pool.acquire().await?;
    let found = candidates(
        &mut db,
        None,
        &fingerprint,
        &title_key,
        Some(auth.identity_id),
        req.origin_document_id,
    )
    .await?;
    let mut matches = Vec::new();
    for candidate in found {
        let c = candidate.comparison;
        let embedded = req.origin_document_id == Some(candidate.id);
        if !embedded && c.relation == Relation::Unrelated {
            continue;
        }
        let role = access::member_role(&mut db, candidate.id, auth.identity_id)
            .await?
            .unwrap_or(MemberRole::Viewer);
        let owner: (Option<String>, String) =
            sqlx::query_as("select display_name,email from identity where id=$1")
                .bind(candidate.owner)
                .fetch_one(&mut *db)
                .await?;
        matches.push(json!({
            "document_id": candidate.id, "title": candidate.title, "kind": candidate.kind,
            "role": role.as_str(), "can_push": role >= MemberRole::Contributor,
            "owner": {"name": owner.0, "email": owner.1, "is_you": candidate.owner == auth.identity_id},
            "embedded_id": embedded, "comparison": comparison_json(&c),
        }));
    }
    matches.sort_by(|a, b| {
        let key = |v: &Value| {
            (
                v["embedded_id"].as_bool().unwrap_or(false),
                v["comparison"]["score"].as_f64().unwrap_or(0.0),
            )
        };
        key(b)
            .partial_cmp(&key(a))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    matches.truncate(8);
    Ok(Json(json!({
        "matches": matches,
        "file": {"blocks": fingerprint.block_count, "words": fingerprint.word_count, "comparable": !fingerprint.is_trivial()},
    })))
}

/// Copies of this document found elsewhere, and documents this one resembles that
/// the viewer can open. Editors and above see them; the owner and managers act on them.
#[utoipa::path(get, path="/documents/{id}/copies", tag="copies", params(("id" = Uuid, Path, description = "Document identifier")), security(("paseto" = [])), responses((status=200, description="Detected copies and similar documents", body=Value),(status=403, description="Editors and above")))]
async fn copies(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    auth: AuthContext,
) -> Result<Json<Value>, ApiError> {
    let mut db = state.pool.acquire().await?;
    let role = access::require_member(&mut db, id, auth.identity_id).await?;
    if role < MemberRole::Editor {
        return Err(ApiError::Forbidden);
    }
    let items: Vec<Value> = sqlx::query_scalar(
        "select jsonb_build_object('id',m.id,'direction',case when m.target_document_id=$1 then 'copy_of_this' else 'this_resembles' end,
           'relation',m.relation,'method',m.method,'score',m.score,'jaccard',m.jaccard,'coverage_source',m.coverage_source,'coverage_target',m.coverage_target,
           'shared_blocks',m.shared_blocks,'title_match',m.title_match,'status',m.status,'detected_at',m.detected_at,
           'document',jsonb_build_object('id',o.id,'title',o.title,'kind',coalesce(o.settings->>'kind','questionnaire'),'role',document_member_role(o.id,$2),
             'filename',(select u.filename from document_upload u where u.document_id=o.id and coalesce(u.report->>'kind','')<>'font' order by u.created_at limit 1)),
           'person',jsonb_build_object('id',p.id,'name',p.display_name,'email',case when p.erased_at is null then p.email end))
         from document_match m
         join document o on o.id=case when m.target_document_id=$1 then m.source_document_id else m.target_document_id end and o.deleted_at is null
         join identity p on p.id=o.created_by
         where (m.target_document_id=$1 or (m.source_document_id=$1 and document_member_role(m.target_document_id,$2) is not null))
         order by m.detected_at desc limit 100",
    )
    .bind(id)
    .bind(auth.identity_id)
    .fetch_all(&mut *db)
    .await?;
    Ok(Json(json!({"items":items,"can_act":role.can_manage()})))
}

#[derive(Deserialize)]
struct Respond {
    /// `invite`: let the uploader send their copy as changes; `dismiss`; `link`
    /// (known and fine).
    action: String,
}
/// The owner answers a detected copy.
#[utoipa::path(post, path="/copies/{match_id}", tag="copies", request_body=Value, params(("match_id" = Uuid, Path, description = "Detected copy")), security(("paseto" = [])), responses((status=200, description="Updated", body=Value),(status=403, description="The owner or a manager of the original document")))]
async fn respond(
    State(state): State<AppState>,
    Path(match_id): Path<Uuid>,
    auth: AuthContext,
    Json(req): Json<Respond>,
) -> Result<Json<Value>, ApiError> {
    if !["invite", "dismiss", "link"].contains(&req.action.as_str()) {
        return Err(ApiError::BadRequest {
            reason: "Choose invite, dismiss or link".into(),
        });
    }
    let mut tx = state.pool.begin().await?;
    let row: (Uuid, Uuid, String) = sqlx::query_as(
        "select source_document_id,target_document_id,status from document_match where id=$1 for update",
    )
    .bind(match_id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(ApiError::NotFound)?;
    let (source, target, _) = row;
    let role = access::require_member(&mut tx, target, auth.identity_id).await?;
    if !role.can_manage() {
        return Err(ApiError::Forbidden);
    }
    let uploader: Uuid = sqlx::query_scalar("select created_by from document where id=$1")
        .bind(source)
        .fetch_one(&mut *tx)
        .await?;
    let status = match req.action.as_str() {
        "invite" => {
            // Contributor access is enough to send changes; never lower an existing role.
            sqlx::query("insert into document_access(document_id,identity_id,role) values($1,$2,'reviewer') on conflict(document_id,identity_id) do nothing")
                .bind(target)
                .bind(uploader)
                .execute(&mut *tx)
                .await?;
            notifications::notify(
                &mut tx,
                uploader,
                "copy_invitation",
                Some(target),
                Some(auth.identity_id),
                Some(match_id),
                json!({"source_document_id":source}),
            )
            .await?;
            "invited"
        }
        "dismiss" => "dismissed",
        _ => "linked",
    };
    sqlx::query("update document_match set status=$2,resolved_by=$3,resolved_at=now() where id=$1")
        .bind(match_id)
        .bind(status)
        .bind(auth.identity_id)
        .execute(&mut *tx)
        .await?;
    crate::product::audit(
        &mut tx,
        auth.identity_id,
        &format!("document.copy_{status}"),
        target,
        json!({"match_id":match_id,"source_document_id":source,"uploader":uploader}),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(json!({"status":status})))
}

#[derive(utoipa::OpenApi)]
#[openapi(paths(check, copies, respond))]
pub struct CopiesApi;
