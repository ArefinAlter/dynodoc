//! The closed table of what the agent may call. The page declares the tools of a run (names,
//! descriptions, parameters) and executes them; this module is the second lock: a run refuses any
//! name that is not in the table, whatever the page or the model says. No name here approves,
//! merges, declines, commits, pushes, deletes, changes a role or a rule, reads a key or runs a
//! command; a test keeps it so, and keeps this list equal to the client's own (`lib/agentTools.ts`).

use super::types::ToolDecl;

pub const CLOSED: [&str; 9] = [
    "history.read",
    "history.verify",
    "diff.get",
    "documents.list",
    "documents.outline",
    "requests.list",
    "requests.summarise",
    "review.propose_comment",
    "commit.draft_summary",
];

/// The most of a tool's answer that goes back to the model (it is untrusted text, and long).
pub const MAX_RESULT_BYTES: usize = 24 * 1024;

pub fn in_table(name: &str) -> bool {
    CLOSED.contains(&name)
}

pub fn validate(decls: &[ToolDecl]) -> Result<(), String> {
    let mut seen = std::collections::HashSet::new();
    for d in decls {
        if !in_table(&d.name) {
            return Err(format!("{} is not a tool the agent may have.", d.name));
        }
        if !seen.insert(d.name.clone()) {
            return Err(format!("{} is declared twice.", d.name));
        }
        if d.description.chars().count() > 600 || d.parameters.to_string().len() > 6000 || !d.parameters.is_object() {
            return Err(format!("The description of {} is too long or not an object.", d.name));
        }
    }
    Ok(())
}

/// A tool's answer as it goes to the model: control characters out, cut at the limit.
pub fn clip(output: &str) -> String {
    let clean: String = output.chars().filter(|c| !c.is_control() || *c == '\n' || *c == '\t').collect();
    if clean.len() <= MAX_RESULT_BYTES {
        return clean;
    }
    let mut end = MAX_RESULT_BYTES;
    while !clean.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}\n[cut: the answer was longer than {} KiB]", &clean[..end], MAX_RESULT_BYTES / 1024)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn nothing_in_the_table_can_decide_or_reach_outside() {
        for name in CLOSED {
            for banned in ["approve", "merge", "decline", "withdraw", "delete", "revoke", "role", "rule", "shell", "exec", "key", "token", "push", "request_changes", "restore"] {
                assert!(!name.contains(banned), "{name} contains {banned}");
            }
        }
        // `commit.draft_summary` drafts text for a person to use; it commits nothing.
        assert!(CLOSED.contains(&"commit.draft_summary"));
        assert!(!CLOSED.iter().any(|n| *n == "commit" || n.starts_with("commit.") && *n != "commit.draft_summary"));
    }

    #[test]
    fn the_table_is_the_clients_table() {
        let source = include_str!("../../../client/src/lib/agentTools.ts");
        let start = source.find("export const TOOLS").expect("the client's table");
        let table = &source[start..];
        let table = &table[..table.find("];").unwrap()];
        let mut names: Vec<String> = table
            .split("name: '")
            .skip(1)
            .map(|s| s.split('\'').next().unwrap().to_string())
            .collect();
        names.sort();
        let mut mine: Vec<String> = CLOSED.iter().map(|s| s.to_string()).collect();
        mine.sort();
        assert_eq!(names, mine, "agentTools.ts and agent/tools.rs name the same tools");
        // And what the client says no tool does is not here.
        let person = source.split("export const PERSON_ONLY = [").nth(1).unwrap().split(']').next().unwrap();
        for action in person.split(',').map(|s| s.trim().trim_matches('\'')) {
            assert!(!CLOSED.iter().any(|n| n.split('.').next() == Some(action) && *n != "commit.draft_summary"), "{action} must stay a person's");
        }
    }

    #[test]
    fn declarations_outside_the_table_are_refused() {
        let ok = ToolDecl { name: "history.read".into(), description: "d".into(), parameters: json!({ "type": "object" }) };
        assert!(validate(&[ok.clone()]).is_ok());
        let evil = ToolDecl { name: "requests.merge".into(), ..ok.clone() };
        assert!(validate(&[evil]).is_err());
        assert!(validate(&[ok.clone(), ok.clone()]).is_err());
        let big = ToolDecl { description: "x".repeat(601), ..ok };
        assert!(validate(&[big]).is_err());
    }

    #[test]
    fn a_long_answer_is_cut_and_stripped() {
        let out = clip(&format!("a\u{0}b{}", "é".repeat(30_000)));
        assert!(out.len() < MAX_RESULT_BYTES + 100);
        assert!(!out.contains('\u{0}') && out.contains("[cut"));
        assert_eq!(clip("short"), "short");
    }
}
