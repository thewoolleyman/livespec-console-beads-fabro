//! A needs-attention stub serving ONE row that names no work-item.
//!
//! # Why this exists separately from [`super::lifecycle`]
//!
//! The lifecycle fixture serves a `valve:<verb>:<id>` row whose `source_ref`
//! carries a `work_item`, and its walkthrough scene waits on the inbox draining
//! to `attention: 0`. An item that can APPEAR but never RESOLVE would hang those
//! waits forever — the fixture's own doc comment says so — so the
//! not-a-work-item case cannot be bolted onto it. It gets its own stub instead.
//!
//! # What it serves
//!
//! One row shaped like a real plan-thread finding: a `source_ref` with a `path`
//! and NO `work_item`. The console resolves an attention row's work-item from
//! `source_ref.work_item`, so this row's is `None` — which is precisely the
//! state the B2 hint-honesty rule has to handle, and the state that no other
//! tmux scene reaches (they all run against an EMPTY inbox).

use std::path::PathBuf;

use super::{HarnessResult, make_executable};

/// The row's stable id, asserted on by the scene.
pub const ROW_ID: &str = "plan:doc-drift-audit";
/// The row's summary text — the scene asserts this RENDERS, which is what
/// proves the inbox is genuinely populated before it asserts on the hints.
///
/// Kept SHORT on purpose. The Attention pane is ~41 columns at the harness's
/// pinned 112x28, and a longer summary is clipped mid-word, so a
/// `contains(SUMMARY)` assertion would fail on truncation rather than on the
/// behavior under test.
pub const ROW_SUMMARY: &str = "Plan thread awaits a decision";

/// A scratch directory holding the stub, removed on drop.
pub struct PathBackedAttentionFixture {
    dir: PathBuf,
    needs_attention: PathBuf,
}

impl PathBackedAttentionFixture {
    /// Materialize the stub.
    pub fn new(label: &str) -> HarnessResult<Self> {
        let unique = format!("{}-{label}", std::process::id());
        let dir = std::env::temp_dir().join(format!("lc-attention-rows-{unique}"));
        std::fs::create_dir_all(&dir).map_err(|error| {
            format!(
                "create attention-rows dir {} failed: {error}",
                dir.display()
            )
        })?;

        let needs_attention = dir.join("needs-attention.sh");
        std::fs::write(&needs_attention, needs_attention_body())
            .map_err(|error| format!("write {} failed: {error}", needs_attention.display()))?;
        make_executable(&needs_attention)?;

        Ok(Self {
            dir,
            needs_attention,
        })
    }

    /// The `extra_env` pair to hand [`super::TmuxConsole::launch_with_env`].
    ///
    /// Appended AFTER the harness's `{}`-stub exports, so this one backing CLI
    /// wins while every other source stays idle.
    #[must_use]
    pub fn env(&self) -> Vec<(&'static str, String)> {
        vec![(
            "LIVESPEC_CONSOLE_NEEDS_ATTENTION_PROGRAM",
            self.needs_attention.display().to_string(),
        )]
    }
}

impl Drop for PathBackedAttentionFixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// `needs-attention --json`: one path-backed row, unconditionally.
///
/// `source_ref.repo` expands `LIVESPEC_CONSOLE_REPO` at run time for the same
/// reason the lifecycle fixture does — ingest scopes the resolvable prior set by
/// repo, and a mismatched repo yields a row that can appear but never resolve.
/// This scene never drains the inbox, but keeping the shape honest costs
/// nothing and stops the stub becoming a trap for the next reader.
///
/// `work_item` is deliberately ABSENT rather than null: absent is what a real
/// plan-thread finding emits.
///
/// The repo is read as bare `$LIVESPEC_CONSOLE_REPO` rather than
/// `${LIVESPEC_CONSOLE_REPO:-}`: inside a `format!` the braced form reads as a
/// formatting argument with a `:-` spec and trips
/// `clippy::literal_string_with_formatting_args`. An unset variable expands
/// empty either way here.
fn needs_attention_body() -> String {
    format!(
        "#!/usr/bin/env bash\n\
         repo=\"$LIVESPEC_CONSOLE_REPO\"\n\
         printf '{{\"attention\":[{{\"id\":\"{ROW_ID}\",\"kind\":\"plan-thread\",\
         \"urgency\":\"normal\",\"summary\":\"{ROW_SUMMARY}\",\
         \"source_ref\":{{\"repo\":\"%s\",\"path\":\"plan/doc-drift-audit/handoff.md\"}},\
         \"handoff\":{{\"kind\":\"plan\",\"command\":\"open:plan/doc-drift-audit\"}}}}]}}\\n' \
         \"$repo\"\n\
         exit 0\n"
    )
}
