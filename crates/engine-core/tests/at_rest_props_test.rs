//! Property, tamper, binding, parser-robustness and rotation tests for the at-rest
//! envelope. Every random choice comes from a fixed-seed generator, so a failure
//! reproduces.
use engine_core::at_rest::{
    has_magic, open, seal, seal_content_addressed, seal_with_salt, Context, DataKey, Dek, DekCache,
    Error, Header, KeyProvider, ManualClock, ProviderError, ProviderInfo, Purpose, Salt,
    TestKeyProvider, WrapContext, Wrapped, HEADER_LEN, MAX_PLAINTEXT_BYTES, MIN_ENVELOPE_LEN,
};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::time::Duration;
use uuid::Uuid;

/// SplitMix64: tiny, fixed-seed, good enough to pick sizes and bytes.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform in `0..n` (`n > 0`).
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }

    fn bytes(&mut self, len: usize) -> Vec<u8> {
        let mut out = Vec::with_capacity(len + 8);
        while out.len() < len {
            out.extend_from_slice(&self.next().to_le_bytes());
        }
        out.truncate(len);
        out
    }
}

fn installation() -> Uuid {
    Uuid::from_u128(0x0f1e_2d3c_4b5a_6978_8796_a5b4_c3d2_e1f0)
}

fn data_key(id: u32) -> DataKey {
    DataKey::new(id, 1, Dek::from_bytes([0x24; 32]))
}

fn ctx<'a>(table: &'a str, column: &'a str, row: &'a [u8]) -> Context<'a> {
    Context::new(installation(), table, column, row).unwrap()
}

#[test]
fn round_trips_at_boundary_and_random_sizes() {
    let mut rng = Rng(0xA7_0001);
    let key = data_key(1);
    let mut sizes: Vec<usize> = vec![
        0,
        1,
        2,
        15,
        16,
        17,
        31,
        32,
        33,
        63,
        64,
        65,
        255,
        256,
        257,
        4095,
        4096,
        4097,
        65_535,
        65_536,
        65_537,
        (1 << 20) + 1,
    ];
    sizes.extend((0..150).map(|_| rng.below(20_000) as usize));
    sizes.extend((0..4).map(|_| rng.below(2 << 20) as usize));
    for (i, size) in sizes.into_iter().enumerate() {
        let plaintext = rng.bytes(size);
        let row = format!("row-{i}");
        let ctx = ctx("checkpoint_object", "bytes", row.as_bytes());
        let stored = seal(&key, &ctx, &plaintext).unwrap();
        assert_eq!(stored.len(), size + MIN_ENVELOPE_LEN, "size {size}");
        assert_eq!(open(&key, &ctx, &stored).unwrap(), plaintext, "size {size}");
        // The ciphertext is not the plaintext (for any non-trivial size).
        if size >= 16 {
            assert_ne!(&stored[HEADER_LEN..HEADER_LEN + size], &plaintext[..]);
        }
    }
}

#[test]
fn eight_mebibytes_round_trip() {
    let mut rng = Rng(0xA7_0002);
    let key = data_key(1);
    let plaintext = rng.bytes(8 << 20);
    let ctx = ctx("document_upload", "content", b"big");
    let stored = seal(&key, &ctx, &plaintext).unwrap();
    assert_eq!(open(&key, &ctx, &stored).unwrap(), plaintext);
}

#[test]
fn plaintext_over_the_bound_is_refused_before_any_work() {
    // The allocation is zeroed lazily and is never touched: the length is checked first.
    let too_big = vec![0u8; MAX_PLAINTEXT_BYTES + 1];
    let error = seal(&data_key(1), &ctx("t", "c", b"r"), &too_big).unwrap_err();
    assert_eq!(
        error,
        Error::PlaintextTooLarge {
            len: MAX_PLAINTEXT_BYTES + 1,
            max: MAX_PLAINTEXT_BYTES
        }
    );
}

/// Flip every bit pattern of every byte of a small envelope: nothing decrypts, and the
/// error says which part was hit.
#[test]
fn flipping_any_byte_fails_and_names_the_part() {
    let key = data_key(7);
    let row = ctx("snapshot", "state", b"s-1");
    let plaintext = b"twenty bytes of data";
    let stored = seal(&key, &row, plaintext).unwrap();
    assert_eq!(stored.len(), 20 + MIN_ENVELOPE_LEN);
    assert_eq!(open(&key, &row, &stored).unwrap(), plaintext);

    for i in 0..stored.len() {
        for mask in [0x01u8, 0x80, 0xFF] {
            let mut bad = stored.clone();
            bad[i] ^= mask;
            let error = open(&key, &row, &bad).unwrap_err();
            let expected = match i {
                0..=7 => Error::BadMagic,
                8 => Error::UnsupportedVersion(1 ^ mask),
                9..=12 => Error::DekMismatch {
                    expected: 7,
                    found: u32::from_be_bytes(bad[9..13].try_into().unwrap()),
                },
                // kek_version, salt, ciphertext and tag are all authenticated.
                _ => Error::Authentication,
            };
            assert_eq!(error, expected, "byte {i} mask {mask:#04x}");
        }
    }
}

#[test]
fn an_envelope_cut_or_extended_never_decrypts() {
    let key = data_key(1);
    let row = ctx("t", "c", b"r");
    let stored = seal(&key, &row, &[7u8; 100]).unwrap();
    for len in 0..stored.len() {
        assert!(open(&key, &row, &stored[..len]).is_err(), "cut to {len}");
    }
    for extra in 1..40 {
        let mut longer = stored.clone();
        longer.extend(std::iter::repeat(0u8).take(extra));
        assert_eq!(
            open(&key, &row, &longer).unwrap_err(),
            Error::Authentication
        );
    }
}

#[test]
fn every_context_field_is_bound() {
    let key = data_key(1);
    let right = ctx("checkpoint_object", "bytes", b"doc/addr");
    let stored = seal(&key, &right, b"secret").unwrap();
    let wrong_installation = Context::new(
        Uuid::from_u128(1),
        "checkpoint_object",
        "bytes",
        b"doc/addr",
    )
    .unwrap();
    for (what, wrong) in [
        ("installation", wrong_installation),
        ("table", ctx("checkpoint_objects", "bytes", b"doc/addr")),
        ("column", ctx("checkpoint_object", "byte", b"doc/addr")),
        ("row_key", ctx("checkpoint_object", "bytes", b"doc/addR")),
        (
            "row_key prefix",
            ctx("checkpoint_object", "bytes", b"doc/add"),
        ),
        (
            "row_key suffix",
            ctx("checkpoint_object", "bytes", b"doc/addr\0"),
        ),
        (
            "table/column swap",
            ctx("bytes", "checkpoint_object", b"doc/addr"),
        ),
    ] {
        assert_eq!(
            open(&key, &wrong, &stored).unwrap_err(),
            Error::Authentication,
            "{what}"
        );
    }
    // A different DEK with the same id also fails.
    let other = DataKey::new(1, 1, Dek::from_bytes([0x25; 32]));
    assert_eq!(
        open(&other, &right, &stored).unwrap_err(),
        Error::Authentication
    );
    assert_eq!(open(&key, &right, &stored).unwrap(), b"secret");
}

#[test]
fn an_envelope_moved_to_another_row_does_not_open() {
    let key = data_key(1);
    let (a, b) = (ctx("t", "c", b"row-a"), ctx("t", "c", b"row-b"));
    let from_a = seal(&key, &a, b"payload a").unwrap();
    let from_b = seal(&key, &b, b"payload b").unwrap();
    assert_eq!(open(&key, &b, &from_a).unwrap_err(), Error::Authentication);
    assert_eq!(open(&key, &a, &from_b).unwrap_err(), Error::Authentication);
    // Even with the salt copied across, the row identity still separates them.
    let mut forged = from_a.clone();
    forged[17..49].copy_from_slice(&from_b[17..49]);
    assert_eq!(open(&key, &a, &forged).unwrap_err(), Error::Authentication);
}

/// With one fixed salt and one fixed plaintext, any two values whose table, column or row
/// differ must still encrypt differently: a repeated ciphertext would mean a repeated key
/// and nonce.
#[test]
fn rows_columns_and_tables_never_share_a_key_and_nonce() {
    let key = data_key(1);
    let salt = Salt::from_slice(&[0x5C; 32]).unwrap();
    let plaintext = [0u8; 48];
    let mut seen = HashSet::new();
    let mut count = 0;
    for table in [
        "checkpoint_object",
        "snapshot",
        "document_upload",
        "draft_edit",
        "t",
    ] {
        for column in ["bytes", "state", "content", "payload", "c"] {
            for row in 0..60u32 {
                let row = row.to_be_bytes();
                let stored =
                    seal_with_salt(&key, &ctx(table, column, &row), &salt, &plaintext).unwrap();
                assert!(
                    seen.insert(stored[HEADER_LEN..].to_vec()),
                    "{table}.{column} {row:?}"
                );
                count += 1;
            }
        }
    }
    assert_eq!(seen.len(), count);
}

#[test]
fn random_salts_do_not_repeat() {
    let key = data_key(1);
    let row = ctx("t", "c", b"r");
    let mut salts = HashSet::new();
    let mut bodies = HashSet::new();
    for _ in 0..3000 {
        let stored = seal(&key, &row, &[0u8; 32]).unwrap();
        assert!(salts.insert(stored[17..49].to_vec()));
        assert!(bodies.insert(stored[HEADER_LEN..].to_vec()));
    }
}

#[test]
fn content_addressed_envelopes_are_stable_and_separate_documents() {
    let mut rng = Rng(0xA7_0003);
    let key = data_key(3);
    let other_key = DataKey::new(3, 1, Dek::from_bytes([0x99; 32]));
    for i in 0..100 {
        let size = rng.below(5000) as usize;
        let plaintext = rng.bytes(size);
        let doc_a = rng.bytes(16);
        let doc_b = rng.bytes(16);
        let address = rng.bytes(32);
        let row_a = [doc_a.clone(), address.clone()].concat();
        let row_b = [doc_b.clone(), address.clone()].concat();
        let (ctx_a, ctx_b) = (
            ctx("checkpoint_object", "bytes", &row_a),
            ctx("checkpoint_object", "bytes", &row_b),
        );

        let first = seal_content_addressed(&key, &ctx_a, &doc_a, &address, &plaintext).unwrap();
        let again = seal_content_addressed(&key, &ctx_a, &doc_a, &address, &plaintext).unwrap();
        assert_eq!(
            first, again,
            "case {i}: same plaintext and address, same bytes"
        );
        assert_eq!(open(&key, &ctx_a, &first).unwrap(), plaintext);

        let elsewhere = seal_content_addressed(&key, &ctx_b, &doc_b, &address, &plaintext).unwrap();
        assert_ne!(
            first, elsewhere,
            "case {i}: another document gives other bytes"
        );
        assert_ne!(first[17..49], elsewhere[17..49]);
        assert_eq!(open(&key, &ctx_b, &elsewhere).unwrap(), plaintext);

        let under_other =
            seal_content_addressed(&other_key, &ctx_a, &doc_a, &address, &plaintext).unwrap();
        assert_ne!(
            first[17..49],
            under_other[17..49],
            "case {i}: the salt depends on the key"
        );
    }
}

#[test]
fn the_parser_never_panics_on_arbitrary_bytes() {
    let mut rng = Rng(0xA7_0004);
    let key = data_key(1);
    let row = ctx("t", "c", b"r");
    let valid = seal(&key, &row, b"a value worth protecting").unwrap();
    let mut parsed_ok = 0usize;
    for round in 0..30_000u32 {
        let input: Vec<u8> = match rng.below(5) {
            // Pure noise.
            0 => {
                let len = rng.below(200) as usize;
                rng.bytes(len)
            }
            // Right magic and version, noise after.
            1 => {
                let len = rng.below(200) as usize;
                let mut v = b"DYNOENC\0\x01".to_vec();
                v.extend(rng.bytes(len));
                v
            }
            // A valid envelope with a few bit flips.
            2 => {
                let mut v = valid.clone();
                for _ in 0..=rng.below(4) {
                    let at = rng.below(v.len() as u64) as usize;
                    v[at] ^= 1 << rng.below(8);
                }
                v
            }
            // A valid envelope cut anywhere.
            3 => valid[..rng.below(valid.len() as u64 + 1) as usize].to_vec(),
            // A valid envelope with a random tail.
            _ => {
                let mut v = valid.clone();
                let len = rng.below(64) as usize + 1;
                v.extend(rng.bytes(len));
                v
            }
        };
        let _ = has_magic(&input);
        if Header::parse(&input).is_ok() {
            parsed_ok += 1;
        }
        let opened = open(&key, &row, &input);
        if input == valid {
            assert_eq!(opened.unwrap(), b"a value worth protecting");
        } else {
            assert!(opened.is_err(), "round {round}: altered input opened");
        }
    }
    assert!(
        parsed_ok > 1000,
        "the fuzz inputs reached the AEAD step: {parsed_ok}"
    );
}

/// A stand-in for the `data_key` table and the store that will use this module: the
/// header's `dek_id` picks the row, the cache and provider produce the key.
struct Store {
    rows: HashMap<u32, Wrapped>,
    wrap_ctx: WrapContext,
}

impl Store {
    fn read(
        &self,
        cache: &DekCache,
        provider: &dyn KeyProvider,
        row: &Context<'_>,
        stored: &[u8],
    ) -> Result<Vec<u8>, String> {
        let header = Header::parse(stored).map_err(|e| e.to_string())?;
        let wrapped = self.rows.get(&header.dek_id).ok_or("no such data key")?;
        let key = cache
            .get_or_unwrap(header.dek_id, wrapped, &self.wrap_ctx, provider)
            .map_err(|e| e.to_string())?;
        open(&key, row, stored).map_err(|e| e.to_string())
    }
}

#[test]
fn kek_rotation_dek_rotation_and_the_kill_switch() {
    let wrap_ctx = WrapContext::new(installation(), Some(Uuid::from_u128(0xBEEF)), Purpose::Data);
    let mut store = Store {
        rows: HashMap::new(),
        wrap_ctx,
    };

    // Day one: KEK version 1, data key 1.
    let day_one = TestKeyProvider::new([(1, [0x11; 32])]);
    let dek1 = Dek::generate().unwrap();
    store
        .rows
        .insert(1, day_one.wrap(&dek1, &wrap_ctx).unwrap());
    let key1 = DataKey::new(1, 1, dek1);
    let rows: Vec<(String, Vec<u8>)> = (0..20)
        .map(|i| (format!("row-{i}"), format!("content {i}").into_bytes()))
        .collect();
    let old: Vec<Vec<u8>> = rows
        .iter()
        .map(|(row, content)| {
            seal(
                &key1,
                &ctx("checkpoint_object", "bytes", row.as_bytes()),
                content,
            )
            .unwrap()
        })
        .collect();

    // KEK rotation: add version 2, rewrap the stored wrapped key. No envelope changes.
    let rotated = TestKeyProvider::new([(1, [0x11; 32]), (2, [0x22; 32])]);
    let rewrapped = rotated.rewrap(&store.rows[&1], &wrap_ctx).unwrap();
    assert_eq!(rewrapped.kek_version(), 2);
    store.rows.insert(1, rewrapped);
    let clock = Arc::new(ManualClock::new());
    let cache = DekCache::with_clock(Duration::from_secs(900), 16, clock.clone());
    for ((row, content), stored) in rows.iter().zip(&old) {
        let read = store.read(
            &cache,
            &rotated,
            &ctx("checkpoint_object", "bytes", row.as_bytes()),
            stored,
        );
        assert_eq!(read.unwrap(), *content);
    }
    // The version-1 KEK alone no longer opens the rewrapped key.
    let only_old = TestKeyProvider::new([(1, [0x11; 32])]);
    assert!(matches!(
        only_old.unwrap(&store.rows[&1], &wrap_ctx),
        Err(ProviderError::NotFound(_))
    ));

    // DEK rotation: data key 2 for new writes; old rows still read through key 1.
    let dek2 = Dek::generate().unwrap();
    store
        .rows
        .insert(2, rotated.wrap(&dek2, &wrap_ctx).unwrap());
    let key2 = DataKey::new(2, 2, dek2);
    let fresh = seal(
        &key2,
        &ctx("checkpoint_object", "bytes", b"row-new"),
        b"new content",
    )
    .unwrap();
    assert_eq!(Header::parse(&fresh).unwrap().dek_id, 2);
    assert_eq!(Header::parse(&old[0]).unwrap().dek_id, 1);
    assert_eq!(
        store
            .read(
                &cache,
                &rotated,
                &ctx("checkpoint_object", "bytes", b"row-new"),
                &fresh
            )
            .unwrap(),
        b"new content"
    );
    assert_eq!(
        store
            .read(
                &cache,
                &rotated,
                &ctx("checkpoint_object", "bytes", b"row-0"),
                &old[0]
            )
            .unwrap(),
        b"content 0"
    );

    // Kill switch: the customer disables the KEK. Cached keys work until their lifetime
    // ends, then every read fails and nothing was stored in the clear.
    struct Switch {
        inner: TestKeyProvider,
        revoked: AtomicBool,
    }
    impl KeyProvider for Switch {
        fn wrap(&self, dek: &Dek, ctx: &WrapContext) -> Result<Wrapped, ProviderError> {
            self.inner.wrap(dek, ctx)
        }
        fn unwrap(&self, wrapped: &Wrapped, ctx: &WrapContext) -> Result<Dek, ProviderError> {
            if self.revoked.load(Ordering::SeqCst) {
                return Err(ProviderError::Denied("the KEK is disabled".into()));
            }
            self.inner.unwrap(wrapped, ctx)
        }
        fn describe(&self) -> ProviderInfo {
            self.inner.describe()
        }
    }
    let switch = Switch {
        inner: rotated,
        revoked: AtomicBool::new(false),
    };
    switch.revoked.store(true, Ordering::SeqCst);
    let row0 = ctx("checkpoint_object", "bytes", b"row-0");
    assert!(
        store.read(&cache, &switch, &row0, &old[0]).is_ok(),
        "within the cache lifetime"
    );
    clock.advance(Duration::from_secs(900));
    let error = store.read(&cache, &switch, &row0, &old[0]).unwrap_err();
    assert!(error.contains("denied"), "{error}");
    switch.revoked.store(false, Ordering::SeqCst);
    assert!(
        store.read(&cache, &switch, &row0, &old[0]).is_ok(),
        "restored by re-enabling"
    );

    // Crypto-erase: with the wrapped key rows destroyed there is nothing to unwrap.
    cache.clear();
    store.rows.clear();
    assert!(store.read(&cache, &switch, &row0, &old[0]).is_err());
}

#[test]
fn the_cache_is_safe_to_share_between_threads() {
    let cache = Arc::new(DekCache::new(Duration::from_secs(60), 8));
    let provider = Arc::new(TestKeyProvider::new([(1, [0x31; 32])]));
    let wrap_ctx = WrapContext::new(installation(), None, Purpose::Data);
    let dek = Dek::from_bytes([0x77; 32]);
    let wrapped = Arc::new(provider.wrap(&dek, &wrap_ctx).unwrap());
    let loads = Arc::new(Mutex::new(BTreeMap::<u32, usize>::new()));
    let handles: Vec<_> = (0..8u32)
        .map(|t| {
            let (cache, provider, wrapped, loads) = (
                cache.clone(),
                provider.clone(),
                wrapped.clone(),
                loads.clone(),
            );
            std::thread::spawn(move || {
                for i in 0..200u32 {
                    let id = (t + i) % 4;
                    let key = cache
                        .get_or_unwrap(id, &wrapped, &wrap_ctx, provider.as_ref())
                        .unwrap();
                    *loads.lock().unwrap().entry(id).or_default() += 1;
                    assert_eq!(key.id(), id);
                    assert_eq!(key.dek().as_bytes(), &[0x77; 32]);
                }
            })
        })
        .collect();
    for handle in handles {
        handle.join().unwrap();
    }
    assert_eq!(loads.lock().unwrap().values().sum::<usize>(), 8 * 200);
    assert!(cache.len() <= 4);
}
