//! Presentations (protocol spec 5.6, 6.8): slides are `section` nodes under the deck
//! root, shapes are `item` nodes under a slide. Shape `src` data URIs (base64 images up
//! to about 1 MiB) are never returned; the text shows their size instead.

use engine_core::materializer::MaterializedNode;

use crate::error::AgentError;
use crate::hash::{notes_hash, shape_hash, Hash};
use crate::model::{field_string, Tree};
use crate::refs::{Ref, RefIndex, ResolveError, SlideSel};

pub const LAYOUTS: [&str; 5] = ["title", "title-content", "section", "two-content", "blank"];

pub struct ShapeView<'a> {
    pub node: &'a MaterializedNode,
    /// `text`, `image` or `shape`.
    pub kind: &'static str,
    /// `title`, `body` or `subtitle`.
    pub role: Option<&'static str>,
    pub text: String,
    pub alt: Option<String>,
    /// Length of the omitted inline image (0 when absent or not an inline raster).
    pub src_len: usize,
    pub bullets: bool,
}

impl ShapeView<'_> {
    pub fn hash(&self) -> Hash {
        shape_hash(self.kind, self.role, &self.text)
    }
}

pub struct SlideView<'a> {
    pub node: &'a MaterializedNode,
    pub layout: &'static str,
    pub hidden: bool,
    pub notes: String,
    pub shapes: Vec<ShapeView<'a>>,
}

fn is_inline_raster(src: &str) -> bool {
    let lower: String = src.chars().take(32).collect::<String>().to_ascii_lowercase();
    ["png", "jpeg", "gif", "webp"]
        .iter()
        .any(|t| lower.starts_with(&format!("data:image/{t};base64,")))
}

impl<'a> SlideView<'a> {
    pub fn new(tree: &Tree<'a>, node: &'a MaterializedNode) -> Self {
        let f = &node.current_fields;
        let layout = LAYOUTS
            .iter()
            .copied()
            .find(|l| f["layout"] == *l)
            .unwrap_or("title-content");
        let shapes = tree
            .children(&node.id)
            .iter()
            .copied()
            .filter(|n| n.node_type == "item")
            .map(|n| {
                let f = &n.current_fields;
                let kind = match f["kind"].as_str() {
                    Some("image") => "image",
                    Some("shape") => "shape",
                    _ => "text",
                };
                let role = match f["role"].as_str() {
                    Some("title") => Some("title"),
                    Some("body") => Some("body"),
                    Some("subtitle") => Some("subtitle"),
                    _ => None,
                };
                let text = match f["text"].as_str() {
                    Some(t) => t.to_string(),
                    None => field_string(&f["label"]),
                };
                let src = f["src"].as_str().filter(|s| is_inline_raster(s));
                ShapeView {
                    node: n,
                    kind,
                    role,
                    text,
                    alt: f["alt"].as_str().map(str::to_string),
                    src_len: src.map_or(0, str::len),
                    bullets: f["style"]["bullets"] == true,
                }
            })
            .collect();
        Self {
            node,
            layout,
            hidden: f["hidden"] == true,
            notes: f["notes"].as_str().unwrap_or("").to_string(),
            shapes,
        }
    }

    /// Text of the first title shape.
    pub fn title(&self) -> String {
        self.shapes
            .iter()
            .find(|s| s.role == Some("title"))
            .map(|s| s.text.clone())
            .unwrap_or_default()
    }

    /// Text shapes that are not the title.
    pub fn body_shapes(&self) -> usize {
        self.shapes
            .iter()
            .filter(|s| s.kind == "text" && s.role != Some("title"))
            .count()
    }

    pub fn notes_words(&self) -> usize {
        self.notes.split_whitespace().count()
    }

    /// `(image shapes, total omitted bytes)`.
    pub fn images(&self) -> (usize, usize) {
        let imgs = self.shapes.iter().filter(|s| s.kind == "image");
        (
            imgs.clone().count(),
            imgs.map(|s| s.src_len).sum(),
        )
    }

    /// Hash of everything the protocol shows about the slide (for `oh` and `diff`).
    pub fn hash(&self) -> Hash {
        let shapes: Vec<String> = self.shapes.iter().map(|s| s.hash().as_hex().to_string()).collect();
        Hash::of(&serde_json::json!({
            "label": field_string(&self.node.current_fields["label"]),
            "layout": self.layout,
            "hidden": self.hidden,
            "notes": notes_hash(&self.notes).as_hex(),
            "shapes": shapes,
        }))
    }
}

/// Resolve a slide selector against the slides at this revision.
pub fn select_slide<'a>(
    tree: &Tree<'a>,
    idx: &RefIndex,
    sel: &SlideSel,
) -> Result<(usize, &'a MaterializedNode), AgentError> {
    let slides = tree.slides();
    let by_ref = |r: &Ref| -> Result<(usize, &'a MaterializedNode), AgentError> {
        match idx.resolve(r) {
            Ok(id) => slides
                .iter()
                .position(|s| s.id == id)
                .map(|i| (i, slides[i]))
                .ok_or(AgentError::UnknownRef { refs: vec![r.clone()] }),
            Err(ResolveError::Unknown) => Err(AgentError::UnknownRef { refs: vec![r.clone()] }),
            Err(ResolveError::Ambiguous(c)) => Err(AgentError::AmbiguousRef { refs: c }),
        }
    };
    match sel {
        SlideSel::Position(n) => match slides.get((*n as usize).wrapping_sub(1)) {
            Some(s) => Ok((*n as usize - 1, s)),
            // A lowercase ref such as `s123` that is not a position at this revision.
            None => Ref::parse(&sel.to_string())
                .map_err(|_| AgentError::BadRequest { field: "slides" })
                .and_then(|r| by_ref(&r)),
        },
        SlideSel::Ref(r) => by_ref(r),
    }
}
