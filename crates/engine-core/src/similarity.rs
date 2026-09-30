//! Content fingerprints for recognising copies of a document, even when a file is
//! renamed, re-saved or partly edited.
//!
//! Three independent signals, cheapest first:
//!
//! 1. `content_hash`: the normalised text of every block, order-insensitive. Equal
//!    hashes mean identical content, whatever the file name.
//! 2. `blocks`: hashes of substantive blocks (paragraphs, spreadsheet rows, slide text
//!    boxes). Shared blocks measure containment: how much of one file appears
//!    verbatim in another. This catches excerpts and files with added material, which
//!    whole-document similarity undervalues.
//! 3. `signature`: a 128-value MinHash of word 3-shingles taken within blocks, which
//!    estimates the Jaccard similarity of the wording (Broder 1997), so edited copies
//!    still match. It uses one-permutation hashing with optimal densification (Li, Owen
//!    & Zhang 2012; Shrivastava 2017): one hash per shingle instead of 128, with the
//!    same estimator. `bands` are locality-sensitive hashing keys (32 bands of 4 rows,
//!    Indyk & Motwani 1998), so candidates come from indexed equality lookups rather
//!    than comparing a new file with every stored document.
//!
//! Shingles never cross block boundaries, so moving paragraphs, rows or slides does
//! not change a fingerprint. Everything is deterministic across processes and
//! platforms, so stored fingerprints stay comparable: changing a constant or the
//! normalisation requires re-indexing (bump [`VERSION`]).
use crate::materializer::DocumentState;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Stored fingerprints with a different version are recomputed.
pub const VERSION: i16 = 1;
/// MinHash signature length; a power of two so the bin is the hash's top bits.
pub const SIGNATURE_LEN: usize = 128;
const BIN_BITS: u32 = 7;
/// Locality-sensitive hashing layout: `BANDS` × `ROWS` = `SIGNATURE_LEN`. A pair
/// becomes a candidate with probability 1 − (1 − J⁴)³², about 0.87 at J = 0.5 and
/// above 0.99 from J = 0.6.
pub const BANDS: usize = 32;
const ROWS: usize = SIGNATURE_LEN / BANDS;
/// Blocks with fewer words ("Yes", "Sheet1", a heading) are too common to count as
/// evidence of copying on their own; they still contribute shingles.
pub const SUBSTANTIVE_WORDS: usize = 3;
/// At most this many block hashes are stored per document.
pub const MAX_BLOCKS: usize = 20_000;
/// Documents with fewer distinct shingles are too small to compare reliably.
pub const MIN_SHINGLES: usize = 12;

const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

/// SplitMix64 finaliser: turns FNV word hashes into well-distributed 64-bit values.
#[inline]
fn mix(mut z: u64) -> u64 {
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

enum CharClass {
    Word,
    Separator,
    /// Joiners and byte-order marks shape Indic script conjuncts; they never split words.
    Ignored,
}
fn class(c: char) -> CharClass {
    if c.is_ascii() {
        return if c.is_ascii_alphanumeric() {
            CharClass::Word
        } else {
            CharClass::Separator
        };
    }
    match c {
        '\u{200C}' | '\u{200D}' | '\u{FEFF}' | '\u{00AD}' => CharClass::Ignored,
        '\u{200B}' | '\u{00A0}' | '।' | '॥' | '«' | '»' | '¡' | '¿' | '·' | '§' | '¶' => {
            CharClass::Separator
        }
        // General punctuation: dashes, curly quotes, bullets, ellipsis, primes.
        '\u{2010}'..='\u{2027}' | '\u{2030}'..='\u{205E}' | '\u{3000}'..='\u{3003}' => {
            CharClass::Separator
        }
        c if c.is_whitespace() => CharClass::Separator,
        _ => CharClass::Word,
    }
}

/// Case-folded word hashes of one block, ignoring punctuation and spacing.
pub fn word_hashes(text: &str, out: &mut Vec<u64>) {
    let mut hash = FNV_OFFSET;
    let mut len = 0usize;
    for c in text.chars() {
        match class(c) {
            CharClass::Ignored => {}
            CharClass::Separator => {
                if len > 0 {
                    out.push(mix(hash));
                    hash = FNV_OFFSET;
                    len = 0;
                }
            }
            CharClass::Word if c.is_ascii() => {
                hash ^= u64::from(c.to_ascii_lowercase() as u8);
                hash = hash.wrapping_mul(FNV_PRIME);
                len += 1;
            }
            CharClass::Word => {
                for lower in c.to_lowercase() {
                    let mut buf = [0u8; 4];
                    for byte in lower.encode_utf8(&mut buf).bytes() {
                        hash ^= u64::from(byte);
                        hash = hash.wrapping_mul(FNV_PRIME);
                    }
                }
                len += 1;
            }
        }
    }
    if len > 0 {
        out.push(mix(hash));
    }
}

fn sequence_hash(words: &[u64]) -> u64 {
    let mut hash = 0x243f_6a88_85a3_08d3 ^ words.len() as u64;
    for &word in words {
        hash = mix(hash.wrapping_mul(0x9e37_79b9_7f4a_7c15) ^ word);
    }
    hash
}

/// The fingerprint of one document state or uploaded file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Fingerprint {
    /// MinHash values; empty when the content has no words.
    pub signature: Vec<u32>,
    /// One locality-sensitive key per band; empty with the signature.
    pub bands: Vec<i64>,
    /// Sorted, distinct hashes of substantive blocks.
    pub blocks: Vec<i64>,
    /// Order-insensitive hash of all non-empty blocks; 0 for empty content.
    pub content_hash: i64,
    pub block_count: u32,
    pub word_count: u32,
    pub shingle_count: u32,
}

impl Fingerprint {
    /// Build a fingerprint from block texts in any order.
    pub fn from_blocks<'a>(blocks: impl IntoIterator<Item = &'a str>) -> Self {
        let mut words = Vec::with_capacity(64);
        let mut shingles: Vec<u64> = Vec::new();
        let mut substantive: Vec<i64> = Vec::new();
        let mut every_block: Vec<u64> = Vec::new();
        let mut word_count = 0usize;
        for text in blocks {
            words.clear();
            word_hashes(text, &mut words);
            if words.is_empty() {
                continue;
            }
            word_count += words.len();
            let block = sequence_hash(&words);
            every_block.push(block);
            if words.len() >= SUBSTANTIVE_WORDS {
                substantive.push(block as i64);
            }
            if words.len() < 3 {
                shingles.push(block);
            } else {
                shingles.extend(words.windows(3).map(|w| {
                    mix(w[0] ^ w[1].rotate_left(21) ^ w[2].rotate_left(42) ^ 0x5851_f42d_4c95_7f2d)
                }));
            }
        }
        shingles.sort_unstable();
        shingles.dedup();
        substantive.sort_unstable();
        substantive.dedup();
        substantive.truncate(MAX_BLOCKS);
        every_block.sort_unstable();
        let content_hash = if every_block.is_empty() {
            0
        } else {
            sequence_hash(&every_block) as i64
        };
        let signature = minhash(&shingles);
        let bands = band_keys(&signature);
        Fingerprint {
            signature,
            bands,
            blocks: substantive,
            content_hash,
            block_count: every_block.len() as u32,
            word_count: word_count as u32,
            shingle_count: shingles.len() as u32,
        }
    }
    /// Too little content to say anything about copying.
    pub fn is_trivial(&self) -> bool {
        (self.shingle_count as usize) < MIN_SHINGLES
    }
    /// The signature as little-endian bytes for storage.
    pub fn signature_bytes(&self) -> Vec<u8> {
        self.signature
            .iter()
            .flat_map(|v| v.to_le_bytes())
            .collect()
    }
    pub fn signature_from_bytes(bytes: &[u8]) -> Vec<u32> {
        bytes
            .chunks_exact(4)
            .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect()
    }
}

/// One-permutation MinHash with optimal densification.
fn minhash(shingles: &[u64]) -> Vec<u32> {
    if shingles.is_empty() {
        return Vec::new();
    }
    let mut bins = [u32::MAX; SIGNATURE_LEN];
    let mut filled = [false; SIGNATURE_LEN];
    for &shingle in shingles {
        let bin = (shingle >> (64 - BIN_BITS)) as usize;
        let value = shingle as u32;
        if !filled[bin] || value < bins[bin] {
            bins[bin] = value;
            filled[bin] = true;
        }
    }
    // An empty bin borrows from the first filled bin on a fixed pseudo-random probe
    // sequence. The sequence is the same for every document, which keeps the
    // collision probability of each position equal to the Jaccard similarity.
    let mut signature = bins;
    for (i, slot) in signature.iter_mut().enumerate() {
        if filled[i] {
            continue;
        }
        let mut attempt = 1u64;
        loop {
            let j = (mix(((i as u64) << 32) | attempt) % SIGNATURE_LEN as u64) as usize;
            if filled[j] {
                *slot = bins[j];
                break;
            }
            attempt += 1;
        }
    }
    signature.to_vec()
}

fn band_keys(signature: &[u32]) -> Vec<i64> {
    if signature.len() != SIGNATURE_LEN {
        return Vec::new();
    }
    signature
        .chunks_exact(ROWS)
        .enumerate()
        .map(|(band, rows)| {
            let mut hash = mix(band as u64 ^ 0x1405_7b7e_f767_814f);
            for &row in rows {
                hash = mix(hash ^ u64::from(row));
            }
            hash as i64
        })
        .collect()
}

/// Estimated Jaccard similarity of two signatures (0 when either is empty).
pub fn jaccard(a: &[u32], b: &[u32]) -> f32 {
    if a.len() != SIGNATURE_LEN || b.len() != SIGNATURE_LEN {
        return 0.0;
    }
    a.iter().zip(b).filter(|(x, y)| x == y).count() as f32 / SIGNATURE_LEN as f32
}

/// Number of values two sorted, distinct lists share.
pub fn shared_count(a: &[i64], b: &[i64]) -> usize {
    let (mut i, mut j, mut shared) = (0, 0, 0);
    while i < a.len() && j < b.len() {
        match a[i].cmp(&b[j]) {
            std::cmp::Ordering::Less => i += 1,
            std::cmp::Ordering::Greater => j += 1,
            std::cmp::Ordering::Equal => {
                shared += 1;
                i += 1;
                j += 1;
            }
        }
    }
    shared
}

/// How one piece of content relates to another, strongest first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Relation {
    /// Same words in every block, possibly reordered.
    Identical,
    /// Near-identical wording.
    Copy,
    /// Recognisably the same document after substantial editing.
    EditedCopy,
    /// Most of the new content comes from the other document.
    Excerpt,
    /// The new content contains most of the other document plus new material.
    Extended,
    /// Some shared passages.
    Overlap,
    /// Different content under the same name.
    SameName,
    Unrelated,
}
impl Relation {
    /// Relations strong enough to tell the owner of the other document.
    pub fn is_copy(self) -> bool {
        matches!(
            self,
            Relation::Identical
                | Relation::Copy
                | Relation::EditedCopy
                | Relation::Excerpt
                | Relation::Extended
        )
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Relation::Identical => "identical",
            Relation::Copy => "copy",
            Relation::EditedCopy => "edited_copy",
            Relation::Excerpt => "excerpt",
            Relation::Extended => "extended",
            Relation::Overlap => "overlap",
            Relation::SameName => "same_name",
            Relation::Unrelated => "unrelated",
        }
    }
}

/// A comparison of new content (`new`) with existing content (`old`).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Comparison {
    pub relation: Relation,
    /// max(jaccard, coverage_new, coverage_old), 0–1.
    pub score: f32,
    pub jaccard: f32,
    /// Share of the new content found in the old.
    pub coverage_new: f32,
    /// Share of the old content found in the new.
    pub coverage_old: f32,
    pub shared_blocks: u32,
    pub title_match: bool,
}

/// Share of `a`'s shingles also in `b`, from the Jaccard estimate and set sizes:
/// |A ∩ B| = J(|A| + |B|) / (1 + J).
fn containment(j: f32, a: u32, b: u32) -> f32 {
    if a == 0 {
        return 0.0;
    }
    (j * (a as f32 + b as f32) / ((1.0 + j) * a as f32)).clamp(0.0, 1.0)
}

/// The stored part of a fingerprint needed for comparison. Block hashes stay in the
/// database; callers supply the number of blocks two documents share.
#[derive(Debug, Clone, Copy)]
pub struct Summary<'a> {
    pub signature: &'a [u32],
    pub shingle_count: u32,
    pub substantive_count: u32,
    pub content_hash: i64,
}
impl Fingerprint {
    pub fn summary(&self) -> Summary<'_> {
        Summary {
            signature: &self.signature,
            shingle_count: self.shingle_count,
            substantive_count: self.blocks.len() as u32,
            content_hash: self.content_hash,
        }
    }
}

pub fn compare(new: &Fingerprint, old: &Fingerprint, title_match: bool) -> Comparison {
    let shared = shared_count(&new.blocks, &old.blocks) as u32;
    compare_summaries(&new.summary(), &old.summary(), shared, title_match)
}

/// Compare new content with existing content, given how many substantive blocks
/// they share.
pub fn compare_summaries(
    new: &Summary,
    old: &Summary,
    shared_blocks: u32,
    title_match: bool,
) -> Comparison {
    let j = jaccard(new.signature, old.signature);
    let share = |n: u32| {
        if n == 0 {
            0.0
        } else {
            (shared_blocks as f32 / n as f32).min(1.0)
        }
    };
    let coverage_new = containment(j, new.shingle_count, old.shingle_count)
        .max(share(new.substantive_count))
        .min(1.0);
    let coverage_old = containment(j, old.shingle_count, new.shingle_count)
        .max(share(old.substantive_count))
        .min(1.0);
    let trivial = |f: &Summary| (f.shingle_count as usize) < MIN_SHINGLES;
    let enough = !trivial(new) && !trivial(old);
    // Excerpts need enough material that shared boilerplate can't explain them.
    let sizable = |f: &Summary| f.substantive_count >= 3 || f.shingle_count >= 40;
    let relation = if enough && new.content_hash != 0 && new.content_hash == old.content_hash {
        Relation::Identical
    } else if enough && (j >= 0.8 || (coverage_new >= 0.9 && coverage_old >= 0.9)) {
        Relation::Copy
    // One side mostly inside the other is a part, even when it is half the whole.
    } else if enough && sizable(new) && coverage_new >= 0.8 && coverage_old < 0.6 {
        Relation::Excerpt
    } else if enough && sizable(old) && coverage_old >= 0.8 && coverage_new < 0.6 {
        Relation::Extended
    } else if enough && (j >= 0.5 || (coverage_new >= 0.6 && coverage_old >= 0.6)) {
        Relation::EditedCopy
    } else if enough && sizable(new) && coverage_new >= 0.6 {
        Relation::Excerpt
    } else if enough && sizable(old) && coverage_old >= 0.6 {
        Relation::Extended
    } else if enough && (j >= 0.3 || coverage_new >= 0.3 || coverage_old >= 0.3) {
        Relation::Overlap
    } else if title_match {
        Relation::SameName
    } else {
        Relation::Unrelated
    };
    Comparison {
        relation,
        score: j.max(coverage_new).max(coverage_old),
        jaccard: j,
        coverage_new,
        coverage_old,
        shared_blocks,
        title_match,
    }
}

/// Words that do not distinguish one file name from another: `Copy of report (2) -
/// final v3.docx` and `report.docx` share the key `report`.
const NAME_NOISE: &[&str] = &[
    "copy", "of", "final", "draft", "edited", "new", "updated", "revised", "rev", "version",
    "latest", "docx", "doc", "xlsx", "xls", "csv", "pptx", "ppt", "odt", "ods", "odp", "pdf",
    "txt", "md",
];

/// Comparable form of a document title or file name. Empty for names made only of
/// noise, which never count as a name match.
pub fn title_key(title: &str) -> String {
    let mut words = Vec::new();
    let mut current = String::new();
    let push = |current: &mut String, words: &mut Vec<String>| {
        if current.is_empty() {
            return;
        }
        let word = std::mem::take(current);
        let revision = word.len() > 1
            && word.starts_with('v')
            && word[1..].chars().all(|c| c.is_ascii_digit());
        let small_number = word.len() <= 3 && word.chars().all(|c| c.is_ascii_digit());
        if !revision && !small_number && !NAME_NOISE.contains(&word.as_str()) {
            words.push(word);
        }
    };
    for c in title.chars() {
        match class(c) {
            CharClass::Ignored => {}
            CharClass::Separator => push(&mut current, &mut words),
            CharClass::Word => current.extend(c.to_lowercase()),
        }
    }
    push(&mut current, &mut words);
    let key = words.join(" ");
    if key.chars().count() < 3 {
        String::new()
    } else {
        key
    }
}

fn field_text(value: &Value) -> Option<String> {
    match value {
        Value::String(s) if !s.trim().is_empty() => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

/// The comparable text of every live block: paragraphs and headings, questionnaire
/// items, spreadsheet rows (cells joined in column order), slide text boxes and
/// speaker notes. Root nodes carry the title, which is compared separately.
pub fn block_texts(state: &DocumentState) -> Vec<String> {
    let mut out = Vec::new();
    for node in state.nodes.values() {
        if node.deleted || node.parent_id.is_none() || state.removed_choices.contains(&node.id) {
            continue;
        }
        let fields = &node.current_fields;
        let label = fields.get("label").and_then(field_text).or_else(|| {
            fields
                .get("content")
                .filter(|c| c.is_object())
                .map(crate::richtext::plain_text)
                .filter(|t| !t.trim().is_empty())
        });
        let mut cells: Vec<(usize, String)> = fields
            .as_object()
            .into_iter()
            .flat_map(|o| o.iter())
            .filter_map(|(key, value)| {
                let column = key.strip_prefix("cell_")?.parse::<usize>().ok()?;
                Some((column, field_text(value)?))
            })
            .collect();
        if let Some(label) = label {
            out.push(label);
        }
        if !cells.is_empty() {
            cells.sort_by_key(|(column, _)| *column);
            out.push(
                cells
                    .into_iter()
                    .map(|(_, v)| v)
                    .collect::<Vec<_>>()
                    .join(" | "),
            );
        }
        if let Some(notes) = fields.get("notes").and_then(field_text) {
            out.push(notes);
        }
    }
    out
}

/// Fingerprint of a materialized document.
pub fn fingerprint_state(state: &DocumentState) -> Fingerprint {
    let texts = block_texts(state);
    Fingerprint::from_blocks(texts.iter().map(String::as_str))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(paragraphs: &[&str]) -> Fingerprint {
        Fingerprint::from_blocks(paragraphs.iter().copied())
    }
    const REPORT: &[&str] = &[
        "The survey reached 1,240 households across four districts between March and May.",
        "Response rates were highest in Sylhet, where enumerators returned for a second visit.",
        "Household size averaged 4.6 people, and 38 percent of households kept livestock.",
        "Access to clean water improved compared with the previous round of data collection.",
        "We recommend a follow-up study focusing on seasonal migration and school attendance.",
        "Appendix tables list every indicator with its confidence interval and sample size.",
    ];

    #[test]
    fn identical_content_matches_whatever_the_order_and_punctuation() {
        let a = doc(REPORT);
        let mut shuffled: Vec<String> = REPORT.iter().rev().map(|p| p.to_uppercase()).collect();
        shuffled[0] = shuffled[0].replace(',', " ,  ");
        let b = Fingerprint::from_blocks(shuffled.iter().map(String::as_str));
        let c = compare(&b, &a, false);
        assert_eq!(c.relation, Relation::Identical);
        assert!((c.score - 1.0).abs() < f32::EPSILON);
        assert_eq!(a.bands, b.bands);
    }

    #[test]
    fn edited_copies_are_recognised_and_unrelated_files_are_not() {
        let original = doc(REPORT);
        let mut edited: Vec<String> = REPORT.iter().map(|p| p.to_string()).collect();
        edited[1] = "Response rates were highest in Sylhet and Khulna after a second visit.".into();
        edited[4] = "A follow-up study should look at migration.".into();
        edited.push("New section: limitations of the sampling frame.".into());
        let edited = Fingerprint::from_blocks(edited.iter().map(String::as_str));
        let c = compare(&edited, &original, false);
        assert!(c.relation.is_copy(), "{c:?}");
        assert!(c.score >= 0.6, "{c:?}");

        let other = doc(&[
            "Quarterly budget for the regional office, including travel and equipment.",
            "Staff costs rose by nine percent because two field officers were hired.",
            "Printing and translation costs were lower than planned this quarter.",
            "The board approved the revised allocation for the next financial year.",
        ]);
        let c = compare(&other, &original, false);
        assert_eq!(c.relation, Relation::Unrelated, "{c:?}");
        assert!(c.score < 0.15, "{c:?}");
        let same_name = compare(&other, &original, true);
        assert_eq!(same_name.relation, Relation::SameName);
    }

    #[test]
    fn excerpts_and_extended_copies_use_block_containment() {
        let original = doc(REPORT);
        let excerpt = doc(&REPORT[..3]);
        assert_eq!(
            compare(&excerpt, &original, false).relation,
            Relation::Excerpt
        );
        let mut extended: Vec<&str> = REPORT.to_vec();
        let topics = [
            "road", "school", "clinic", "market", "river", "harvest", "bank", "phone", "bus",
            "radio", "loan", "well",
        ];
        let extra: Vec<String> = topics
            .iter()
            .enumerate()
            .map(|(i, t)| {
                format!(
                    "Chapter {i} on the {t} asks how {t} access changed and why {t} costs vary."
                )
            })
            .collect();
        extended.extend(extra.iter().map(String::as_str));
        let extended = Fingerprint::from_blocks(extended);
        assert_eq!(
            compare(&extended, &original, false).relation,
            Relation::Extended
        );
    }

    #[test]
    fn similar_documents_share_lsh_bands() {
        let original = doc(REPORT);
        let mut edited: Vec<String> = REPORT.iter().map(|p| p.to_string()).collect();
        edited[5] = "Appendix tables list each indicator.".into();
        let edited = Fingerprint::from_blocks(edited.iter().map(String::as_str));
        let shared = original
            .bands
            .iter()
            .zip(&edited.bands)
            .filter(|(a, b)| a == b)
            .count();
        assert!(shared > 0);
    }

    #[test]
    fn bengali_words_keep_their_joiners_and_split_on_danda() {
        let mut words = Vec::new();
        word_hashes("আমি বাংলায় গান গাই। ক্ষ", &mut words);
        assert_eq!(words.len(), 5);
        let mut joined = Vec::new();
        word_hashes("ক্\u{200D}ষ", &mut joined);
        let mut plain = Vec::new();
        word_hashes("ক্ষ", &mut plain);
        assert_eq!(joined, plain);
    }

    #[test]
    fn fingerprints_are_stable_across_releases() {
        // Stored fingerprints must stay comparable; a change here needs a VERSION bump.
        let f = doc(&["Stable fingerprints keep stored documents comparable over time."]);
        assert_eq!(f.signature.len(), SIGNATURE_LEN);
        assert_eq!(f.shingle_count, 6);
        assert_eq!(f.blocks.len(), 1);
        let again = doc(&["stable FINGERPRINTS keep stored documents, comparable over time"]);
        assert_eq!(f, again);
    }

    #[test]
    fn empty_and_tiny_content_is_never_a_copy() {
        let empty = doc(&["", "   "]);
        assert!(empty.signature.is_empty() && empty.bands.is_empty());
        assert_eq!(empty.content_hash, 0);
        let tiny = doc(&["Yes", "No"]);
        assert!(!compare(&tiny, &tiny.clone(), false).relation.is_copy());
    }

    #[test]
    fn title_keys_ignore_copies_versions_and_extensions() {
        assert_eq!(title_key("Group report (v42).docx"), "group report");
        assert_eq!(
            title_key("Copy of Group_Report - FINAL (2).docx"),
            "group report"
        );
        assert_eq!(title_key("group-report v3"), "group report");
        assert_eq!(title_key("Budget 2024.xlsx"), "budget 2024");
        assert_eq!(title_key("Copy (1).docx"), "");
        assert_eq!(title_key("রিপোর্ট.docx"), "রিপোর্ট");
    }

    #[test]
    fn spreadsheet_rows_and_slide_notes_become_blocks() {
        use crate::materializer::{DocumentState, MaterializedNode};
        use engine_shared::NodeId;
        let mut state = DocumentState::default();
        let node = |id: &str, parent: Option<&str>, fields: Value| MaterializedNode {
            id: NodeId(id.into()),
            parent_id: parent.map(|p| NodeId(p.into())),
            node_type: "item".into(),
            pos: "a".into(),
            current_fields: fields,
            var_name: None,
            deleted: false,
        };
        for n in [
            node("root", None, serde_json::json!({"label":"Title"})),
            node(
                "row",
                Some("root"),
                serde_json::json!({"cell_1":12,"cell_0":"Tariq","cell_10":"x"}),
            ),
            node(
                "shape",
                Some("root"),
                serde_json::json!({"label":"Agenda","notes":"Say hello"}),
            ),
        ] {
            state.nodes.insert(n.id.clone(), n);
        }
        let mut texts = block_texts(&state);
        texts.sort();
        assert_eq!(texts, vec!["Agenda", "Say hello", "Tariq | 12 | x"]);
    }
}
