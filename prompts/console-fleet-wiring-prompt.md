# Console fleet-wiring — install the fleet GitHub App, then auto-complete the wiring

Run this to finish wiring **`livespec-console-beads-fabro`** into the fleet.
It is the resolution of ledger item **`livespec-inxg`** ("Wire
livespec-console-beads-fabro into the fleet — clear register-first
conformance red"). This prompt walks the maintainer through the ONE manual
step only an owner can do, then the session auto-completes the rest and
verifies the unblock.

## Why this matters (the blocked state)

The console repo is registered in the fleet manifest
(`livespec/.livespec-fleet-manifest.jsonc`, class `console`) but is **not yet
GitHub-wired**, so the central **fleet-conformance** assert is RED. That red is
the required preflight for the dev-tooling release fan-out, so it currently:

- **deadlocks `livespec-dev-tooling` master** (the `test_master_ci_green`
  fleet-conformance check fails → every dev-tooling PR is blocked from
  merging), which **blocks the `zs22.7.9` convergence epic** (livespec3's
  PR **#190** is built and parked, awaiting this), and
- leaves **automatic release fan-out broken** fleet-wide (consumers are
  stranded on dev-tooling v0.22.0 though newer releases exist).

Three findings block the console's fleet-conformance row; only the first is
manual:

1. **GitHub App not installed on the console repo** — owner-only; no CLI/PAT
   can install an App on a repo.
2. Merge settings not rebase-only — machine-fixable.
3. Missing `livespec-sibling` topic — machine-fixable.

## STEP 1 — MANUAL (maintainer only): install the fleet GitHub App

Only a repository owner can do this; it cannot be scripted.

1. Open the fleet GitHub App's installation settings (GitHub → your
   org/account **Settings → GitHub Apps → the fleet app → Configure**, or the
   app's "Install / Configure" page).
2. Under **Repository access → Only select repositories**, add
   **`thewoolleyman/livespec-console-beads-fabro`** to the selected
   repositories, and save.
3. Confirm the app now lists the console repo among its installed repos.

When done, tell the session "App installed" (or just let it proceed) and it
will complete the rest.

## STEP 2 — AUTO (the session does this after the App is installed)

Run from the **`livespec-dev-tooling`** checkout (the package host), under the
livespec 1Password env wrapper so `APP_ID` / `APP_PRIVATE_KEY` are available for
projection. Secrets are **probe-only** — never echo a value.

1. **Reconcile the wiring** (fixes merge-settings + topic, projects the App
   secrets if missing):

   ```bash
   cd /data/projects/livespec-dev-tooling
   source /data/projects/1password-env-wrapper/with-livespec-env.sh \
     uv run python -m livespec_dev_tooling.fleet.wire_fleet_member \
     --repo livespec-console-beads-fabro
   ```

2. **Verify the assert is green** — the console's fleet-conformance row should
   now pass on all four findings:

   ```bash
   cd /data/projects/livespec-dev-tooling
   just check-fleet-conformance
   ```

3. **Clear the deadlock** — re-run the failed `livespec-dev-tooling` master CI
   (the `fleet-conformance` / `test_master_ci_green` preflight). With the
   console wired, it goes green and dev-tooling PRs can merge again.

4. **Confirm + report** — `livespec-inxg` is resolved once fleet-conformance is
   green and dev-tooling master CI is green.

## What this unblocks (and the hand-back)

Once dev-tooling master is green:

- livespec3's PR **#190** (`zs22.7.9.2` — `check_plugin_structure` port) merges,
  and the convergence epic's remaining slices (`.2/.3/.5/.6`) proceed.
- Automatic release fan-out is restored fleet-wide.

Report completion so the **`livespec-overseer`** session resumes the dependent
tracks (it is tracking livespec3's parked epic and will continue it once the
console is green). If the App install cannot be completed, report that too so
the overseer holds the dependent tracks cleanly rather than spinning.

## Discipline

- Any repo change goes **worktree → PR → rebase-merge**; never commit on a
  primary checkout; never `--no-verify`; operate only in a worktree you create.
- Secrets are **probe-only** (`printenv NAME | wc -c`), never echoed.
- Hand off to a refreshed copy of this prompt + the ledger if you approach
  ~50% context before the wiring is verified.
