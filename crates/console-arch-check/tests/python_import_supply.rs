use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const fn checker() -> &'static str {
    env!("CARGO_BIN_EXE_console-arch-check")
}

fn temp_root(name: &str) -> std::io::Result<PathBuf> {
    let mut root = std::env::temp_dir();
    root.push(format!(
        "console-arch-check-{name}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    fs::create_dir_all(&root)?;
    fs::write(
        root.join("Cargo.toml"),
        r#"
            [workspace]
            members = []

            [workspace.package]
            rust-version = "1.90"
        "#,
    )?;
    fs::write(
        root.join("rust-toolchain.toml"),
        r#"
            [toolchain]
            channel = "1.90.0"
            components = ["clippy", "rustfmt"]
        "#,
    )?;
    Ok(root)
}

fn run_checker(root: &Path) -> std::io::Result<String> {
    let output = Command::new(checker()).current_dir(root).output()?;
    let mut combined = String::from_utf8_lossy(&output.stdout).into_owned();
    combined.push_str(&String::from_utf8_lossy(&output.stderr));
    Ok(combined)
}

#[test]
fn empty_first_party_python_scan_is_flagged() -> std::io::Result<()> {
    let root = temp_root("python-import-supply-empty")?;

    let output = run_checker(&root)?;

    assert!(
        output.contains("first-party Python import-supply scan found no Python files"),
        "{output}"
    );
    fs::remove_dir_all(root).ok();
    Ok(())
}

#[test]
fn undeclared_first_party_python_import_is_flagged() -> std::io::Result<()> {
    let root = temp_root("python-import-supply-undeclared")?;
    fs::create_dir_all(root.join("dev-tooling"))?;
    fs::write(
        root.join("dev-tooling/check.py"),
        "from returns.pipeline import is_successful\n",
    )?;
    fs::write(
        root.join("pyproject.toml"),
        r#"
            [project]
            name = "synthetic"
            version = "0.0.0"
        "#,
    )?;

    let output = run_checker(&root)?;

    assert!(
        output.contains("Python import `returns` maps to distribution `returns`"),
        "{output}"
    );
    assert!(
        output.contains("not declared by this repo and was not found under `_vendor`"),
        "{output}"
    );
    fs::remove_dir_all(root).ok();
    Ok(())
}
