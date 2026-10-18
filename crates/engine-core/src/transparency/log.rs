//! The append-only transparency log: typed leaves in a tile-backed Merkle tree,
//! checkpoints signed with the log key, inclusion bundles for any leaf.
//!
//! This is the storage-agnostic core (design phase W1, pure part). It does not
//! queue, lock, schedule or talk to witnesses; whoever drives it provides a
//! single writer, batches leaves, and decides when to sign a checkpoint (design
//! 4.2 cadence).
use super::bundle::InclusionBundle;
use super::leaf::Leaf;
use super::merkle::{self, Hash, HashSource};
use super::note::{Checkpoint, Note, Origin, Signer};
use super::tiles::{TileLog, TileStore};
use super::Error;

pub struct Log<S: TileStore> {
    origin: Origin,
    tiles: TileLog<S>,
}

impl<S: TileStore> Log<S> {
    /// Open the log stored in `store` (empty for a new store).
    pub fn open(origin: Origin, store: S) -> Result<Self, Error> {
        Ok(Self {
            origin,
            tiles: TileLog::open(store)?,
        })
    }

    pub fn origin(&self) -> &Origin {
        &self.origin
    }

    pub fn size(&self) -> u64 {
        self.tiles.size()
    }

    pub fn root(&self) -> Hash {
        self.tiles.root()
    }

    pub fn store(&self) -> &S {
        self.tiles.store()
    }

    pub fn into_store(self) -> S {
        self.tiles.into_store()
    }

    /// Append one leaf; returns its index.
    pub fn append(&mut self, leaf: &Leaf) -> Result<u64, Error> {
        self.append_batch(std::slice::from_ref(leaf))
    }

    /// Append leaves and commit them together; returns the index of the first.
    /// Every leaf is encoded before anything is written, so an invalid one aborts
    /// the whole batch.
    pub fn append_batch(&mut self, leaves: &[Leaf]) -> Result<u64, Error> {
        let entries = leaves
            .iter()
            .map(Leaf::entry)
            .collect::<Result<Vec<_>, _>>()?;
        self.tiles.append_batch(&entries)
    }

    /// The leaf at `index`.
    pub fn leaf(&self, index: u64) -> Result<Leaf, Error> {
        Leaf::from_entry(&self.tiles.entry(index)?)
    }

    /// The unsigned checkpoint for the current size.
    pub fn checkpoint(&self) -> Checkpoint {
        Checkpoint {
            origin: self.origin.as_str().to_string(),
            size: self.size(),
            root: self.root(),
        }
    }

    /// The current checkpoint signed by the log key (which must be named by the
    /// origin).
    pub fn signed_checkpoint(&self, signer: &Signer) -> Result<Note, Error> {
        let text = self.checkpoint().text();
        Note::new(&text)?.with_signature(signer.sign_checkpoint(&text)?)
    }

    /// Audit path of `index` in the tree of the first `size` leaves.
    pub fn inclusion_proof(&self, index: u64, size: u64) -> Result<Vec<Hash>, Error> {
        merkle::inclusion_proof(self, index, size)
    }

    /// Proof that the first `first` leaves are a prefix of the first `second`.
    pub fn consistency_proof(&self, first: u64, second: u64) -> Result<Vec<Hash>, Error> {
        merkle::consistency_proof(self, first, second)
    }

    /// The inclusion bundle for leaf `index` under `note`, a checkpoint note of
    /// this log (the log's own, or one carrying cosignatures); the path is for the
    /// note's tree size.
    pub fn bundle(&self, index: u64, note: &Note) -> Result<InclusionBundle, Error> {
        let checkpoint = Checkpoint::parse(note.text())?;
        if checkpoint.origin != self.origin.as_str() {
            return Err(Error::WrongOrigin {
                expected: self.origin.as_str().to_string(),
                found: checkpoint.origin,
            });
        }
        Ok(InclusionBundle {
            leaf: self.leaf(index)?,
            index,
            size: checkpoint.size,
            path: self.inclusion_proof(index, checkpoint.size)?,
            checkpoint: note.encode(),
        })
    }
}

impl<S: TileStore> HashSource for Log<S> {
    fn size(&self) -> u64 {
        self.tiles.size()
    }

    fn node(&self, level: u8, index: u64) -> Result<Hash, Error> {
        self.tiles.node(level, index)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transparency::note::KeyKind;
    use crate::transparency::tiles::MemoryTileStore;

    const ORIGIN: &str = "log.dynodoc.example/dynodoc/v1/transparency";

    fn leaf(i: u64) -> Leaf {
        Leaf::EventHeads {
            root: [i as u8; 32],
            count: i,
            interval: i / 7,
        }
    }

    #[test]
    fn append_checkpoint_and_bundle_round_trip() {
        let origin = Origin::new(ORIGIN).unwrap();
        let signer = Signer::from_seed(ORIGIN, KeyKind::Log, &[1; 32]).unwrap();
        let mut log = Log::open(origin, MemoryTileStore::new()).unwrap();
        assert_eq!(log.size(), 0);
        assert_eq!(log.checkpoint().root, merkle::empty_root());
        let leaves: Vec<Leaf> = (0..300).map(leaf).collect();
        assert_eq!(log.append_batch(&leaves[..10]).unwrap(), 0);
        assert_eq!(log.append(&leaves[10]).unwrap(), 10);
        assert_eq!(log.append_batch(&leaves[11..]).unwrap(), 11);
        assert_eq!(log.size(), 300);
        let note = log.signed_checkpoint(&signer).unwrap();
        for index in [0, 1, 255, 256, 299] {
            assert_eq!(log.leaf(index).unwrap(), leaves[index as usize]);
            let bundle = log.bundle(index, &note).unwrap();
            assert_eq!(bundle.size, 300);
            bundle.verify_path(&log.root()).unwrap();
        }
        // A bundle for an older checkpoint of the same log.
        let old = Note::new(
            &Checkpoint {
                origin: ORIGIN.into(),
                size: 100,
                root: merkle::tree_root(&log, 100).unwrap(),
            }
            .text(),
        )
        .unwrap();
        let bundle = log.bundle(40, &old).unwrap();
        assert_eq!(bundle.size, 100);
        bundle
            .verify_path(&merkle::tree_root(&log, 100).unwrap())
            .unwrap();
        assert!(log.bundle(150, &old).is_err());
        assert!(log.leaf(300).is_err());
    }

    #[test]
    fn invalid_leaves_abort_the_whole_batch() {
        let origin = Origin::new(ORIGIN).unwrap();
        let mut log = Log::open(origin, MemoryTileStore::new()).unwrap();
        let too_long = Leaf::LogSuccessor {
            prev_origin: Origin::new(&format!("{}/dynodoc/v1/transparency", "h".repeat(150)))
                .unwrap(),
            size: 1,
            root: [0; 32],
        };
        assert!(log.append_batch(&[leaf(1), too_long]).is_err());
        assert_eq!(log.size(), 0);
    }

    #[test]
    fn only_the_origins_log_key_can_sign_and_foreign_notes_are_refused() {
        let origin = Origin::new(ORIGIN).unwrap();
        let mut log = Log::open(origin, MemoryTileStore::new()).unwrap();
        log.append(&leaf(1)).unwrap();
        let other = Signer::from_seed(
            "other.example/dynodoc/v1/transparency",
            KeyKind::Log,
            &[1; 32],
        )
        .unwrap();
        assert!(matches!(
            log.signed_checkpoint(&other),
            Err(Error::WrongOrigin { .. })
        ));
        let foreign = Note::new(
            &Checkpoint {
                origin: "other.example/dynodoc/v1/transparency".into(),
                size: 1,
                root: log.root(),
            }
            .text(),
        )
        .unwrap();
        assert!(matches!(
            log.bundle(0, &foreign),
            Err(Error::WrongOrigin { .. })
        ));
    }
}
