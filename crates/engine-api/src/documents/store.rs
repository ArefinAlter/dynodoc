//! Database reads/writes behind the `/documents` endpoints.
//!
//! Reads are cursor-paginated over the `document` table; the current materialized
//! state served alongside a document comes from the snapshot engine (snapshot + tail
//! fold), never an O(N) replay. Creating a document also seeds the creator's access
//! list with the `Author` role so the write path authorizes immediately.

use engine_core::governance::Role;
use engine_shared::{Document, DocumentId};
use sqlx::PgPool;
use uuid::Uuid;

use crate::error::ApiError;
use crate::pagination::Cursor;

/// Does this document exist? (a 404 vs. empty-result distinction for streams/ops).
pub async fn document_exists(pool: &PgPool, document_id: DocumentId) -> Result<bool, ApiError> {
    let exists: Option<(Uuid,)> = sqlx::query_as("select id from document where id = $1")
        .bind(document_id.0)
        .fetch_optional(pool)
        .await?;
    Ok(exists.is_some())
}

/// Fetch one document's metadata row, or `None` if it does not exist.
pub async fn get_document(
    pool: &PgPool,
    document_id: DocumentId,
) -> Result<Option<Document>, ApiError> {
    let doc = sqlx::query_as::<_, Document>("select * from document where id = $1")
        .bind(document_id.0)
        .fetch_optional(pool)
        .await?;
    Ok(doc)
}

/// List documents for the public index, cursor-paginated and optionally filtered by
/// `status`. Order is `(created_at, id)` descending-stable so the cursor is a stable
/// position even as new documents are inserted. Returns `limit + 1` is handled by the
/// caller deciding the next cursor; here we fetch exactly `limit` after the cursor.
///
/// The cursor's `last_seq` carries the last row's `created_at` as epoch microseconds
/// and `last_id` the UUID, so paging continues strictly after the last returned row.
pub async fn list_documents(
    pool: &PgPool,
    identity_id: Uuid,
    status: Option<&str>,
    after: Option<&Cursor>,
    limit: i64,
) -> Result<Vec<Document>, ApiError> {
    // Keyset pagination on (created_at, id). We page in descending created_at order;
    // the cursor pins the exact (created_at, id) boundary so inserts never shift it.
    let (after_micros, after_id) = match after {
        Some(c) => (
            Some(c.last_seq),
            Uuid::parse_str(&c.last_id).map_err(|_| ApiError::BadRequest {
                reason: "invalid cursor".into(),
            })?,
        ),
        None => (None, Uuid::nil()),
    };

    let rows = sqlx::query_as::<_, Document>(
        "select * from document
         where ($1::text = 'archived' or status <> 'archived')
           and deleted_at is null and effective_document_role(id,$5) is not null
           and ($1::text is null or status = $1)
           and (
                $2::bigint is null
                or (extract(epoch from created_at) * 1000000)::bigint < $2
                or ((extract(epoch from created_at) * 1000000)::bigint = $2 and id < $3)
           )
         order by created_at desc, id desc
         limit $4",
    )
    .bind(status)
    .bind(after_micros)
    .bind(after_id)
    .bind(limit)
    .bind(identity_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// The opaque cursor for a document row: `created_at` as epoch microseconds + its UUID.
pub fn document_cursor(doc: &Document) -> Cursor {
    let micros = doc.created_at.timestamp_micros();
    Cursor {
        last_seq: micros,
        last_id: doc.id.0.to_string(),
    }
}

/// Create a document and grant its creator the `Author` role in one transaction, so the
/// creator can immediately drive the write path.
pub async fn create_document(
    pool: &PgPool,
    title: &str,
    languages: &[String],
    created_by: Uuid,
) -> Result<Document, ApiError> {
    let mut tx = pool.begin().await?;

    let langs: Vec<String> = if languages.is_empty() {
        vec!["en".to_string()]
    } else {
        languages.to_vec()
    };

    let doc = sqlx::query_as::<_, Document>(
        "insert into document (title, languages, created_by)
         values ($1, $2, $3)
         returning *",
    )
    .bind(title)
    .bind(&langs)
    .bind(created_by)
    .fetch_one(&mut *tx)
    .await?;

    sqlx::query(
        "insert into document_access (document_id, identity_id, role)
         values ($1, $2, $3)
         on conflict (document_id, identity_id) do update set role = excluded.role",
    )
    .bind(doc.id.0)
    .bind(created_by)
    .bind(Role::Author.as_str())
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(doc)
}
