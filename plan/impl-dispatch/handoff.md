# Console impl-dispatch — thread handoff

**Status:** PARKED — grooming complete; factory dispatch **BLOCKED** on the
GitHub-App-auth / dispatch-env credential model, now owned by
`plan/github-app-auth/` (core epic `livespec-2ef0`; resume via
`/livespec-orchestrator-beads-fabro:plan github-app-auth`).

**Parked:** 2026-07-02.

## What this thread is

Groom and dispatch the console's behavioral-coverage impl work (the NFR
clause→scenario→test backfill and its prerequisite scenario realizations) to
the Beads+Fabro factory, following the v012 spec revise that authored the NFR
contributor scenarios.

## Accomplished (done + verified)

1. **Spec v012 landed** — PR #76 merged (rebased onto master). `/livespec:revise`
   accepted proposed change `nfr-contributor-scenarios`: authored 9
   contributor-facing Gherkin theme H2s (Contributor Scenario A–I) in
   `SPECIFICATION/non-functional-requirements.md` §Scenarios, covering all 52
   NFR contributor-facing normative clauses. Also reworded 4 pre-existing
   `§"heading"` source citations to file-level (satisfying
   `doctor-no-spec-section-citation-in-code`). doctor-static: 0 fail.
2. **Ledger status conformance** (done fleet-wide by the coordinator) — the
   console tenant's 10 legacy bd-native `open` items were remediated to
   `backlog`. They predated the 7-state lifecycle redesign and were never
   migrated; `next` correctly returned 0 because it ranks only the `ready`
   lane, not `backlog`.
3. **Grooming** (done this thread, verified) — the two dependency-root leaves
   promoted `backlog → ready` with `admission:auto` + `acceptance:ai-then-human`:
   - `livespec-console-beads-fabro-qvrwag` — Realize Scenario 6 (policy-rejected
     factory drain emits `command.rejected`, no side effect).
   - `livespec-console-beads-fabro-idgql3` — Realize Scenario 7 (command
     crash-gap reconciliation reconstructs a missing outcome).
   Promotion used the impl plugin's own store seams (`update_work_item_status`
   + `client.update_issue(add_labels=[...])`) because no CLI exists for a
   `backlog → ready` promotion with policy-setting (the intake
   Definition-of-Ready checklist is applied at capture time, not
   retroactively).

## Dependency chain (why these two first)

```
qvrwag (Scenario 6) ┐
idgql3 (Scenario 7) ┴─► cvqcnx (operator backfill, 30 clauses) ─► cc3nlr (NFR backfill / B-nfr, 52 clauses) ─► 77t6mk (flip console-spec-check to fail; closes keystone rrr4i4)
```

`cvqcnx`, `cc3nlr`, `77t6mk` remain `backlog` and blocked until the two roots
complete. Keystone epic `rrr4i4` ("Port the Rust behavioral-coverage checker")
stays open: its checker code (`crates/console-spec-check`) already exists in
`warn` mode, but the epic's "done" is the full **fail-mode** gate (the part-(b)
clause backfill + the `77t6mk` flip), so it is NOT closeable now.

## BLOCKED: factory dispatch

Dispatching `idgql3` (canary) to the factory failed twice — both **cleanly
PRE-launch** (no Fabro run, no PR, no merge; **master untouched**) — due to the
dispatch environment under `/usr/local/bin/with-livespec-env.sh`:

1. **`GH_TOKEN` absent.** The wrapper's 1Password Environment projects the
   GitHub **App** credentials (`GITHUB_APP_ID` / `GITHUB_CLIENT_ID` /
   `GITHUB_CLIENT_SECRET` / `GITHUB_PRIVATE_KEY`) plus `CLAUDE_CODE_OAUTH_TOKEN`
   and `BEADS_DOLT_PASSWORD` — but **no bearer `GH_TOKEN`**, which the Dispatcher
   requires to project into the Fabro sandbox. (An App installation token can be
   minted from the App creds — installation `131208965`, verified grants
   `contents:write` + `pull_requests:write` on this repo — but the credential
   MODEL is being redesigned in `plan/github-app-auth`, so do NOT mint ad-hoc
   tokens here.)
2. **`fabro` / toolchain PATH scrubbed.** The wrapper's `env -i` sets a minimal
   `PATH` that drops `/home/ubuntu/.local/bin` (where `fabro` lives) and the
   mise/cargo/just toolchain the post-merge janitor needs.

Both are exactly the credential + dispatch-env problems now owned by
`plan/github-app-auth/` (core epic `livespec-2ef0`). Do NOT reconstruct the
dispatch env or mint ad-hoc tokens in this thread — that work belongs there.

## Resume / next action (single path)

Once `plan/github-app-auth` lands the credential + dispatch-env model:

1. Confirm `idgql3` + `qvrwag` are still `ready` / `admission:auto` /
   `acceptance:ai-then-human` (`list-work-items --json` via the wrapper).
2. Re-attempt the **canary dispatch of `idgql3` only** (single-item
   `dispatcher.py dispatch --item <id>`, NOT the autonomous `loop`) through the
   now-correct dispatch harness.
3. **Acceptance semantics to expect:** `acceptance:ai-then-human` **auto-merges**
   the run's PR to master on green (CI + janitor hard gate are the pre-merge
   gates), then parks `idgql3` at `acceptance` for a human accept/revert — it
   does NOT hold before merge. Validate the full `active → self-merge →
   acceptance` cycle end-to-end, then release `qvrwag` the same way.
4. Chain then progresses: `cvqcnx` → `cc3nlr` → `77t6mk` (closes keystone
   `rrr4i4`).

## Key pointers

- **Impl plugin** (dispatcher / next / list / groom): `livespec-orchestrator-beads-fabro`
  (cache `.../e0d801ebac24`). Dispatch = `dispatcher.py dispatch --repo <path>
  --item <id>` (single) or `loop --mode autonomous --budget <n>` (drain the
  ready+admission:auto set).
- **Beads tenant:** `livespec-console-beads-fabro` (server-mode Dolt @
  127.0.0.1:3307). All reads/writes go through `/usr/local/bin/with-livespec-env.sh`
  (supplies `BEADS_DOLT_PASSWORD`); the thin skills shell `bd` directly with no
  wrapper, so run them wrapped.
- **Blocking thread:** `plan/github-app-auth/` — core epic `livespec-2ef0` —
  resume `/livespec-orchestrator-beads-fabro:plan github-app-auth`.
- **Repo state at park:** console checkout clean on `master`; no worktrees or
  branches left from this thread.
