//! A bounded, process-local cache of materialised document states for the agent routes
//! (docs/AI-AGENT-PROTOCOL-SPEC.md section 19, "State cache").
//!
//! **Key.** `(document, revision, chain)`. The chain hash is the stored `chain_hash` of
//! the event at the revision, read from the database on every request just before the
//! lookup, so an entry can only be served for the exact history it was built from.
//!
//! **Authorisation.** The cache holds derived content and is consulted only after the
//! request has been authorised against the database (credential, grant, project role,
//! file role); nothing in it is a credential or a decision. A hit therefore never
//! widens access.
//!
//! **Bounds.** At most [`MAX_ENTRIES`] states and [`MAX_BYTES`] in total, measured as
//! the size of each state serialised as JSON (a deterministic proxy; resident memory is
//! a small multiple of it). A state larger than [`MAX_ENTRY_BYTES`] is served but never
//! cached. The least recently used entry is evicted first. Worst case the cache holds
//! `MAX_BYTES` of serialised state; states in flight in running requests are bounded by
//! the admission limits of [`crate::workloads`], not by this cache.
//!
//! **Erasure.** [`StateCache::purge_document`] drops every entry of a document and is
//! called by the audited erasure path in the same process. Another API process cannot
//! be reached from here, but it cannot serve the entries either: each request first
//! reads the document row, which erasure deleted.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use engine_core::materializer::DocumentState;
use uuid::Uuid;

/// Most states kept at once.
pub const MAX_ENTRIES: usize = 64;
/// Most serialised bytes kept in total (32 MiB).
pub const MAX_BYTES: usize = 32 * 1024 * 1024;
/// Largest state that is cached (8 MiB serialised); larger ones are never retained.
pub const MAX_ENTRY_BYTES: usize = 8 * 1024 * 1024;

/// `(document, revision, chain_hash)`.
pub type Key = (Uuid, u64, [u8; 32]);

struct Entry {
    state: Arc<DocumentState>,
    bytes: usize,
    used: u64,
    born: std::time::Instant,
}

/// An entry older than this is dropped on its next use, so erased or rewritten text cannot
/// linger in a process that never saw the erasure.
pub const MAX_AGE: std::time::Duration = std::time::Duration::from_secs(300);

#[derive(Default)]
struct Inner {
    entries: HashMap<Key, Entry>,
    bytes: usize,
    tick: u64,
    hits: u64,
    misses: u64,
}

/// Cheap to clone; clones share one cache.
#[derive(Clone, Default)]
pub struct StateCache(Arc<Mutex<Inner>>);

/// Serialised size of a state, without allocating the serialisation.
pub fn estimate_bytes(state: &DocumentState) -> usize {
    struct Count(usize);
    impl std::io::Write for Count {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0 += buf.len();
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut count = Count(0);
    // A state that cannot be serialised is treated as too large to cache.
    match serde_json::to_writer(&mut count, state) {
        Ok(()) => count.0,
        Err(_) => usize::MAX,
    }
}

impl StateCache {
    /// The state for `key`, marking it most recently used.
    pub fn get(&self, key: &Key) -> Option<Arc<DocumentState>> {
        let mut inner = self.0.lock().ok()?;
        inner.tick += 1;
        let tick = inner.tick;
        let Some(entry) = inner.entries.get_mut(key) else {
            inner.misses += 1;
            return None;
        };
        if entry.born.elapsed() > MAX_AGE {
            if let Some(old) = inner.entries.remove(key) {
                inner.bytes -= old.bytes;
            }
            inner.misses += 1;
            return None;
        }
        entry.used = tick;
        let state = entry.state.clone();
        inner.hits += 1;
        Some(state)
    }

    /// Keep `state` (of `bytes` serialised bytes) unless it is too large; evicts the least
    /// recently used entries to stay inside the bounds. Returns whether it was kept.
    pub fn insert(&self, key: Key, state: Arc<DocumentState>, bytes: usize) -> bool {
        if bytes > MAX_ENTRY_BYTES {
            return false;
        }
        let Ok(mut inner) = self.0.lock() else {
            return false;
        };
        if let Some(old) = inner.entries.remove(&key) {
            inner.bytes -= old.bytes;
        }
        while inner.entries.len() >= MAX_ENTRIES || inner.bytes + bytes > MAX_BYTES {
            let Some(oldest) = inner
                .entries
                .iter()
                .min_by_key(|(_, e)| e.used)
                .map(|(k, _)| *k)
            else {
                break;
            };
            if let Some(gone) = inner.entries.remove(&oldest) {
                inner.bytes -= gone.bytes;
            }
        }
        inner.tick += 1;
        let used = inner.tick;
        inner.bytes += bytes;
        inner.entries.insert(
            key,
            Entry {
                state,
                bytes,
                used,
                born: std::time::Instant::now(),
            },
        );
        true
    }

    /// Drop every entry of `document` (audited erasure). Returns how many were dropped.
    pub fn purge_document(&self, document: Uuid) -> usize {
        let Ok(mut inner) = self.0.lock() else {
            return 0;
        };
        let doomed: Vec<Key> = inner
            .entries
            .keys()
            .filter(|k| k.0 == document)
            .copied()
            .collect();
        for key in &doomed {
            if let Some(gone) = inner.entries.remove(key) {
                inner.bytes -= gone.bytes;
            }
        }
        doomed.len()
    }

    /// The distinct documents that have at least one entry, for purging a whole project.
    pub fn documents(&self) -> Vec<Uuid> {
        let Ok(inner) = self.0.lock() else {
            return Vec::new();
        };
        let mut docs: Vec<Uuid> = inner.entries.keys().map(|k| k.0).collect();
        docs.sort_unstable();
        docs.dedup();
        docs
    }

    /// `(hits, misses)` of [`get`](Self::get) since the process started.
    pub fn lookups(&self) -> (u64, u64) {
        self.0
            .lock()
            .map(|inner| (inner.hits, inner.misses))
            .unwrap_or((0, 0))
    }

    /// `(entries, serialised bytes)` currently held.
    pub fn usage(&self) -> (usize, usize) {
        self.0
            .lock()
            .map(|inner| (inner.entries.len(), inner.bytes))
            .unwrap_or((0, 0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(doc: u128, rev: u64) -> Key {
        (Uuid::from_u128(doc), rev, [rev as u8; 32])
    }
    fn state() -> Arc<DocumentState> {
        Arc::new(DocumentState::default())
    }

    #[test]
    fn hits_misses_and_chain_is_part_of_the_key() {
        let cache = StateCache::default();
        assert!(cache.get(&key(1, 3)).is_none());
        assert!(cache.insert(key(1, 3), state(), 100));
        assert!(cache.get(&key(1, 3)).is_some());
        assert_eq!(cache.lookups(), (1, 1));
        let mut other_chain = key(1, 3);
        other_chain.2[0] ^= 1;
        assert!(cache.get(&other_chain).is_none());
        assert!(cache.get(&key(1, 4)).is_none());
        assert_eq!(cache.usage(), (1, 100));
    }

    #[test]
    fn an_entry_older_than_the_maximum_age_is_dropped_on_its_next_use() {
        let cache = StateCache::default();
        assert!(cache.insert(key(1, 3), state(), 100));
        // Age the entry past the limit without waiting for it.
        {
            let mut inner = cache.0.lock().unwrap();
            let entry = inner.entries.get_mut(&key(1, 3)).unwrap();
            entry.born = std::time::Instant::now()
                .checked_sub(MAX_AGE + std::time::Duration::from_secs(1))
                .expect("the monotonic clock has run for longer than the maximum age");
        }
        assert!(cache.get(&key(1, 3)).is_none());
        assert_eq!(cache.usage(), (0, 0));
    }

    #[test]
    fn entry_count_is_bounded_and_the_least_recently_used_goes_first() {
        let cache = StateCache::default();
        for rev in 0..MAX_ENTRIES as u64 {
            assert!(cache.insert(key(1, rev), state(), 10));
        }
        // Touch revision 0 so revision 1 is now the oldest.
        assert!(cache.get(&key(1, 0)).is_some());
        assert!(cache.insert(key(2, 0), state(), 10));
        assert_eq!(cache.usage().0, MAX_ENTRIES);
        assert!(cache.get(&key(1, 0)).is_some());
        assert!(cache.get(&key(1, 1)).is_none());
        assert!(cache.get(&key(2, 0)).is_some());
    }

    #[test]
    fn byte_budget_is_bounded_and_oversized_states_are_not_kept() {
        let cache = StateCache::default();
        assert!(!cache.insert(key(1, 1), state(), MAX_ENTRY_BYTES + 1));
        assert_eq!(cache.usage(), (0, 0));
        let each = MAX_ENTRY_BYTES;
        for rev in 0..10u64 {
            assert!(cache.insert(key(1, rev), state(), each));
            assert!(cache.usage().1 <= MAX_BYTES);
        }
        assert_eq!(cache.usage().0, MAX_BYTES / each);
        // Replacing an entry does not double count it.
        let before = cache.usage();
        assert!(cache.insert(key(1, 9), state(), each));
        assert_eq!(cache.usage(), before);
    }

    #[test]
    fn purge_removes_every_revision_of_one_document_only() {
        let cache = StateCache::default();
        for rev in 0..5 {
            cache.insert(key(1, rev), state(), 50);
            cache.insert(key(2, rev), state(), 70);
        }
        assert_eq!(cache.purge_document(Uuid::from_u128(1)), 5);
        assert!(cache.get(&key(1, 0)).is_none());
        assert!(cache.get(&key(2, 0)).is_some());
        assert_eq!(cache.usage(), (5, 350));
        assert_eq!(cache.purge_document(Uuid::from_u128(1)), 0);
    }

    #[test]
    fn size_estimate_counts_serialised_bytes() {
        let empty = estimate_bytes(&DocumentState::default());
        assert!(empty > 0 && empty < 1024);
    }
}
