# Console impl-dispatch — thread handoff

**Status:** ACTIVE, but see §"Is this thread still earning its keep?" — its
original job has largely migrated to `plan/cockpit-ux-docs-release/`.

**Refreshed:** 2026-07-19 (full ledger re-derivation; master at `b4f1b8f`).

## Scope

This thread is the **dispatch-queue view** of the console repo: what is
dispatchable right now, what is gated, and *on whom*. It is deliberately NOT a
second copy of the program plan — the spec-driven implementation program lives in
`plan/cockpit-ux-docs-release/handoff.md` and that file is authoritative for
B1–B8 status.

## Read First

Derive live work status from the Beads ledger before acting:

```bash
/data/projects/1password-env-wrapper/with-livespec-env.sh -- bd list --json
/data/projects/1password-env-wrapper/with-livespec-env.sh -- python3 \
  /home/ubuntu/.claude/plugins/cache/livespec-orchestrator-beads-fabro/livespec-orchestrator-beads-fabro/<VERSION>/scripts/bin/next.py \
  --project-root /data/projects/livespec-console-beads-fabro --json
```

> **`<VERSION>` moves.** The previous refresh of this file hard-coded plugin
> version `0.13.9`; that path no longer exists and the command failed outright.
> Resolve the live version rather than copying a literal — the SessionStart hook
> prints the installed commit-ish, or `ls` the cache dir and take the newest.
> Equivalently, invoke the skill `/livespec-orchestrator-beads-fabro:next`.

A bare `bd list` fails with `Access denied` — the tenant password only arrives
through the credential wrapper. The ledger is authoritative; this file is a
durable orientation note that goes stale between refreshes.

## THE DISPATCH QUEUE (2026-07-19) — 17 open items

**Autonomously dispatchable right now: effectively nothing.** One item is `ready`
and it is a deliberate long-run exercise, not queue work:

- **`-6sf`** (`ready`) — "Add console-domain crate docs". This is the **>60-minute
  TTL exercise**: its mandated first step is ~67 minutes of `sleep`, existing
  solely to prove the publish node re-mints an expired GitHub-App installation
  token. The actual code change is one `//!` doc comment. **Run it only when
  intentionally exercising that path** — it is not a queue-drain candidate, and
  draining it "because it's ready" wastes an hour and proves nothing.

Everything else is gated. The gates are the real content of this file:

### Gate 1 — ONE genuine maintainer valve

- **`-f2k`** (`acceptance`) — E2E tmux harness private-socket scoping +
  enforcement check. The work **is done and merged** (`dd2ccb4` "enforce private
  tmux sockets in e2e harness", `85b2976` "harden tmux socket-scope
  enforcement"); `acceptance` is the correct lane for delivered work awaiting the
  human accept. **This is a real valve — walk it.** Cross-repo child of livespec
  epic `livespec-yiycvd` (tmux-fleet-kill-prevention), whose thread carries a
  binding maintainer instruction that "done" means *demonstrated live on the
  target runtime*, not merged — so check that thread's bar before accepting.

### Gate 1b — STALE RECORDS: five `pending-approval` items are ALREADY DELIVERED

**Do not walk these as admission valves. They would admit work that is already
merged on master.** Verified against the code 2026-07-19:

| Item | Claim | Code evidence |
|---|---|---|
| `-636m46` **W3** | config port via orchestrator API; delete direct JSONC writer | `DispatcherSettingsPort` live (`console-cli/src/main.rs:191`); `LivespecJsoncArmingPort`, `set_autonomous_mode_in_jsonc`, `read_autonomous_mode_from_jsonc` **all absent** — deleted as required |
| `-j3ts23` **W4** | `TuiView::Settings` + delete arming surface | `TuiView::Settings` present (`console-application/src/lib.rs:107,120`); `AutonomousModeConfirm`, `ToggleAutonomousMode`, `toggle_autonomous_mode` **all absent** |
| `-2ctzhm` **W5** | per-item override valves + context help + README rewrite | `ReviewFixCap` / `AcceptanceReworkCap` valve variants (`:311-380`); `render_help_overlay` takes `selected_section` with a per-pane section per view; README carries the settings table incl. `wip_cap` "structurally not per-item" |
| `-zmunjo` **W6** | mechanical API→Settings→docs completeness check | crate `console-completeness-check` exists and is wired into `just check` (`justfile:163,223`) — and is *currently the gate failing the pin train*, which proves it runs |
| `-yvikqp.1` **W7** | per-item selection + move to ANY status | `selected_work_item_id()` (`:1193`) + `WorkItemMoveRequested` wiring (`:2962`) |

Corroborated independently by `livespec/plan/autonomous-mode/handoff.md`
(2026-07-18, cont. 22), which states the operator surface is "genuinely DONE:
W3/W4/W5-valves/W6/W7" — the ledger records simply never advanced.

Their parent epic **`-yvikqp`** is likewise still `backlog` with every child
delivered. The upstream dependency `bd-ib-wx4lbd` (orchestrator O10) closed
2026-07-16, so nothing was ever waiting on engineering.

**Action: ledger reconciliation, not a valve walk.** Close W3–W7 + `-yvikqp` with
verified-delivery close reasons (the pattern used for `-6tn` and `-0tu`). Note the
ship-guard refuses `accept:` from `pending-approval` for items the factory never
carried, so these want an admin close-as-completed, exactly as `-0tu` did.

**This is the third instance of the same failure mode in one week** (`-6tn`,
`-0tu`, now ×5 + an epic). Delivered work is not getting its ledger record
advanced. That pattern is worth a fix at the process level, not just another
round of cleanup — otherwise the queue keeps accumulating phantom blockers that
make the repo look gated when it is not.

### Gate 2 — needs regroom before dispatch

- `-txtzn5` (`backlog`, epic, `needs-regroom`) — region-coverage gate + CI
  merge-gate fuzz and mutation jobs. Three distinct CI jobs in one epic.
- `-topr34` (`backlog`, chore, `needs-regroom`) — nightly fuzz+mutation soak;
  needs CI beads credentials (host/ops half is not factory-safe).
- `-8aw` (`backlog`, epic) — the four non-valve initial commands
  (`factory.dispatch_item_requested`, `pause`, `resume`,
  `spec.doctor_requested`). Explicitly "regroom separately before building".

### Gate 3 — needs a human decision on substance

- `-25rvmd` (`blocked`, `blocked-reason:needs-human`) — B1 transition-epoch
  follow-up. A not-observed event's id is stable per `(source, repo)`, so a
  *re-down after recovery* dedups onto the original low-`global_seq` row and a
  stale higher-seq positive wins the fold — a source whose latest poll failed can
  render as available in a persistent cross-run store. Needs an id/epoch scheme
  decision, not just code.

### Gate 4 — plain backlog, dispatchable once admitted

- `-mvu22t` (P1) — Rust Red-Green-Replay commit-msg enforcement. **Note the
  inconsistency:** status is `backlog` but it carries a `ready` *label*. One of
  the two is wrong; reconcile before trusting either.
- `-7wy` (P2) — rewrite the section-sign (§) spec-citation in
  `console-application` to file-level form. Wanted *before* the next core-pin
  bump past v0.16.0. **See §"The pin train" — this is NOT what is blocking it.**
- `-ble` (P2) — extend `distinguish_repeatable_command` idempotency-key fix from
  move-only to all repeatable operator actions (`set-admission`,
  `set-acceptance`, `set-override`, `resolve-blocked`, `reject`).
- `-ipi` (P3) — TUI needs-attention render path, lane-derived →
  `attention_item.*` stream. Carry-over from cross-repo epic `livespec-bj9x`,
  now parented to the living `livespec-yes5` hardening epic.
- `-nxsfih` (epic) — console-cruft-cleanup plan-thread anchor; its thread is
  **archived** at `plan/archive/console-cruft-cleanup/` while the epic stays
  `backlog`. Reconcile or close.

## THE PIN TRAIN — 12 stacked dependency PRs, all red for ONE reason

`gh pr list` shows **13 open PRs; 12 are automated pin bumps** (livespec core
v0.17.0 → v0.18.0, oldest #257 from 2026-07-17), plus release-please PR #265
("release 0.3.0", open since 2026-07-18). Master still pins `v0.16.0`.

**Root cause (verified on PR #287, run `29672028993`):** every one fails
`check-completeness` with

```
console-completeness-check: the config-manifest capture was taken at orchestrator
pin `v0.16.0` but `.livespec.jsonc` now pins `v0.18.0` -- the capture is stale;
run `just refresh-config-manifest`
```

The bump-pin automation rewrites `.livespec.jsonc`'s `compat.pinned` but never
refreshes the committed capture at `tests/fixtures/orchestrator-config-manifest.json`,
so the gate fails **deterministically** on every bump. It cannot self-heal in CI:
`just refresh-config-manifest` shells a **live** orchestrator
(`livespec-orchestrator-drive --action config-manifest --json`), which the CI
container has no route to.

Every other check on #287 passes — including `check-doctor-static`. So `-7wy`
(the §-citation item) is **not** the blocker here, despite its description
predicting trouble at the next pin bump past v0.16.0. Do not conflate them.

**This has no work-item.** It is the single highest-value unfiled bug in the
repo: one automation gap holding a dozen PRs. It needs either (a) the bump-pin
workflow to also refresh + commit the capture, or (b) the gate to tolerate a pin
delta and demand refresh only when keys actually change. That is a design call.

Also stale: **PR #195** (dev-tooling pin v0.43.2, open since 2026-07-13) is
obsolete — master is already at v0.49.2. Close it.

## LOOSE ENDS ON DISK

- **`b6-spec-review-fixes`** — a worktree at
  `/home/ubuntu/.worktrees/livespec-console-beads-fabro/b6-spec-review-fixes`,
  clean, one commit ahead of master (`0b6ef76` "docs(spec): propose B6
  corrections — pin the settings-doc anchor, widen the Boundary"). **Pushed to
  origin but has NO open PR.** It postdates the cockpit handoff's worktree
  audit, so that file does not mention it. Decide: open the PR, or reap.
- **`ci-concurrency-group`** — carries uncommitted CI work
  (`.github/workflows/ci.yml` + `Cargo.lock` drift); its head is already in
  master. Another session's in-progress CI-infra worktree. Left untouched by the
  2026-07-19 reap, deliberately.
- `SPECIFICATION/proposed_changes/` is **empty** (README only) on master — the B6
  corrections proposal lives only on the unmerged branch above.

## CORRECTIONS TO THE PREVIOUS REVISION OF THIS FILE

The 2026-07-10 revision had drifted badly. Recorded so the same errors are not
re-derived:

1. **"zero ready candidates / dispatch queue holds no ready work"** — false at
   this refresh; `-6sf` is `ready`. More importantly the framing was wrong: the
   queue is not empty, it is **valve-bound**.
2. **The `next.py` invocation was broken** — hard-coded plugin version `0.13.9`,
   long since removed from the cache. Anyone following the "Read First" block got
   a `No such file or directory`.
3. **It listed `-6tn` as "almost certainly ALREADY SATISFIED"** by reasoning from
   the item's *title*. That reasoning was unsound — the item's real acceptance
   criteria demanded one *specific appended sentence*, not merely the presence of
   a crate-level doc. The conclusion happened to hold: the sentence is present
   verbatim at `crates/console-eventstore/src/lib.rs:8`, landed by `e046b20`.
   **`-6tn` was closed as completed on 2026-07-19** on that verified basis.
4. **It knew nothing of the W3–W7 chain, epic `-yvikqp`, or `-f2k`** — six
   maintainer valves that are now the dominant blocker did not exist at that
   refresh.
5. **`-mb64bv`** (needs-regroom→backlog-bounce rename) and its ratification gate
   `-iblkzp`, both described as live, are **no longer open items** at all.

Separately, `-0tu` ("Remove baked-in explanatory doc prose") was closed
2026-07-19 — delivered by B5 / Scenario 21 / PR #289, exactly as the archived
`console-autonomous-mode` closing record predicted it would need to be.

## HOW THIS REPO RELATES TO THE REST OF THE FLEET

The console is a **participant** in programs driven from `livespec` core, not a
standalone track. Verified 2026-07-19.

### Threads that drive console work

- **`livespec:plan/autonomous-mode/handoff.md`** — the *overall* MVP driver, and
  the authority on **Stage-2** (the maintainer-gated acceptance: drive MULTIPLE
  REAL fleet items end-to-end through the live TUI). Its cont.22 entry records a
  "CRITICAL REORIENTATION": the thread had been tracking only Stage-2 + release
  and had lost the pointer to the console's real remaining MVP body. It now
  correctly defers to `livespec-console-beads-fabro:plan/cockpit-ux-docs-release/`
  as the authoritative program tracker. **A previous "Stage-2 accept" was
  performed on a THROWAWAY item (`bd-ib-dqt`) and does NOT satisfy the
  requirement** — Stage-2 remains genuinely NOT-DONE.
- **`livespec:plan/tmux-fleet-kill-prevention/handoff.md`** — ledger epic
  `livespec-yiycvd`. The console's `-f2k` is its child (mirror of
  `livespec-n3o5e5`, filed in the console tenant because factory dispatch
  requires item-tenant == target-repo-tenant). Origin: an agent working the
  thread killed the maintainer's live tmux fleet — twice. Its cardinal
  instruction is that "done" means installed and demonstrated live with a
  payload, never merged-and-CI-green. **Apply that bar to `-f2k` before
  accepting.**
- **`livespec-orchestrator-beads-fabro`** — the sibling half of ratified spec
  **v034** (epic `bd-ib-24j5uy`, children O0..O10). Its O10 (`bd-ib-wx4lbd`,
  closed 2026-07-16) publishes the machine-readable API-configurable-key manifest
  that this repo's `console-completeness-check` consumes. **The
  No-Circular-Dependency Directive governs the seam**: the orchestrator owns
  setting state and publishes; the console only commands, observes, and checks.
  A check placed on the orchestrator side that read the console would invert the
  dependency and is forbidden.

### Cross-repo epics with a console child

- `livespec-bj9x` (needs-attention rollout, closed) → carried forward into the
  living `livespec-yes5` hardening epic. Console child: **`-ipi`**. Cross-tenant
  items carry no typed `depends_on` — the association is prose only, so it will
  not show up in dependency queries.

### Other livespec threads (no current console dependency, but they move fast)

`fabro-ci-image-factoring`, `overseer-productization`, `rop-sweep-fleet-policy`,
`shipped-hook-seam-hardening` — all active in `livespec:plan/` and several were
touched today. The CI-image thread is the one most likely to reach this repo,
since the console's CI pins `ghcr.io/thewoolleyman/livespec-fabro-sandbox:python-rust-v0.49.2`.

### The seam that keeps biting

Console spec revisions ratify **here**, but the contracts they must satisfy live
in the **orchestrator** repo's `SPECIFICATION/contracts.md`. Three of the five
stale W items quote orchestrator contract line-anchors verbatim. Line anchors
drift; prefer the section names ("Control surface and audit",
"API-configurable completeness") when re-deriving.

## IS THIS THREAD STILL EARNING ITS KEEP?

Raising this deliberately rather than quietly refreshing forever.

The archived `console-autonomous-mode` handoff asserted (2026-07-10, §"Ledger
items") that "`plan/impl-dispatch/` is complete/unrelated — archive separately."
This file's own guardrail says the opposite: archive only when the console
implementation track is complete or the maintainer says so. The track is **not**
complete, so this refresh kept the thread alive.

But its distinct content has thinned. The real implementation program is
`plan/cockpit-ux-docs-release/` (B6/B7/B8 + backfill + Stage-2). What remains
uniquely here is the *queue-and-gates* view above — and on this refresh that view
earned its keep by catching five `pending-approval` records for work already
merged, an unfiled automation bug holding a dozen PRs, and a pushed branch with
no PR. None of those are visible from the program plan. That is an argument for
keeping a dispatch-queue view; it is not an argument that it must be *this*
thread.

**Maintainer call:** keep this as the standing dispatch-queue view, or fold the
gate list into the cockpit thread and archive this one.

## Guardrails

- Do not archive this thread merely because a subchain closes. Archive only when
  the console implementation track itself is complete or the maintainer
  explicitly decides to close this plan topic.
- Always run Beads/orchestrator ledger commands from the repo root under
  `/data/projects/1password-env-wrapper/with-livespec-env.sh`.
- Never hard-code an orchestrator plugin version into this file (see correction
  #2). Resolve it live.
- Repo changes still use the required worktree → PR → merge → cleanup path.
- Do not treat a `ready` lane as dispatch authorization on its own — `-6sf` is
  the standing counter-example.
