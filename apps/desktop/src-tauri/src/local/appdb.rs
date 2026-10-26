//! What the client keeps beside the engine's data, in its own database (`dynodoc_app`) of the same
//! PostgreSQL: the changes captured and not yet committed, commits not yet pushed, agent threads,
//! the client's own choices, commit messages (the engine records changes, not messages), the
//! folder each project is bound to, the links between local and remote projects, and the agent's
//! usage ledger. It is the local counterpart of the hosted pre-alpha service (`deploy/pre-alpha`),
//! with the same routes and shapes, so the client's code is the same in both.
//!
//! One person owns the workspace, so nothing here is keyed by account. A backup of the cluster
//! carries all of it.

use serde_json::{json, Value};
use tokio_postgres::Client;

pub const STATE_KEYS: [&str; 4] = ["working", "local", "threads", "client"];
pub const MAX_STATE_BYTES: usize = 256 * 1024;
pub const MAX_NOTES_PER_DOCUMENT: i64 = 5000;
const NOTE_KINDS: [&str; 7] = ["revision", "merge", "revert", "cherry_pick", "restore", "rebase", "request"];

/// The schema, one step per number. A step runs once; later releases add steps, never edit one.
const STEPS: &[&str] = &[
    "create table kv (key text primary key, value jsonb not null, updated_at bigint not null);
     create table client_state (key text primary key, value jsonb not null, updated_at bigint not null);
     create table note (
        id bigserial primary key,
        document_id text not null,
        branch text not null,
        from_seq integer not null,
        to_seq integer not null,
        kind text not null,
        summary text not null,
        description text not null default '',
        merged_from text,
        author_id text not null,
        edits jsonb,
        created_at bigint not null);
     create index note_document on note (document_id);",
    // 2: folder bindings and the files seen in them (folder watch)
    "create table folder_binding (
        project_id text primary key,
        folder text not null,
        created_at bigint not null);
     create table folder_file (
        project_id text not null,
        rel_path text not null,
        document_id text not null,
        base_seq integer not null,
        seen_hash text not null default '',
        seen_mtime bigint not null default 0,
        primary key (project_id, rel_path));",
    // 3: links to remote projects and the files in them (sync)
    "create table remote_link (
        project_id text primary key,
        server_id text not null,
        remote_project_id text not null,
        linked_at bigint not null,
        last_synced_at bigint);
     create table remote_file (
        project_id text not null,
        document_id text not null,
        remote_document_id text not null,
        local_seq integer not null default 0,
        remote_seq integer not null default 0,
        primary key (project_id, document_id));",
    // 4: the agent's usage ledger
    "create table usage_ledger (
        id bigserial primary key,
        at bigint not null,
        thread text not null,
        provider text not null,
        model text not null,
        tokens_in bigint not null default 0,
        tokens_out bigint not null default 0,
        cost_micros bigint not null default 0);
     create index usage_at on usage_ledger (at);",
];

pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[derive(Debug)]
pub struct AppDbError(pub String);

impl std::fmt::Display for AppDbError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

fn err<T>(m: &str) -> Result<T, AppDbError> {
    Err(AppDbError(m.to_string()))
}

/// Brings the schema up to date (each step in its own transaction).
pub async fn ensure_schema(client: &mut Client) -> Result<(), AppDbError> {
    client
        .batch_execute("create table if not exists schema_step (n integer primary key, at bigint not null)")
        .await
        .or_else(|_| err("The workspace's own data could not be prepared."))?;
    let done: i32 = client
        .query_one("select coalesce(max(n), 0) from schema_step", &[])
        .await
        .map(|r| r.get(0))
        .or_else(|_| err("The workspace's own data could not be read."))?;
    for (i, sql) in STEPS.iter().enumerate() {
        let n = i as i32 + 1;
        if n <= done {
            continue;
        }
        let tx = client
            .transaction()
            .await
            .or_else(|_| err("The workspace's own data could not be updated."))?;
        tx.batch_execute(sql)
            .await
            .or_else(|_| err("The workspace's own data could not be updated."))?;
        tx.execute("insert into schema_step (n, at) values ($1, $2)", &[&n, &now_ms()])
            .await
            .or_else(|_| err("The workspace's own data could not be updated."))?;
        tx.commit()
            .await
            .or_else(|_| err("The workspace's own data could not be updated."))?;
    }
    Ok(())
}

pub async fn get_state(client: &Client, key: &str) -> Result<Value, AppDbError> {
    if !STATE_KEYS.contains(&key) {
        return err("Not found");
    }
    let row = client
        .query_opt("select value, updated_at from client_state where key = $1", &[&key])
        .await
        .or_else(|_| err("The client's state could not be read."))?;
    Ok(match row {
        Some(r) => json!({ "value": r.get::<_, Value>(0), "updated_at": r.get::<_, i64>(1) }),
        None => json!({ "value": null, "updated_at": null }),
    })
}

pub async fn put_state(client: &Client, key: &str, value: &Value) -> Result<Value, AppDbError> {
    if !STATE_KEYS.contains(&key) {
        return err("Not found");
    }
    if serde_json::to_vec(value).map(|v| v.len()).unwrap_or(usize::MAX) > MAX_STATE_BYTES {
        return err("Too large.");
    }
    let at = now_ms();
    client
        .execute(
            "insert into client_state (key, value, updated_at) values ($1, $2, $3)
             on conflict (key) do update set value = excluded.value, updated_at = excluded.updated_at",
            &[&key, value, &at],
        )
        .await
        .or_else(|_| err("The client's state could not be saved."))?;
    Ok(json!({ "updated_at": at }))
}

pub async fn kv_get(client: &Client, key: &str) -> Result<Option<Value>, AppDbError> {
    client
        .query_opt("select value from kv where key = $1", &[&key])
        .await
        .map(|r| r.map(|r| r.get::<_, Value>(0)))
        .or_else(|_| err("A setting could not be read."))
}

pub async fn kv_put(client: &Client, key: &str, value: &Value) -> Result<(), AppDbError> {
    client
        .execute(
            "insert into kv (key, value, updated_at) values ($1, $2, $3)
             on conflict (key) do update set value = excluded.value, updated_at = excluded.updated_at",
            &[&key, value, &now_ms()],
        )
        .await
        .map(|_| ())
        .or_else(|_| err("A setting could not be saved."))
}

pub fn is_uuid(text: &str) -> bool {
    text.len() == 36
        && text.char_indices().all(|(i, c)| {
            if [8, 13, 18, 23].contains(&i) {
                c == '-'
            } else {
                c.is_ascii_hexdigit() && !c.is_ascii_uppercase()
            }
        })
}

/// The notes (commit messages) of up to 50 files, oldest first.
pub async fn notes_for(client: &Client, documents: &[String]) -> Result<Value, AppDbError> {
    let ids: Vec<&String> = documents.iter().filter(|d| is_uuid(d)).take(50).collect();
    if ids.is_empty() {
        return Ok(json!({ "items": [] }));
    }
    let list: Vec<String> = ids.iter().map(|s| s.to_string()).collect();
    let rows = client
        .query(
            "select id, document_id, branch, from_seq, to_seq, kind, summary, description, merged_from, author_id, edits, created_at
             from note where document_id = any($1) order by id",
            &[&list],
        )
        .await
        .or_else(|_| err("The commit messages could not be read."))?;
    let items: Vec<Value> = rows
        .iter()
        .map(|r| {
            json!({
                "id": r.get::<_, i64>(0),
                "document_id": r.get::<_, String>(1),
                "branch": r.get::<_, String>(2),
                "from_seq": r.get::<_, i32>(3),
                "to_seq": r.get::<_, i32>(4),
                "kind": r.get::<_, String>(5),
                "summary": r.get::<_, String>(6),
                "description": r.get::<_, String>(7),
                "merged_from": r.get::<_, Option<String>>(8),
                "author_id": r.get::<_, String>(9),
                "edits": r.get::<_, Option<Value>>(10),
                "created_at": r.get::<_, i64>(11),
            })
        })
        .collect();
    Ok(json!({ "items": items }))
}

/// Checks a note the way the hosted service does, before it is stored.
pub fn note_is_valid(n: &Value) -> bool {
    let text = |k: &str| n.get(k).and_then(Value::as_str);
    let int = |k: &str| n.get(k).and_then(Value::as_i64);
    text("document_id").is_some_and(is_uuid)
        && text("branch").is_some_and(|b| b.len() <= 64)
        && int("from_seq").is_some()
        && int("to_seq").is_some()
        && text("kind").is_some_and(|k| NOTE_KINDS.contains(&k))
        && text("summary").is_some_and(|s| !s.trim().is_empty() && s.chars().count() <= 200)
        && n.get("description").map_or(true, |d| d.as_str().is_some_and(|s| s.chars().count() <= 4000))
        && n.get("edits").map_or(true, |e| e.is_null() || (e.is_array() && e.to_string().len() <= 64 * 1024))
}

pub async fn add_note(client: &Client, n: &Value, author: &str) -> Result<Value, AppDbError> {
    if !note_is_valid(n) {
        return err("Invalid note");
    }
    let doc = n["document_id"].as_str().unwrap_or_default().to_string();
    let count: i64 = client
        .query_one("select count(*) from note where document_id = $1", &[&doc])
        .await
        .map(|r| r.get(0))
        .or_else(|_| err("The commit message could not be saved."))?;
    if count >= MAX_NOTES_PER_DOCUMENT {
        return err("This file has too many notes.");
    }
    let from = n["from_seq"].as_i64().unwrap_or(0) as i32;
    let to = n["to_seq"].as_i64().unwrap_or(0) as i32;
    let merged = n.get("merged_from").and_then(Value::as_str).map(|s| s.chars().take(64).collect::<String>());
    let edits = n.get("edits").filter(|e| !e.is_null()).cloned();
    let row = client
        .query_one(
            "insert into note (document_id, branch, from_seq, to_seq, kind, summary, description, merged_from, author_id, edits, created_at)
             values ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11) returning id",
            &[
                &doc,
                &n["branch"].as_str().unwrap_or("main"),
                &from,
                &to,
                &n["kind"].as_str().unwrap_or("revision"),
                &n["summary"].as_str().unwrap_or(""),
                &n.get("description").and_then(Value::as_str).unwrap_or(""),
                &merged,
                &author,
                &edits,
                &now_ms(),
            ],
        )
        .await
        .or_else(|_| err("The commit message could not be saved."))?;
    Ok(json!({ "id": row.get::<_, i64>(0) }))
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOC: &str = "0190f5b2-1c2d-7000-8000-00000000abcd";

    #[test]
    fn a_note_needs_a_file_a_kind_and_a_short_summary() {
        let ok = json!({ "document_id": DOC, "branch": "main", "from_seq": 3, "to_seq": 4, "kind": "revision", "summary": "Fix totals" });
        assert!(note_is_valid(&ok));
        let mut bad = ok.clone();
        bad["kind"] = json!("shell");
        assert!(!note_is_valid(&bad));
        bad = ok.clone();
        bad["summary"] = json!("  ");
        assert!(!note_is_valid(&bad));
        bad = ok.clone();
        bad["summary"] = json!("x".repeat(201));
        assert!(!note_is_valid(&bad));
        bad = ok.clone();
        bad["document_id"] = json!("../../etc");
        assert!(!note_is_valid(&bad));
        bad = ok.clone();
        bad["description"] = json!("d".repeat(4001));
        assert!(!note_is_valid(&bad));
    }

    #[test]
    fn the_schema_steps_are_numbered_and_only_grow() {
        assert!(STEPS.len() >= 4);
        for step in STEPS {
            assert!(!step.contains("drop table"), "a step never removes data");
        }
    }

    #[test]
    fn the_state_keys_are_the_hosted_services_four() {
        assert_eq!(STATE_KEYS, ["working", "local", "threads", "client"]);
    }
}
