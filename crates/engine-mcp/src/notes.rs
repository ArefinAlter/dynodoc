//! The one line this crate adds to the answer of a write tool.
//!
//! A person reviews and merges every proposal; the agent only proposes. The service says so
//! in a header word (`state=draft`), which a model may skim past, so the answer of `propose`,
//! `submit` and a proposal's `status` ends with a line that spells out what the state means:
//! `note "proposal only ...; nothing is live ..."`. The sentences are fixed in this file and
//! chosen from the header's `state` word alone; nothing the service or a document wrote is
//! repeated in them. The service's own text, header and body, is passed on unchanged before
//! the note.
//!
//! Whether a change is live is a fact about the team version; no answer of this crate ever
//! claims a change is live, merged or applied except `state=merged`, which the service
//! reports after a person merged it.

use crate::{backend::header_pairs, tools::Tool};

/// `text` with the note of `tool` appended, or `text` unchanged for a tool that has none.
pub fn annotate(tool: Tool, text: String) -> String {
    let Some(note) = note_for(tool, &text) else {
        return text;
    };
    let mut out = text;
    if !out.ends_with('\n') {
        out.push('\n');
    }
    out.push_str("note \"");
    out.push_str(&note);
    out.push_str("\"\n");
    out
}

/// What a `state` word means for the person who reads the agent's report.
fn state_note(state: &str) -> &'static str {
    match state {
        "draft" => "proposal only, still a private draft; nothing is live. A person must review and merge it.",
        "open" => "proposal open for review; nothing is live until a person approves and merges it.",
        "changes_requested" => "reviewers asked for changes; nothing is live.",
        "declined" => "declined by a reviewer; nothing is live.",
        "merged" => "merged by a person after review.",
        "withdrawn" => "withdrawn; nothing is live. A person can still reopen it.",
        "awaiting_person" => "waiting for a person to submit; nothing is live. Tell the user to open the proposal in Dynodoc and submit it.",
        _ => "state not recognised; do not assume anything is live.",
    }
}

fn note_for(tool: Tool, text: &str) -> Option<String> {
    let pairs = header_pairs(text.lines().next().unwrap_or(""));
    let get = |key: &str| {
        pairs
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    };
    match tool {
        Tool::Propose => {
            if get("would_apply").is_some() {
                return Some(
                    "dry run: nothing was stored and there is no proposal yet. Repeat the call without dry_run to stage it as a proposal for people to review."
                        .to_owned(),
                );
            }
            let mut note = String::new();
            if matches!(get("replayed"), Some("1" | "true")) {
                note.push_str("replay of an earlier call, nothing new was stored; ");
            }
            note.push_str(state_note(get("state").unwrap_or("draft")));
            let stale = get("stale")
                .and_then(|n| n.parse::<u64>().ok())
                .unwrap_or(0);
            if stale > 0 || get("warn").is_some_and(|w| w.split(',').any(|w| w == "stale")) {
                match get("stale_refs").filter(|refs| !refs.is_empty()) {
                    Some(refs) => note.push_str(&format!(
                        " stale: {refs} changed after the rev you read; read them again and revise the proposal if it matters."
                    )),
                    None => note.push_str(
                        " stale: some targets changed after the rev you read; read them again and revise the proposal if it matters.",
                    ),
                }
            }
            Some(note)
        }
        Tool::Submit => {
            let state = get("state")?;
            Some(
                match (state, get("changed")) {
                    ("open", Some("1" | "true")) => {
                        "submitted for review; nothing is live until a person approves and merges it."
                    }
                    ("open", _) => "already submitted, waiting for review; nothing is live.",
                    (other, _) => state_note(other),
                }
                .to_owned(),
            )
        }
        Tool::Status => match get("what")? {
            "proposal" => Some(state_note(get("state")?).to_owned()),
            "access" if get("write_mode") == Some("none") => Some(
                "write_mode=none: this project lets agents read only; propose is refused."
                    .to_owned(),
            ),
            _ => None,
        },
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(tool: Tool, header: &str) -> Option<String> {
        let text = annotate(tool, format!("{header}\n"));
        text.lines().nth(1).map(|l| {
            l.trim_start_matches("note \"")
                .trim_end_matches('"')
                .to_owned()
        })
    }

    #[test]
    fn a_stored_proposal_is_never_said_to_be_live() {
        for state in [
            "draft",
            "open",
            "changes_requested",
            "declined",
            "withdrawn",
            "?",
        ] {
            let said = note(
                Tool::Propose,
                &format!("dd/1 propose file=7034d450 proposal=x rev=1 applied=1 state={state}"),
            )
            .unwrap();
            assert!(
                said.contains("nothing is live") || said.contains("do not assume"),
                "{state}: {said}"
            );
        }
        // No state word at all (it cannot happen) reads as a draft.
        let said = note(Tool::Propose, "dd/1 propose file=7034d450 applied=1").unwrap();
        assert!(said.contains("proposal only") && said.contains("A person must review"));
    }

    #[test]
    fn dry_runs_replays_and_stale_targets_are_each_said() {
        let dry = note(
            Tool::Propose,
            "dd/1 propose file=7034d450 rev=1 applied=0 would_apply=3",
        )
        .unwrap();
        assert!(dry.starts_with("dry run: nothing was stored"));
        let replay = note(
            Tool::Propose,
            "dd/1 propose file=7034d450 proposal=x applied=1 stale=1 stale_refs=7C02,A19D state=draft replayed=1 warn=stale",
        )
        .unwrap();
        assert!(replay.starts_with("replay of an earlier call"), "{replay}");
        assert!(replay.contains("stale: 7C02,A19D changed after the rev you read; read them again"));
        let counted = note(Tool::Propose, "dd/1 propose applied=1 stale=2 state=draft").unwrap();
        assert!(counted.contains("stale: some targets changed"));
    }

    #[test]
    fn submit_and_status_say_what_the_state_means() {
        let said = |header: &str| note(Tool::Submit, header).unwrap();
        assert!(
            said("dd/1 submit action=submit state=awaiting_person changed=0")
                .starts_with("waiting for a person to submit; nothing is live.")
        );
        assert!(said("dd/1 submit action=submit state=open changed=1")
            .starts_with("submitted for review; nothing is live"));
        assert!(
            said("dd/1 submit action=submit state=open changed=0").starts_with("already submitted")
        );
        assert!(
            said("dd/1 submit action=withdraw state=withdrawn changed=1")
                .starts_with("withdrawn; nothing is live")
        );
        let status = |header: &str| note(Tool::Status, header);
        assert!(status("dd/1 status what=proposal state=changes_requested")
            .unwrap()
            .starts_with("reviewers asked for changes"));
        assert_eq!(
            status("dd/1 status what=proposal state=merged").unwrap(),
            "merged by a person after review."
        );
        assert!(
            status("dd/1 status what=access role=editor write_mode=none")
                .unwrap()
                .starts_with("write_mode=none")
        );
        assert_eq!(
            status("dd/1 status what=access role=editor write_mode=draft"),
            None
        );
    }

    #[test]
    fn the_read_tools_and_odd_answers_are_left_alone() {
        for tool in Tool::READ_ONLY.into_iter().filter(|t| *t != Tool::Status) {
            let text = "dd/1 outline file=7034d450 rev=1\n".to_owned();
            assert_eq!(annotate(tool, text.clone()), text);
        }
        // A submit answer with no state word gets no note rather than a guess.
        assert_eq!(
            annotate(Tool::Submit, "dd/1 submit\n".into()),
            "dd/1 submit\n"
        );
        // The note starts a line of its own even when the answer lacks a final line feed.
        assert_eq!(
            annotate(Tool::Submit, "dd/1 submit state=withdrawn".into()),
            "dd/1 submit state=withdrawn\nnote \"withdrawn; nothing is live. A person can still reopen it.\"\n"
        );
    }
}
