//! Finding E — the console must invoke resolved `.py` backing CLIs through the
//! Python interpreter, not exec them directly.
//!
//! The Claude plugin installer does not uniformly mark the orchestrator's
//! backing scripts executable: on a real host `needs_attention.py` and
//! `drive.py` ship non-executable (`-rw-rw-r--`) while their siblings ship
//! `+x`. Exec-ing the path directly then fails with "Permission denied", so the
//! needs-attention source silently degrades to unavailable (cockpit attention
//! reads 0) and the `drive` operator valves fail the same way.
//!
//! This end-to-end test reproduces the exec-bit gap with a real temporary,
//! non-executable `.py` script and proves that the normalized invocation
//! (`python3 <script> …`) runs it successfully while a direct exec of the same
//! path is refused. `python3` is required by the enforcement suite itself
//! (`just check` runs the Python doctor-static phase), so it is always present
//! wherever this test is gated.

use std::error::Error;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use livespec_console_beads_fabro::python_normalized_invocation;

/// Write a minimal, NON-executable `.py` script that prints a JSON sentinel to
/// stdout, mirroring how the orchestrator's `needs_attention.py` emits `--json`.
fn write_non_executable_script(sentinel: &str) -> Result<PathBuf, Box<dyn Error>> {
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "livespec-console-finding-e-{}-{nanos}",
        std::process::id()
    ));
    fs::create_dir_all(&dir)?;
    let script = dir.join("needs_attention.py");
    fs::write(
        &script,
        format!("#!/usr/bin/env python3\nprint('{sentinel}')\n"),
    )?;
    // 0o644 — readable, NOT executable — exactly the mode the installer leaves
    // on needs_attention.py / drive.py in the marketplace cache.
    fs::set_permissions(&script, fs::Permissions::from_mode(0o644))?;
    Ok(script)
}

#[test]
fn finding_e_non_executable_py_script_runs_through_python3() -> Result<(), Box<dyn Error>> {
    let sentinel = "{\"attention\": [1, 2, 3, 4, 5, 6]}";
    let script = write_non_executable_script(sentinel)?;
    let script_str = script.to_str().ok_or("temp path is not valid UTF-8")?;

    // Sanity: the script really is non-executable (no owner/group/other +x).
    let mode = fs::metadata(&script)?.permissions().mode();
    assert_eq!(mode & 0o111, 0, "the fixture script must be non-executable");

    // The bug: exec-ing the resolved `.py` path directly is refused, so the
    // source would degrade to unavailable (attention 0; valves dead).
    let direct = Command::new(script_str).arg("--json").output();
    assert!(
        matches!(&direct, Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied),
        "direct exec of a non-executable .py must be Permission denied, got: {direct:?}"
    );

    // The fix: the normalized invocation runs the SAME non-executable script
    // through python3 and captures its JSON, so the exec bit no longer matters.
    let (program, args) = python_normalized_invocation(script_str, &["--json"]);
    assert_eq!(program, "python3");
    assert_eq!(args, vec![script_str, "--json"]);

    let normalized = Command::new(program).args(&args).output()?;
    assert!(
        normalized.status.success(),
        "python3-normalized invocation must succeed; stderr: {}",
        String::from_utf8_lossy(&normalized.stderr)
    );
    let stdout = String::from_utf8_lossy(&normalized.stdout);
    assert!(
        stdout.contains(sentinel),
        "python3-normalized invocation must emit the script's JSON; got: {stdout}"
    );

    if let Some(parent) = script.parent() {
        let _ignored = fs::remove_dir_all(parent);
    }
    Ok(())
}
