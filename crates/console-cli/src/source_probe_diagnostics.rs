//! Diagnostic text for a failed source-command probe
//! (`livespec-console-beads-fabro-pzbdbo.29` AC3,
//! `livespec-console-beads-fabro-mx9u.27`).
//!
//! `SystemSourceProbe::run_command` (the binary's composition root, `#[cfg]`-
//! excluded from test/coverage builds because it spawns a real subprocess)
//! used to fold every command that ran and exited non-zero straight to the
//! bare `"source command exited non-zero"` reason -- `output.stderr` was read
//! into the child's `Output` and then dropped on the floor. That is the ONE
//! piece of text that would explain WHY a command-based source (livespec,
//! reconcile-runs, github, orchestrator, fabro) went unreachable, and with it
//! discarded there was nothing left to diagnose a real, live failure with.
//! pzbdbo.29 AC3 fixed that for stderr; mx9u.27 found the other half of the
//! same bug -- at least one source (reconcile-runs) is suspected of reporting
//! its diagnostic on STDOUT while still exiting non-zero, and the probe threw
//! stdout away unconditionally on that path. `describe_command_failure` now
//! carries both streams, each independently labelled so a reader (and any
//! later automated triage) can tell which stream said what.
//!
//! This module holds the decisions that turn captured stderr/stdout into a
//! safe, bounded, informative reason, as pure text transforms over injected
//! inputs -- so they are testable even though the process spawn that feeds
//! them is not:
//!
//! * **Bounding it.** `source.not_observed_finding_observed` writes this text
//!   to the event store on EVERY failed poll of a command-based source, so an
//!   unbounded copy of stderr or stdout would let one pathological CLI (a
//!   stack trace, a runaway debug dump) balloon the store forever. Each
//!   stream is bounded independently, to the same limit.
//! * **Redacting it.** These commands run inside this repo's 1Password
//!   credential wrapper (CLAUDE.md "Beads runtime prerequisites"), which
//!   injects real secrets -- a shared work-items store password among them --
//!   into the shelled command's own environment. A crashing CLI can plausibly
//!   echo its environment back (a Python traceback's locals, a `set -x`
//!   trace, a connection-string error) on EITHER stream, and that text is
//!   exactly what this module writes into a stored, permanent event payload.
//!   Both streams go through the identical redaction path -- stdout is at
//!   least as likely as stderr to echo a connection string.

use std::collections::BTreeMap;

/// Stderr or stdout text longer than this is truncated before it is folded
/// into a stored not-observed reason. 4096 bytes comfortably holds many
/// lines of a normal CLI failure (a Python traceback's last few frames, a
/// one-line "command not found") while still bounding a flood -- generous
/// for the diagnostic, small next to the rest of the event payload it rides
/// in. Applied independently to each stream, so a failure with both a noisy
/// stderr and a noisy stdout can carry up to 4096 bytes of each.
const STDERR_DIAGNOSTIC_LIMIT_BYTES: usize = 4096;

/// Env var names treated as secret-shaped, matched as a case-insensitive
/// substring against every name in the probe's own process environment.
const SECRET_ENV_NAME_MARKERS: [&str; 5] = ["PASSWORD", "TOKEN", "SECRET", "API_KEY", "APIKEY"];

/// Env values this short are skipped even when their name looks secret-shaped.
/// Short values are exactly the shape of unrelated, legitimate text a real
/// stderr line contains (an exit code, a flag) -- redacting them would eat
/// that text instead of protecting anything, and this wrapper's real secrets
/// (a Dolt password, a service-account token) are never this short.
const MIN_REDACTED_VALUE_LEN: usize = 6;

fn is_secret_env_name(name: &str) -> bool {
    let upper = name.to_ascii_uppercase();
    SECRET_ENV_NAME_MARKERS
        .iter()
        .any(|marker| upper.contains(marker))
}

/// Redact every verbatim occurrence of a secret-shaped env var's value in
/// `text`, so a captured stderr can never carry a live credential into the
/// event store.
#[must_use]
pub fn redact_secret_env_values(text: &str, env: &BTreeMap<String, String>) -> String {
    let mut redacted = text.to_owned();
    for (name, value) in env {
        if value.len() >= MIN_REDACTED_VALUE_LEN
            && is_secret_env_name(name)
            && redacted.contains(value.as_str())
        {
            redacted = redacted.replace(value.as_str(), "[REDACTED]");
        }
    }
    redacted
}

/// Bound `text` to [`STDERR_DIAGNOSTIC_LIMIT_BYTES`].
///
/// The truncation is recorded IN the returned text rather than dropped
/// silently, so a bounded reason still says it was bounded.
#[must_use]
pub fn bounded_diagnostic_text(text: &str) -> String {
    if text.len() <= STDERR_DIAGNOSTIC_LIMIT_BYTES {
        return text.to_owned();
    }
    let mut end = STDERR_DIAGNOSTIC_LIMIT_BYTES;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    format!(
        "{}\u{2026} [truncated, showing first {end} of {} bytes]",
        &text[..end],
        text.len()
    )
}

/// Redact then bound one captured stream, ready to fold into a diagnostic
/// reason (or to be skipped, if it comes back empty). Shared by stderr and
/// stdout so both go through the identical path (AC2) -- there is exactly
/// one place that decides what "safe to store" means for captured process
/// output.
fn redact_and_bound(text: &str, env: &BTreeMap<String, String>) -> String {
    let redacted = redact_secret_env_values(text.trim(), env);
    bounded_diagnostic_text(&redacted)
}

/// The not-observed reason for a source command that ran and exited non-zero.
///
/// Composes an exit-status summary (`status_display`, e.g. `exit status: 1`
/// or the platform's signal-death rendering) with whatever stderr and stdout
/// the command produced, each redacted then bounded independently and
/// labelled so the two are distinguishable
/// (`livespec-console-beads-fabro-mx9u.27` AC1). A stream that comes back
/// empty (after trimming, redaction, and bounding) contributes no labelled
/// section at all -- AC3, so an empty stream never pads the reason with
/// `stdout: ` or `stderr: ` and nothing after it. Never empty overall -- a
/// command that exits non-zero with both streams empty still reports the
/// exit status, which is strictly more than the bare `"source command
/// exited non-zero"` reason this text replaces.
#[must_use]
pub fn describe_command_failure(
    status_display: &str,
    stderr: &str,
    stdout: &str,
    env: &BTreeMap<String, String>,
) -> String {
    let bounded_stderr = redact_and_bound(stderr, env);
    let bounded_stdout = redact_and_bound(stdout, env);
    let mut sections = Vec::new();
    if !bounded_stderr.is_empty() {
        sections.push(format!("stderr: {bounded_stderr}"));
    }
    if !bounded_stdout.is_empty() {
        sections.push(format!("stdout: {bounded_stdout}"));
    }
    if sections.is_empty() {
        format!("source command exited non-zero ({status_display})")
    } else {
        format!(
            "source command exited non-zero ({status_display}): {}",
            sections.join(" | ")
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{bounded_diagnostic_text, describe_command_failure, redact_secret_env_values};
    use std::collections::BTreeMap;

    fn env_with(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
            .collect()
    }

    #[test]
    fn carries_stderr_and_exit_status() {
        let reason =
            describe_command_failure("exit status: 1", "boom: no such file", "", &BTreeMap::new());
        assert_eq!(
            reason,
            "source command exited non-zero (exit status: 1): stderr: boom: no such file"
        );
    }

    #[test]
    fn falls_back_to_the_bare_exit_status_when_both_streams_are_blank() {
        let reason = describe_command_failure("exit status: 2", "   ", "  ", &BTreeMap::new());
        assert_eq!(reason, "source command exited non-zero (exit status: 2)");
    }

    #[test]
    fn distinguishes_a_signal_death_from_a_plain_exit_code() {
        let exit = describe_command_failure("exit status: 1", "", "", &BTreeMap::new());
        let signal = describe_command_failure("signal: 9 (SIGKILL)", "", "", &BTreeMap::new());
        assert_ne!(exit, signal);
        assert!(signal.contains("signal: 9"));
    }

    /// AC4: the four-way matrix over which streams are non-empty on a
    /// non-zero exit -- stdout only, stderr only, both, and neither. Neither
    /// is covered by `falls_back_to_the_bare_exit_status_when_both_streams_are_blank`
    /// above; the other three are asserted together here so the labelling and
    /// ordering (stderr before stdout) are pinned in one place.
    #[test]
    fn labels_stdout_and_stderr_distinguishably_across_the_presence_matrix() {
        let stderr_only =
            describe_command_failure("exit status: 1", "stderr line", "", &BTreeMap::new());
        assert_eq!(
            stderr_only,
            "source command exited non-zero (exit status: 1): stderr: stderr line"
        );

        let stdout_only = describe_command_failure(
            "exit status: 1",
            "",
            r#"{"errors": ["survey failed"]}"#,
            &BTreeMap::new(),
        );
        assert_eq!(
            stdout_only,
            "source command exited non-zero (exit status: 1): stdout: {\"errors\": [\"survey failed\"]}"
        );

        let both = describe_command_failure(
            "exit status: 1",
            "stderr line",
            r#"{"errors": ["survey failed"]}"#,
            &BTreeMap::new(),
        );
        assert_eq!(
            both,
            "source command exited non-zero (exit status: 1): stderr: stderr line | stdout: {\"errors\": [\"survey failed\"]}"
        );
    }

    #[test]
    fn a_present_stdout_never_gets_padded_by_an_empty_stderr_section() {
        let reason = describe_command_failure(
            "exit status: 1",
            "",
            "diagnostic on stdout",
            &BTreeMap::new(),
        );
        assert_eq!(
            reason,
            "source command exited non-zero (exit status: 1): stdout: diagnostic on stdout"
        );
        assert!(!reason.contains("stderr:"));
    }

    #[test]
    fn a_present_stderr_never_gets_padded_by_an_empty_stdout_section() {
        let reason = describe_command_failure(
            "exit status: 1",
            "diagnostic on stderr",
            "",
            &BTreeMap::new(),
        );
        assert_eq!(
            reason,
            "source command exited non-zero (exit status: 1): stderr: diagnostic on stderr"
        );
        assert!(!reason.contains("stdout:"));
    }

    #[test]
    fn redacts_a_secret_shaped_env_var_value_found_verbatim_in_stderr() {
        let env = env_with(&[("SOME_STORE_PASSWORD", "hunter2-supersecret")]);
        let reason = describe_command_failure(
            "exit status: 1",
            "auth failed: hunter2-supersecret",
            "",
            &env,
        );
        assert!(!reason.contains("hunter2-supersecret"));
        assert!(reason.contains("[REDACTED]"));
    }

    #[test]
    fn redacts_a_secret_shaped_env_var_value_found_verbatim_in_stdout() {
        let env = env_with(&[("SOME_STORE_PASSWORD", "hunter2-supersecret")]);
        let reason = describe_command_failure(
            "exit status: 1",
            "",
            "connection string: hunter2-supersecret",
            &env,
        );
        assert!(!reason.contains("hunter2-supersecret"));
        assert!(reason.contains("[REDACTED]"));
    }

    #[test]
    fn redacts_every_occurrence_and_every_matching_var() {
        let env = env_with(&[
            ("OP_SERVICE_ACCOUNT_TOKEN", "ops_abcdef123456"),
            ("SOME_OTHER_VAR", "not-a-secret"),
        ]);
        let stderr = "token ops_abcdef123456 rejected (value: not-a-secret); \
                      retried with ops_abcdef123456 again";
        let redacted = redact_secret_env_values(stderr, &env);
        assert!(!redacted.contains("ops_abcdef123456"));
        assert_eq!(redacted.matches("[REDACTED]").count(), 2);
        assert!(redacted.contains("not-a-secret"));
    }

    #[test]
    fn does_not_redact_short_env_values() {
        let env = env_with(&[("SOME_TOKEN", "abc")]);
        let reason = describe_command_failure("exit status: 1", "flag abc invalid", "", &env);
        assert!(reason.contains("abc"));
    }

    #[test]
    fn a_non_secret_shaped_env_var_is_never_redacted() {
        let env = env_with(&[("LIVESPEC_CONSOLE_REPO_PATH", "/data/projects/console-repo")]);
        let reason = describe_command_failure(
            "exit status: 1",
            "cannot find /data/projects/console-repo",
            "",
            &env,
        );
        assert!(reason.contains("/data/projects/console-repo"));
    }

    #[test]
    fn leaves_short_stderr_untouched() {
        assert_eq!(bounded_diagnostic_text("boom"), "boom");
    }

    #[test]
    fn truncation_boundary_is_exact_at_the_limit() {
        let exact = "y".repeat(4096);
        assert_eq!(bounded_diagnostic_text(&exact), exact);
    }

    #[test]
    fn truncates_stderr_over_the_byte_limit_and_says_so_visibly() {
        let over = "x".repeat(5000);
        let bounded = bounded_diagnostic_text(&over);
        assert!(bounded.len() < 5000);
        assert!(bounded.starts_with(&"x".repeat(4096)));
        assert!(bounded.contains("truncated"));
        assert!(bounded.contains("4096"));
        assert!(bounded.contains("5000"));
    }

    #[test]
    fn truncation_respects_a_multi_byte_char_boundary() {
        // Each '€' is 3 UTF-8 bytes and 4096 is not a multiple of 3 (4096 =
        // 3*1365 + 1), so the byte-4096 cut point lands ONE byte into a
        // character -- the back-off loop must actually step back (more than
        // once, if needed) to reach a char boundary, rather than panicking on
        // a sliced code point.
        let over = "\u{20ac}".repeat(2000);
        let bounded = bounded_diagnostic_text(&over);
        assert!(bounded.contains("truncated"));
        assert!(bounded.is_char_boundary(bounded.find('\u{2026}').unwrap_or(0)));
    }
}
