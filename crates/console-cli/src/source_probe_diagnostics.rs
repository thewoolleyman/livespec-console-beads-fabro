//! Diagnostic text for a failed source-command probe
//! (`livespec-console-beads-fabro-pzbdbo.29` AC3).
//!
//! `SystemSourceProbe::run_command` (the binary's composition root, `#[cfg]`-
//! excluded from test/coverage builds because it spawns a real subprocess)
//! used to fold every command that ran and exited non-zero straight to the
//! bare `"source command exited non-zero"` reason -- `output.stderr` was read
//! into the child's `Output` and then dropped on the floor. That is the ONE
//! piece of text that would explain WHY a command-based source (livespec,
//! reconcile-runs, github, orchestrator, fabro) went unreachable, and with it
//! discarded there was nothing left to diagnose a real, live failure with.
//!
//! This module holds the two decisions that turn a captured stderr into a
//! safe, bounded, informative reason, as pure text transforms over injected
//! inputs -- so they are testable even though the process spawn that feeds
//! them is not:
//!
//! * **Bounding it.** `source.not_observed_finding_observed` writes this text
//!   to the event store on EVERY failed poll of a command-based source, so an
//!   unbounded copy of stderr would let one pathological CLI (a stack trace, a
//!   runaway debug dump) balloon the store forever.
//! * **Redacting it.** These commands run inside this repo's 1Password
//!   credential wrapper (CLAUDE.md "Beads runtime prerequisites"), which
//!   injects real secrets -- a shared work-items store password among them --
//!   into the shelled command's own environment. A crashing CLI can plausibly
//!   echo its environment back (a Python traceback's locals, a `set -x`
//!   trace, a connection-string error), and that text is exactly what this
//!   module writes into a stored, permanent event payload.

use std::collections::BTreeMap;

/// Stderr text longer than this is truncated before it is folded into a
/// stored not-observed reason. 4096 bytes comfortably holds many lines of a
/// normal CLI failure (a Python traceback's last few frames, a one-line
/// "command not found") while still bounding a flood -- generous for the
/// diagnostic, small next to the rest of the event payload it rides in.
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

/// The not-observed reason for a source command that ran and exited non-zero.
///
/// Composes an exit-status summary (`status_display`, e.g. `exit status: 1`
/// or the platform's signal-death rendering) with whatever stderr the command
/// produced, redacted then bounded. Never empty -- a command that exits
/// non-zero with no stderr still reports the exit status, which is strictly
/// more than the bare `"source command exited non-zero"` reason this text
/// replaces.
#[must_use]
pub fn describe_command_failure(
    status_display: &str,
    stderr: &str,
    env: &BTreeMap<String, String>,
) -> String {
    let redacted = redact_secret_env_values(stderr.trim(), env);
    let bounded = bounded_diagnostic_text(&redacted);
    if bounded.is_empty() {
        format!("source command exited non-zero ({status_display})")
    } else {
        format!("source command exited non-zero ({status_display}): {bounded}")
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
            describe_command_failure("exit status: 1", "boom: no such file", &BTreeMap::new());
        assert_eq!(
            reason,
            "source command exited non-zero (exit status: 1): boom: no such file"
        );
    }

    #[test]
    fn falls_back_to_the_bare_exit_status_when_stderr_is_blank() {
        let reason = describe_command_failure("exit status: 2", "   ", &BTreeMap::new());
        assert_eq!(reason, "source command exited non-zero (exit status: 2)");
    }

    #[test]
    fn distinguishes_a_signal_death_from_a_plain_exit_code() {
        let exit = describe_command_failure("exit status: 1", "", &BTreeMap::new());
        let signal = describe_command_failure("signal: 9 (SIGKILL)", "", &BTreeMap::new());
        assert_ne!(exit, signal);
        assert!(signal.contains("signal: 9"));
    }

    #[test]
    fn redacts_a_secret_shaped_env_var_value_found_verbatim_in_stderr() {
        let env = env_with(&[("SOME_STORE_PASSWORD", "hunter2-supersecret")]);
        let reason =
            describe_command_failure("exit status: 1", "auth failed: hunter2-supersecret", &env);
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
        let reason = describe_command_failure("exit status: 1", "flag abc invalid", &env);
        assert!(reason.contains("abc"));
    }

    #[test]
    fn a_non_secret_shaped_env_var_is_never_redacted() {
        let env = env_with(&[("LIVESPEC_CONSOLE_REPO_PATH", "/data/projects/console-repo")]);
        let reason = describe_command_failure(
            "exit status: 1",
            "cannot find /data/projects/console-repo",
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
