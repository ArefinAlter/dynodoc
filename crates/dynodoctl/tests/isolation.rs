//! dynodoctl is proprietary and is NOT part of the public engine mirror
//! (docs/SELF-HOSTED-OPERATIONS.md, "Licence"). The exclusion used to rest on a manual note; these
//! tests make it mechanical: no other crate may depend on dynodoctl (then the mirror, which leaves
//! dynodoctl out, would not build), and no mirror manifest may list it.

use std::fs;
use std::path::{Path, PathBuf};

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("crates/dynodoctl sits two levels below the workspace root")
        .to_path_buf()
}

/// `(package name, its dependency names)` from the text of a Cargo.lock.
fn lock_packages(text: &str) -> Vec<(String, Vec<String>)> {
    let mut packages = Vec::new();
    let mut name = String::new();
    let mut dependencies: Vec<String> = Vec::new();
    let mut in_dependencies = false;
    for line in text.lines() {
        let line = line.trim();
        if line == "[[package]]" {
            if !name.is_empty() {
                packages.push((std::mem::take(&mut name), std::mem::take(&mut dependencies)));
            }
            in_dependencies = false;
        } else if let Some(n) = line.strip_prefix("name = ") {
            name = n.trim_matches('"').to_string();
        } else if line.starts_with("dependencies = [") {
            in_dependencies = true;
        } else if in_dependencies {
            if line == "]" {
                in_dependencies = false;
            } else {
                // `"serde 1.0.1",` or `"serde",`
                let dep = line.trim_matches(|c| c == '"' || c == ',');
                dependencies.push(dep.split_whitespace().next().unwrap_or("").to_string());
            }
        }
    }
    if !name.is_empty() {
        packages.push((name, dependencies));
    }
    packages
}

#[test]
fn no_crate_of_the_workspace_depends_on_dynodoctl() {
    let lock = fs::read_to_string(workspace_root().join("Cargo.lock")).expect("Cargo.lock");
    let packages = lock_packages(&lock);
    assert!(
        packages.iter().any(|(n, _)| n == "dynodoctl"),
        "the lock parser found no dynodoctl package: the test would pass for the wrong reason"
    );
    let dependants: Vec<&str> = packages
        .iter()
        .filter(|(_, deps)| deps.iter().any(|d| d == "dynodoctl"))
        .map(|(n, _)| n.as_str())
        .collect();
    assert!(
        dependants.is_empty(),
        "these packages depend on dynodoctl, which is not part of the public engine mirror: {dependants:?}"
    );
}

#[test]
fn no_manifest_of_another_crate_names_dynodoctl() {
    let crates = workspace_root().join("crates");
    let mut offenders = Vec::new();
    for entry in fs::read_dir(&crates).expect("crates/").flatten() {
        let path = entry.path();
        if path.file_name().is_some_and(|n| n == "dynodoctl") {
            continue;
        }
        let manifest = path.join("Cargo.toml");
        if let Ok(text) = fs::read_to_string(&manifest) {
            if text.contains("dynodoctl") {
                offenders.push(manifest.display().to_string());
            }
        }
    }
    assert!(offenders.is_empty(), "{offenders:?}");
}

#[test]
fn no_mirror_manifest_lists_dynodoctl() {
    // Any file whose name says it describes the mirror, in the places such files live.
    let root = workspace_root();
    let mut checked = 0;
    for dir in ["tools", "scripts", ".github/workflows", "infra"] {
        let Ok(entries) = fs::read_dir(root.join(dir)) else {
            continue;
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
            if !name.contains("mirror") {
                continue;
            }
            checked += 1;
            let text = fs::read_to_string(entry.path()).unwrap_or_default();
            assert!(
                !text.contains("dynodoctl"),
                "{} lists dynodoctl: it must stay out of the public mirror",
                entry.path().display()
            );
        }
    }
    let _ = checked; // none exist in this repository today; the loop is the guard for when one does
}

#[test]
fn the_lock_parser_reads_dependencies_with_and_without_versions() {
    let text = "[[package]]\nname = \"a\"\nversion = \"1\"\ndependencies = [\n \"b 1.0.0\",\n \"dynodoctl\",\n]\n\n[[package]]\nname = \"b\"\nversion = \"1\"\n";
    let parsed = lock_packages(text);
    assert_eq!(parsed.len(), 2);
    assert_eq!(parsed[0].1, vec!["b".to_string(), "dynodoctl".to_string()]);
    assert!(parsed[1].1.is_empty());
}
