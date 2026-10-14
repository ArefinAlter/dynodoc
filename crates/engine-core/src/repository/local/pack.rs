//! Transparent per-object compression at rest for local object rows (schema 3).
//!
//! The *logical* object is the canonical encoded byte string whose hash is its
//! address. These functions change only how those bytes sit in the SQLite row:
//! addresses, quotas, transfer packets and every verification still see the
//! logical bytes. A row records its codec and its logical length, so decoding is
//! bounded by a declared size that is itself checked against the per-object limit
//! before any inflation starts and enforced while inflating.
//!
//! Codecs: `0` identity (also every row written by schema 1 and 2) and `1` raw
//! DEFLATE (RFC 1951) through the pure-Rust `miniz_oxide`. An object is stored
//! compressed only when it is at least [`MIN_LOGICAL_BYTES`] long and the result is
//! at most nine tenths of its size; otherwise it stays identity. The table accepts
//! any non-negative codec number, so a later codec needs no table rebuild; a build
//! that does not know a codec refuses to read such a row.
use crate::shared_checkpoint::Error;
use miniz_oxide::deflate::core::{
    compress, create_comp_flags_from_zip_params, CompressorOxide, TDEFLFlush, TDEFLStatus,
};
use std::cell::RefCell;

pub const IDENTITY: i64 = 0;
pub const DEFLATE: i64 = 1;
/// The existing per-object limit (also the table's CHECK constraint).
pub const MAX_LOGICAL_BYTES: usize = 1_048_576;
/// Shorter objects are not worth a compression attempt: a sweep of 48 to 192 bytes
/// on the synthetic and documentation corpora gained nothing below 128.
pub const MIN_LOGICAL_BYTES: usize = 128;
/// A compressed form is kept only if `stored * 10 <= logical * 9`.
pub const MIN_SAVING_PERCENT: usize = 10;
/// DEFLATE effort. Level 6 is the zlib default; see docs/LOCAL-STORAGE-BUDGETS.md
/// for the measured size and speed against other levels.
pub const LEVEL: u8 = 6;

thread_local! {
    /// One compressor per thread, reset for every object: building its tables
    /// dominates the cost of compressing the small objects this store holds.
    static DEFLATER: RefCell<Option<Box<CompressorOxide>>> = const { RefCell::new(None) };
}

/// The compressed form of `encoded`, or `None` when it should stay identity.
pub fn pack(encoded: &[u8]) -> Option<Vec<u8>> {
    if encoded.len() < MIN_LOGICAL_BYTES || encoded.len() > MAX_LOGICAL_BYTES {
        return None;
    }
    // The most a result may occupy; a stream that does not fit is not worth
    // keeping, so the compressor simply never reaches `Done`.
    let budget = encoded.len() * (100 - MIN_SAVING_PERCENT) / 100;
    DEFLATER.with(|slot| {
        let mut slot = slot.borrow_mut();
        let compressor = slot.get_or_insert_with(|| {
            Box::new(CompressorOxide::new(create_comp_flags_from_zip_params(
                i32::from(LEVEL),
                0,
                0,
            )))
        });
        compressor.reset();
        let mut out = vec![0u8; budget];
        let (status, consumed, written) =
            compress(compressor, encoded, &mut out, TDEFLFlush::Finish);
        if status != TDEFLStatus::Done || consumed != encoded.len() {
            return None;
        }
        out.truncate(written);
        (!out.is_empty()).then_some(out)
    })
}

/// The logical bytes of a stored row.
///
/// `size` is the declared logical length (`None` for a row written before schema
/// 3, which is identity). A compressed row must declare a size within the object
/// limit that is strictly larger than what is stored; inflation is capped at that
/// size and must produce exactly that many bytes.
pub fn unpack(codec: i64, stored: Vec<u8>, size: Option<i64>) -> Result<Vec<u8>, Error> {
    match codec {
        IDENTITY => match size {
            None => Ok(stored),
            Some(size) if usize::try_from(size) == Ok(stored.len()) => Ok(stored),
            Some(_) => Err(Error::Invalid("stored object length")),
        },
        DEFLATE => {
            let size = size
                .and_then(|size| usize::try_from(size).ok())
                .filter(|size| (1..=MAX_LOGICAL_BYTES).contains(size))
                .ok_or(Error::Invalid("stored object length"))?;
            if stored.is_empty() || stored.len() >= size {
                return Err(Error::Invalid("stored object length"));
            }
            // The limit is the declared size, so a hostile stream cannot inflate
            // past it: the decoder stops with an error instead of allocating more.
            let logical = miniz_oxide::inflate::decompress_to_vec_with_limit(&stored, size)
                .map_err(|_| Error::Invalid("stored object compression"))?;
            if logical.len() != size {
                return Err(Error::Invalid("stored object length"));
            }
            Ok(logical)
        }
        _ => Err(Error::Invalid("stored object codec")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn json_like(paragraphs: usize) -> Vec<u8> {
        (0..paragraphs)
            .map(|i| {
                format!(
                    "{{\"kind\":\"text\",\"value\":{{\"bytes\":{},\"root\":\"{:064x}\"}},\"label\":\"Paragraph {i} of the quarterly plan\"}}",
                    i * 31,
                    i
                )
            })
            .collect::<Vec<_>>()
            .join(",")
            .into_bytes()
    }

    #[test]
    fn compressible_objects_round_trip_and_small_or_random_ones_stay_identity() {
        let data = json_like(40);
        let stored = pack(&data).expect("repetitive JSON compresses");
        assert!(stored.len() * 10 <= data.len() * 9);
        assert_eq!(
            unpack(DEFLATE, stored, Some(data.len() as i64)).unwrap(),
            data
        );
        assert!(pack(&data[..MIN_LOGICAL_BYTES - 1]).is_none());
        // Incompressible bytes fall below the 10 percent threshold.
        let mut state = 0x9E37_79B9_7F4A_7C15u64;
        let random: Vec<u8> = (0..4096)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                (state >> 24) as u8
            })
            .collect();
        assert!(pack(&random).is_none());
        // A row from before schema 3 is identity with no declared size.
        assert_eq!(unpack(IDENTITY, data.clone(), None).unwrap(), data);
        assert_eq!(
            unpack(IDENTITY, data.clone(), Some(data.len() as i64)).unwrap(),
            data
        );
        assert!(unpack(IDENTITY, data.clone(), Some(1)).is_err());
    }

    #[test]
    fn corrupt_mismatched_and_bomb_rows_are_rejected_within_the_declared_size() {
        let data = json_like(60);
        let stored = pack(&data).unwrap();
        let size = data.len() as i64;
        // Unknown codec, absent or absurd declared sizes.
        assert!(unpack(7, stored.clone(), Some(size)).is_err());
        assert!(unpack(DEFLATE, stored.clone(), None).is_err());
        assert!(unpack(DEFLATE, stored.clone(), Some(0)).is_err());
        assert!(unpack(DEFLATE, stored.clone(), Some(-5)).is_err());
        assert!(unpack(DEFLATE, stored.clone(), Some(MAX_LOGICAL_BYTES as i64 + 1)).is_err());
        assert!(unpack(DEFLATE, Vec::new(), Some(size)).is_err());
        // A declared size no larger than the stored bytes is not compression.
        assert!(unpack(DEFLATE, stored.clone(), Some(stored.len() as i64)).is_err());
        // Wrong declared length, both smaller (stream exceeds it) and larger.
        assert!(unpack(DEFLATE, stored.clone(), Some(size - 1)).is_err());
        assert!(unpack(DEFLATE, stored.clone(), Some(size + 1)).is_err());
        // Flipped and truncated streams.
        let mut flipped = stored.clone();
        flipped[stored.len() / 2] ^= 0x55;
        let outcome = unpack(DEFLATE, flipped, Some(size));
        assert!(
            outcome.is_err() || outcome.unwrap() != data,
            "a corrupt stream must never decode to the original bytes"
        );
        assert!(unpack(DEFLATE, stored[..stored.len() - 3].to_vec(), Some(size)).is_err());
        // A real bomb: a megabyte of zeros is a few hundred bytes. Declaring a
        // small size must stop inflation at that size.
        let bomb_source = vec![0u8; MAX_LOGICAL_BYTES];
        let bomb = miniz_oxide::deflate::compress_to_vec(&bomb_source, 9);
        assert!(bomb.len() < 2048);
        assert!(unpack(DEFLATE, bomb.clone(), Some(4096)).is_err());
        assert_eq!(
            unpack(DEFLATE, bomb, Some(MAX_LOGICAL_BYTES as i64))
                .unwrap()
                .len(),
            MAX_LOGICAL_BYTES
        );
    }
}
