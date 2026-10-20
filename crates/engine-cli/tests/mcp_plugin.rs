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
}

#[test]
fn the_skill_is_within_the_vendor_limits_and_names_the_real_tools() {
    let skill = read(plugin().join("skills/dynodoc/SKILL.md"));
    let rest = skill.strip_prefix("---\n").expect("front matter");
    let (front, body) = rest.split_once("\n---\n").expect("front matter ends");
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
        body.len() <= 6_000,
        "the skill is {} bytes; keep it short",
        body.len()
    );
    for tool in Profile::read_only().tools() {
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
    // Nothing offered that does not exist.
    for tool in ["propose", "submit"] {
        assert!(Tool::parse(tool).is_none());
        assert!(
            !body.contains(&format!("`{tool}`")),
            "{tool} is not offered by this server"
        );
    }
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
