//! The operation compiler (protocol spec 7.7 step 4, 8, 9): turns an agent's compact edit
//! instructions into engine operations, or refuses them, without touching anything.
//!
//! [`compile`] takes the parsed [`Op`]s of one `propose` call (see [`crate::grammar`]), the
//! state they apply to and the role of the caller, and returns the exact list of
//! [`EventPayload`]s that `edit_draft` would store, in a valid order:
//!
//! * refs and sub-addresses resolve with the [`RefIndex`] of the state at the start of the
//!   call; `$1`, `$2` name the blocks created earlier in the call;
//! * every produced engine op goes through the same pipeline as `edit_draft`
//!   (`governance::authorize`, the `redundant_label` skip, `richtext::compact_event`) and is
//!   applied with `materializer::apply_payload` to a working copy, so the next op sees the
//!   effect of the previous one and **whatever comes out applies**;
//! * the call is atomic: the first op that cannot compile fails the call with an
//!   [`AgentError`] that names the op ([`CompileError`]); there is no partial result;
//! * nothing the agent writes becomes a field name, a node type or a variable name: the
//!   compiler alone chooses `content`, `label`, `cell_N`, `columns`, `text`, `notes`, and it
//!   never emits `var_name`, so an op cannot take a taken name, reach another document (a ref
//!   is resolved only among the nodes of this file) or break containment (`validate_op`).
//!
//! Staleness (spec 9.2): an op may carry `base`, the hash its target had when the agent
//! read it. Each is compared with the team's current state (`CompileInput::head`, else the
//! input state). A difference is recorded in [`Compiled::stale`] and the op still compiles
//! against the state it is given (the draft is a branch; the three-way merge decides at
//! submit); with `strict` the first difference fails the call as `stale_base`, or as
//! `conflict` when the target no longer exists in the team's state.
//!
//! Identities: new nodes get ids from an [`IdSource`], so the crate stays free of clocks and
//! randomness. [`DerivedIds`] makes them deterministic functions of a seed (the `request_id`),
//! which gives a dry run and the real call the same new refs.

use std::collections::BTreeSet;

use engine_core::governance::{self, GovernanceError, Role};
use engine_core::materializer::{apply_payload, DocumentState, MaterializedNode};
use engine_core::ops::OpError;
use engine_core::richtext::{compact_event, redundant_label};
use engine_shared::{EventPayload, NodeId};
use serde_json::{json, Value};

use crate::error::{AgentError, CompileError};
use crate::grammar::{Anchor, Op, OpKind, MAX_OPS};
use crate::hash::{block_hash, notes_hash, row_hash, Hash, ShortHash};
use crate::model::{field_string, FileKind, FileMeta};
use crate::refs::{Ref, RefIndex, ResolveError};
use crate::sheet::used_cols;
use crate::slides::ShapeView;

mod doc;
mod sheet;
mod slides;

/// Engine operations of one proposal (spec 8.6). `edit_draft` takes 5,000 per request:
/// a caller that stores more than that in one transaction splits the list.
pub const MAX_ENGINE_OPS: usize = 10_000;
/// Changed nodes listed per op in [`OpResult`]; the rest are counted.
pub const MAX_RESULT_CHANGES: usize = 200;

type R<T> = Result<T, AgentError>;

fn bad(field: &'static str) -> AgentError {
    AgentError::BadRequest { field }
}

/// Source of ids for new nodes. A ULID is 26 Crockford base32 characters; the crate does not
/// read a clock, so the caller supplies the source (`ulid::Ulid::new()` in the server,
/// [`DerivedIds`] when the ids must be a function of the request).
pub trait IdSource {
    fn next_id(&mut self) -> NodeId;
}

impl<F: FnMut() -> NodeId> IdSource for F {
    fn next_id(&mut self) -> NodeId {
        self()
    }
}

const CROCKFORD: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";

/// ULIDs that are a pure function of `(timestamp, seed, counter)`: 48 bits of the supplied
/// millisecond timestamp, then 80 bits of SHA-256 over the seed and the counter. The same
/// request compiled twice (a dry run, then the real call; a retry) names its new blocks the
/// same way.
#[derive(Debug, Clone)]
pub struct DerivedIds {
    timestamp_ms: u64,
    seed: String,
    counter: u64,
}

impl DerivedIds {
    pub fn new(timestamp_ms: u64, seed: &str) -> Self {
        Self {
            timestamp_ms,
            seed: seed.to_string(),
            counter: 0,
        }
    }
}

impl IdSource for DerivedIds {
    fn next_id(&mut self) -> NodeId {
        self.counter += 1;
        let digest = Hash::of(&json!([self.seed, self.counter]));
        let random = u128::from_str_radix(&digest.as_hex()[..20], 16).expect("hex");
        let time = u128::from(self.timestamp_ms & ((1 << 48) - 1));
        let value = (time << 80) | random;
        let mut id = String::with_capacity(26);
        for i in (0..26).rev() {
            id.push(CROCKFORD[((value >> (5 * i)) & 31) as usize] as char);
        }
        NodeId(id)
    }
}

/// Whether `id` has the shape of a ULID.
fn is_ulid(id: &NodeId) -> bool {
    id.0.len() == 26 && id.0.bytes().all(|b| CROCKFORD.contains(&b)) && id.0.as_bytes()[0] <= b'7'
}

/// What to compile against.
pub struct CompileInput<'a> {
    /// Kind and revision of the file (the revision is echoed, not interpreted).
    pub file: &'a FileMeta,
    /// The state the ops apply to: for a new proposal the state at the agent's `rev`, for an
    /// extension the proposal's current state. Refs resolve against it.
    pub state: &'a DocumentState,
    /// The team's current state, for the `base` comparison. `None`: compare with `state`.
    pub head: Option<&'a DocumentState>,
    /// The role every produced op is authorised as (the spec's `Role::Author`, as `edit_draft`).
    pub role: Role,
    /// Fail the call on the first stale `base` instead of recording it.
    pub strict: bool,
    /// Nodes the caller withholds from the agent (DLP, spec 7.4). An op that names one is
    /// `redacted_block`; `replace_all` leaves them out of its scope, as the agent cannot
    /// have read them.
    pub redacted: BTreeSet<NodeId>,
}

impl<'a> CompileInput<'a> {
    pub fn new(file: &'a FileMeta, state: &'a DocumentState, role: Role) -> Self {
        Self {
            file,
            state,
            head: None,
            role,
            strict: false,
            redacted: BTreeSet::new(),
        }
    }
}

/// A `base` that no longer matches the team's version (spec 9.2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stale {
    /// 0-based index of the op.
    pub op: usize,
    /// The ref (`7C02`) or range (`R91W!B2:C3`) the op named.
    pub at: String,
    /// The team's current hash, `None` when the target no longer exists there.
    pub current: Option<Hash>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeKind {
    Changed,
    Created,
    Deleted,
    Moved,
}

/// One node an op changed, with its hash before and after the op.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    pub at: Ref,
    pub kind: ChangeKind,
    pub old: Option<Hash>,
    pub new: Option<Hash>,
}

/// What one agent op did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpResult {
    pub index: usize,
    pub id: Option<String>,
    pub op: &'static str,
    /// The reason the agent gave, for the reviewer (untrusted text of at most 280
    /// characters, without control or hidden characters).
    pub why: Option<String>,
    /// Engine operations the op expanded into.
    pub engine_ops: usize,
    /// At most [`MAX_RESULT_CHANGES`], in the order the op touched them. Refs are valid in
    /// [`Compiled::after`] (and in the state the call started from, for existing nodes).
    pub changes: Vec<Change>,
    /// Nodes touched beyond the listed ones.
    pub more_changes: usize,
}

/// The result of a successful compile.
#[derive(Debug, Clone, PartialEq)]
pub struct Compiled {
    /// Engine operations in the order they apply, in the form `edit_draft` stores.
    pub ops: Vec<EventPayload>,
    /// One per agent op, in order.
    pub results: Vec<OpResult>,
    pub stale: Vec<Stale>,
    /// The input state with every op in `ops` applied.
    pub after: DocumentState,
}

struct Touch {
    id: NodeId,
    old: Option<Hash>,
    moved: bool,
}

struct Raw {
    index: usize,
    id: Option<String>,
    why: Option<String>,
    op: &'static str,
    engine_ops: usize,
    touched: Vec<Touch>,
    more: usize,
}

/// The working state of one compile.
pub(crate) struct Work<'a> {
    input: &'a CompileInput<'a>,
    pub(crate) kind: FileKind,
    pub(crate) idx: RefIndex,
    pub(crate) root: Option<NodeId>,
    pub(crate) work: DocumentState,
    out: Vec<EventPayload>,
    /// Blocks created so far, for `$1`, `$2`.
    pub(crate) created: Vec<NodeId>,
    ids: &'a mut dyn IdSource,
    stale: Vec<Stale>,
    // The op in progress.
    start: usize,
    touched: Vec<Touch>,
    seen: BTreeSet<NodeId>,
    more: usize,
    done: Vec<Raw>,
}

fn map_governance(e: GovernanceError) -> AgentError {
    match e {
        GovernanceError::Unauthorized { .. } => AgentError::Forbidden,
        GovernanceError::SheetProtection(_) => AgentError::Protected,
        GovernanceError::Integrity(_) => AgentError::Internal,
        GovernanceError::Op(op) => match op {
            OpError::NodeNotFound(_) | OpError::NodeDeleted(_) => {
                AgentError::NotFound { refs: Vec::new() }
            }
            OpError::NodeHasChildren(_)
            | OpError::IllegalContainment { .. }
            | OpError::InvalidRoot(_) => AgentError::UnsupportedStructure,
            OpError::DuplicateVarName(_) => bad("var_name"),
            OpError::MoveCreatesCycle { .. } => bad("after"),
            _ => AgentError::Internal,
        },
    }
}

impl<'a> Work<'a> {
    fn new(input: &'a CompileInput<'a>, ids: &'a mut dyn IdSource) -> Self {
        let tree = crate::model::Tree::new(input.state);
        Self {
            input,
            kind: input.file.kind,
            idx: RefIndex::from_state(input.state),
            root: tree.root.map(|r| r.id.clone()),
            work: input.state.clone(),
            out: Vec::new(),
            created: Vec::new(),
            ids,
            stale: Vec::new(),
            start: 0,
            touched: Vec::new(),
            seen: BTreeSet::new(),
            more: 0,
            done: Vec::new(),
        }
    }

    pub(crate) fn head(&self) -> &DocumentState {
        self.input.head.unwrap_or(self.input.state)
    }

    pub(crate) fn redacted(&self, id: &NodeId) -> bool {
        self.input.redacted.contains(id)
    }

    /// A new node id.
    pub(crate) fn new_id(&mut self) -> R<NodeId> {
        let id = self.ids.next_id();
        if is_ulid(&id) && !self.work.nodes.contains_key(&id) {
            Ok(id)
        } else {
            Err(AgentError::Internal)
        }
    }

    /// The short ref of a node (its full id when the index does not know it).
    pub(crate) fn ref_of(&self, id: &NodeId) -> Ref {
        self.idx
            .ref_of(id)
            .or_else(|| Ref::parse(&id.0).ok())
            .unwrap_or_else(|| Ref::parse("0000").expect("a valid ref"))
    }

    /// Resolve a ref or a `$n` to a live node of this file.
    pub(crate) fn resolve(&self, field: &'static str, anchor: &Anchor) -> R<NodeId> {
        let id = match anchor {
            Anchor::New(n) => n
                .checked_sub(1)
                .and_then(|i| self.created.get(i))
                .cloned()
                .ok_or(bad(field))?,
            Anchor::Ref(r) => match self.idx.resolve(r) {
                Ok(id) => id,
                Err(ResolveError::Unknown) => {
                    return Err(AgentError::UnknownRef {
                        refs: vec![r.clone()],
                    })
                }
                Err(ResolveError::Ambiguous(c)) => {
                    return Err(AgentError::AmbiguousRef { refs: c })
                }
            },
        };
        match self.work.nodes.get(&id) {
            Some(n) if !n.deleted => Ok(id),
            _ => Err(AgentError::NotFound {
                refs: vec![self.ref_of(&id)],
            }),
        }
    }

    /// `redacted_block` when the caller withholds the node.
    pub(crate) fn check_redacted(&self, id: &NodeId) -> R<()> {
        if self.redacted(id) {
            Err(AgentError::RedactedBlock {
                refs: vec![self.ref_of(id)],
            })
        } else {
            Ok(())
        }
    }

    /// Compare an op's `base` with the team's hash (`current`, `None` when the target is
    /// gone there). Records the difference; fails the call when `strict`.
    pub(crate) fn check_base(
        &mut self,
        op: &Op,
        at: String,
        target: &Ref,
        current: Option<Hash>,
    ) -> R<()> {
        let Some(base) = &op.base else {
            return Ok(());
        };
        if current.as_ref().is_some_and(|h| base.matches(h)) {
            return Ok(());
        }
        self.stale.push(Stale {
            op: op.index,
            at,
            current: current.clone(),
        });
        if !self.input.strict {
            return Ok(());
        }
        Err(match current {
            Some(h) => AgentError::StaleBase {
                target: target.clone(),
                current: ShortHash::parse(h.h6()).expect("six hex characters"),
            },
            None => AgentError::Conflict {
                refs: vec![target.clone()],
            },
        })
    }

    /// Run one engine op through the pipeline of `edit_draft` and apply it to the working
    /// state. Redundant label edits are dropped and whole-content edits become patches when
    /// that is smaller, exactly as the server does when it stores the op.
    pub(crate) fn emit(&mut self, op: EventPayload) -> R<()> {
        governance::authorize(self.input.role, &op, &self.work).map_err(map_governance)?;
        if redundant_label(&self.work, &op) {
            return Ok(());
        }
        let op = compact_event(&self.work, &op);
        if self.out.len() >= MAX_ENGINE_OPS {
            return Err(AgentError::LimitExceeded {
                field: "engine_ops",
                limit: MAX_ENGINE_OPS as u64,
                value: self.out.len() as u64 + 1,
            });
        }
        if let Some(id) = op.target_node_id() {
            self.touch(id, matches!(op, EventPayload::NodeMoved { .. }));
        }
        apply_payload(&mut self.work, &op).map_err(|_| AgentError::Internal)?;
        self.out.push(op);
        Ok(())
    }

    /// Note a node as touched by the op in progress, with its hash before the first change.
    fn touch(&mut self, id: &NodeId, moved: bool) {
        if !self.seen.insert(id.clone()) {
            if moved {
                if let Some(t) = self.touched.iter_mut().find(|t| &t.id == id) {
                    t.moved = true;
                }
            }
            return;
        }
        if self.touched.len() >= MAX_RESULT_CHANGES {
            self.more += 1;
            return;
        }
        let old = self.describe(&self.work, id);
        self.touched.push(Touch {
            id: id.clone(),
            old,
            moved,
        });
    }

    /// The protocol hash of a node in `state`: a block's `h`, a row's `h` (over the sheet's
    /// used columns), a shape's `h` or a slide's notes hash. `None` when the node is absent,
    /// deleted or has none.
    fn describe(&self, state: &DocumentState, id: &NodeId) -> Option<Hash> {
        describe_in(state, self.kind, self.root.as_ref(), id)
    }

    /// The same hash in the team's current state.
    pub(crate) fn head_hash(&self, id: &NodeId) -> Option<Hash> {
        self.describe(self.head(), id)
    }

    fn begin(&mut self) {
        self.start = self.out.len();
        self.touched.clear();
        self.seen.clear();
        self.more = 0;
    }

    fn finish(&mut self, op: &Op) {
        self.done.push(Raw {
            index: op.index,
            id: op.id.clone(),
            why: op.why.clone(),
            op: op.kind.name(),
            engine_ops: self.out.len() - self.start,
            touched: std::mem::take(&mut self.touched),
            more: self.more,
        });
    }

    fn run(&mut self, op: &Op) -> R<()> {
        let want = match &op.kind {
            OpKind::SetText { .. }
            | OpKind::Replace { .. }
            | OpKind::ReplaceAll { .. }
            | OpKind::Insert { .. }
            | OpKind::Delete { .. }
            | OpKind::Move { .. }
            | OpKind::SetType { .. } => FileKind::Document,
            OpKind::SetCells { .. } | OpKind::Clear { .. } => FileKind::Spreadsheet,
            OpKind::SetShape { .. } | OpKind::SetNotes { .. } => FileKind::Presentation,
        };
        if want != self.kind {
            return Err(AgentError::UnsupportedStructure);
        }
        match &op.kind {
            OpKind::SetText { at, text } => doc::set_text(self, op, at, text),
            OpKind::Replace {
                at,
                find,
                with,
                which,
                expect,
            } => doc::replace(self, op, at, find, with, *which, *expect),
            OpKind::ReplaceAll {
                find,
                with,
                expect,
                scope,
                word,
                case,
            } => doc::replace_all(self, op, find, with, *expect, scope, *word, *case),
            OpKind::Insert { place, blocks } => doc::insert(self, op, place, blocks),
            OpKind::Delete { at } => doc::delete(self, op, at),
            OpKind::Move { at, place } => doc::move_block(self, op, at, place),
            OpKind::SetType { at, to } => doc::set_type(self, op, at, *to),
            OpKind::SetCells { sheet, at, values } => sheet::set_cells(self, op, sheet, at, values),
            OpKind::Clear { sheet, range } => sheet::clear(self, op, sheet.as_deref(), range),
            OpKind::SetShape { slide, shape, text } => {
                slides::set_shape(self, op, slide, shape, text)
            }
            OpKind::SetNotes { slide, text } => slides::set_notes(self, op, slide, text),
        }
    }

    fn into_compiled(self) -> Compiled {
        let idx = RefIndex::from_state(&self.work);
        let results = self
            .done
            .into_iter()
            .map(|raw| {
                let mut changes = Vec::new();
                for t in raw.touched {
                    let new = describe_in(&self.work, self.kind, self.root.as_ref(), &t.id);
                    let kind = match (&t.old, &new) {
                        (None, Some(_)) => ChangeKind::Created,
                        (Some(_), None) => ChangeKind::Deleted,
                        (Some(a), Some(b)) if a != b => ChangeKind::Changed,
                        _ if t.moved => ChangeKind::Moved,
                        _ => continue,
                    };
                    let Some(at) = idx.ref_of(&t.id) else {
                        continue;
                    };
                    changes.push(Change {
                        at,
                        kind,
                        old: t.old,
                        new,
                    });
                }
                OpResult {
                    index: raw.index,
                    id: raw.id,
                    why: raw.why,
                    op: raw.op,
                    engine_ops: raw.engine_ops,
                    changes,
                    more_changes: raw.more,
                }
            })
            .collect();
        Compiled {
            ops: self.out,
            results,
            stale: self.stale,
            after: self.work,
        }
    }
}

/// The protocol hash of a node in `state` (see `Work::describe`).
fn describe_in(
    state: &DocumentState,
    kind: FileKind,
    root: Option<&NodeId>,
    id: &NodeId,
) -> Option<Hash> {
    let n = state.nodes.get(id).filter(|n| !n.deleted)?;
    match kind {
        FileKind::Document => (n.parent_id.as_ref() == root
            && n.current_fields["content"].is_object())
        .then(|| block_hash(&n.current_fields["content"])),
        FileKind::Spreadsheet => {
            let sheet = n.parent_id.as_ref()?;
            (n.node_type == "item").then(|| {
                let rows: Vec<&MaterializedNode> = state
                    .nodes
                    .values()
                    .filter(|r| {
                        !r.deleted && r.node_type == "item" && r.parent_id.as_ref() == Some(sheet)
                    })
                    .collect();
                let cols = used_cols(&rows);
                let cells: Vec<String> = (0..cols)
                    .map(|c| field_string(&n.current_fields[&format!("cell_{c}")]))
                    .collect();
                row_hash(&cells)
            })
        }
        FileKind::Presentation => match n.node_type.as_str() {
            "item" => Some(ShapeView::new(n).hash()),
            "section" => Some(notes_hash(n.current_fields["notes"].as_str().unwrap_or(""))),
            _ => None,
        },
    }
}

/// Compile the ops of one `propose` call.
///
/// The ops apply in order and later ops see earlier ones. Any failure fails the whole call
/// ([`CompileError::op`] names the op). `ids` supplies the ids of new nodes.
pub fn compile(
    input: &CompileInput,
    ops: &[Op],
    ids: &mut dyn IdSource,
) -> Result<Compiled, CompileError> {
    if ops.is_empty() {
        return Err(bad("ops").into());
    }
    if ops.len() > MAX_OPS {
        return Err(AgentError::LimitExceeded {
            field: "ops",
            limit: MAX_OPS as u64,
            value: ops.len() as u64,
        }
        .into());
    }
    let mut w = Work::new(input, ids);
    for op in ops {
        w.begin();
        w.run(op)
            .map_err(|e| CompileError::at(op.index, op.id.as_deref(), e))?;
        w.finish(op);
    }
    Ok(w.into_compiled())
}

/// The plain-text label of a block's content (`textOf` of the editor).
pub(crate) fn label_of(content: &Value) -> Value {
    Value::String(engine_core::richtext::plain_text(content))
}

/// Children of `parent` that are live, as `(pos, id)` in the order the readers use.
pub(crate) fn live_children(state: &DocumentState, parent: &NodeId) -> Vec<(String, NodeId)> {
    let mut v: Vec<(String, NodeId)> = state
        .nodes
        .values()
        .filter(|n| !n.deleted && n.parent_id.as_ref() == Some(parent))
        .map(|n| (n.pos.clone(), n.id.clone()))
        .collect();
    v.sort();
    v
}
