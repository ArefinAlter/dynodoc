//! An in-memory cache of unwrapped data keys with an absolute lifetime and a size bound,
//! so a key provider is called once per data key per lifetime, not once per row
//! (docs/END-TO-END-ENCRYPTION.md, section 7.6).
//!
//! - A key is dropped `ttl` after it was inserted, measured from insertion and not
//!   extended by use. Every call first removes every expired entry, so no call ever
//!   returns or counts a key past its lifetime; a service that is idle should still call
//!   [`DekCache::purge_expired`] from a timer so an expired key does not stay in memory.
//! - At most `max_entries` keys are held; inserting beyond that drops the oldest.
//! - Keys are shared as `Arc<DataKey>`. The cache's reference is released at expiry; the
//!   key bytes are overwritten when the last reference anywhere is dropped, so a caller
//!   that keeps its `Arc` keeps the key alive. Zeroisation is best effort (see
//!   [`DataKey`]).
//! - Nothing is written to disk. The cache is for one installation: entries are keyed by
//!   `dek_id` alone.
//! - The clock is injected ([`Clock`]); [`ManualClock`] makes lifetimes testable.
use super::{DataKey, Dek, KeyProvider, ProviderError, WrapContext, Wrapped};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex, MutexGuard},
    time::{Duration, Instant},
};

/// The default lifetime of a cached key (docs/END-TO-END-ENCRYPTION.md, section 7.6).
pub const DEFAULT_TTL: Duration = Duration::from_secs(15 * 60);
pub const DEFAULT_MAX_ENTRIES: usize = 1024;

/// A monotonic time source: the time since some fixed origin that only moves forward.
pub trait Clock: Send + Sync {
    fn now(&self) -> Duration;
}

/// The real clock: time since this value was created, from `std::time::Instant`.
#[derive(Debug)]
pub struct SystemClock(Instant);

impl SystemClock {
    pub fn new() -> Self {
        Self(Instant::now())
    }
}

impl Default for SystemClock {
    fn default() -> Self {
        Self::new()
    }
}

impl Clock for SystemClock {
    fn now(&self) -> Duration {
        self.0.elapsed()
    }
}

/// A clock that moves only when told to, for tests.
#[derive(Debug, Default)]
pub struct ManualClock(Mutex<Duration>);

impl ManualClock {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn advance(&self, by: Duration) {
        let mut now = self.0.lock().unwrap_or_else(|e| e.into_inner());
        *now = now.saturating_add(by);
    }
}

impl Clock for ManualClock {
    fn now(&self) -> Duration {
        *self.0.lock().unwrap_or_else(|e| e.into_inner())
    }
}

impl<C: Clock + ?Sized> Clock for Arc<C> {
    fn now(&self) -> Duration {
        (**self).now()
    }
}

struct Entry {
    key: Arc<DataKey>,
    inserted: Duration,
}

pub struct DekCache {
    ttl: Duration,
    max_entries: usize,
    clock: Box<dyn Clock>,
    entries: Mutex<HashMap<u32, Entry>>,
}

impl std::fmt::Debug for DekCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DekCache")
            .field("ttl", &self.ttl)
            .field("max_entries", &self.max_entries)
            .finish_non_exhaustive()
    }
}

impl DekCache {
    /// A cache on the real clock. A zero `ttl` or `max_entries` retains nothing.
    pub fn new(ttl: Duration, max_entries: usize) -> Self {
        Self::with_clock(ttl, max_entries, SystemClock::new())
    }

    pub fn with_clock(ttl: Duration, max_entries: usize, clock: impl Clock + 'static) -> Self {
        Self {
            ttl,
            max_entries,
            clock: Box::new(clock),
            entries: Mutex::new(HashMap::new()),
        }
    }

    fn locked(&self) -> MutexGuard<'_, HashMap<u32, Entry>> {
        let mut entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
        let now = self.clock.now();
        let ttl = self.ttl;
        entries.retain(|_, entry| now.saturating_sub(entry.inserted) < ttl);
        entries
    }

    /// The cached key, if it is still within its lifetime.
    pub fn get(&self, dek_id: u32) -> Option<Arc<DataKey>> {
        self.locked().get(&dek_id).map(|entry| entry.key.clone())
    }

    /// Cache `key` for `ttl` from now, replacing any entry with the same id. Returns the
    /// shared handle. With the cache full, the oldest entry is dropped first.
    pub fn insert(&self, key: DataKey) -> Arc<DataKey> {
        let key = Arc::new(key);
        if self.max_entries == 0 || self.ttl.is_zero() {
            return key;
        }
        let mut entries = self.locked();
        entries.remove(&key.id());
        while entries.len() >= self.max_entries {
            let oldest = entries
                .iter()
                .min_by_key(|(id, entry)| (entry.inserted, **id))
                .map(|(id, _)| *id);
            match oldest {
                Some(id) => entries.remove(&id),
                None => break,
            };
        }
        entries.insert(
            key.id(),
            Entry {
                key: key.clone(),
                inserted: self.clock.now(),
            },
        );
        key
    }

    /// The cached key, or the result of `load` cached for the next caller. A failed load
    /// caches nothing. Concurrent misses may each load; the last insert wins.
    pub fn get_or_load<E>(
        &self,
        dek_id: u32,
        load: impl FnOnce() -> Result<DataKey, E>,
    ) -> Result<Arc<DataKey>, E> {
        if let Some(key) = self.get(dek_id) {
            return Ok(key);
        }
        Ok(self.insert(load()?))
    }

    /// How a store gets the key for an envelope's `dek_id`: from the cache, else by
    /// unwrapping `wrapped` (the `data_key` row) with the provider.
    pub fn get_or_unwrap(
        &self,
        dek_id: u32,
        wrapped: &Wrapped,
        ctx: &WrapContext,
        provider: &dyn KeyProvider,
    ) -> Result<Arc<DataKey>, ProviderError> {
        self.get_or_load(dek_id, || {
            let dek: Dek = provider.unwrap(wrapped, ctx)?;
            Ok(DataKey::new(dek_id, wrapped.kek_version(), dek))
        })
    }

    /// Drop every expired entry now; returns how many entries remain.
    pub fn purge_expired(&self) -> usize {
        self.locked().len()
    }

    /// Drop everything (for example on a kill switch).
    pub fn clear(&self) {
        self.entries
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clear();
    }

    /// Entries within their lifetime.
    pub fn len(&self) -> usize {
        self.purge_expired()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::at_rest::{Purpose, TestKeyProvider};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use uuid::Uuid;

    fn key(id: u32) -> DataKey {
        DataKey::new(id, 1, Dek::from_bytes([id as u8; 32]))
    }

    fn cache(ttl_secs: u64, max: usize) -> (DekCache, Arc<ManualClock>) {
        let clock = Arc::new(ManualClock::new());
        (
            DekCache::with_clock(Duration::from_secs(ttl_secs), max, clock.clone()),
            clock,
        )
    }

    #[test]
    fn a_key_is_served_until_its_lifetime_ends_and_never_after() {
        let (cache, clock) = cache(900, 8);
        cache.insert(key(1));
        assert_eq!(cache.get(1).unwrap().id(), 1);
        clock.advance(Duration::from_secs(899));
        assert!(cache.get(1).is_some());
        clock.advance(Duration::from_secs(1));
        assert!(cache.get(1).is_none(), "expired at exactly the ttl");
        assert_eq!(cache.len(), 0);
        assert!(cache.is_empty());
    }

    #[test]
    fn use_does_not_extend_the_lifetime() {
        let (cache, clock) = cache(100, 8);
        cache.insert(key(1));
        for _ in 0..9 {
            clock.advance(Duration::from_secs(10));
            assert!(cache.get(1).is_some());
        }
        clock.advance(Duration::from_secs(10));
        assert!(cache.get(1).is_none());
    }

    #[test]
    fn expiry_releases_the_cache_reference_and_wipes_an_unshared_key() {
        let (cache, clock) = cache(10, 8);
        let held = cache.insert(key(1));
        assert_eq!(Arc::strong_count(&held), 2);
        clock.advance(Duration::from_secs(10));
        assert_eq!(cache.purge_expired(), 0);
        assert_eq!(Arc::strong_count(&held), 1, "the cache kept no reference");
    }

    #[test]
    fn expired_entries_are_purged_by_any_call_not_only_by_get() {
        let (cache, clock) = cache(10, 8);
        let a = cache.insert(key(1));
        let b = cache.insert(key(2));
        clock.advance(Duration::from_secs(10));
        // An unrelated insert drops both expired keys.
        cache.insert(key(3));
        assert_eq!((Arc::strong_count(&a), Arc::strong_count(&b)), (1, 1));
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn the_size_bound_drops_the_oldest_entry() {
        let (cache, clock) = cache(1000, 3);
        for id in 1..=3 {
            cache.insert(key(id));
            clock.advance(Duration::from_secs(1));
        }
        assert_eq!(cache.len(), 3);
        cache.insert(key(4));
        assert_eq!(cache.len(), 3);
        assert!(cache.get(1).is_none(), "the oldest went");
        assert!(cache.get(2).is_some() && cache.get(3).is_some() && cache.get(4).is_some());
        // Replacing an id does not grow the cache or evict another.
        cache.insert(key(3));
        assert_eq!(cache.len(), 3);
        assert!(cache.get(2).is_some());
    }

    #[test]
    fn a_replaced_entry_gets_a_fresh_lifetime() {
        let (cache, clock) = cache(100, 8);
        cache.insert(key(1));
        clock.advance(Duration::from_secs(90));
        cache.insert(key(1));
        clock.advance(Duration::from_secs(90));
        assert!(cache.get(1).is_some());
        clock.advance(Duration::from_secs(10));
        assert!(cache.get(1).is_none());
    }

    #[test]
    fn zero_ttl_or_zero_capacity_retains_nothing() {
        let (cache, _) = cache(0, 8);
        let held = cache.insert(key(1));
        assert!(cache.get(1).is_none());
        assert_eq!(Arc::strong_count(&held), 1);
        let (cache, _) = self::cache(10, 0);
        cache.insert(key(1));
        assert!(cache.get(1).is_none());
    }

    #[test]
    fn clear_drops_everything() {
        let (cache, _) = cache(100, 8);
        cache.insert(key(1));
        cache.insert(key(2));
        cache.clear();
        assert!(cache.is_empty());
    }

    #[test]
    fn get_or_load_loads_once_and_does_not_cache_failures() {
        let (cache, clock) = cache(10, 8);
        let loads = AtomicUsize::new(0);
        let load = |id| {
            loads.fetch_add(1, Ordering::SeqCst);
            Ok::<_, ()>(key(id))
        };
        cache.get_or_load(1, || load(1)).unwrap();
        cache.get_or_load(1, || load(1)).unwrap();
        assert_eq!(loads.load(Ordering::SeqCst), 1);
        clock.advance(Duration::from_secs(10));
        cache.get_or_load(1, || load(1)).unwrap();
        assert_eq!(loads.load(Ordering::SeqCst), 2);
        assert_eq!(cache.get_or_load(2, || Err("down")).unwrap_err(), "down");
        assert!(cache.get(2).is_none());
    }

    #[test]
    fn get_or_unwrap_calls_the_provider_once_per_lifetime() {
        struct Counting(TestKeyProvider, AtomicUsize);
        impl KeyProvider for Counting {
            fn wrap(&self, dek: &Dek, ctx: &WrapContext) -> Result<Wrapped, ProviderError> {
                self.0.wrap(dek, ctx)
            }
            fn unwrap(&self, wrapped: &Wrapped, ctx: &WrapContext) -> Result<Dek, ProviderError> {
                self.1.fetch_add(1, Ordering::SeqCst);
                self.0.unwrap(wrapped, ctx)
            }
            fn describe(&self) -> super::super::ProviderInfo {
                self.0.describe()
            }
        }
        let provider = Counting(TestKeyProvider::new([(5, [9; 32])]), AtomicUsize::new(0));
        let ctx = WrapContext::new(Uuid::from_u128(1), None, Purpose::Data);
        let dek = Dek::from_bytes([0x33; 32]);
        let wrapped = provider.wrap(&dek, &ctx).unwrap();

        let (cache, clock) = cache(60, 8);
        let first = cache.get_or_unwrap(12, &wrapped, &ctx, &provider).unwrap();
        let second = cache.get_or_unwrap(12, &wrapped, &ctx, &provider).unwrap();
        assert_eq!(provider.1.load(Ordering::SeqCst), 1);
        assert!(Arc::ptr_eq(&first, &second));
        assert_eq!((first.id(), first.kek_version()), (12, 5));
        assert_eq!(first.dek().as_bytes(), dek.as_bytes());
        clock.advance(Duration::from_secs(60));
        cache.get_or_unwrap(12, &wrapped, &ctx, &provider).unwrap();
        assert_eq!(provider.1.load(Ordering::SeqCst), 2);

        // A provider that refuses caches nothing.
        let other = WrapContext::new(Uuid::from_u128(2), None, Purpose::Data);
        assert!(cache
            .get_or_unwrap(13, &wrapped, &other, &provider)
            .is_err());
        assert!(cache.get(13).is_none());
    }

    #[test]
    fn the_system_clock_moves_forward() {
        let clock = SystemClock::default();
        let a = clock.now();
        std::thread::sleep(Duration::from_millis(2));
        assert!(clock.now() > a);
        let cache = DekCache::new(DEFAULT_TTL, DEFAULT_MAX_ENTRIES);
        cache.insert(key(1));
        assert!(cache.get(1).is_some());
        assert!(format!("{cache:?}").contains("DekCache"));
    }
}
