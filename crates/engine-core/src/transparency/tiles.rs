//! Tile-backed tree storage in the C2SP `tlog-tiles` layout (height 8).
//!
//! A hash tile at level `L`, index `N` holds the 256 consecutive stored hashes
//! `N * 256 ..` of tree level `8 * L` (`tile/L/N`, path rules below). A tile that is
//! not yet full is stored as `tile/L/N.p/W` with its `W` hashes. Interior levels
//! that are not multiples of 8 are not stored: they are recomputed from the tile
//! below. Entry bundles (`tile/entries/N[.p/W]`) hold the log entries themselves,
//! each as a big-endian `u16` length followed by the bytes, 256 per bundle.
//!
//! The storage trait is path-keyed bytes plus one commit marker (the tree size), so
//! an object store, a table or a directory can back it. `TileLog::append_batch`
//! writes every tile first and the size last: a crash before the size is written
//! leaves the previous tree intact and ignores the stray tiles. This follows the
//! tile layout as remembered from c2sp.org/tlog-tiles (unverified: check at build
//! time against the published text; the Python vectors pin the path spelling and
//! tile bytes this module produces).
use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Component, Path, PathBuf};

use super::merkle::{empty_root, leaf_hash, node_hash, Hash, HashSource};
use super::Error;

/// Hashes per tile: `2^TILE_HEIGHT`.
pub const TILE_WIDTH: u64 = 256;
const TILE_HEIGHT: u32 = 8;

/// `N` of a tile or bundle path: decimal digits in groups of three, every group but
/// the last prefixed `x` (`1234067` is `x001/x234/067`, `5` is `005`).
pub fn tile_index_path(index: u64) -> String {
    let digits = index.to_string();
    let padded = format!("{}{digits}", "0".repeat((3 - digits.len() % 3) % 3));
    let groups: Vec<&str> = (0..padded.len())
        .step_by(3)
        .map(|i| &padded[i..i + 3])
        .collect();
    let last = groups.len() - 1;
    groups
        .iter()
        .enumerate()
        .map(|(i, g)| {
            if i < last {
                format!("x{g}")
            } else {
                (*g).to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("/")
}

fn with_width(path: String, width: u64) -> String {
    debug_assert!((1..=TILE_WIDTH).contains(&width));
    if width == TILE_WIDTH {
        path
    } else {
        format!("{path}.p/{width}")
    }
}

/// Path of a hash tile holding `width` hashes (`1..=256`; 256 is a full tile).
pub fn hash_tile_path(level: u8, index: u64, width: u64) -> String {
    with_width(format!("tile/{level}/{}", tile_index_path(index)), width)
}

/// Path of an entry bundle holding `width` entries (`1..=256`).
pub fn entry_bundle_path(index: u64, width: u64) -> String {
    with_width(format!("tile/entries/{}", tile_index_path(index)), width)
}

/// Path-keyed byte storage with an atomic commit marker. Single writer.
pub trait TileStore {
    fn get(&self, path: &str) -> Result<Option<Vec<u8>>, Error>;
    /// Create or atomically replace one object.
    fn put(&mut self, path: &str, bytes: &[u8]) -> Result<(), Error>;
    /// Committed tree size (0 for a new store).
    fn size(&self) -> Result<u64, Error>;
    /// Durably record the tree size; called after every tile of the new size is written.
    fn set_size(&mut self, size: u64) -> Result<(), Error>;
}

/// Volatile store for tests and the vector generator's cross-checks.
#[derive(Clone, Debug, Default)]
pub struct MemoryTileStore {
    files: BTreeMap<String, Vec<u8>>,
    size: u64,
}

impl MemoryTileStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Every stored object by path, in path order.
    pub fn files(&self) -> &BTreeMap<String, Vec<u8>> {
        &self.files
    }
}

impl TileStore for MemoryTileStore {
    fn get(&self, path: &str) -> Result<Option<Vec<u8>>, Error> {
        Ok(self.files.get(path).cloned())
    }
    fn put(&mut self, path: &str, bytes: &[u8]) -> Result<(), Error> {
        self.files.insert(path.to_string(), bytes.to_vec());
        Ok(())
    }
    fn size(&self) -> Result<u64, Error> {
        Ok(self.size)
    }
    fn set_size(&mut self, size: u64) -> Result<(), Error> {
        self.size = size;
        Ok(())
    }
}

/// Directory-backed store: each object is a file under the root, replaced through a
/// temporary file and a rename; the size is the file `size` holding a decimal. One
/// writer at a time (no locking is attempted here).
#[derive(Clone, Debug)]
pub struct FsTileStore {
    root: PathBuf,
}

fn io(error: std::io::Error) -> Error {
    Error::Storage(error.to_string())
}

impl FsTileStore {
    pub fn open(root: impl Into<PathBuf>) -> Result<Self, Error> {
        let root = root.into();
        fs::create_dir_all(&root).map_err(io)?;
        Ok(Self { root })
    }

    fn resolve(&self, path: &str) -> Result<PathBuf, Error> {
        let relative = Path::new(path);
        let plain = relative
            .components()
            .all(|c| matches!(c, Component::Normal(_)))
            && !path.contains(['\\', ':'])
            && !path.is_empty();
        if !plain {
            return Err(Error::Storage(format!("refusing path {path:?}")));
        }
        Ok(self.root.join(relative))
    }

    fn replace(&self, target: &Path, bytes: &[u8]) -> Result<(), Error> {
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(io)?;
        }
        let mut temporary = target.as_os_str().to_owned();
        temporary.push(".tmp");
        let temporary = PathBuf::from(temporary);
        let mut file = fs::File::create(&temporary).map_err(io)?;
        file.write_all(bytes).map_err(io)?;
        file.sync_all().map_err(io)?;
        drop(file);
        fs::rename(&temporary, target).map_err(io)
    }
}

impl TileStore for FsTileStore {
    fn get(&self, path: &str) -> Result<Option<Vec<u8>>, Error> {
        match fs::read(self.resolve(path)?) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(io(e)),
        }
    }
    fn put(&mut self, path: &str, bytes: &[u8]) -> Result<(), Error> {
        self.replace(&self.resolve(path)?, bytes)
    }
    fn size(&self) -> Result<u64, Error> {
        match fs::read_to_string(self.root.join("size")) {
            Ok(text) => text
                .parse()
                .map_err(|_| Error::Storage("the size marker is not a decimal".into())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(0),
            Err(e) => Err(io(e)),
        }
    }
    fn set_size(&mut self, size: u64) -> Result<(), Error> {
        self.replace(&self.root.join("size"), size.to_string().as_bytes())
    }
}

/// An append-only tree over a [`TileStore`]. Holds the right-edge subtree roots
/// (at most 64 hashes) in memory; everything else is read from tiles on demand.
pub struct TileLog<S: TileStore> {
    store: S,
    size: u64,
    /// Roots of the complete subtrees that make up the tree, largest first.
    frontier: Vec<(u8, Hash)>,
}

impl<S: TileStore> TileLog<S> {
    /// Open the tree committed in `store` (empty for a new store).
    pub fn open(store: S) -> Result<Self, Error> {
        let size = store.size()?;
        let mut log = Self {
            store,
            size,
            frontier: Vec::new(),
        };
        let mut position = 0u64;
        for level in (0..64u8).rev() {
            if size >> level & 1 == 1 {
                let node = log.node(level, position >> level)?;
                log.frontier.push((level, node));
                position += 1 << level;
            }
        }
        Ok(log)
    }

    pub fn size(&self) -> u64 {
        self.size
    }

    pub fn root(&self) -> Hash {
        self.frontier
            .iter()
            .rev()
            .map(|(_, hash)| *hash)
            .reduce(|right, left| node_hash(&left, &right))
            .unwrap_or_else(empty_root)
    }

    pub fn store(&self) -> &S {
        &self.store
    }

    pub fn into_store(self) -> S {
        self.store
    }

    /// Append entries and commit them together; returns the index of the first.
    /// Nothing is visible to a reopened tree unless every tile was written.
    pub fn append_batch<E: AsRef<[u8]>>(&mut self, entries: &[E]) -> Result<u64, Error> {
        let first = self.size;
        if entries.is_empty() {
            return Ok(first);
        }
        if let Some(entry) = entries
            .iter()
            .find(|e| e.as_ref().len() > usize::from(u16::MAX))
        {
            return Err(Error::EntryTooLarge(entry.as_ref().len()));
        }
        // New stored hashes per (tile level, tile index), in position order.
        let mut hashes: BTreeMap<(u8, u64), Vec<Hash>> = BTreeMap::new();
        let mut size = self.size;
        let mut frontier = self.frontier.clone();
        for entry in entries {
            let mut hash = leaf_hash(entry.as_ref());
            let mut level = 0u8;
            hashes.entry((0, size / TILE_WIDTH)).or_default().push(hash);
            size += 1;
            while let Some(&(l, left)) = frontier.last() {
                if l != level {
                    break;
                }
                frontier.pop();
                hash = node_hash(&left, &hash);
                level += 1;
                if u32::from(level) % TILE_HEIGHT == 0 {
                    let position = (size >> level) - 1;
                    hashes
                        .entry((level / TILE_HEIGHT as u8, position / TILE_WIDTH))
                        .or_default()
                        .push(hash);
                }
            }
            frontier.push((level, hash));
        }
        for ((tile_level, tile), new) in hashes {
            // Hashes of this tile already committed: everything before the first new one.
            let stored = self.size >> (u32::from(tile_level) * TILE_HEIGHT);
            let committed = stored.saturating_sub(tile * TILE_WIDTH);
            let mut bytes = if committed > 0 {
                self.read_hash_tile(tile_level, tile, committed)?
            } else {
                Vec::new()
            };
            for hash in &new {
                bytes.extend_from_slice(hash);
            }
            let width = bytes.len() as u64 / 32;
            self.store
                .put(&hash_tile_path(tile_level, tile, width), &bytes)?;
        }
        let mut bundles: BTreeMap<u64, Vec<u8>> = BTreeMap::new();
        for (i, entry) in entries.iter().enumerate() {
            let bundle = (first + i as u64) / TILE_WIDTH;
            let bytes = bundles.entry(bundle).or_default();
            bytes.extend_from_slice(&(entry.as_ref().len() as u16).to_be_bytes());
            bytes.extend_from_slice(entry.as_ref());
        }
        for (bundle, new) in bundles {
            let committed = self.size.saturating_sub(bundle * TILE_WIDTH);
            let mut bytes = if committed > 0 {
                self.read_object(&entry_bundle_path(bundle, committed))?
            } else {
                Vec::new()
            };
            let before = count_entries(&bytes)?;
            bytes.extend_from_slice(&new);
            let width = count_entries(&bytes)?;
            debug_assert_eq!(before, committed);
            self.store.put(&entry_bundle_path(bundle, width), &bytes)?;
        }
        self.store.set_size(size)?;
        self.size = size;
        self.frontier = frontier;
        Ok(first)
    }

    /// The entry stored at `index`.
    pub fn entry(&self, index: u64) -> Result<Vec<u8>, Error> {
        if index >= self.size {
            return Err(Error::IndexOutOfRange {
                index,
                size: self.size,
            });
        }
        let bundle = index / TILE_WIDTH;
        let width = (self.size - bundle * TILE_WIDTH).min(TILE_WIDTH);
        let path = entry_bundle_path(bundle, width);
        let bytes = self.read_object(&path)?;
        let mut rest = bytes.as_slice();
        for position in 0..=(index % TILE_WIDTH) {
            let (len, tail) = rest
                .split_first_chunk::<2>()
                .ok_or_else(|| Error::CorruptTile(path.clone()))?;
            let len = usize::from(u16::from_be_bytes(*len));
            if tail.len() < len {
                return Err(Error::CorruptTile(path));
            }
            let (entry, tail) = tail.split_at(len);
            if position == index % TILE_WIDTH {
                return Ok(entry.to_vec());
            }
            rest = tail;
        }
        unreachable!("the loop returns at the requested position")
    }

    fn read_object(&self, path: &str) -> Result<Vec<u8>, Error> {
        self.store
            .get(path)?
            .ok_or_else(|| Error::MissingTile(path.to_string()))
    }

    fn read_hash_tile(&self, level: u8, index: u64, width: u64) -> Result<Vec<u8>, Error> {
        let path = hash_tile_path(level, index, width);
        let bytes = self.read_object(&path)?;
        if bytes.len() as u64 != width * 32 {
            return Err(Error::CorruptTile(path));
        }
        Ok(bytes)
    }
}

fn count_entries(mut bytes: &[u8]) -> Result<u64, Error> {
    let mut count = 0;
    while !bytes.is_empty() {
        let corrupt = || Error::CorruptTile("entry bundle".into());
        let (len, tail) = bytes.split_first_chunk::<2>().ok_or_else(corrupt)?;
        let len = usize::from(u16::from_be_bytes(*len));
        bytes = tail.get(len..).ok_or_else(corrupt)?;
        count += 1;
    }
    Ok(count)
}

impl<S: TileStore> HashSource for TileLog<S> {
    fn size(&self) -> u64 {
        self.size
    }

    fn node(&self, level: u8, index: u64) -> Result<Hash, Error> {
        let missing = Error::MissingNode { level, index };
        if level >= 64 {
            return Err(missing);
        }
        let tile_level = level / TILE_HEIGHT as u8;
        let sub = u32::from(level) % TILE_HEIGHT;
        let complete = self.size >> (u32::from(tile_level) * TILE_HEIGHT);
        let first = index.checked_mul(1 << sub).ok_or_else(|| missing.clone())?;
        if first.checked_add(1 << sub).is_none_or(|end| end > complete) {
            return Err(missing);
        }
        let tile = first / TILE_WIDTH;
        let width = (complete - tile * TILE_WIDTH).min(TILE_WIDTH);
        let bytes = self.read_hash_tile(tile_level, tile, width)?;
        let offset = (first % TILE_WIDTH) as usize;
        let mut nodes: Vec<Hash> = bytes[offset * 32..(offset + (1 << sub)) * 32]
            .chunks_exact(32)
            .map(|chunk| chunk.try_into().expect("chunks are 32 bytes"))
            .collect();
        while nodes.len() > 1 {
            nodes = nodes
                .chunks_exact(2)
                .map(|pair| node_hash(&pair[0], &pair[1]))
                .collect();
        }
        Ok(nodes[0])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transparency::merkle::{consistency_proof, inclusion_proof, tree_root, MemoryTree};

    fn entries(range: std::ops::Range<u64>) -> Vec<Vec<u8>> {
        range.map(|i| format!("entry {i}").into_bytes()).collect()
    }

    #[test]
    fn tile_paths_follow_the_layout() {
        assert_eq!(tile_index_path(0), "000");
        assert_eq!(tile_index_path(5), "005");
        assert_eq!(tile_index_path(999), "999");
        assert_eq!(tile_index_path(1000), "x001/000");
        assert_eq!(tile_index_path(1_234_067), "x001/x234/067");
        assert_eq!(hash_tile_path(0, 0, 256), "tile/0/000");
        assert_eq!(hash_tile_path(2, 1000, 7), "tile/2/x001/000.p/7");
        assert_eq!(entry_bundle_path(12, 255), "tile/entries/012.p/255");
        assert_eq!(entry_bundle_path(12, 256), "tile/entries/012");
    }

    #[test]
    fn tile_log_matches_the_memory_tree_across_batches() {
        let all = entries(0..700);
        let mut memory = MemoryTree::new();
        for e in &all {
            memory.append_entry(e);
        }
        let mut log = TileLog::open(MemoryTileStore::new()).unwrap();
        // Uneven batches that cross tile and bundle boundaries.
        let mut start = 0usize;
        for len in [1, 2, 253, 1, 255, 3, 100, 85] {
            let batch = &all[start..start + len];
            assert_eq!(log.append_batch(batch).unwrap(), start as u64);
            start += len;
            assert_eq!(log.size(), start as u64);
            assert_eq!(log.root(), tree_root(&memory, start as u64).unwrap());
            assert_eq!(log.root(), tree_root(&log, start as u64).unwrap());
        }
        assert_eq!(start, 700);
        for size in [1u64, 2, 255, 256, 257, 511, 512, 513, 699, 700] {
            for index in [0, size / 2, size - 1] {
                assert_eq!(
                    inclusion_proof(&log, index, size).unwrap(),
                    inclusion_proof(&memory, index, size).unwrap()
                );
            }
            assert_eq!(
                consistency_proof(&log, size / 2, size).unwrap(),
                consistency_proof(&memory, size / 2, size).unwrap()
            );
        }
        for index in [0, 1, 255, 256, 300, 511, 512, 699] {
            assert_eq!(log.entry(index).unwrap(), all[index as usize]);
        }
        assert!(log.entry(700).is_err());
    }

    #[test]
    fn stores_the_expected_files() {
        let mut log = TileLog::open(MemoryTileStore::new()).unwrap();
        log.append_batch(&entries(0..600)).unwrap();
        let paths: Vec<&str> = log.store().files().keys().map(String::as_str).collect();
        for expected in [
            "tile/0/000",
            "tile/0/001",
            "tile/0/002.p/88",
            "tile/1/000.p/2",
            "tile/entries/000",
            "tile/entries/001",
            "tile/entries/002.p/88",
        ] {
            assert!(
                paths.contains(&expected),
                "{expected} missing from {paths:?}"
            );
        }
        assert_eq!(log.store().files()["tile/0/000"].len(), 256 * 32);
        assert_eq!(log.store().files()["tile/1/000.p/2"].len(), 2 * 32);
    }

    #[test]
    fn reopening_continues_the_same_tree() {
        let all = entries(0..600);
        let mut memory = MemoryTree::new();
        for e in &all {
            memory.append_entry(e);
        }
        let mut log = TileLog::open(MemoryTileStore::new()).unwrap();
        log.append_batch(&all[..300]).unwrap();
        let store = log.into_store();
        let mut log = TileLog::open(store).unwrap();
        assert_eq!(log.size(), 300);
        assert_eq!(log.root(), tree_root(&memory, 300).unwrap());
        log.append_batch(&all[300..]).unwrap();
        assert_eq!(log.root(), memory.root());
    }

    #[test]
    fn a_crash_before_the_size_commit_leaves_the_old_tree() {
        struct Crashing(MemoryTileStore, bool);
        impl TileStore for Crashing {
            fn get(&self, path: &str) -> Result<Option<Vec<u8>>, Error> {
                self.0.get(path)
            }
            fn put(&mut self, path: &str, bytes: &[u8]) -> Result<(), Error> {
                self.0.put(path, bytes)
            }
            fn size(&self) -> Result<u64, Error> {
                self.0.size()
            }
            fn set_size(&mut self, size: u64) -> Result<(), Error> {
                if self.1 {
                    return Err(Error::Storage("crash".into()));
                }
                self.0.set_size(size)
            }
        }
        let all = entries(0..400);
        let mut memory = MemoryTree::new();
        for e in &all {
            memory.append_entry(e);
        }
        let mut log = TileLog::open(Crashing(MemoryTileStore::new(), false)).unwrap();
        log.append_batch(&all[..100]).unwrap();
        let before = log.root();
        log.store.1 = true;
        assert!(log.append_batch(&all[100..300]).is_err());
        assert_eq!((log.size(), log.root()), (100, before));
        // A reopened tree sees only the committed 100 entries and can carry on,
        // overwriting the stray tiles of the failed batch.
        let mut store = log.into_store();
        store.1 = false;
        let mut log = TileLog::open(store).unwrap();
        assert_eq!(log.root(), before);
        log.append_batch(&all[100..]).unwrap();
        assert_eq!(log.root(), memory.root());
        assert_eq!(log.entry(399).unwrap(), all[399]);
    }

    #[test]
    fn corrupt_or_missing_tiles_are_errors() {
        let mut log = TileLog::open(MemoryTileStore::new()).unwrap();
        log.append_batch(&entries(0..10)).unwrap();
        let mut store = log.into_store();
        let path = hash_tile_path(0, 0, 10);
        let mut bytes = store.get(&path).unwrap().unwrap();
        bytes.pop();
        store.put(&path, &bytes).unwrap();
        assert!(matches!(
            TileLog::open(store.clone()),
            Err(Error::CorruptTile(_))
        ));
        store.files.remove(&path);
        assert!(matches!(TileLog::open(store), Err(Error::MissingTile(_))));
    }

    #[test]
    fn oversized_entries_are_refused_before_anything_is_written() {
        let mut log = TileLog::open(MemoryTileStore::new()).unwrap();
        let big = vec![0u8; 70_000];
        assert_eq!(
            log.append_batch(&[b"ok".to_vec(), big]),
            Err(Error::EntryTooLarge(70_000))
        );
        assert_eq!(log.size(), 0);
        assert!(log.store().files().is_empty());
    }

    #[test]
    fn filesystem_store_survives_a_restart() {
        let dir = std::env::temp_dir().join(format!("dynodoc-tiles-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let all = entries(0..300);
        let mut memory = MemoryTree::new();
        for e in &all {
            memory.append_entry(e);
        }
        let mut log = TileLog::open(FsTileStore::open(&dir).unwrap()).unwrap();
        log.append_batch(&all[..257]).unwrap();
        drop(log);
        let mut log = TileLog::open(FsTileStore::open(&dir).unwrap()).unwrap();
        assert_eq!(log.size(), 257);
        assert_eq!(log.root(), tree_root(&memory, 257).unwrap());
        log.append_batch(&all[257..]).unwrap();
        assert_eq!(log.root(), memory.root());
        assert_eq!(log.entry(299).unwrap(), all[299]);
        assert!(dir
            .join("tile")
            .join("0")
            .join("001.p")
            .join("44")
            .is_file());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn filesystem_store_refuses_paths_outside_its_root() {
        let dir = std::env::temp_dir().join(format!("dynodoc-tiles-safe-{}", std::process::id()));
        let mut store = FsTileStore::open(&dir).unwrap();
        for bad in ["../x", "/abs", "a/../b", "a\\b", "c:x", ""] {
            assert!(store.put(bad, b"x").is_err(), "{bad:?}");
            assert!(store.get(bad).is_err(), "{bad:?}");
        }
        fs::remove_dir_all(&dir).unwrap();
    }
}
