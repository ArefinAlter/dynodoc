//! Random operations that `validate_op` accepts on a given state, covering every event
//! type. Shared by property tests here and in engine-api (`#[path]`).
#![allow(dead_code)]

use engine_core::{
    materializer::{apply_payload, DocumentState, MaterializedNode},
    ops::{decode_wrapped_op, validate_op},
    richtext,
};
use engine_shared::{DocumentId, Event, EventId, EventPayload, IdentityId, NodeId, NodeType};
use serde_json::json;

/// The number of event types; [`Gen::op`] draws one of them per call.
pub const KINDS: usize = 15;

/// A discussion thread: the id of its `CommentAdded` event and whether it is resolved.
pub type Thread = (String, bool);

pub struct Gen {
    seed: u64,
    tag: u64,
    next: u64,
}

impl Gen {
    pub fn new(seed: u64) -> Self {
        Self {
            seed: seed.max(1),
            tag: seed % 100_000,
            next: 0,
        }
    }

    fn below(&mut self, n: usize) -> usize {
        self.seed ^= self.seed << 13;
        self.seed ^= self.seed >> 7;
        self.seed ^= self.seed << 17;
        (self.seed % n as u64) as usize
    }

    /// A fresh 26-character id, the length of the ULIDs clients send. Node ids are
    /// unique across documents, so each seed has its own.
    pub fn id(&mut self) -> String {
        self.next += 1;
        format!("01G{:05}{:018}", self.tag, self.next)
    }

    fn pick<T: Clone>(&mut self, items: &[T]) -> Option<T> {
        (!items.is_empty()).then(|| items[self.below(items.len())].clone())
    }

    fn words(&mut self) -> String {
        format!("text {}", self.below(1000))
    }

    /// An operation an author may make on `state`, or `None` when the drawn kind has no
    /// target here. Replies go only to unresolved `threads`.
    pub fn op(&mut self, state: &DocumentState, threads: &[Thread]) -> Option<EventPayload> {
        let nodes = |keep: &dyn Fn(&MaterializedNode) -> bool| -> Vec<NodeId> {
            state
                .nodes
                .values()
                .filter(|n| keep(n))
                .map(|n| n.id.clone())
                .collect()
        };
        let live = |n: &MaterializedNode| !n.deleted && !state.removed_choices.contains(&n.id);
        let pending: Vec<String> = state
            .suggestions
            .iter()
            .filter(|(_, s)| !s.accepted && s.rejected_reason.is_none())
            .map(|(id, _)| id.clone())
            .collect();
        let op = if state.nodes.is_empty() {
            EventPayload::NodeCreated {
                node_id: NodeId(self.id()),
                node_type: NodeType::Form,
                parent_id: None,
                pos: "a0".into(),
                fields: json!({"label": "Root"}),
                var_name: None,
            }
        } else {
            match self.below(KINDS) {
                0 => {
                    let parent = self.pick(&nodes(&|n| {
                        live(n) && n.node_type != "item" && n.node_type != "choice"
                    }))?;
                    let item = self.below(2) == 0;
                    let text = self.words();
                    EventPayload::NodeCreated {
                        node_id: NodeId(self.id()),
                        node_type: if item {
                            NodeType::Item
                        } else {
                            NodeType::Section
                        },
                        parent_id: Some(parent),
                        pos: format!("p{}", self.below(4)),
                        fields: if item {
                            json!({"label": text, "content": paragraph(&text)})
                        } else {
                            json!({"label": text})
                        },
                        var_name: (item && self.below(2) == 0).then(|| format!("v{}", self.next)),
                    }
                }
                1 => EventPayload::NodeMoved {
                    node_id: self.pick(&nodes(&|n| {
                        live(n) && n.parent_id.is_some() && n.node_type != "choice"
                    }))?,
                    new_parent_id: Some(self.pick(&nodes(&|n| {
                        live(n) && n.node_type != "item" && n.node_type != "choice"
                    }))?),
                    new_pos: format!("m{}", self.below(4)),
                },
                2 => EventPayload::NodeDeleted {
                    node_id: self.pick(&nodes(&|n| {
                        live(n) && n.parent_id.is_some() && n.node_type != "choice"
                    }))?,
                },
                3 => EventPayload::NodeRestored {
                    node_id: self.pick(&nodes(&|n| n.deleted && n.node_type != "choice"))?,
                },
                4 => EventPayload::FieldEdited {
                    node_id: self.pick(&nodes(&|n| live(n) && n.node_type != "choice"))?,
                    field: "hint".into(),
                    value: json!(self.words()),
                },
                5 => {
                    let node = self.pick(&nodes(&|n| {
                        live(n) && n.current_fields["content"].is_object()
                    }))?;
                    let before = &state.nodes[&node].current_fields["content"];
                    let after = paragraph(&self.words());
                    EventPayload::RichTextPatched {
                        node_id: node,
                        patch: richtext::make_patch(before, &after)?,
                    }
                }
                6 => EventPayload::ChoiceAdded {
                    node_id: self.pick(&nodes(&|n| live(n) && n.node_type == "item"))?,
                    choice_id: NodeId(self.id()),
                    fields: json!({"label": self.words(), "value": self.below(9).to_string()}),
                },
                7 => {
                    let choice = self.pick(&nodes(&|n| live(n) && n.node_type == "choice"))?;
                    EventPayload::ChoiceRemoved {
                        node_id: state.nodes[&choice].parent_id.clone()?,
                        choice_id: choice,
                    }
                }
                8 => EventPayload::CommentAdded {
                    node_id: if self.below(2) == 0 {
                        Some(self.pick(&nodes(&live))?)
                    } else {
                        None
                    },
                    body: self.words(),
                },
                9 => EventPayload::CommentReplied {
                    thread_id: self.pick(
                        &threads
                            .iter()
                            .filter(|t| !t.1)
                            .map(|t| t.0.clone())
                            .collect::<Vec<_>>(),
                    )?,
                    body: self.words(),
                },
                10 => {
                    let (thread_id, resolved) = self.pick(threads)?;
                    EventPayload::CommentResolved {
                        thread_id,
                        resolved: !resolved,
                    }
                }
                11 => {
                    let node = self.pick(&nodes(&|n| live(n) && n.node_type != "choice"))?;
                    let wrapped = EventPayload::FieldEdited {
                        node_id: node.clone(),
                        field: "label".into(),
                        value: json!(self.words()),
                    };
                    EventPayload::SuggestionProposed {
                        suggestion_id: self.id(),
                        target_node_id: Some(node),
                        detail: serde_json::to_value(wrapped).unwrap(),
                    }
                }
                12 => {
                    let suggestion_id = self.pick(&pending)?;
                    // Acceptance applies the wrapped change, so it must still apply.
                    let wrapped =
                        decode_wrapped_op(&state.suggestions[&suggestion_id].detail).ok()?;
                    validate_op(state, &wrapped).ok()?;
                    EventPayload::SuggestionAccepted { suggestion_id }
                }
                13 => EventPayload::SuggestionRejected {
                    suggestion_id: self.pick(&pending)?,
                    reason: Some("not now".into()),
                },
                _ => EventPayload::Deployed { snapshot_id: None },
            }
        };
        validate_op(state, &op).is_ok().then_some(op)
    }
}

pub fn paragraph(text: &str) -> serde_json::Value {
    json!({"type":"paragraph","content":[{"type":"text","text":text}]})
}

/// `count` events generated one by one from an empty document, as stored rows. The id
/// of each `CommentAdded` event becomes a thread id, as on the server.
pub fn history(seed: u64, count: usize) -> Vec<Event> {
    let mut gen = Gen::new(seed);
    let mut state = DocumentState::default();
    let mut threads: Vec<Thread> = Vec::new();
    let mut events = Vec::new();
    while events.len() < count {
        let Some(op) = gen.op(&state, &threads) else {
            continue;
        };
        apply_payload(&mut state, &op).unwrap();
        let seq = events.len() as i64 + 1;
        let id = format!("01EVT{seq:021}");
        match &op {
            EventPayload::CommentAdded { .. } => threads.push((id.clone(), false)),
            EventPayload::CommentResolved {
                thread_id,
                resolved,
            } => {
                if let Some(t) = threads.iter_mut().find(|t| &t.0 == thread_id) {
                    t.1 = *resolved;
                }
            }
            _ => {}
        }
        events.push(Event {
            id: EventId(id),
            document_id: DocumentId(uuid::Uuid::nil()),
            seq,
            event_type: op.event_type().as_str().to_string(),
            actor_id: IdentityId(uuid::Uuid::nil()),
            target_node_id: op.target_node_id().cloned(),
            payload: serde_json::to_value(&op).unwrap(),
            content_hash: vec![0; 32],
            prev_chain_hash: vec![0; 32],
            chain_hash: vec![0; 32],
            created_at: chrono::Utc::now(),
        });
    }
    events
}
