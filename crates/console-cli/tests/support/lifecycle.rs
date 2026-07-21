//! A STATEFUL backing-CLI fixture: one dummy work-item the operator can drive
//! through the lifecycle in a real TUI.
//!
//! # Why the shared `{}` stub is not enough
//!
//! [`super::TmuxConsole`] points every backing CLI at one trivial stub that
//! prints `{}`. That is deliberate and correct for the cockpit-chrome scenarios
//! — it makes each source a deterministic not-observed finding — but it means
//! the board is EMPTY by construction: `{}` is not even a legal
//! `list-work-items --json` payload (the parser wants a JSON array), so no
//! work-item can exist, and no lifecycle can be walked.
//!
//! The B7 walkthrough has to drive a real item from `backlog` to `done` through
//! the shipped keybindings, so it needs backing CLIs that (a) serve a
//! work-item, (b) accept the drive actions the TUI issues, and (c) reflect the
//! resulting lane on the next poll. This fixture supplies exactly that while
//! staying hermetic: no tenant, no network, no credential wrapper.
//!
//! # Shape
//!
//! Three scripts share one state directory:
//!
//! - `work-items.sh` — `--json` -> a one-element array whose `lane`/`status`
//!   are read from the state file.
//! - `needs-attention.sh` — `--json` -> `{"attention":[...]}`, carrying a valve
//!   item only while the lane is one that actually waits on a human
//!   (`pending-approval` -> approve, `acceptance` -> accept). Any other lane
//!   yields an empty inbox, which is what makes "the Attention view empties
//!   when you accept" observable.
//! - `drive.sh` — parses the `--action <id>` the port appends LAST, mutates the
//!   lane, and appends the action id to an audit log the test can assert on.
//!   It also answers the bare `config` read with the six dispatcher settings,
//!   so the Settings view is populated rather than `not observed`.
//!
//! The drive stub deliberately mirrors the orchestrator's real grammar
//! (`approve:<id>`, `accept:<id>`, `move:<id>:<target>`,
//! `resolve-blocked:<id>:<target>`) rather than inventing one, so a walkthrough
//! verified against this fixture is verified against the action ids the console
//! genuinely emits.

use std::path::{Path, PathBuf};

use super::{HarnessResult, make_executable};

/// The dummy work-item every walkthrough step acts on.
pub const ITEM_ID: &str = "livespec-console-beads-fabro-dummy1";
/// Its title, rendered in the detail pane.
pub const ITEM_TITLE: &str = "Dummy work-item for the lifecycle walkthrough";

/// A scratch directory holding the stateful stub scripts and their shared state.
///
/// Dropping the fixture removes the directory, so a failed assertion never
/// leaks a state file into the temp dir.
pub struct LifecycleFixture {
    dir: PathBuf,
    work_items: PathBuf,
    needs_attention: PathBuf,
    drive: PathBuf,
}

impl LifecycleFixture {
    /// Materialize the fixture with the item starting in `initial_lane`.
    pub fn new(label: &str, initial_lane: &str) -> HarnessResult<Self> {
        let unique = format!("{}-{label}", std::process::id());
        let dir = std::env::temp_dir().join(format!("lc-lifecycle-{unique}"));
        std::fs::create_dir_all(&dir)
            .map_err(|error| format!("create lifecycle dir {} failed: {error}", dir.display()))?;

        std::fs::write(dir.join("lane"), format!("{initial_lane}\n"))
            .map_err(|error| format!("seed lane failed: {error}"))?;
        std::fs::write(dir.join("actions.log"), "")
            .map_err(|error| format!("seed action log failed: {error}"))?;

        let state = dir.join("lane");
        let log = dir.join("actions.log");

        let work_items = write_script(&dir, "work-items.sh", &work_items_body(&state))?;
        let needs_attention =
            write_script(&dir, "needs-attention.sh", &needs_attention_body(&state))?;
        let drive = write_script(&dir, "drive.sh", &drive_body(&state, &log))?;

        Ok(Self {
            dir,
            work_items,
            needs_attention,
            drive,
        })
    }

    /// The `extra_env` pairs to hand [`super::TmuxConsole::launch_with_env`].
    ///
    /// They are appended AFTER the harness's own `{}`-stub exports, so these
    /// three win while every other backing CLI stays idle.
    #[must_use]
    pub fn env(&self) -> Vec<(&'static str, String)> {
        vec![
            (
                "LIVESPEC_CONSOLE_LIST_WORK_ITEMS_PROGRAM",
                self.work_items.display().to_string(),
            ),
            (
                "LIVESPEC_CONSOLE_NEEDS_ATTENTION_PROGRAM",
                self.needs_attention.display().to_string(),
            ),
            (
                "LIVESPEC_CONSOLE_DRIVE_PROGRAM",
                self.drive.display().to_string(),
            ),
        ]
    }

    /// Move the item WITHOUT an operator action, standing in for the factory.
    ///
    /// The operator cannot drive every transition: `move` refuses `acceptance`,
    /// `done`, and `pending-approval` (the ship-guard), because those lanes are
    /// reached by the factory finishing work or by a human valve, never by an
    /// operator relocating an item. `active` -> `acceptance` is the factory's
    /// step, so a walkthrough that pretends the operator performs it would
    /// document a transition the TUI cannot make.
    pub fn factory_move(&self, lane: &str) -> HarnessResult<()> {
        std::fs::write(self.dir.join("lane"), format!("{lane}\n"))
            .map_err(|error| format!("factory_move to {lane} failed: {error}"))
    }

    /// The lane the stub currently reports, with whitespace trimmed.
    pub fn lane(&self) -> HarnessResult<String> {
        std::fs::read_to_string(self.dir.join("lane"))
            .map(|value| value.trim().to_owned())
            .map_err(|error| format!("read lane failed: {error}"))
    }

    /// Every drive action id the console has issued, oldest first.
    pub fn actions(&self) -> HarnessResult<Vec<String>> {
        let log = std::fs::read_to_string(self.dir.join("actions.log"))
            .map_err(|error| format!("read action log failed: {error}"))?;
        Ok(log
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(ToOwned::to_owned)
            .collect())
    }
}

impl Drop for LifecycleFixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn write_script(dir: &Path, name: &str, body: &str) -> HarnessResult<PathBuf> {
    let path = dir.join(name);
    std::fs::write(&path, body)
        .map_err(|error| format!("write {} failed: {error}", path.display()))?;
    make_executable(&path)?;
    Ok(path)
}

/// `list-work-items --json`: a one-element array reflecting the current lane.
fn work_items_body(state: &Path) -> String {
    format!(
        "#!/usr/bin/env bash\n\
         lane=$(tr -d '[:space:]' < {state} 2>/dev/null)\n\
         [ -n \"$lane\" ] || lane=backlog\n\
         printf '[{{\"id\":\"{item}\",\"lane\":\"%s\",\"lane_reason\":null,\"rank\":\"a0\",\
         \"status\":\"%s\",\"title\":\"{title}\",\"type\":\"task\",\"origin\":\"freeform\"}}]\\n' \
         \"$lane\" \"$lane\"\n\
         exit 0\n",
        state = shell_quote(state),
        item = ITEM_ID,
        title = ITEM_TITLE,
    )
}

/// `needs-attention --json`: a valve item only while a human is genuinely owed.
///
/// The item's `source_ref.repo` MUST be the REAL ingesting repo (the harness
/// exports `LIVESPEC_CONSOLE_REPO`, so the script expands it at run time):
/// `ingest_needs_attention` scopes the resolvable prior set to
/// `source_ref.repo == <ingesting repo>`, so an item carrying anything else can
/// APPEAR but can never be RESOLVED — the walkthrough's `attention: 0` waits
/// would then time out forever. (The pre-repo-scoping ingest resolved every
/// absent item regardless of repo, which masked exactly this mismatch.)
fn needs_attention_body(state: &Path) -> String {
    format!(
        "#!/usr/bin/env bash\n\
         lane=$(tr -d '[:space:]' < {state} 2>/dev/null)\n\
         case \"$lane\" in\n\
         \x20 pending-approval) verb=approve ;;\n\
         \x20 acceptance) verb=accept ;;\n\
         \x20 *) printf '{{\"attention\":[]}}\\n'; exit 0 ;;\n\
         esac\n\
         repo=\"${{LIVESPEC_CONSOLE_REPO:-}}\"\n\
         printf '{{\"attention\":[{{\"id\":\"valve:%s:{item}\",\"kind\":\"human-valve\",\
         \"urgency\":\"high\",\"summary\":\"%s work-item {item}\",\
         \"source_ref\":{{\"repo\":\"%s\",\"work_item\":\"{item}\"}},\
         \"handoff\":{{\"kind\":\"%s\",\"action_id\":\"%s:{item}\",\"command\":\"%s:{item}\"}}}}]}}\\n' \
         \"$verb\" \"$verb\" \"$repo\" \"$verb\" \"$verb\" \"$verb\"\n\
         exit 0\n",
        state = shell_quote(state),
        item = ITEM_ID,
    )
}

/// `drive --repo <path> --json --action <id>`: mutate the lane, log the action.
///
/// Success is signalled by the EXIT CODE alone — `run_action` discards stdout
/// entirely and switches on the probe outcome — so every branch exits 0 except
/// an unparseable action, which exits 1 so a typo surfaces as a failed valve
/// rather than a silent no-op.
fn drive_body(state: &Path, log: &Path) -> String {
    format!(
        "#!/usr/bin/env bash\n\
         action=''\n\
         while [ $# -gt 0 ]; do\n\
         \x20 case \"$1\" in\n\
         \x20   --action) action=\"$2\"; shift 2 ;;\n\
         \x20   *) shift ;;\n\
         \x20 esac\n\
         done\n\
         if [ \"$action\" = config ]; then\n\
         \x20 printf '{{\"settings\":[\
         {{\"key\":\"auto_approve_ready\",\"value\":false}},\
         {{\"key\":\"merge_on_review_cap\",\"value\":false}},\
         {{\"key\":\"acceptance_mode\",\"value\":\"ai-then-human\"}},\
         {{\"key\":\"review_fix_cap\",\"value\":3}},\
         {{\"key\":\"acceptance_rework_cap\",\"value\":2}},\
         {{\"key\":\"wip_cap\",\"value\":5}}]}}\\n'\n\
         \x20 exit 0\n\
         fi\n\
         printf '%s\\n' \"$action\" >> {log}\n\
         case \"$action\" in\n\
         \x20 approve:*) printf 'ready\\n' > {state} ;;\n\
         \x20 accept:*) printf 'done\\n' > {state} ;;\n\
         \x20 move:*:*|resolve-blocked:*:*) printf '%s\\n' \"${{action##*:}}\" > {state} ;;\n\
         \x20 reject:*:*) printf 'blocked\\n' > {state} ;;\n\
         \x20 set-admission:*|set-acceptance:*|set-*-cap:*|set-config:*) : ;;\n\
         \x20 '') exit 1 ;;\n\
         esac\n\
         printf '{{}}\\n'\n\
         exit 0\n",
        state = shell_quote(state),
        log = shell_quote(log),
    )
}

fn shell_quote(path: &Path) -> String {
    format!("'{}'", path.display().to_string().replace('\'', r"'\''"))
}
