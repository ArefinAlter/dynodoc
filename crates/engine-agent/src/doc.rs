//! Document blocks with their refs, text analysis and hashes, shared by the tools.

use engine_core::materializer::MaterializedNode;

use crate::error::AgentError;
use crate::hash::{block_hash, Hash};
use crate::model::Tree;
use crate::refs::{Ref, RefIndex, ResolveError};
use crate::text::{analyze_block, BlockText};

pub struct DocBlock<'a> {
    pub node: &'a MaterializedNode,
    pub r: Ref,
    pub text: BlockText,
    pub hash: Hash,
    /// Heading level 1 to 6.
    pub level: Option<u8>,
}

/// The live blocks of a document in order. Blocks whose id is not addressable (not a
/// 4 to 26 character alphanumeric, so not a ULID) are left out: they have no ref.
pub fn doc_blocks<'a>(tree: &Tree<'a>, idx: &RefIndex, md: bool) -> Vec<DocBlock<'a>> {
    tree.blocks()
        .into_iter()
        .filter_map(|node| {
            let r = idx.ref_of(&node.id)?;
            let content = &node.current_fields["content"];
            let text = analyze_block(content, md);
            let level = text
                .label
                .strip_prefix('H')
                .and_then(|n| n.parse::<u8>().ok())
                .filter(|_| content["type"] == "heading");
            Some(DocBlock {
                node,
                r,
                hash: block_hash(content),
                text,
                level,
            })
        })
        .collect()
}

/// The blocks of the section that starts at heading `i`: the heading and every block up
/// to (excluding) the next heading of the same or a higher level.
pub fn section_end(blocks: &[DocBlock], i: usize) -> usize {
    let Some(level) = blocks[i].level else {
        return i + 1;
    };
    blocks[i + 1..]
        .iter()
        .position(|b| b.level.is_some_and(|l| l <= level))
        .map_or(blocks.len(), |p| i + 1 + p)
}

/// Index of the heading block that `selector` (a ref) names; `field` names the request
/// field in errors.
pub fn find_heading(
    blocks: &[DocBlock],
    idx: &RefIndex,
    field: &'static str,
    selector: &str,
) -> Result<usize, AgentError> {
    let r = Ref::parse(selector).map_err(|_| AgentError::BadRequest { field })?;
    match idx.resolve(&r) {
        Ok(id) => blocks
            .iter()
            .position(|b| b.node.id == id && b.level.is_some())
            .ok_or(AgentError::BadRequest { field }),
        Err(ResolveError::Unknown) => Err(AgentError::UnknownRef { refs: vec![r] }),
        Err(ResolveError::Ambiguous(c)) => Err(AgentError::AmbiguousRef { refs: c }),
    }
}
