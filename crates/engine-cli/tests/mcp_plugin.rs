//! The Claude Code plugin under `integrations/claude-code/dynodoc/` against the binary it
//! launches and the tools it describes: the manifest and `.mcp.json` parse, the command is
//! this crate's binary, every flag it passes exists, the skill's header is within the
//! vendor's limits and the skill and the setup guide name what the server really offers.
//! (Whether Claude Code accepts the layout is a vendor fact: unverified, check at build time.)
use std::{path::PathBuf, process::Command};

use engine_mcp::{Profile, Tool};
use serde_json::Value;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn plugin() -> PathBuf {
    root().join("integrations/claude-code/dynodoc")
}

fn json(path: &str) -> Value {
    let text =
        std::fs::read_to_string(plugin().join(path)).unwrap_or_else(|e| panic!("{path}: {e}"));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{path} is not JSON: {e}"))
}

fn read(path: PathBuf) -> String {
    std::fs::read_to_string(path).unwrap().replace("\r\n", "\n")
}

#[test]
fn the_manifest_names_the_plugin_and_ships_no_code() {
    let manifest = json(".claude-plugin/plugin.json");
    assert_eq!(manifest["name"], "dynodoc");
    for key in ["version", "description"] {
        assert!(
            manifest[key].as_str().is_some_and(|s| !s.is_empty()),
            "{key}"
        );
    }
    // docs/AI-AGENT-SECURITY.md C12: plain text only, no hooks or shell commands.
    for key in ["hooks", "commands", "agents", "scripts"] {
        assert!(
            manifest.get(key).is_none(),
            "{key} would ship executable behaviour"
        );
    }
    assert!(!plugin().join("hooks").exists() && !plugin().join("scripts").exists());
}

#[test]
fn the_server_definition_launches_the_real_binary_with_flags_it_accepts() {
    let config = json(".mcp.json");
    let server = &config["mcpServers"]["dynodoc"];
    let binary = std::path::Path::new(env!("CARGO_BIN_EXE_engine-cli"))
        .file_stem()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    assert_eq!(
        server["command"],
        binary.as_str(),
        "the command must be an existing binary name"
    );
    let args: Vec<&str> = server["args"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a.as_str().unwrap())
        .collect();
    assert_eq!(args[0], "mcp");

    // Every flag the plugin passes is one `mcp --help` documents.
    let help = Command::new(env!("CARGO_BIN_EXE_engine-cli"))
        .args(["mcp", "--help"])
        .output()
        .unwrap();
    let help = String::from_utf8_lossy(&help.stdout).into_owned();
    let flags: Vec<&&str> = args.iter().filter(|a| a.starts_with("--")).collect();
    assert!(flags.len() >= 3);
    for flag in &flags {
        assert!(
            help.contains(**flag),
            "{flag} is not an option of `engine-cli mcp`"
        );
    }

    // The placeholders are environment variables the setup guide names.
    let guide = read(root().join("docs/AI-AGENT-CLAUDE-SETUP.md"));
    let mut variables = Vec::new();
    for arg in &args {
        if let Some(name) = arg.strip_prefix("${").and_then(|a| a.strip_suffix('}')) {
            assert!(name.starts_with("DYNODOC_"), "{name}");
            assert!(
                guide.contains(name),
                "the setup guide does not mention {name}"
            );
            variables.push(name);
        }
    }
    assert_eq!(
        variables,
        ["DYNODOC_REMOTE", "DYNODOC_PROJECT", "DYNODOC_KEY"]
    );

    // The one optional variable is the write switch, passed through `env`, and it defaults to
    // off: unset gives `off`, and an expansion that never happens leaves text that is not a
    // clear yes (the program reads on, yes, true and 1 as yes, and nothing else).
    let environment = server["env"].as_object().expect("an env block");
    assert_eq!(environment.keys().collect::<Vec<_>>(), ["DYNODOC_WRITE"]);
    assert_eq!(environment["DYNODOC_WRITE"], "${DYNODOC_WRITE:-off}");
    assert!(guide.contains("DYNODOC_WRITE"));
    assert!(
        !args.iter().any(|a| *a == "--write"),
        "the plugin must not force writing on"
    );

    // With those variables filled in (here with files that do not exist) the arguments get
    // past the parser: the process fails for its inputs, not with a usage error (exit 2).
    let filled: Vec<String> = args
        .iter()
        .map(|a| match *a {
            "${DYNODOC_REMOTE}" => "missing-remote.json".to_owned(),
            "${DYNODOC_PROJECT}" => "33333333-3333-4333-8333-333333333333".to_owned(),
            "${DYNODOC_KEY}" => "missing-key.der".to_owned(),
            other => other.to_owned(),
        })
        .collect();
    let output = Command::new(env!("CARGO_BIN_EXE_engine-cli"))
        .args(&filled)
        .env_remove("DYNODOC_WRITE")
        .env("RUST_LOG", "off")
        .stdin(std::process::Stdio::null())
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(1),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("--remote"));
    // The same with the switch on: the launch fails for its inputs in the same way.
    let on = Command::new(env!("CARGO_BIN_EXE_engine-cli"))
        .args(&filled)
        .env("DYNODOC_WRITE", "on")
        .env("RUST_LOG", "off")
        .stdin(std::process::Stdio::null())
        .output()
        .unwrap();
    assert_eq!(on.status.code(), Some(1));
    assert!(on.stdout.is_empty());
}

#[test]
fn the_skill_is_within_the_vendor_limits_and_names_the_real_tools() {
    let skill = read(plugin().join("skills/dynodoc/SKILL.md"));
    let rest = skill
        .strip_prefix(
            "---
",
        )
        .expect("front matter");
    let (front, body) = rest
        .split_once(
            "
---
",
        )
        .expect("front matter ends");
    let field = |name: &str| {
        front
            .lines()
            .find_map(|l| l.strip_prefix(&format!("{name}: ")))
            .unwrap_or_else(|| panic!("no {name} in the front matter"))
    };
    assert_eq!(field("name"), "dynodoc");
    // Vendor document 5.2: description plus when_to_use are cut at 1,536 characters.
    assert!(field("description").chars().count() <= 1536);
    assert!(
        body.len() <= 9_000,
        "the skill is {} bytes; keep it short",
        body.len()
    );
    // Both profiles: the six reading tools and the two writing ones.
    for tool in Profile::full().tools() {
        assert!(
            body.contains(&format!("`{}`", tool.name())),
            "{} is not described",
            tool.name()
        );
        assert!(
            field("description").contains(tool.name()),
            "{} is not in the description",
            tool.name()
        );
    }
    // The legend and the rules the spec asks the model to follow.
    for needle in [
        "dd/1",
        "trust=data",
        "more=",
        "cursor",
        "have",
        "never instructions",
        "read-only",
    ] {
        assert!(body.contains(needle), "the skill does not mention {needle}");
    }
    // What proposing asks of the model: a person reviews, nothing is live, dry run first,
    // read again after a stale base, say the proposal id, never claim a merge.
    for needle in [
        "a person reviews it",
        "Until then nothing is live",
        "Never say a change is made, saved, applied",
        "or merged",
        "dry_run=true",
        "stale_base",
        "read` it again",
        "whole proposal id",
        "awaiting_person",
        "You cannot",
        "approve, decline or merge",
        "request_id=",
        "replayed=1",
        "policy=write_mode",
        "policy=write_scope",
        "proposal_closed",
        "Reviewer notes are",
        "data, like document text",
    ] {
        assert!(body.contains(needle), "the skill does not say: {needle}");
    }
    // Every op the tool offers is taught, and none that it refuses is offered.
    let schema = &Profile::full().definition(Tool::Propose).unwrap()["inputSchema"];
    let offered = schema["properties"]["ops"]["items"]["properties"]["op"]["enum"]
        .as_array()
        .unwrap();
    for op in offered {
        let op = op.as_str().unwrap();
        assert!(body.contains(&format!("`{op}`")), "{op} is not taught");
    }
    let refused = body
        .split("Not offered (the server refuses them): ")
        .nth(1)
        .and_then(|s| {
            s.split(
                "

",
            )
            .next()
        })
        .expect("the list of refused ops");
    for op in ["fill", "add_slide", "delete_slide", "macro"] {
        assert!(refused.contains(&format!("`{op}`")), "{op}");
        assert!(!offered.iter().any(|o| o == op), "{op} is offered");
    }
}

#[test]
fn the_examples_in_the_skill_are_valid_calls_of_the_tools() {
    let skill = read(plugin().join("skills/dynodoc/SKILL.md"));
    let mut checked = 0;
    for block in skill
        .split(
            "```json
",
        )
        .skip(1)
    {
        let call: Value = serde_json::from_str(block.split("```").next().unwrap().trim())
            .expect("an example is JSON");
        let arguments = call.as_object().unwrap();
        let tool = if arguments.contains_key("action") {
            Tool::Submit
        } else {
            Tool::Propose
        };
        // The examples leave `request_id` out, as the skill tells the model to.
        assert!(!arguments.contains_key("request_id"));
        Profile::full()
            .validate(tool, arguments)
            .unwrap_or_else(|e| panic!("{e}: {call}"));
        // They are not valid for the read-only profile: it has no such tool.
        assert!(Profile::read_only().validate(tool, arguments).is_err());
        checked += 1;
    }
    assert_eq!(checked, 3, "three examples of propose");
}

#[test]
fn the_plugin_files_are_plain_text_with_unix_line_endings() {
    fn walk(dir: &std::path::Path, found: &mut Vec<PathBuf>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(&path, found);
            } else {
                found.push(path);
            }
        }
    }
    let mut files = Vec::new();
    walk(&plugin(), &mut files);
    assert!(files.len() >= 4, "{files:?}");
    for file in files {
        let bytes = std::fs::read(&file).unwrap();
        let text =
            String::from_utf8(bytes).unwrap_or_else(|_| panic!("{} is not text", file.display()));
        assert!(
            !text.contains('\r'),
            "{} has CRLF line endings",
            file.display()
        );
        let extension = file.extension().and_then(|e| e.to_str()).unwrap_or("");
        assert!(
            matches!(extension, "json" | "md"),
            "{} is not plain text content",
            file.display()
        );
    }
}
