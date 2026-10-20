//! Stdout belongs to the protocol. The library prints nothing and names the process's
//! standard output in exactly one place, `Server::serve_stdio`; logging goes through
//! `tracing`, which the binary points at stderr. (The behavioural half of this guarantee,
//! that hostile input never puts anything else on the writer, is in `tests/server.rs`.)

use std::path::Path;

const FORBIDDEN: [&str; 7] = [
    "println!",
    "print!(",
    "eprintln!",
    "eprint!(",
    "dbg!(",
    "std::io::stdout",
    "std::process::exit",
];

#[test]
fn nothing_in_the_library_prints_and_only_serve_stdio_names_stdout() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = 0;
    let mut stdout_mentions = Vec::new();
    for entry in std::fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_none_or(|e| e != "rs") {
            continue;
        }
        files += 1;
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let source = std::fs::read_to_string(&path).unwrap();
        for (number, line) in source.lines().enumerate() {
            let code = line.trim_start();
            if code.starts_with("//") || code.starts_with("#!") {
                continue;
            }
            for call in FORBIDDEN {
                assert!(!code.contains(call), "{name}:{}: {call}", number + 1);
            }
            if code.contains("stdout") {
                stdout_mentions.push(format!("{name}: {code}"));
            }
        }
    }
    assert!(files >= 8, "only {files} source files were scanned");
    assert_eq!(
        stdout_mentions,
        ["server.rs: tokio::io::stdout(),"],
        "stdout may be named only by Server::serve_stdio"
    );
}
