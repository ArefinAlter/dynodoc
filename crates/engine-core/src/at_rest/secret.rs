//! Secret byte holders for the at-rest envelope: they overwrite their bytes when
//! dropped and never print them.
//!
//! This is the same best-effort pattern as `engine-cli`'s `passphrase::Secret`
//! (volatile writes plus a compiler fence). `engine-cli` depends on this crate, so the
//! few lines are repeated here rather than shared. Zeroisation is best effort:
//! `ring` keeps its own expanded copies of the keys it is given and cannot wipe them,
//! and the operating system may swap or dump process memory.
use super::Error;
use ring::rand::{SecureRandom, SystemRandom};

/// Overwrite with zeros using volatile writes, so the compiler cannot remove the
/// stores as dead.
pub(crate) fn wipe(buffer: &mut [u8]) {
    for byte in buffer.iter_mut() {
        // SAFETY: `byte` is a valid, aligned, exclusive reference.
        unsafe { std::ptr::write_volatile(byte, 0) };
    }
    std::sync::atomic::compiler_fence(std::sync::atomic::Ordering::SeqCst);
}

/// `N` secret bytes, wiped on drop. Deliberately not `Clone`; `Debug` prints nothing.
pub(crate) struct Wiped<const N: usize>(pub(crate) [u8; N]);

impl<const N: usize> Wiped<N> {
    pub(crate) fn zero() -> Self {
        Self([0u8; N])
    }
}

impl<const N: usize> Drop for Wiped<N> {
    fn drop(&mut self) {
        wipe(&mut self.0);
    }
}

impl<const N: usize> std::fmt::Debug for Wiped<N> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Wiped(<redacted>)")
    }
}

/// A 32-byte secret key with a given purpose: a data-encryption key (DEK) or a
/// key-encryption key (KEK). Wiped on drop, not `Clone`, `Debug` prints nothing.
macro_rules! key32 {
    ($(#[$meta:meta])* $name:ident, $label:literal) => {
        $(#[$meta])*
        pub struct $name(Wiped<32>);

        impl $name {
            /// Key length in bytes.
            pub const LEN: usize = 32;

            /// Take ownership of key bytes. The caller's own copy of the array is
            /// not wiped by this call.
            pub fn from_bytes(bytes: [u8; 32]) -> Self {
                Self(Wiped(bytes))
            }

            /// Copy from a slice that must hold exactly 32 bytes.
            pub fn from_slice(bytes: &[u8]) -> Result<Self, Error> {
                let bytes: [u8; 32] = bytes
                    .try_into()
                    .map_err(|_| Error::BadKeyLength { len: bytes.len() })?;
                Ok(Self::from_bytes(bytes))
            }

            /// The raw key bytes. Handle with care: do not log or persist them.
            pub fn as_bytes(&self) -> &[u8; 32] {
                &(self.0).0
            }
        }

        impl std::fmt::Debug for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(concat!($label, "(<redacted>)"))
            }
        }
    };
}

key32!(
    /// A data-encryption key: 32 random bytes, one per project (and one for
    /// non-project data), stored only wrapped by a key provider.
    Dek,
    "Dek"
);
key32!(
    /// A key-encryption key held by a [`KeyProvider`](super::KeyProvider) such as the
    /// local file provider. It wraps DEKs and never encrypts row data.
    Kek,
    "Kek"
);

impl Dek {
    /// A fresh random DEK from the operating system's generator.
    pub fn generate() -> Result<Self, Error> {
        let mut bytes = Wiped::<32>::zero();
        SystemRandom::new()
            .fill(&mut bytes.0)
            .map_err(|_| Error::Random)?;
        Ok(Self(bytes))
    }
}

/// A DEK together with the identifiers stored in every envelope it writes: the
/// `data_key` row id and the KEK version that wrapped it. This is the handle the
/// encrypt and decrypt functions take.
#[derive(Debug)]
pub struct DataKey {
    id: u32,
    kek_version: u32,
    dek: Dek,
}

impl DataKey {
    pub fn new(id: u32, kek_version: u32, dek: Dek) -> Self {
        Self {
            id,
            kek_version,
            dek,
        }
    }

    /// The `data_key` row id; written to the envelope header and checked on open.
    pub fn id(&self) -> u32 {
        self.id
    }

    /// The KEK version that wrapped this DEK when it was loaded. Informational: it is
    /// recorded in new envelopes but never selects a key.
    pub fn kek_version(&self) -> u32 {
        self.kek_version
    }

    pub fn dek(&self) -> &Dek {
        &self.dek
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::ManuallyDrop;

    #[test]
    fn wipe_zeroes_every_byte() {
        let mut buffer = [0xAAu8; 77];
        wipe(&mut buffer);
        assert_eq!(buffer, [0u8; 77]);
    }

    #[test]
    fn dropping_a_dek_overwrites_its_bytes() {
        let mut dek = ManuallyDrop::new(Dek::from_bytes([0x5Au8; 32]));
        let at = dek.as_bytes().as_ptr();
        // SAFETY: `ManuallyDrop` keeps the storage alive and initialised; `drop` runs
        // the destructor once, and the bytes are read through a pointer taken before it.
        let after = unsafe {
            ManuallyDrop::drop(&mut dek);
            std::slice::from_raw_parts(at, 32).to_vec()
        };
        assert_eq!(after, vec![0u8; 32]);
    }

    #[test]
    fn debug_never_prints_key_bytes() {
        let dek = Dek::from_bytes([0xABu8; 32]);
        let kek = Kek::from_bytes([0xCDu8; 32]);
        let handle = DataKey::new(7, 1, Dek::from_bytes([0xEFu8; 32]));
        let shown = format!("{dek:?} {kek:?} {handle:?}");
        for needle in ["ab", "AB", "171", "cd", "CD", "205", "ef", "EF", "239"] {
            assert!(!shown.contains(needle), "{needle} leaked in {shown}");
        }
        assert!(shown.contains("redacted"));
    }

    #[test]
    fn slices_of_the_wrong_length_are_refused() {
        for len in [0usize, 1, 31, 33, 64] {
            assert_eq!(
                Dek::from_slice(&vec![1u8; len]).unwrap_err(),
                Error::BadKeyLength { len }
            );
        }
        assert!(Kek::from_slice(&[1u8; 32]).is_ok());
    }

    #[test]
    fn generated_deks_differ() {
        let a = Dek::generate().unwrap();
        let b = Dek::generate().unwrap();
        assert_ne!(a.as_bytes(), b.as_bytes());
        assert_ne!(a.as_bytes(), &[0u8; 32]);
    }
}
