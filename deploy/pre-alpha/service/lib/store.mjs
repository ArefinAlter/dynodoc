// The pre-alpha service's own database (SQLite in a volume). It holds only what the Dynodoc
// server has no place for: the visitor sandboxes, the client's per-account state (captured
// changes, unpushed commits, agent threads) and commit messages for revisions. No secret of a
// person is kept: the sandbox's sign-in token is a one-use magic-link token, deleted when claimed.
import { DatabaseSync } from 'node:sqlite';

export const STATE_KEYS = ['working', 'local', 'threads', 'client'];
export const MAX_STATE_BYTES = 256 * 1024;
export const MAX_NOTES_PER_DOCUMENT = 5000;

export function open(path) {
  const db = new DatabaseSync(path);
  db.exec(`
    PRAGMA journal_mode = WAL;
    PRAGMA busy_timeout = 5000;
    CREATE TABLE IF NOT EXISTS sandbox (
      id TEXT PRIMARY KEY,
      email TEXT NOT NULL UNIQUE,
      identity_id TEXT,
      display_name TEXT NOT NULL,
      status TEXT NOT NULL,
      created_at INTEGER NOT NULL,
      ready_at INTEGER,
      token TEXT,
      token_at INTEGER,
      claimed_at INTEGER,
      expires_at INTEGER,
      expired_at INTEGER,
      resources TEXT NOT NULL DEFAULT '{}',
      error TEXT
    );
    CREATE INDEX IF NOT EXISTS sandbox_status ON sandbox(status);
    CREATE TABLE IF NOT EXISTS client_state (
      identity_id TEXT NOT NULL,
      key TEXT NOT NULL,
      value TEXT NOT NULL,
      updated_at INTEGER NOT NULL,
      PRIMARY KEY (identity_id, key)
    );
    CREATE TABLE IF NOT EXISTS note (
      id INTEGER PRIMARY KEY AUTOINCREMENT,
      document_id TEXT NOT NULL,
      branch TEXT NOT NULL,
      from_seq INTEGER NOT NULL,
      to_seq INTEGER NOT NULL,
      kind TEXT NOT NULL,
      summary TEXT NOT NULL,
      description TEXT NOT NULL DEFAULT '',
      merged_from TEXT,
      author_id TEXT NOT NULL,
      edits TEXT,
      created_at INTEGER NOT NULL
    );
    CREATE INDEX IF NOT EXISTS note_document ON note(document_id);
    CREATE TABLE IF NOT EXISTS rate (
      bucket TEXT PRIMARY KEY,
      window_start INTEGER NOT NULL,
      count INTEGER NOT NULL
    );
  `);
  return db;
}

/** A fixed-window counter: true while `bucket` has had fewer than `limit` hits in `windowMs`. */
export function allow(db, bucket, limit, windowMs, now = Date.now()) {
  const row = db.prepare('SELECT window_start, count FROM rate WHERE bucket = ?').get(bucket);
  if (!row || now - row.window_start >= windowMs) {
    db.prepare('INSERT INTO rate (bucket, window_start, count) VALUES (?, ?, 1) ON CONFLICT(bucket) DO UPDATE SET window_start = excluded.window_start, count = 1').run(bucket, now);
    return true;
  }
  if (row.count >= limit) return false;
  db.prepare('UPDATE rate SET count = count + 1 WHERE bucket = ?').run(bucket);
  return true;
}

export function getState(db, identity, key) {
  const row = db.prepare('SELECT value, updated_at FROM client_state WHERE identity_id = ? AND key = ?').get(identity, key);
  return row ? { value: JSON.parse(row.value), updated_at: row.updated_at } : { value: null, updated_at: null };
}

export function putState(db, identity, key, value, now = Date.now()) {
  const text = JSON.stringify(value);
  if (Buffer.byteLength(text) > MAX_STATE_BYTES) throw Object.assign(new Error('Too large.'), { status: 413 });
  db.prepare('INSERT INTO client_state (identity_id, key, value, updated_at) VALUES (?, ?, ?, ?) ON CONFLICT(identity_id, key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at').run(identity, key, text, now);
  return now;
}

export function dropState(db, identity) {
  db.prepare('DELETE FROM client_state WHERE identity_id = ?').run(identity);
}

const NOTE_KINDS = new Set(['revision', 'merge', 'revert', 'cherry_pick', 'restore', 'rebase', 'request']);

export function addNote(db, n, now = Date.now()) {
  if (!NOTE_KINDS.has(n.kind)) throw Object.assign(new Error('Unknown kind.'), { status: 400 });
  const count = db.prepare('SELECT COUNT(*) AS n FROM note WHERE document_id = ?').get(n.document_id).n;
  if (count >= MAX_NOTES_PER_DOCUMENT) throw Object.assign(new Error('This file has too many notes.'), { status: 429 });
  const r = db
    .prepare('INSERT INTO note (document_id, branch, from_seq, to_seq, kind, summary, description, merged_from, author_id, edits, created_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)')
    .run(n.document_id, n.branch, n.from_seq, n.to_seq, n.kind, n.summary, n.description ?? '', n.merged_from ?? null, n.author_id, n.edits ? JSON.stringify(n.edits) : null, now);
  return Number(r.lastInsertRowid);
}

export function notesFor(db, documentIds) {
  if (!documentIds.length) return [];
  const marks = documentIds.map(() => '?').join(',');
  return db
    .prepare(`SELECT id, document_id, branch, from_seq, to_seq, kind, summary, description, merged_from, author_id, edits, created_at FROM note WHERE document_id IN (${marks}) ORDER BY id`)
    .all(...documentIds)
    .map((r) => ({ ...r, edits: r.edits ? JSON.parse(r.edits) : null }));
}
