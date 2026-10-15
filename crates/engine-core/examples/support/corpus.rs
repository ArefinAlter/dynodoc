//! Deterministic synthetic documents for benchmarks: a long report, a spreadsheet and
//! a questionnaire, built only from operations that validate and replay.
#![allow(dead_code)]

use engine_core::materializer::{apply_payload, DocumentState};
use engine_shared::{EventPayload, NodeId, NodeType};
use serde_json::{json, Value};

const CROCKFORD: &[u8] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";

/// xorshift64: the same seed gives the same corpus on every machine.
pub struct Rng(u64);
impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(seed.max(1))
    }
    pub fn below(&mut self, n: u64) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0 % n
    }
}

pub struct Corpus {
    rng: Rng,
    vocab: Vec<String>,
    millis: u64,
    pub events: Vec<EventPayload>,
}

impl Corpus {
    pub fn new(seed: u64) -> Self {
        let mut rng = Rng::new(seed);
        let vocab = (0..4000)
            .map(|_| {
                let len = 2 + rng.below(9);
                (0..len)
                    .map(|_| (b'a' + rng.below(26) as u8) as char)
                    .collect()
            })
            .collect();
        Self {
            rng,
            vocab,
            millis: 1_759_000_000_000,
            events: Vec::new(),
        }
    }

    /// A ULID-shaped id with increasing time, like ids made by the app.
    pub fn id(&mut self) -> NodeId {
        self.millis += 1 + self.rng.below(4000);
        let time = (0..10).map(|i| CROCKFORD[((self.millis >> (5 * (9 - i))) & 31) as usize]);
        let random: Vec<u8> = (0..16)
            .map(|_| CROCKFORD[self.rng.below(32) as usize])
            .collect();
        NodeId(String::from_utf8(time.chain(random).collect()).unwrap())
    }

    /// Words drawn with Zipf-like weights, so common words repeat as in real text.
    pub fn words(&mut self, min: u64, max: u64) -> String {
        let count = min + self.rng.below(max - min + 1);
        let words: Vec<&str> = (0..count)
            .map(|_| {
                let rank = (self.vocab.len() as f64).powf(self.rng.below(10_000) as f64 / 10_000.0);
                self.vocab[rank as usize - 1].as_str()
            })
            .collect();
        format!("{}.", words.join(" "))
    }

    fn create(
        &mut self,
        id: &NodeId,
        node_type: NodeType,
        parent: Option<&NodeId>,
        pos: String,
        fields: Value,
        var_name: Option<String>,
    ) {
        self.events.push(EventPayload::NodeCreated {
            node_id: id.clone(),
            node_type,
            parent_id: parent.cloned(),
            pos,
            fields,
            var_name,
        });
    }

    pub fn state(&self) -> DocumentState {
        let mut state = DocumentState::default();
        for op in &self.events {
            apply_payload(&mut state, op).expect("synthetic events apply");
        }
        state
    }
}

pub fn paragraph(text: &str) -> Value {
    json!({"type":"paragraph","content":[{"type":"text","text":text}]})
}

/// A report: one root and `paragraphs` rich-text blocks of 20-90 words.
pub fn report(seed: u64, paragraphs: usize) -> (Corpus, Vec<NodeId>) {
    let mut c = Corpus::new(seed);
    let root = c.id();
    c.create(
        &root,
        NodeType::Form,
        None,
        "000000".into(),
        json!({"label":"Report","page_setup":{"size":"A4","margin":25.4}}),
        None,
    );
    let mut blocks = Vec::new();
    for i in 0..paragraphs {
        let id = c.id();
        let text = c.words(20, 90);
        c.create(
            &id,
            NodeType::Item,
            Some(&root),
            format!("{i:06}"),
            json!({"content":paragraph(&text),"label":text}),
            None,
        );
        blocks.push(id);
    }
    (c, blocks)
}

/// A spreadsheet: one sheet section with `rows` rows of 8 cells.
pub fn sheet(seed: u64, rows: usize) -> Corpus {
    let mut c = Corpus::new(seed);
    let root = c.id();
    c.create(
        &root,
        NodeType::Form,
        None,
        "000000".into(),
        json!({"label":"Workbook"}),
        None,
    );
    let sheet = c.id();
    c.create(
        &sheet,
        NodeType::Section,
        Some(&root),
        "000001".into(),
        json!({"label":"Sheet1","kind":"sheet","columns":12}),
        None,
    );
    for r in 0..rows {
        let id = c.id();
        let mut fields = json!({"row_height":28});
        for col in 0..8 {
            fields[format!("cell_{col}")] = match c.rng.below(5) {
                0 => json!(c.rng.below(100_000).to_string()),
                1 => json!(format!("{:.2}", c.rng.below(100_000) as f64 / 100.0)),
                2 => json!(c.words(1, 1)),
                3 => json!(if c.rng.below(2) == 0 { "Yes" } else { "No" }),
                _ => json!(""),
            };
        }
        c.create(
            &id,
            NodeType::Item,
            Some(&sheet),
            format!("{r:08}"),
            fields,
            None,
        );
    }
    c
}

/// A questionnaire: `questions` single-choice questions with 5 choices each.
pub fn questionnaire(seed: u64, questions: usize) -> Corpus {
    let mut c = Corpus::new(seed);
    let root = c.id();
    c.create(
        &root,
        NodeType::Form,
        None,
        "000000".into(),
        json!({"label":"Survey"}),
        None,
    );
    for q in 0..questions {
        let id = c.id();
        let label = c.words(6, 16);
        c.create(
            &id,
            NodeType::Item,
            Some(&root),
            format!("{q:06}"),
            json!({"label":label,"item_type":"select_one","required":true}),
            Some(format!("q{q}")),
        );
        for (n, choice) in [
            "Strongly agree",
            "Agree",
            "Neutral",
            "Disagree",
            "Strongly disagree",
        ]
        .iter()
        .enumerate()
        {
            let choice_id = c.id();
            c.create(
                &choice_id,
                NodeType::Choice,
                Some(&id),
                format!("{n:03}"),
                json!({"label":choice,"value":(n + 1).to_string()}),
                None,
            );
        }
    }
    c
}
