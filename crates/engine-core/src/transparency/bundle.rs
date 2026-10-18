//! The inclusion bundle (design 4.4): a leaf, its position, the audit path and the
//! signed checkpoint note it folds to. Verification of the signatures against a
//! trust policy is [`super::assurance`]; this module is the typed value and its
//! canonical JSON.
use serde::Deserialize;
use serde_json::{json, Value};

use super::encoding::{hex, unhex_array};
use super::leaf::Leaf;
use super::merkle::{verify_inclusion, Hash};
use super::Error;

/// The only bundle version defined.
pub const BUNDLE_VERSION: u64 = 1;
/// A tree of `u64::MAX` leaves needs at most 64 path hashes.
const MAX_PATH: usize = 64;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InclusionBundle {
    pub leaf: Leaf,
    pub index: u64,
    /// Tree size the path is for; it must be the size of the checkpoint.
    pub size: u64,
    /// Sibling hashes, leaf level first.
    pub path: Vec<Hash>,
    /// The signed checkpoint note, with cosignatures, as text.
    pub checkpoint: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Wire {
    version: Value,
    leaf: Value,
    index: u64,
    size: u64,
    path: Vec<String>,
    checkpoint: String,
}

impl InclusionBundle {
    /// Canonical JSON: sorted keys, no whitespace.
    pub fn to_json(&self) -> Vec<u8> {
        crate::log::canonical_json(&json!({
            "version": BUNDLE_VERSION,
            "leaf": self.leaf.to_value(),
            "index": self.index,
            "size": self.size,
            "path": self.path.iter().map(|h| hex(h)).collect::<Vec<_>>(),
            "checkpoint": self.checkpoint,
        }))
    }

    /// Parse a bundle. Whitespace is tolerated; unknown, missing or duplicate
    /// top-level fields, another version, malformed hex or an unknown leaf are not.
    /// (A key repeated inside the `leaf` object is not detected: the last one wins,
    /// and the leaf hash is over the typed value, so the proof still binds it.)
    pub fn from_json(bytes: &[u8]) -> Result<Self, Error> {
        let wire: Wire = serde_json::from_slice(bytes).map_err(|_| {
            Error::InvalidBundle("not a bundle object with exactly the defined fields")
        })?;
        match wire.version.as_u64() {
            Some(BUNDLE_VERSION) => {}
            Some(other) => return Err(Error::BundleVersion(other)),
            None => return Err(Error::InvalidBundle("version is an unsigned integer")),
        }
        if wire.path.len() > MAX_PATH {
            return Err(Error::InvalidBundle("path is longer than any tree needs"));
        }
        let path = wire
            .path
            .iter()
            .map(|h| unhex_array(h))
            .collect::<Result<_, _>>()
            .map_err(|_| Error::InvalidBundle("path entries are 64 lowercase hex digits"))?;
        Ok(Self {
            leaf: Leaf::from_value(&wire.leaf)?,
            index: wire.index,
            size: wire.size,
            path,
            checkpoint: wire.checkpoint,
        })
    }

    /// Recompute the leaf hash and fold the path for `(index, size)`; the result
    /// must equal `root`.
    pub fn verify_path(&self, root: &Hash) -> Result<(), Error> {
        verify_inclusion(&self.leaf.hash()?, self.index, self.size, &self.path, root)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transparency::merkle::{inclusion_proof, MemoryTree};

    fn bundle() -> (InclusionBundle, Hash) {
        let mut tree = MemoryTree::new();
        let mut leaves = Vec::new();
        for i in 0..13u8 {
            let leaf = Leaf::RefReceipt {
                tag: [i; 32],
                receipt: [i + 100; 32],
            };
            tree.append_entry(&leaf.entry().unwrap());
            leaves.push(leaf);
        }
        let bundle = InclusionBundle {
            leaf: leaves[5].clone(),
            index: 5,
            size: 13,
            path: inclusion_proof(&tree, 5, 13).unwrap(),
            checkpoint: "origin\n13\nroot\n\n\u{2014} name AAAAAAAA\n".into(),
        };
        (bundle, tree.root())
    }

    #[test]
    fn canonical_json_round_trips_and_verifies() {
        let (bundle, root) = bundle();
        bundle.verify_path(&root).unwrap();
        let json = bundle.to_json();
        let text = String::from_utf8(json.clone()).unwrap();
        assert!(text.starts_with(r#"{"checkpoint":"origin\n13\nroot\n\n"#));
        assert!(!text.contains(' ') || text.contains("\u{2014} name"));
        assert_eq!(InclusionBundle::from_json(&json).unwrap(), bundle);
        // Pretty-printed input parses to the same value.
        let pretty =
            serde_json::to_vec_pretty(&serde_json::from_slice::<Value>(&json).unwrap()).unwrap();
        assert_eq!(InclusionBundle::from_json(&pretty).unwrap(), bundle);
    }

    #[test]
    fn wrong_path_leaf_index_or_root_fails() {
        let (bundle, root) = bundle();
        let mut wrong = bundle.clone();
        wrong.path[0][0] ^= 1;
        assert!(wrong.verify_path(&root).is_err());
        let mut wrong = bundle.clone();
        wrong.index = 4;
        assert!(wrong.verify_path(&root).is_err());
        let mut wrong = bundle.clone();
        wrong.leaf = Leaf::RefReceipt {
            tag: [5; 32],
            receipt: [0; 32],
        };
        assert!(wrong.verify_path(&root).is_err());
        let mut wrong_root = root;
        wrong_root[31] ^= 1;
        assert!(bundle.verify_path(&wrong_root).is_err());
        let mut wrong = bundle;
        wrong.index = 13;
        assert!(matches!(
            wrong.verify_path(&root),
            Err(Error::IndexOutOfRange { .. })
        ));
    }

    #[test]
    fn malformed_bundles_are_rejected() {
        let (bundle, _) = bundle();
        let good: Value = serde_json::from_slice(&bundle.to_json()).unwrap();
        let parse = |v: &Value| InclusionBundle::from_json(&serde_json::to_vec(v).unwrap());
        assert!(parse(&good).is_ok());
        let with = |key: &str, value: Value| {
            let mut v = good.clone();
            v[key] = value;
            v
        };
        let without = |key: &str| {
            let mut v = good.clone();
            v.as_object_mut().unwrap().remove(key);
            v
        };
        assert_eq!(
            parse(&with("version", json!(2))),
            Err(Error::BundleVersion(2))
        );
        assert_eq!(
            parse(&with("version", json!(0))),
            Err(Error::BundleVersion(0))
        );
        for bad in [
            with("version", json!("1")),
            with("extra", json!(1)),
            with("index", json!(-1)),
            with("size", json!("13")),
            with("path", json!(["zz"])),
            with("path", json!([hex(&[0xab; 32]).to_uppercase()])),
            with("path", json!(["00"])),
            with("path", json!(vec![hex(&[1; 32]); 65])),
            with("checkpoint", json!(5)),
            with("leaf", json!({"t": "ref"})),
            without("version"),
            without("leaf"),
            without("index"),
            without("size"),
            without("path"),
            without("checkpoint"),
        ] {
            assert!(parse(&bad).is_err(), "{bad}");
        }
        assert!(InclusionBundle::from_json(b"[]").is_err());
        assert!(InclusionBundle::from_json(b"").is_err());
        let duplicated = br#"{"version":1,"version":1}"#;
        assert!(InclusionBundle::from_json(duplicated).is_err());
    }
}
