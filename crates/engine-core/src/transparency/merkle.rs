//! RFC 6962 Merkle tree: hashing, tree heads, and inclusion / consistency proofs.
//!
//! Leaf hash `SHA-256(0x00 || entry)`, node hash `SHA-256(0x01 || left || right)`,
//! empty tree `SHA-256("")`, left subtree of size `n` = largest power of two below
//! `n` (RFC 6962 section 2.1). Proof generation follows the recursive `PATH` and
//! `SUBPROOF` definitions of RFC 6962; verification follows the iterative
//! algorithms of RFC 9162 sections 2.1.3.2 and 2.1.4.2, which describe the same
//! hash construction (RFC 9162 is Experimental; only the construction is relied
//! on). Both texts were reproduced from memory, not re-read, while this was
//! written (unverified: check at build time); the eight published RFC 6962 test
//! roots in the tests below and the independent Python vectors guard the result.
use ring::digest::{Context, SHA256};

use super::Error;

pub type Hash = [u8; 32];

/// Longest proof any tree of up to `u64::MAX` leaves can need (plus slack).
const MAX_PROOF_LEN: usize = 65;

fn sha256(parts: &[&[u8]]) -> Hash {
    let mut context = Context::new(&SHA256);
    for part in parts {
        context.update(part);
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(context.finish().as_ref());
    out
}

/// Root of the empty tree: `SHA-256("")`.
pub fn empty_root() -> Hash {
    sha256(&[])
}

/// RFC 6962 leaf hash of an entry.
pub fn leaf_hash(entry: &[u8]) -> Hash {
    sha256(&[&[0], entry])
}

/// RFC 6962 interior node hash.
pub fn node_hash(left: &Hash, right: &Hash) -> Hash {
    sha256(&[&[1], left, right])
}

/// Largest power of two strictly below `n`, for `n >= 2`.
fn split(n: u64) -> u64 {
    1u64 << (63 - (n - 1).leading_zeros())
}

/// Read access to the stored hashes of a tree.
pub trait HashSource {
    /// Number of leaves stored.
    fn size(&self) -> u64;
    /// Hash of the complete subtree of `2^level` leaves at position `index`, i.e.
    /// the subtree over leaves `index * 2^level .. (index + 1) * 2^level`.
    fn node(&self, level: u8, index: u64) -> Result<Hash, Error>;
}

fn range_hash<S: HashSource + ?Sized>(source: &S, lo: u64, n: u64) -> Result<Hash, Error> {
    if n.is_power_of_two() && lo % n == 0 {
        return source.node(n.trailing_zeros() as u8, lo / n);
    }
    let k = split(n);
    Ok(node_hash(
        &range_hash(source, lo, k)?,
        &range_hash(source, lo + k, n - k)?,
    ))
}

fn check_size<S: HashSource + ?Sized>(source: &S, size: u64) -> Result<(), Error> {
    if size > source.size() {
        return Err(Error::SizeOutOfRange {
            size,
            max: source.size(),
        });
    }
    Ok(())
}

/// Tree head of the first `size` leaves.
pub fn tree_root<S: HashSource + ?Sized>(source: &S, size: u64) -> Result<Hash, Error> {
    check_size(source, size)?;
    if size == 0 {
        return Ok(empty_root());
    }
    range_hash(source, 0, size)
}

/// Audit path of leaf `index` in the tree of the first `size` leaves, leaf level
/// first (RFC 6962 `PATH`).
pub fn inclusion_proof<S: HashSource + ?Sized>(
    source: &S,
    index: u64,
    size: u64,
) -> Result<Vec<Hash>, Error> {
    check_size(source, size)?;
    if index >= size {
        return Err(Error::IndexOutOfRange { index, size });
    }
    let (mut lo, mut n, mut m) = (0, size, index);
    let mut path = Vec::new();
    while n > 1 {
        let k = split(n);
        if m < k {
            path.push(range_hash(source, lo + k, n - k)?);
            n = k;
        } else {
            path.push(range_hash(source, lo, k)?);
            lo += k;
            m -= k;
            n -= k;
        }
    }
    path.reverse();
    Ok(path)
}

/// Proof that the tree of the first `first` leaves is a prefix of the tree of the
/// first `second` leaves (RFC 6962 `PROOF` / `SUBPROOF`). Empty when `first` is 0
/// or equals `second`.
pub fn consistency_proof<S: HashSource + ?Sized>(
    source: &S,
    first: u64,
    second: u64,
) -> Result<Vec<Hash>, Error> {
    check_size(source, second)?;
    if first > second {
        return Err(Error::SizeOutOfRange {
            size: first,
            max: second,
        });
    }
    if first == 0 || first == second {
        return Ok(Vec::new());
    }
    let (mut lo, mut n, mut m, mut complete) = (0, second, first, true);
    let mut proof = Vec::new();
    loop {
        if m == n {
            if !complete {
                proof.push(range_hash(source, lo, n)?);
            }
            break;
        }
        let k = split(n);
        if m <= k {
            proof.push(range_hash(source, lo + k, n - k)?);
            n = k;
        } else {
            proof.push(range_hash(source, lo, k)?);
            lo += k;
            m -= k;
            n -= k;
            complete = false;
        }
    }
    proof.reverse();
    Ok(proof)
}

/// Check that `leaf` sits at `index` of the tree of `size` leaves with head `root`
/// (RFC 9162 section 2.1.3.2).
pub fn verify_inclusion(
    leaf: &Hash,
    index: u64,
    size: u64,
    proof: &[Hash],
    root: &Hash,
) -> Result<(), Error> {
    if index >= size {
        return Err(Error::IndexOutOfRange { index, size });
    }
    if proof.len() > MAX_PROOF_LEN {
        return Err(Error::BadInclusionProof);
    }
    let (mut fnode, mut snode) = (index, size - 1);
    let mut hash = *leaf;
    for sibling in proof {
        if snode == 0 {
            return Err(Error::BadInclusionProof);
        }
        if fnode & 1 == 1 || fnode == snode {
            hash = node_hash(sibling, &hash);
            while fnode & 1 == 0 && fnode != 0 {
                fnode >>= 1;
                snode >>= 1;
            }
        } else {
            hash = node_hash(&hash, sibling);
        }
        fnode >>= 1;
        snode >>= 1;
    }
    if snode == 0 && hash == *root {
        Ok(())
    } else {
        Err(Error::BadInclusionProof)
    }
}

/// Check that the tree of `first` leaves with head `first_root` is a prefix of the
/// tree of `second` leaves with head `second_root` (RFC 9162 section 2.1.4.2). A
/// tree of size 0 is a prefix of every tree.
pub fn verify_consistency(
    first: u64,
    second: u64,
    first_root: &Hash,
    second_root: &Hash,
    proof: &[Hash],
) -> Result<(), Error> {
    let bad = Err(Error::BadConsistencyProof);
    if first > second || proof.len() > MAX_PROOF_LEN {
        return bad;
    }
    if first == second {
        return if proof.is_empty() && first_root == second_root {
            Ok(())
        } else {
            bad
        };
    }
    if first == 0 {
        return if proof.is_empty() && *first_root == empty_root() {
            Ok(())
        } else {
            bad
        };
    }
    let mut path = Vec::with_capacity(proof.len() + 1);
    if first.is_power_of_two() {
        path.push(*first_root);
    }
    path.extend_from_slice(proof);
    let Some((start, rest)) = path.split_first() else {
        return bad;
    };
    let (mut fnode, mut snode) = (first - 1, second - 1);
    while fnode & 1 == 1 {
        fnode >>= 1;
        snode >>= 1;
    }
    let (mut first_hash, mut second_hash) = (*start, *start);
    for sibling in rest {
        if snode == 0 {
            return bad;
        }
        if fnode & 1 == 1 || fnode == snode {
            first_hash = node_hash(sibling, &first_hash);
            second_hash = node_hash(sibling, &second_hash);
            while fnode & 1 == 0 && fnode != 0 {
                fnode >>= 1;
                snode >>= 1;
            }
        } else {
            second_hash = node_hash(&second_hash, sibling);
        }
        fnode >>= 1;
        snode >>= 1;
    }
    if snode == 0 && first_hash == *first_root && second_hash == *second_root {
        Ok(())
    } else {
        bad
    }
}

/// A tree kept entirely in memory: every complete subtree hash, level by level.
/// Used for tests and for callers holding all leaves; the tile store is the
/// durable form.
#[derive(Clone, Debug, Default)]
pub struct MemoryTree {
    levels: Vec<Vec<Hash>>,
}

impl MemoryTree {
    pub fn new() -> Self {
        Self::default()
    }

    /// Append an entry (hashing it as a leaf); returns its index.
    pub fn append_entry(&mut self, entry: &[u8]) -> u64 {
        self.append_leaf_hash(leaf_hash(entry))
    }

    /// Append an already hashed leaf; returns its index.
    pub fn append_leaf_hash(&mut self, hash: Hash) -> u64 {
        if self.levels.is_empty() {
            self.levels.push(Vec::new());
        }
        self.levels[0].push(hash);
        let index = self.levels[0].len() as u64 - 1;
        let mut level = 0;
        while self.levels[level].len() % 2 == 0 {
            let len = self.levels[level].len();
            let parent = node_hash(&self.levels[level][len - 2], &self.levels[level][len - 1]);
            if self.levels.len() == level + 1 {
                self.levels.push(Vec::new());
            }
            self.levels[level + 1].push(parent);
            level += 1;
        }
        index
    }

    pub fn root(&self) -> Hash {
        tree_root(self, self.size()).expect("the full tree is stored")
    }
}

impl HashSource for MemoryTree {
    fn size(&self) -> u64 {
        self.levels.first().map_or(0, |leaves| leaves.len() as u64)
    }

    fn node(&self, level: u8, index: u64) -> Result<Hash, Error> {
        usize::try_from(index)
            .ok()
            .and_then(|i| self.levels.get(usize::from(level))?.get(i))
            .copied()
            .ok_or(Error::MissingNode { level, index })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transparency::encoding::hex;

    fn tree(entries: &[Vec<u8>]) -> MemoryTree {
        let mut tree = MemoryTree::new();
        for entry in entries {
            tree.append_entry(entry);
        }
        tree
    }

    fn synthetic(n: u64) -> Vec<Vec<u8>> {
        (0..n).map(|i| format!("entry {i}").into_bytes()).collect()
    }

    /// Certificate Transparency's published test data: leaves "", 00, 10, 2021,
    /// 3031, 40..43, 50..57, 60..6f and the roots of their first n leaves. The
    /// roots were recalled from memory and re-derived by the Python generator
    /// (`gen_transparency_vectors.py`) from the RFC 6962 definitions; a wrong
    /// memory of any value fails here, it cannot pass by accident.
    #[test]
    fn rfc6962_published_roots() {
        let leaves: Vec<Vec<u8>> = vec![
            vec![],
            vec![0x00],
            vec![0x10],
            vec![0x20, 0x21],
            vec![0x30, 0x31],
            vec![0x40, 0x41, 0x42, 0x43],
            (0x50..0x58).collect(),
            (0x60..0x70).collect(),
        ];
        let roots = [
            "6e340b9cffb37a989ca544e6bb780a2c78901d3fb33738768511a30617afa01d",
            "fac54203e7cc696cf0dfcb42c92a1d9dbaf70ad9e621f4bd8d98662f00e3c125",
            "aeb6bcfe274b70a14fb067a5e5578264db0fa9b51af5e0ba159158f329e06e77",
            "d37ee418976dd95753c1c73862b9398fa2a2cf9b4ff0fdfe8b30cd95209614b7",
            "4e3bbb1f7b478dcfe71fb631631519a3bca12c9aefca1612bfce4c13a86264d4",
            "76e67dadbcdf1e10e1b74ddc608abd2f98dfb16fbce75277b5232a127f2087ef",
            "ddb89be403809e325750d3d263cd78929c2942b7942a34b77e122c9594a74c8c",
            "5dc9da79a70659a9ad559cb701ded9a2ab9d823aad2f4960cfe370eff4604328",
        ];
        let tree = tree(&leaves);
        for (i, root) in roots.iter().enumerate() {
            assert_eq!(hex(&tree_root(&tree, i as u64 + 1).unwrap()), *root);
        }
        assert_eq!(
            hex(&empty_root()),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(tree.root(), tree_root(&tree, 8).unwrap());
    }

    #[test]
    fn every_inclusion_proof_verifies_up_to_70_leaves() {
        let tree = tree(&synthetic(70));
        for size in 1..=70u64 {
            let root = tree_root(&tree, size).unwrap();
            for index in 0..size {
                let proof = inclusion_proof(&tree, index, size).unwrap();
                let leaf = tree.node(0, index).unwrap();
                verify_inclusion(&leaf, index, size, &proof, &root)
                    .unwrap_or_else(|e| panic!("size {size} index {index}: {e}"));
            }
        }
    }

    #[test]
    fn every_consistency_proof_verifies_up_to_70_leaves() {
        let tree = tree(&synthetic(70));
        for second in 1..=70u64 {
            let second_root = tree_root(&tree, second).unwrap();
            for first in 0..=second {
                let proof = consistency_proof(&tree, first, second).unwrap();
                let first_root = tree_root(&tree, first).unwrap();
                verify_consistency(first, second, &first_root, &second_root, &proof)
                    .unwrap_or_else(|e| panic!("{first} -> {second}: {e}"));
            }
        }
    }

    #[test]
    fn proof_lengths_are_logarithmic() {
        let tree = tree(&synthetic(1000));
        let proof = inclusion_proof(&tree, 500, 1000).unwrap();
        assert!(proof.len() <= 10);
        assert!(consistency_proof(&tree, 300, 1000).unwrap().len() <= 20);
        assert!(inclusion_proof(&tree, 0, 1).unwrap().is_empty());
    }

    #[test]
    fn out_of_range_requests_are_rejected() {
        let tree = tree(&synthetic(5));
        assert_eq!(
            inclusion_proof(&tree, 5, 5),
            Err(Error::IndexOutOfRange { index: 5, size: 5 })
        );
        assert!(matches!(
            inclusion_proof(&tree, 0, 6),
            Err(Error::SizeOutOfRange { .. })
        ));
        assert!(matches!(
            consistency_proof(&tree, 3, 6),
            Err(Error::SizeOutOfRange { .. })
        ));
        assert!(matches!(
            consistency_proof(&tree, 4, 3),
            Err(Error::SizeOutOfRange { .. })
        ));
        assert!(matches!(
            tree_root(&tree, 6),
            Err(Error::SizeOutOfRange { .. })
        ));
        let leaf = tree.node(0, 0).unwrap();
        let root = tree.root();
        assert_eq!(
            verify_inclusion(&leaf, 5, 5, &[], &root),
            Err(Error::IndexOutOfRange { index: 5, size: 5 })
        );
    }

    #[test]
    fn inclusion_proofs_reject_wrong_shape_and_content() {
        let tree = tree(&synthetic(14));
        let size = 13;
        let root = tree_root(&tree, size).unwrap();
        let index = 6;
        let leaf = tree.node(0, index).unwrap();
        let proof = inclusion_proof(&tree, index, size).unwrap();
        verify_inclusion(&leaf, index, size, &proof, &root).unwrap();
        // Truncated, extended, reordered, wrong index, wrong size, wrong leaf.
        let mut short = proof.clone();
        short.pop();
        assert!(verify_inclusion(&leaf, index, size, &short, &root).is_err());
        let mut long = proof.clone();
        long.push([0; 32]);
        assert!(verify_inclusion(&leaf, index, size, &long, &root).is_err());
        let mut swapped = proof.clone();
        swapped.swap(0, 1);
        assert!(verify_inclusion(&leaf, index, size, &swapped, &root).is_err());
        assert!(verify_inclusion(&leaf, index + 1, size, &proof, &root).is_err());
        // The path is only good for the root of the size it was made for.
        assert!(verify_inclusion(&leaf, index, size + 1, &proof, &tree.root()).is_err());
        assert!(verify_inclusion(&leaf_hash(b"other"), index, size, &proof, &root).is_err());
        assert!(verify_inclusion(&leaf, index, size, &[], &root).is_err());
        let oversized = vec![[0u8; 32]; MAX_PROOF_LEN + 1];
        assert!(verify_inclusion(&leaf, index, size, &oversized, &root).is_err());
    }

    #[test]
    fn consistency_proofs_reject_wrong_shape_and_content() {
        let tree = tree(&synthetic(22));
        let (first, second) = (6, 21);
        let (r1, r2) = (
            tree_root(&tree, first).unwrap(),
            tree_root(&tree, second).unwrap(),
        );
        let proof = consistency_proof(&tree, first, second).unwrap();
        verify_consistency(first, second, &r1, &r2, &proof).unwrap();
        let mut short = proof.clone();
        short.pop();
        assert!(verify_consistency(first, second, &r1, &r2, &short).is_err());
        let mut long = proof.clone();
        long.push([0; 32]);
        assert!(verify_consistency(first, second, &r1, &r2, &long).is_err());
        assert!(verify_consistency(first, second, &r2, &r1, &proof).is_err());
        assert!(verify_consistency(first + 1, second, &r1, &r2, &proof).is_err());
        let r_before = tree_root(&tree, second - 1).unwrap();
        assert!(verify_consistency(first, second - 1, &r1, &r_before, &proof).is_err());
        assert!(verify_consistency(first, second + 1, &r1, &tree.root(), &proof).is_err());
        assert!(verify_consistency(second, first, &r2, &r1, &proof).is_err());
        // Same size needs equal roots and no proof; size 0 needs the empty root.
        assert!(verify_consistency(6, 6, &r1, &r2, &[]).is_err());
        assert!(verify_consistency(6, 6, &r1, &r1, &[]).is_ok());
        assert!(verify_consistency(6, 6, &r1, &r1, &proof).is_err());
        assert!(verify_consistency(0, 6, &empty_root(), &r1, &[]).is_ok());
        assert!(verify_consistency(0, 6, &r1, &r1, &[]).is_err());
        assert!(verify_consistency(0, 6, &empty_root(), &r1, &proof).is_err());
    }

    #[test]
    fn missing_nodes_are_reported() {
        let tree = tree(&synthetic(3));
        assert_eq!(
            tree.node(1, 1),
            Err(Error::MissingNode { level: 1, index: 1 })
        );
        assert_eq!(
            MemoryTree::new().node(0, 0),
            Err(Error::MissingNode { level: 0, index: 0 })
        );
        assert_eq!(MemoryTree::new().root(), empty_root());
    }
}
