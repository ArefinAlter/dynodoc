//! SQLite local history with immutable content, verified commits and transactional
//! ref CAS. No remote permissions or automatic native-file writes are implied.

use super::{recovery, Commit, Error, Head, SignedCommit, MAX_ANCESTORS, MAX_COMMIT_BYTES};
use crate::{
    materializer::DocumentState,
    shared_checkpoint::{self, typed, Address, Limits, ObjectStore, WriteStats},
};
use engine_shared::DocumentId;
use sqlx::{
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous},
    SqliteConnection, SqlitePool,
};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    path::Path,
    time::Duration,
};

mod collector;
mod compact;
mod pack;
mod retention;
mod storage;
pub use collector::{GcApplyOptions, GcReport};
pub use compact::{CompactOptions, CompactReport, MAX_BATCH_OBJECTS};
pub use retention::{GcPlan, GcPlanOptions, Physical};
pub use storage::{StorageLimits, StorageUsage, SCHEMA_VERSION};

const SCHEMA: &str = "
create table if not exists local_meta(singleton integer primary key check(singleton=1), version integer not null, project text not null);
create table if not exists objects(document text not null,address text not null,bytes blob not null check(length(bytes) between 1 and 1048576),primary key(document,address));
create table if not exists commits(address text primary key,body blob not null check(length(body)<=1049600),height integer not null check(height>=0));
create table if not exists refs(name text primary key,head text not null references commits(address),generation integer not null check(generation>0));
create trigger if not exists objects_no_update before update on objects begin select raise(abort,'immutable object'); end;
create trigger if not exists objects_no_delete before delete on objects begin select raise(abort,'immutable object'); end;
create trigger if not exists commits_no_update before update on commits begin select raise(abort,'immutable commit'); end;
create trigger if not exists commits_no_delete before delete on commits begin select raise(abort,'immutable commit'); end;
";

#[derive(Clone)]
pub struct LocalRepository {
    pool: SqlitePool,
    project: uuid::Uuid,
    /// Local schema this handle opened: 3 after a writable open (which upgrades),
    /// 1 to 3 for a read-only open, which never upgrades anything.
    schema: i64,
    /// Connection settings, reused to open private read-only planning connections.
    options: SqliteConnectOptions,
}

/// Transfer inventory uses stable keyset cursors. Object bytes are always scoped
/// by document, including when the same content occurs in several project files.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObjectKey {
    pub document: DocumentId,
    pub address: Address,
}

pub struct ObjectPacket {
    pub key: ObjectKey,
    pub bytes: Vec<u8>,
}

struct Store<'a> {
    connection: &'a mut SqliteConnection,
    read_budget: Option<Limits>,
    /// Schema 3 rows carry a codec and a logical size; earlier rows are identity.
    packed: bool,
}
impl ObjectStore for Store<'_> {
    async fn put(
        &mut self,
        document: DocumentId,
        address: &Address,
        bytes: &[u8],
    ) -> Result<bool, shared_checkpoint::Error> {
        shared_checkpoint::validate_stored(address, bytes)?;
        // The address and every quota are about `bytes`; only the row differs.
        let query = if !self.packed {
            sqlx::query("insert into objects(document,address,bytes) values(?,?,?) on conflict(document,address) do nothing")
                .bind(document.0.to_string()).bind(address.as_str()).bind(bytes.to_vec())
        } else if let Some(stored) = pack::pack(bytes) {
            sqlx::query("insert into objects(document,address,bytes,codec,size) values(?,?,?,1,?) on conflict(document,address) do nothing")
                .bind(document.0.to_string()).bind(address.as_str()).bind(stored).bind(bytes.len() as i64)
        } else {
            sqlx::query("insert into objects(document,address,bytes,codec,size) values(?,?,?,0,?) on conflict(document,address) do nothing")
                .bind(document.0.to_string()).bind(address.as_str()).bind(bytes.to_vec()).bind(bytes.len() as i64)
        };
        let inserted = query
            .execute(&mut *self.connection)
            .await
            .map_err(storage::object_error)?
            .rows_affected()
            == 1;
        if !inserted && self.get(document, address).await? != bytes {
            return Err(shared_checkpoint::Error::HashMismatch);
        }
        Ok(inserted)
    }
    async fn get(
        &mut self,
        document: DocumentId,
        address: &Address,
    ) -> Result<Vec<u8>, shared_checkpoint::Error> {
        if self.read_budget.is_some_and(|limit| limit.max_objects == 0) {
            return Err(shared_checkpoint::Error::Limit("objects"));
        }
        let (stored, codec, size): (Vec<u8>, i64, Option<i64>) = sqlx::query_as(if self.packed {
            "select bytes,codec,size from objects where document=? and address=? and length(bytes)<=1048576"
        } else {
            "select bytes,0,null from objects where document=? and address=? and length(bytes)<=1048576"
        })
        .bind(document.0.to_string())
        .bind(address.as_str())
        .fetch_optional(&mut *self.connection)
        .await?
        .ok_or_else(|| shared_checkpoint::Error::Missing(address.as_str().into()))?;
        // Logical bytes from here on: the budget, the hash and the caller all see
        // exactly what was written, whatever the row stores.
        let bytes = pack::unpack(codec, stored, size)?;
        if let Some(budget) = &mut self.read_budget {
            budget.consume(bytes.len())?;
        }
        shared_checkpoint::validate_stored(address, &bytes)?;
        Ok(bytes)
    }
}

fn decode_commit(
    bytes: &[u8],
    expected: &Address,
    project: uuid::Uuid,
) -> Result<SignedCommit, Error> {
    if bytes.len() > MAX_COMMIT_BYTES + 1024 {
        return Err(Error::Limit("commit envelope bytes"));
    }
    let signed: SignedCommit = serde_json::from_slice(bytes)?;
    if signed.commit.project != project
        || signed.verify()? != *expected
        || serde_json::to_vec(&signed)? != bytes
    {
        return Err(Error::Invalid("stored commit address, scope or encoding"));
    }
    Ok(signed)
}

async fn load_commit(
    connection: &mut SqliteConnection,
    project: uuid::Uuid,
    address: &Address,
) -> Result<(SignedCommit, i64), Error> {
    let (bytes, height): (Vec<u8>, i64) =
        sqlx::query_as("select body,height from commits where address=? and length(body)<=1049600")
            .bind(address.as_str())
            .fetch_optional(connection)
            .await?
            .ok_or_else(|| Error::Missing(address.clone()))?;
    Ok((decode_commit(&bytes, address, project)?, height))
}

async fn load_recovery_plan(
    connection: &mut SqliteConnection,
    project: uuid::Uuid,
    kind: recovery::Kind,
    source: &Address,
    onto: &Address,
    mainline: Option<&Address>,
) -> Result<recovery::Plan, Error> {
    let source = load_commit(connection, project, source).await?.0;
    let onto = load_commit(connection, project, onto).await?.0;
    let mainline = match (mainline, source.commit.parents.as_slice()) {
        (Some(chosen), parents) if parents.contains(chosen) => Some(chosen),
        (Some(_), _) => return Err(Error::Invalid("recovery mainline is not a source parent")),
        (None, []) => None,
        (None, [parent]) => Some(parent),
        (None, _) => {
            return Err(Error::Invalid(
                "merge recovery requires an explicit mainline",
            ))
        }
    };
    let parent = match mainline {
        Some(address) => Some(load_commit(connection, project, address).await?.0),
        None => None,
    };
    recovery::plan(
        kind,
        &source.commit,
        &onto.commit,
        parent.as_ref().map(|p| &p.commit),
    )
}

/// Ancestry (parents only) of `head`, bounded by commit count and bytes.
async fn collect_ancestors(
    connection: &mut SqliteConnection,
    project: uuid::Uuid,
    head: &Address,
) -> Result<BTreeMap<Address, Vec<Address>>, Error> {
    let mut queue = VecDeque::from([head.clone()]);
    let mut found = BTreeMap::new();
    let mut bytes = 64 * 1024 * 1024usize;
    while let Some(address) = queue.pop_front() {
        if found.contains_key(&address) {
            continue;
        }
        if found.len() == MAX_ANCESTORS {
            return Err(Error::Limit("ancestor count"));
        }
        let (commit, _) = load_commit(connection, project, &address).await?;
        bytes = bytes
            .checked_sub(commit.commit.bytes()?.len())
            .ok_or(Error::Limit("ancestor bytes"))?;
        queue.extend(commit.commit.parents.iter().cloned());
        found.insert(address, commit.commit.parents);
    }
    Ok(found)
}

/// Best common ancestors of two ancestry maps; criss-cross may yield several.
fn best_common(
    left: &BTreeMap<Address, Vec<Address>>,
    right: &BTreeMap<Address, Vec<Address>>,
) -> Vec<Address> {
    let common: BTreeSet<_> = left
        .keys()
        .filter(|key| right.contains_key(*key))
        .cloned()
        .collect();
    let dominated: BTreeSet<_> = common
        .iter()
        .flat_map(|key| left[key].iter().cloned())
        .collect();
    common.difference(&dominated).cloned().collect()
}

/// A v3 commit's records must describe its actual parents and their unique merge
/// base. The merge algorithm itself is not re-run here (see `project_merge`):
/// importing must not depend on the algorithm staying unchanged.
async fn check_same_file_provenance(
    connection: &mut SqliteConnection,
    project: uuid::Uuid,
    commit: &Commit,
    parents: &[SignedCommit],
) -> Result<(), Error> {
    let [left, right] = parents else {
        return Err(Error::Invalid("same-file resolution requires two parents"));
    };
    let left_ancestors = collect_ancestors(connection, project, &commit.parents[0]).await?;
    let right_ancestors = collect_ancestors(connection, project, &commit.parents[1]).await?;
    let bases = best_common(&left_ancestors, &right_ancestors);
    let [base] = bases.as_slice() else {
        return Err(Error::Invalid(
            "same-file resolution requires a unique merge base",
        ));
    };
    let base = load_commit(connection, project, base).await?.0;
    for record in &commit.same_file_resolutions {
        let matches = |side: &SignedCommit, state: &Address| {
            side.commit
                .files
                .get(&record.path)
                .is_some_and(|f| f.document == record.document && f.state == *state)
        };
        if !(matches(&base, &record.base)
            && matches(left, &record.left)
            && matches(right, &record.right))
        {
            return Err(Error::Invalid(
                "same-file resolution does not match its parents",
            ));
        }
    }
    Ok(())
}

impl LocalRepository {
    /// Create or reopen one project repository. WAL + FULL synchronous commits
    /// supply crash recovery under SQLite/filesystem durability guarantees.
    pub async fn open(path: &Path, project: uuid::Uuid) -> Result<Self, Error> {
        if project.is_nil() {
            return Err(Error::Invalid("nil project"));
        }
        let options = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .synchronous(SqliteSynchronous::Full)
            .foreign_keys(true)
            .busy_timeout(Duration::from_secs(5));
        let pool = SqlitePoolOptions::new()
            .max_connections(4)
            .connect_with(options.clone())
            .await?;
        let mut tx = pool.begin().await?;
        sqlx::raw_sql(SCHEMA).execute(&mut *tx).await?;
        sqlx::query("insert into local_meta(singleton,version,project) values(1,1,?) on conflict(singleton) do nothing")
            .bind(project.to_string()).execute(&mut *tx).await?;
        let (version, actual): (i64, String) =
            sqlx::query_as("select version,project from local_meta where singleton=1")
                .fetch_one(&mut *tx)
                .await?;
        if !storage::supported(version) || actual != project.to_string() {
            return Err(Error::Invalid("local schema/project"));
        }
        // Each step is transactional with the rest of this initialization. An
        // older client rejects the number it does not know, and a failed step
        // leaves the previous schema in place.
        if version == 1 {
            storage::upgrade(&mut tx).await?;
        }
        if version <= 2 {
            storage::upgrade_packed(&mut tx).await?;
        }
        // Missing accounting state is corruption, never permission to admit
        // unbounded writes or to reconstruct lower usage on reopen.
        storage::usage(&mut tx, SCHEMA_VERSION).await?;
        tx.commit().await?;
        Ok(Self {
            pool,
            project,
            schema: SCHEMA_VERSION,
            options,
        })
    }

    /// Open an existing repository without any chance of writing: nothing is
    /// created, no schema is upgraded and every connection is `query_only`.
    /// Read methods work; mutating methods fail with a database error.
    pub async fn open_read_only(path: &Path, project: uuid::Uuid) -> Result<Self, Error> {
        if project.is_nil() {
            return Err(Error::Invalid("nil project"));
        }
        let options = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(false)
            .foreign_keys(true)
            .busy_timeout(Duration::from_secs(5))
            .pragma("query_only", "ON");
        let pool = SqlitePoolOptions::new()
            .max_connections(4)
            .connect_with(options.clone())
            .await?;
        let (version, actual): (i64, String) =
            sqlx::query_as("select version,project from local_meta where singleton=1")
                .fetch_one(&pool)
                .await?;
        if !storage::supported(version) || actual != project.to_string() {
            return Err(Error::Invalid("local schema/project"));
        }
        Ok(Self {
            pool,
            project,
            schema: version,
            options,
        })
    }
    pub async fn close(self) {
        self.pool.close().await;
    }
    pub fn project(&self) -> uuid::Uuid {
        self.project
    }

    pub async fn storage_usage(&self) -> Result<StorageUsage, Error> {
        let mut connection = self.pool.acquire().await?;
        storage::usage(&mut connection, self.schema).await
    }

    /// Explicit local-owner action; never evicts history or permits limits below
    /// retained usage. All writes, including staged transfer objects, share these
    /// persistent limits across connections and restarts.
    pub async fn set_storage_limits(&self, limits: StorageLimits) -> Result<StorageUsage, Error> {
        limits.validate()?;
        let mut tx = self.pool.begin().await?;
        let changed = sqlx::query("update local_storage set byte_limit=?,object_limit=?,commit_limit=?,branch_limit=? where singleton=1 and bytes_used<=? and objects_used<=? and commits_used<=? and branches_used<=?")
            .bind(limits.bytes).bind(limits.objects).bind(limits.commits).bind(limits.branches)
            .bind(limits.bytes).bind(limits.objects).bind(limits.commits).bind(limits.branches)
            .execute(&mut *tx).await?.rows_affected();
        if changed != 1 {
            return Err(Error::Limit("local storage limits below retained usage"));
        }
        let usage = storage::usage(&mut tx, self.schema).await?;
        tx.commit().await?;
        Ok(usage)
    }

    /// Transactional, verified materialization; no branch is changed by this call.
    pub async fn write_state(
        &self,
        document: DocumentId,
        state: &DocumentState,
    ) -> Result<(Address, WriteStats), Error> {
        let mut tx = self.pool.begin().await?;
        let mut store = Store {
            connection: &mut tx,
            read_budget: None,
            packed: self.schema >= 3,
        };
        let (root, stats) = typed::write_state(
            &mut store,
            document,
            state,
            Limits::default(),
            Default::default(),
        )
        .await?;
        if typed::read_state(
            &mut store,
            document,
            &root,
            Limits::default(),
            Default::default(),
        )
        .await?
            != *state
        {
            return Err(Error::Invalid("local reconstruction"));
        }
        tx.commit().await?;
        Ok((root, stats))
    }
    pub async fn read_state(
        &self,
        document: DocumentId,
        root: &Address,
    ) -> Result<DocumentState, Error> {
        let mut connection = self.pool.acquire().await?;
        Ok(typed::read_state(
            &mut Store {
                connection: &mut connection,
                read_budget: None,
                packed: self.schema >= 3,
            },
            document,
            root,
            Limits::default(),
            Default::default(),
        )
        .await?)
    }

    /// Import a signed commit only after all parents and complete file graphs are
    /// available and verified. Reordered transfers can retry without duplication.
    /// All file traversals share one global I/O budget; only one state is resident.
    pub async fn import_commit(&self, signed: &SignedCommit) -> Result<Address, Error> {
        let address = signed.verify()?;
        if signed.commit.project != self.project {
            return Err(Error::Invalid("commit project"));
        }
        let bytes = serde_json::to_vec(signed)?;
        if bytes.len() > MAX_COMMIT_BYTES + 1024 {
            return Err(Error::Limit("commit envelope bytes"));
        }
        let mut tx = self.pool.begin().await?;
        // Acquire SQLite's writer reservation before reading parents, avoiding
        // deferred-transaction lock upgrade races between two importers.
        sqlx::query("update local_meta set version=version where singleton=1")
            .execute(&mut *tx)
            .await?;
        let mut height = 0;
        let mut parents = Vec::new();
        for parent in &signed.commit.parents {
            let (loaded, parent_height) = load_commit(&mut tx, self.project, parent).await?;
            height = height.max(
                parent_height
                    .checked_add(1)
                    .ok_or(Error::Limit("commit height"))?,
            );
            parents.push(loaded);
        }
        if !signed.commit.same_file_resolutions.is_empty() {
            check_same_file_provenance(&mut tx, self.project, &signed.commit, &parents).await?;
        }
        if let Some(source) = &signed.commit.recovery {
            let plan = load_recovery_plan(
                &mut tx,
                self.project,
                source.kind,
                &source.commit,
                &signed.commit.parents[0],
                source.mainline.as_ref(),
            )
            .await?;
            // A signed source must record the actual selected mainline, including
            // the automatic sole-parent selection made by local preview commands.
            if plan.source != *source {
                return Err(Error::Invalid("recovery mainline must be recorded"));
            }
            plan.validate_files(&signed.commit.files)?;
        }
        let mut store = Store {
            connection: &mut tx,
            read_budget: Some(Limits::default()),
            packed: self.schema >= 3,
        };
        for file in signed.commit.files.values() {
            typed::read_state(
                &mut store,
                file.document,
                &file.state,
                Limits::default(),
                Default::default(),
            )
            .await?;
        }
        sqlx::query("insert into commits(address,body,height) values(?,?,?) on conflict(address) do nothing")
            .bind(address.as_str()).bind(&bytes).bind(height).execute(&mut *tx).await.map_err(storage::error)?;
        let existing = load_commit(&mut tx, self.project, &address).await?;
        if existing.0.commit != signed.commit || existing.1 != height {
            return Err(Error::Invalid("commit collision"));
        }
        tx.commit().await?;
        Ok(address)
    }
    pub async fn commit(&self, address: &Address) -> Result<SignedCommit, Error> {
        let mut connection = self.pool.acquire().await?;
        Ok(load_commit(&mut connection, self.project, address).await?.0)
    }

    /// Preview a historical delta against an exact current commit, without
    /// storing a result or changing refs. Two-parent sources require a mainline.
    pub async fn plan_recovery(
        &self,
        kind: recovery::Kind,
        source: &Address,
        onto: &Address,
        mainline: Option<&Address>,
    ) -> Result<recovery::Plan, Error> {
        let mut connection = self.pool.acquire().await?;
        load_recovery_plan(&mut connection, self.project, kind, source, onto, mainline).await
    }

    pub async fn head(&self, branch: &str) -> Result<Option<Head>, Error> {
        if !super::valid_branch(branch) {
            return Err(Error::Invalid("branch name"));
        }
        let row: Option<(String, i64)> =
            sqlx::query_as("select head,generation from refs where name=?")
                .bind(branch)
                .fetch_optional(&self.pool)
                .await?;
        row.map(|(commit, generation)| {
            Ok(Head {
                commit: Address::try_from(commit)?,
                generation,
            })
        })
        .transpose()
    }
    /// Atomic project-wide ref publication. History objects are already durable.
    /// A stale expected generation never resets another client's branch update.
    pub async fn compare_and_swap(
        &self,
        branch: &str,
        expected: Option<&Head>,
        new: &Address,
    ) -> Result<Head, Error> {
        if !super::valid_branch(branch) {
            return Err(Error::Invalid("branch name"));
        }
        let generation = expected.map_or(Ok(1), |v| {
            v.generation
                .checked_add(1)
                .ok_or(Error::Limit("ref generation"))
        })?;
        let mut tx = self.pool.begin().await?;
        sqlx::query("update local_meta set version=version where singleton=1")
            .execute(&mut *tx)
            .await?;
        load_commit(&mut tx, self.project, new).await?;
        let affected = if let Some(old) = expected {
            sqlx::query(
                "update refs set head=?,generation=? where name=? and head=? and generation=?",
            )
            .bind(new.as_str())
            .bind(generation)
            .bind(branch)
            .bind(old.commit.as_str())
            .bind(old.generation)
            .execute(&mut *tx)
            .await
            .map_err(storage::error)?
            .rows_affected()
        } else {
            sqlx::query(
                "insert into refs(name,head,generation) values(?,?,1) on conflict(name) do nothing",
            )
            .bind(branch)
            .bind(new.as_str())
            .execute(&mut *tx)
            .await
            .map_err(storage::error)?
            .rows_affected()
        };
        if affected != 1 {
            return Err(Error::StaleRef);
        }
        tx.commit().await?;
        Ok(Head {
            commit: new.clone(),
            generation,
        })
    }

    async fn ancestors(&self, head: &Address) -> Result<BTreeMap<Address, Vec<Address>>, Error> {
        let mut connection = self.pool.acquire().await?;
        collect_ancestors(&mut connection, self.project, head).await
    }
    /// Return all best common ancestors. Criss-cross histories may have several;
    /// callers must not silently choose one as the unique merge base.
    pub async fn merge_bases(
        &self,
        left: &Address,
        right: &Address,
    ) -> Result<Vec<Address>, Error> {
        let left = self.ancestors(left).await?;
        let right = self.ancestors(right).await?;
        Ok(best_common(&left, &right))
    }

    /// Bounded resumable inventory. A cursor is the last key in a successful page.
    pub async fn inventory(
        &self,
        after: Option<&ObjectKey>,
        limit: usize,
    ) -> Result<Vec<ObjectKey>, Error> {
        if limit == 0 || limit > 256 {
            return Err(Error::Limit("inventory page"));
        }
        let (document, address) = after.map_or((String::new(), String::new()), |key| {
            (key.document.0.to_string(), key.address.as_str().into())
        });
        let rows: Vec<(String, String)> = sqlx::query_as("select document,address from objects where (document,address)>(?,?) order by document,address limit ?")
            .bind(document).bind(address).bind(limit as i64).fetch_all(&self.pool).await?;
        rows.into_iter()
            .map(|(document, address)| {
                Ok(ObjectKey {
                    document: DocumentId(
                        uuid::Uuid::parse_str(&document)
                            .map_err(|_| Error::Invalid("stored document id"))?,
                    ),
                    address: Address::try_from(address)?,
                })
            })
            .collect()
    }
    pub async fn missing(&self, keys: &[ObjectKey]) -> Result<Vec<ObjectKey>, Error> {
        if keys.len() > 256 {
            return Err(Error::Limit("negotiation page"));
        }
        let mut missing = Vec::new();
        let mut connection = self.pool.acquire().await?;
        for key in keys {
            let exists: bool = sqlx::query_scalar(
                "select exists(select 1 from objects where document=? and address=?)",
            )
            .bind(key.document.0.to_string())
            .bind(key.address.as_str())
            .fetch_one(&mut *connection)
            .await?;
            if !exists {
                missing.push(key.clone());
            }
        }
        Ok(missing)
    }
    pub async fn object(&self, key: &ObjectKey) -> Result<ObjectPacket, Error> {
        let mut connection = self.pool.acquire().await?;
        let bytes = Store {
            connection: &mut connection,
            read_budget: None,
            packed: self.schema >= 3,
        }
        .get(key.document, &key.address)
        .await?;
        Ok(ObjectPacket {
            key: key.clone(),
            bytes,
        })
    }
    /// Each transfer batch is atomic and idempotent. Its objects may be unreachable
    /// until later batches complete; no commit or branch is published implicitly.
    pub async fn import_objects(&self, objects: &[ObjectPacket]) -> Result<usize, Error> {
        if objects.len() > 64
            || objects
                .iter()
                .try_fold(0usize, |total, object| {
                    total.checked_add(object.bytes.len())
                })
                .is_none_or(|n| n > 1024 * 1024)
        {
            return Err(Error::Limit("transfer batch"));
        }
        let mut tx = self.pool.begin().await?;
        let mut count = 0;
        let mut store = Store {
            connection: &mut tx,
            read_budget: None,
            packed: self.schema >= 3,
        };
        for object in objects {
            count += usize::from(
                store
                    .put(object.key.document, &object.key.address, &object.bytes)
                    .await?,
            );
        }
        tx.commit().await?;
        Ok(count)
    }
}

#[cfg(test)]
mod collector_tests;
#[cfg(test)]
mod convergence_tests;
#[cfg(test)]
mod merge_tests;
#[cfg(test)]
mod pack_tests;
#[cfg(test)]
mod retention_tests;
#[cfg(test)]
mod storage_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        materializer::MaterializedNode,
        repository::{tests::signed, FileEntry},
    };
    use engine_shared::NodeId;
    use serde_json::json;

    pub(super) fn path() -> std::path::PathBuf {
        std::env::temp_dir().join(format!("dynodoc-repo-test-{}.sqlite", uuid::Uuid::new_v4()))
    }
    pub(super) fn state(text: &str) -> DocumentState {
        let node = MaterializedNode {
            id: NodeId("01ARZ3NDEKTSV4RRFFQ69G5FAV".into()),
            parent_id: None,
            node_type: "paragraph".into(),
            pos: "a0".into(),
            var_name: None,
            deleted: false,
            current_fields: json!({"label":text}),
        };
        DocumentState {
            nodes: BTreeMap::from([(node.id.clone(), node)]),
            ..Default::default()
        }
    }
    pub(super) async fn cleanup(repo: LocalRepository, file: &Path) {
        repo.close().await;
        // On Windows SQLite's worker can release its last file handle just after
        // Pool::close completes. Only retry that specific sharing violation.
        for attempt in 0..20 {
            match std::fs::remove_file(file) {
                Ok(()) => return,
                Err(error) if matches!(error.raw_os_error(), Some(32 | 33)) && attempt < 19 => {
                    tokio::time::sleep(Duration::from_millis(25)).await;
                }
                Err(error) => panic!("owned test database cleanup: {error}"),
            }
        }
    }

    #[tokio::test]
    async fn recovery_fifty_commits_back_preserves_new_work_and_survives_restart() {
        use crate::repository::{recovery::Kind, SignedCommit};
        use ring::signature::Ed25519KeyPair;
        let file = path();
        let project = uuid::Uuid::new_v4();
        let repo = LocalRepository::open(&file, project).await.unwrap();
        let report = DocumentId(uuid::Uuid::new_v4());
        let budget = DocumentId(uuid::Uuid::new_v4());
        let before = repo
            .write_state(report, &state("Recoverable original paragraph"))
            .await
            .unwrap()
            .0;
        let after = repo
            .write_state(report, &state("Replacement paragraph"))
            .await
            .unwrap()
            .0;
        let budget_root = repo
            .write_state(budget, &state("Initial budget"))
            .await
            .unwrap()
            .0;
        let base = signed(
            project,
            vec![],
            BTreeMap::from([
                (
                    "Report.docx".into(),
                    FileEntry {
                        document: report,
                        state: before.clone(),
                    },
                ),
                (
                    "Budget.xlsx".into(),
                    FileEntry {
                        document: budget,
                        state: budget_root,
                    },
                ),
            ]),
            "Initial",
        );
        let base_id = repo.import_commit(&base).await.unwrap();
        let mut source_files = base.commit.files.clone();
        source_files.get_mut("Report.docx").unwrap().state = after.clone();
        let source = signed(
            project,
            vec![base_id.clone()],
            source_files,
            "Replace original paragraph",
        );
        let source_id = repo.import_commit(&source).await.unwrap();
        let later_budget = repo
            .write_state(budget, &state("Retain this later budget"))
            .await
            .unwrap()
            .0;
        let mut later_files = source.commit.files.clone();
        later_files.get_mut("Budget.xlsx").unwrap().state = later_budget.clone();
        let mut current = source_id.clone();
        for i in 0..50 {
            current = repo
                .import_commit(&signed(
                    project,
                    vec![current],
                    later_files.clone(),
                    &format!("Later work {i}"),
                ))
                .await
                .unwrap();
        }
        let head = repo.compare_and_swap("main", None, &current).await.unwrap();
        let object_count = repo.inventory(None, 256).await.unwrap().len();
        let plan = repo
            .plan_recovery(Kind::Revert, &source_id, &current, None)
            .await
            .unwrap();
        let key = Ed25519KeyPair::from_seed_unchecked(&[7; 32]).unwrap();
        let recovered = SignedCommit::sign(
            plan.resolve(&BTreeMap::new(), 2, "Recover old paragraph".into())
                .unwrap(),
            &key,
        )
        .unwrap();
        let recovered_id = repo.import_commit(&recovered).await.unwrap();
        assert_eq!(recovered.commit.parents, vec![current.clone()]);
        assert_eq!(recovered.commit.files["Report.docx"].state, before);
        assert_eq!(recovered.commit.files["Budget.xlsx"].state, later_budget);
        assert_eq!(repo.head("main").await.unwrap(), Some(head.clone()));
        assert_eq!(repo.inventory(None, 256).await.unwrap().len(), object_count);
        repo.close().await;
        let repo = LocalRepository::open(&file, project).await.unwrap();
        assert_eq!(repo.import_commit(&recovered).await.unwrap(), recovered_id);
        let published = repo
            .compare_and_swap("main", Some(&head), &recovered_id)
            .await
            .unwrap();
        let read = repo.commit(&published.commit).await.unwrap();
        assert_eq!(read.commit.recovery.unwrap().commit, source_id);
        assert_eq!(
            repo.read_state(report, &before).await.unwrap(),
            state("Recoverable original paragraph")
        );
        let pick = repo
            .plan_recovery(Kind::CherryPick, &source_id, &recovered_id, None)
            .await
            .unwrap();
        let picked = SignedCommit::sign(
            pick.resolve(&BTreeMap::new(), 3, "Reapply selected change".into())
                .unwrap(),
            &key,
        )
        .unwrap();
        repo.import_commit(&picked).await.unwrap();
        assert_eq!(picked.commit.files, later_files);
        assert_eq!(
            repo.merge_bases(&recovered_id, &current).await.unwrap(),
            vec![current]
        );
        assert!(matches!(
            repo.compare_and_swap("main", Some(&head), &base_id).await,
            Err(Error::StaleRef)
        ));
        cleanup(repo, &file).await;
    }

    #[tokio::test]
    async fn imported_recovery_requires_source_mainline_and_preserves_settled_paths() {
        use crate::repository::{recovery::Kind, SignedCommit};
        use ring::signature::Ed25519KeyPair;
        let file = path();
        let other_file = path();
        let project = uuid::Uuid::new_v4();
        let repo = LocalRepository::open(&file, project).await.unwrap();
        let other = LocalRepository::open(&other_file, project).await.unwrap();
        let document = DocumentId(uuid::Uuid::new_v4());
        let root = repo
            .write_state(document, &state("Retained"))
            .await
            .unwrap()
            .0;
        let base = signed(project, vec![], BTreeMap::new(), "Empty");
        let base_id = repo.import_commit(&base).await.unwrap();
        other.import_commit(&base).await.unwrap();
        let source = signed(
            project,
            vec![base_id.clone()],
            BTreeMap::from([(
                "a".into(),
                FileEntry {
                    document,
                    state: root,
                },
            )]),
            "Add file",
        );
        let source_id = repo.import_commit(&source).await.unwrap();
        let key = Ed25519KeyPair::from_seed_unchecked(&[7; 32]).unwrap();
        let plan = repo
            .plan_recovery(Kind::CherryPick, &source_id, &base_id, None)
            .await
            .unwrap();
        let valid = plan
            .resolve(&BTreeMap::new(), 2, "Recover addition".into())
            .unwrap();
        let signed_valid = SignedCommit::sign(valid.clone(), &key).unwrap();
        assert!(matches!(
            other.import_commit(&signed_valid).await,
            Err(Error::Missing(_))
        ));
        let mut bad = valid.clone();
        bad.recovery.as_mut().unwrap().mainline = None;
        assert!(repo
            .import_commit(&SignedCommit::sign(bad, &key).unwrap())
            .await
            .is_err());
        let mut bad = valid.clone();
        bad.recovery.as_mut().unwrap().mainline = Some(source_id.clone());
        assert!(repo
            .import_commit(&SignedCommit::sign(bad, &key).unwrap())
            .await
            .is_err());
        let mut bad = valid;
        bad.files.clear();
        let bad = SignedCommit::sign(bad, &key).unwrap();
        assert!(repo.import_commit(&bad).await.is_err());
        assert!(matches!(
            repo.commit(&bad.verify().unwrap()).await,
            Err(Error::Missing(_))
        ));
        repo.import_commit(&signed_valid).await.unwrap();
        let merge = signed(
            project,
            vec![base_id.clone(), source_id.clone()],
            source.commit.files.clone(),
            "Merge",
        );
        let merge_id = repo.import_commit(&merge).await.unwrap();
        assert!(repo
            .plan_recovery(Kind::Revert, &merge_id, &merge_id, None)
            .await
            .is_err());
        assert!(repo
            .plan_recovery(Kind::Revert, &merge_id, &merge_id, Some(&merge_id))
            .await
            .is_err());
        let plan = repo
            .plan_recovery(Kind::Revert, &merge_id, &merge_id, Some(&base_id))
            .await
            .unwrap();
        let undone = SignedCommit::sign(
            plan.resolve(
                &BTreeMap::new(),
                3,
                "Undo merge relative to empty parent".into(),
            )
            .unwrap(),
            &key,
        )
        .unwrap();
        assert!(undone.commit.files.is_empty());
        repo.import_commit(&undone).await.unwrap();
        cleanup(other, &other_file).await;
        cleanup(repo, &file).await;
    }

    #[tokio::test]
    async fn multi_file_commits_branches_and_old_states_survive_reopening() {
        let file = path();
        let project = uuid::Uuid::new_v4();
        let repo = LocalRepository::open(&file, project).await.unwrap();
        let document = DocumentId(uuid::Uuid::new_v4());
        let sheet = DocumentId(uuid::Uuid::new_v4());
        let (root, _) = repo
            .write_state(document, &state("First document"))
            .await
            .unwrap();
        let (sheet_root, _) = repo
            .write_state(sheet, &state("First sheet"))
            .await
            .unwrap();
        let initial = signed(
            project,
            vec![],
            BTreeMap::from([
                (
                    "Report.docx".into(),
                    FileEntry {
                        document,
                        state: root.clone(),
                    },
                ),
                (
                    "Budget.xlsx".into(),
                    FileEntry {
                        document: sheet,
                        state: sheet_root.clone(),
                    },
                ),
            ]),
            "Initial project",
        );
        let first = repo.import_commit(&initial).await.unwrap();
        let head = repo.compare_and_swap("main", None, &first).await.unwrap();
        let (second_root, _) = repo
            .write_state(document, &state("Edited document"))
            .await
            .unwrap();
        let mut changed = initial.commit.files.clone();
        changed.get_mut("Report.docx").unwrap().state = second_root;
        let second = repo
            .import_commit(&signed(
                project,
                vec![first.clone()],
                changed,
                "Edit report",
            ))
            .await
            .unwrap();
        repo.compare_and_swap("review/report", None, &second)
            .await
            .unwrap();
        repo.close().await;
        let repo = LocalRepository::open(&file, project).await.unwrap();
        assert_eq!(repo.head("main").await.unwrap(), Some(head));
        assert_eq!(
            repo.head("review/report").await.unwrap().unwrap().commit,
            second
        );
        assert_eq!(
            repo.read_state(document, &root).await.unwrap(),
            state("First document")
        );
        assert_eq!(
            repo.commit(&second).await.unwrap().commit.files["Budget.xlsx"].state,
            sheet_root
        );
        assert_eq!(
            repo.merge_bases(&first, &second).await.unwrap(),
            vec![first]
        );
        assert!(LocalRepository::open(&file, uuid::Uuid::new_v4())
            .await
            .is_err());
        cleanup(repo, &file).await;
    }

    #[tokio::test]
    async fn concurrent_cas_has_one_winner_and_prevents_aba() {
        let file = path();
        let project = uuid::Uuid::new_v4();
        let repo = LocalRepository::open(&file, project).await.unwrap();
        let a = repo
            .import_commit(&signed(project, vec![], BTreeMap::new(), "A"))
            .await
            .unwrap();
        let b = repo
            .import_commit(&signed(project, vec![a.clone()], BTreeMap::new(), "B"))
            .await
            .unwrap();
        let c = repo
            .import_commit(&signed(project, vec![a.clone()], BTreeMap::new(), "C"))
            .await
            .unwrap();
        let old = repo.compare_and_swap("main", None, &a).await.unwrap();
        let other = LocalRepository::open(&file, project).await.unwrap();
        let (left, right) = tokio::join!(
            repo.compare_and_swap("main", Some(&old), &b),
            other.compare_and_swap("main", Some(&old), &c)
        );
        assert_eq!(usize::from(left.is_ok()) + usize::from(right.is_ok()), 1);
        assert!(matches!(left, Err(Error::StaleRef)) || matches!(right, Err(Error::StaleRef)));
        let current = repo.head("main").await.unwrap().unwrap();
        let restored = repo
            .compare_and_swap("main", Some(&current), &a)
            .await
            .unwrap();
        assert!(restored.generation > old.generation);
        assert!(matches!(
            repo.compare_and_swap("main", Some(&old), &b).await,
            Err(Error::StaleRef)
        ));
        other.close().await;
        cleanup(repo, &file).await;
    }

    #[tokio::test]
    async fn missing_object_exchange_is_resumable_idempotent_and_reorder_safe() {
        let a_path = path();
        let b_path = path();
        let project = uuid::Uuid::new_v4();
        let a = LocalRepository::open(&a_path, project).await.unwrap();
        let b = LocalRepository::open(&b_path, project).await.unwrap();
        let document = DocumentId(uuid::Uuid::new_v4());
        let content = state(&"large content ".repeat(120_000));
        let (root, _) = a.write_state(document, &content).await.unwrap();
        let base = signed(
            project,
            vec![],
            BTreeMap::from([(
                "Report.docx".into(),
                FileEntry {
                    document,
                    state: root,
                },
            )]),
            "Initial",
        );
        let base_id = a.import_commit(&base).await.unwrap();
        let child = signed(
            project,
            vec![base_id.clone()],
            base.commit.files.clone(),
            "Reviewed",
        );
        let child_id = a.import_commit(&child).await.unwrap();
        assert!(b.import_commit(&base).await.is_err());
        assert!(matches!(
            b.import_commit(&child).await,
            Err(Error::Missing(_))
        ));
        assert!(b.compare_and_swap("main", None, &base_id).await.is_err());
        let mut cursor = None;
        loop {
            let page = a.inventory(cursor.as_ref(), 5).await.unwrap();
            if page.is_empty() {
                break;
            }
            let missing = b.missing(&page).await.unwrap();
            for key in missing.iter().rev() {
                // reordering object packets is permitted
                let packet = a.object(key).await.unwrap();
                assert_eq!(
                    b.import_objects(std::slice::from_ref(&packet))
                        .await
                        .unwrap(),
                    1
                );
                assert_eq!(
                    b.import_objects(std::slice::from_ref(&packet))
                        .await
                        .unwrap(),
                    0
                );
            }
            assert!(b.missing(&page).await.unwrap().is_empty());
            cursor = page.last().cloned();
        }
        // Close/reopen between transport and commit publication; no queue memory
        // or connected editor session is needed to recover the verified objects.
        b.close().await;
        let b = LocalRepository::open(&b_path, project).await.unwrap();
        assert_eq!(b.import_commit(&base).await.unwrap(), base_id);
        assert_eq!(b.import_commit(&base).await.unwrap(), base_id);
        assert_eq!(b.import_commit(&child).await.unwrap(), child_id);
        b.compare_and_swap("main", None, &child_id).await.unwrap();
        let file = &b.commit(&child_id).await.unwrap().commit.files["Report.docx"];
        assert_eq!(
            b.read_state(file.document, &file.state).await.unwrap(),
            content
        );
        cleanup(a, &a_path).await;
        cleanup(b, &b_path).await;
    }

    #[tokio::test]
    async fn failed_batch_and_uncommitted_transaction_publish_nothing() {
        let a_path = path();
        let b_path = path();
        let project = uuid::Uuid::new_v4();
        let a = LocalRepository::open(&a_path, project).await.unwrap();
        let b = LocalRepository::open(&b_path, project).await.unwrap();
        a.write_state(DocumentId(uuid::Uuid::new_v4()), &state("A"))
            .await
            .unwrap();
        let keys = a.inventory(None, 10).await.unwrap();
        assert!(keys.len() >= 2);
        let good = a.object(&keys[0]).await.unwrap();
        let mut bad = a.object(&keys[1]).await.unwrap();
        bad.bytes[0] ^= 1;
        assert!(b.import_objects(&[good, bad]).await.is_err());
        assert!(b.inventory(None, 10).await.unwrap().is_empty());
        let good = a.object(&keys[0]).await.unwrap();
        let mut tx = b.pool.begin().await.unwrap();
        Store {
            connection: &mut tx,
            read_budget: None,
            packed: true,
        }
        .put(good.key.document, &good.key.address, &good.bytes)
        .await
        .unwrap();
        tx.rollback().await.unwrap();
        assert!(b.inventory(None, 10).await.unwrap().is_empty());
        b.import_objects(&[good]).await.unwrap();
        assert!(sqlx::query("delete from objects")
            .execute(&b.pool)
            .await
            .is_err());
        assert!(sqlx::query("update objects set bytes=bytes")
            .execute(&b.pool)
            .await
            .is_err());
        cleanup(a, &a_path).await;
        cleanup(b, &b_path).await;
    }

    #[tokio::test]
    async fn criss_cross_returns_all_best_bases_and_disconnected_history_returns_none() {
        let file = path();
        let project = uuid::Uuid::new_v4();
        let repo = LocalRepository::open(&file, project).await.unwrap();
        let a = repo
            .import_commit(&signed(project, vec![], BTreeMap::new(), "A"))
            .await
            .unwrap();
        let b = repo
            .import_commit(&signed(project, vec![a.clone()], BTreeMap::new(), "B"))
            .await
            .unwrap();
        let c = repo
            .import_commit(&signed(project, vec![a], BTreeMap::new(), "C"))
            .await
            .unwrap();
        let d = repo
            .import_commit(&signed(
                project,
                vec![b.clone(), c.clone()],
                BTreeMap::new(),
                "D",
            ))
            .await
            .unwrap();
        let e = repo
            .import_commit(&signed(
                project,
                vec![c.clone(), b.clone()],
                BTreeMap::new(),
                "E",
            ))
            .await
            .unwrap();
        let mut expected = vec![b, c];
        expected.sort();
        assert_eq!(repo.merge_bases(&d, &e).await.unwrap(), expected);
        let unrelated = repo
            .import_commit(&signed(project, vec![], BTreeMap::new(), "unrelated"))
            .await
            .unwrap();
        assert!(repo.merge_bases(&d, &unrelated).await.unwrap().is_empty());
        assert!(sqlx::query("delete from commits")
            .execute(&repo.pool)
            .await
            .is_err());
        cleanup(repo, &file).await;
    }
}
