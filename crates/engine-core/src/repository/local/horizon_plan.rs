//! Read-only horizon planning for one local project repository.
//!
//! Loads a [`HistoryView`] inside one read snapshot on a connection SQLite itself
//! opens with `query_only` (the same discipline as `gc_plan` in `retention.rs`) and
//! hands it to the pure planner. Nothing is written, signed or deleted; there is no
//! schema change and no horizon table (design phases P2 and later).
//!
//! Cost: one pass over the commit table (a signature check per commit, as
//! `gc_plan`), one point read per distinct reachable object, and the planner's own
//! costs (see `repository::horizon::plan`). The tracking limits of `gc_plan` apply:
//! larger repositories are refused, never planned partially.

use super::{
    decode_commit, pack,
    retention::{address_of, raw, MAX_TRACKED_COMMITS, MAX_TRACKED_OBJECTS},
    storage, LocalRepository,
};
use crate::{
    repository::horizon::{
        plan::{self, HistoryView, HorizonPlan, ObjectGraph, PlanOptions},
        policy::Policy,
        Error, HeadRef,
    },
    shared_checkpoint::{self, Address},
};
use sqlx::{Connection, SqliteConnection};
use std::collections::HashMap;

const COMMIT_PAGE: i64 = 16;

impl LocalRepository {
    /// Describe what a history horizon under `policy` would release, changing
    /// nothing. See `docs/HORIZON-RECORD-V1.md` for the plan and its hash.
    pub async fn horizon_plan(
        &self,
        policy: &Policy,
        options: &PlanOptions,
    ) -> Result<HorizonPlan, Error> {
        // A private connection that SQLite itself refuses to write through.
        let mut connection = SqliteConnection::connect_with(
            &self
                .options
                .clone()
                .create_if_missing(false)
                .pragma("query_only", "ON"),
        )
        .await
        .map_err(crate::repository::Error::from)?;
        let view = async {
            let mut tx = connection
                .begin()
                .await
                .map_err(crate::repository::Error::from)?;
            let view = load_view(&mut tx, self.project).await?;
            tx.rollback()
                .await
                .map_err(crate::repository::Error::from)?;
            Ok::<_, Error>(view)
        }
        .await;
        let _ = connection.close().await;
        plan::plan(&view?, policy, options)
    }
}

async fn load_view(tx: &mut SqliteConnection, project: uuid::Uuid) -> Result<HistoryView, Error> {
    let db = |e: sqlx::Error| Error::from(crate::repository::Error::from(e));
    let (schema, bound): (i64, String) =
        sqlx::query_as("select version,project from local_meta where singleton=1")
            .fetch_one(&mut *tx)
            .await
            .map_err(db)?;
    if bound != project.to_string() || !storage::supported(schema) {
        return Err(crate::repository::Error::Invalid("local schema/project").into());
    }
    let packed = schema >= 3;
    let object_rows: i64 = sqlx::query_scalar("select count(*) from objects")
        .fetch_one(&mut *tx)
        .await
        .map_err(db)?;
    let commit_rows: i64 = sqlx::query_scalar("select count(*) from commits")
        .fetch_one(&mut *tx)
        .await
        .map_err(db)?;
    if object_rows > MAX_TRACKED_OBJECTS {
        return Err(crate::repository::Error::Limit("horizon plan tracked objects").into());
    }
    if commit_rows > MAX_TRACKED_COMMITS {
        return Err(crate::repository::Error::Limit("horizon plan tracked commits").into());
    }
    let mut view = HistoryView::new(project);
    let heads: Vec<(String, String, i64)> =
        sqlx::query_as("select name,head,generation from refs order by name")
            .fetch_all(&mut *tx)
            .await
            .map_err(db)?;
    for (branch, commit, generation) in heads {
        view.add_branch(HeadRef {
            branch,
            commit: Address::try_from(commit)?,
            generation,
        })?;
    }

    // Verified commits, streamed in address order.
    let mut after = String::new();
    loop {
        let rows: Vec<(String, Vec<u8>, i64)> = sqlx::query_as(
            "select address,body,height from commits where address>? order by address limit ?",
        )
        .bind(&after)
        .bind(COMMIT_PAGE)
        .fetch_all(&mut *tx)
        .await
        .map_err(db)?;
        let Some(last) = rows.last() else { break };
        after = last.0.clone();
        for (address, body, height) in rows {
            let address = Address::try_from(address)?;
            let signed = decode_commit(&body, &address, project)?;
            view.add_commit(
                &address,
                &signed.commit,
                body.len() as u64,
                u64::try_from(height).ok(),
            )?;
        }
    }

    // Object graphs of every distinct state, each object read once. Objects are
    // interned by `(document, address)`: the same bytes under another document are
    // another object.
    let mut interner = Interner::default();
    let mut stack: Vec<u32> = Vec::new();
    for (document, root) in view.state_keys() {
        let object = interner.intern((document.as_u128(), raw(&root)?));
        view.set_state_object(document, &root, object);
        stack.push(object);
    }
    let mut sizes: Vec<u64> = Vec::new();
    let mut children: Vec<Vec<u32>> = Vec::new();
    let mut done: Vec<bool> = Vec::new();
    while let Some(object) = stack.pop() {
        let count = interner.keys.len();
        sizes.resize(count, 0);
        children.resize(count, Vec::new());
        done.resize(count, false);
        if std::mem::replace(&mut done[object as usize], true) {
            continue;
        }
        let (document, raw_address) = interner.keys[object as usize];
        let address = address_of(&raw_address)?;
        let (stored, codec, size): (Vec<u8>, i64, Option<i64>) = sqlx::query_as(if packed {
            "select bytes,codec,size from objects where document=? and address=? and length(bytes)<=1048576"
        } else {
            "select bytes,0,null from objects where document=? and address=? and length(bytes)<=1048576"
        })
        .bind(uuid::Uuid::from_u128(document).to_string())
        .bind(address.as_str())
        .fetch_optional(&mut *tx)
        .await
        .map_err(db)?
        .ok_or_else(|| shared_checkpoint::Error::Missing(address.as_str().into()))?;
        let bytes = pack::unpack(codec, stored, size)?;
        sizes[object as usize] = bytes.len() as u64;
        for child in shared_checkpoint::references(&address, &bytes)? {
            let child = interner.intern((document, raw(&child)?));
            children[object as usize].push(child);
            if done.get(child as usize) != Some(&true) {
                stack.push(child);
            }
        }
    }
    let count = interner.keys.len();
    sizes.resize(count, 0);
    children.resize(count, Vec::new());
    view.objects = ObjectGraph { sizes, children };
    Ok(view)
}

type ObjectKey = (u128, [u8; 32]);

/// Dense indices for `(document, address)` object keys, in discovery order.
#[derive(Default)]
struct Interner {
    index: HashMap<ObjectKey, u32>,
    keys: Vec<ObjectKey>,
}

impl Interner {
    fn intern(&mut self, key: ObjectKey) -> u32 {
        let next = self.keys.len() as u32;
        let keys = &mut self.keys;
        *self.index.entry(key).or_insert_with(|| {
            keys.push(key);
            next
        })
    }
}
