#![forbid(unsafe_code)]

use std::fs;
use std::path::{Path, PathBuf};

const SCANNED_CRATES: &[&str] = &[
    "crates/console-application",
    "crates/console-cli",
    "crates/console-domain",
    "crates/console-eventstore",
    "crates/console-tui",
];

const FORBIDDEN_SNIPPETS: &[&str] = &[".unwrap(", ".expect(", "unsafe {"];

fn main() {
    let root = PathBuf::from(".");
    match run_checks(&root) {
        Ok(()) => {}
        Err(findings) => {
            for finding in findings {
                eprintln!("{finding}");
            }
            std::process::exit(1);
        }
    }
}

fn run_checks(root: &Path) -> Result<(), Vec<String>> {
    let mut findings = Vec::new();
    for crate_dir in SCANNED_CRATES {
        let absolute = root.join(crate_dir);
        check_forbid_unsafe(&absolute, &mut findings);
        check_source_text(&absolute, &mut findings);
    }
    check_domain_dependencies(root, &mut findings);
    if findings.is_empty() {
        Ok(())
    } else {
        Err(findings)
    }
}

fn check_forbid_unsafe(crate_dir: &Path, findings: &mut Vec<String>) {
    for entrypoint in ["src/lib.rs", "src/main.rs"] {
        let path = crate_dir.join(entrypoint);
        if !path.exists() {
            continue;
        }
        let Ok(source) = fs::read_to_string(&path) else {
            findings.push(format!("could not read {}", path.display()));
            continue;
        };
        if !source.contains("#![forbid(unsafe_code)]") {
            findings.push(format!(
                "{} must declare forbid unsafe_code",
                path.display()
            ));
        }
    }
}

fn check_source_text(crate_dir: &Path, findings: &mut Vec<String>) {
    for path in rust_files(crate_dir) {
        let Ok(source) = fs::read_to_string(&path) else {
            findings.push(format!("could not read {}", path.display()));
            continue;
        };
        for snippet in FORBIDDEN_SNIPPETS {
            if source.contains(snippet) {
                findings.push(format!(
                    "{} contains forbidden snippet {snippet}",
                    path.display()
                ));
            }
        }
    }
}

fn rust_files(crate_dir: &Path) -> Vec<PathBuf> {
    let mut pending = vec![crate_dir.join("src")];
    let mut files = Vec::new();
    while let Some(path) = pending.pop() {
        let Ok(metadata) = fs::metadata(&path) else {
            continue;
        };
        if metadata.is_dir() {
            let Ok(entries) = fs::read_dir(&path) else {
                continue;
            };
            for entry in entries.flatten() {
                pending.push(entry.path());
            }
            continue;
        }
        if path.extension().is_some_and(|extension| extension == "rs") {
            files.push(path);
        }
    }
    files
}

fn check_domain_dependencies(root: &Path, findings: &mut Vec<String>) {
    let manifest = root.join("crates/console-domain/Cargo.toml");
    let Ok(source) = fs::read_to_string(&manifest) else {
        findings.push(format!("could not read {}", manifest.display()));
        return;
    };
    for forbidden in [
        "console-adapter",
        "console-eventstore",
        "console-tui",
        "rusqlite",
        "sqlx",
        "reqwest",
        "tokio",
    ] {
        if source.contains(forbidden) {
            findings.push(format!(
                "{} must not depend on infrastructure token {forbidden}",
                manifest.display()
            ));
        }
    }
}
