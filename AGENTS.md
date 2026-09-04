## ⛔ READ FIRST — never work around an upstream orchestrator dependency

> ⛔ NEVER WORK AROUND AN UPSTREAM ORCHESTRATOR DEPENDENCY — never again in this plan. (Maintainer ruling 2026-09-02, binding on every item owned by epic livespec-console-beads-fabro-pzbdbo.) The console CONSUMES orchestrator primitives (charter D2); it never rebuilds, substitutes, hand-bridges, or writes a literal in place of one. When work hits a missing or broken orchestrator primitive: STOP. File the orchestrator item. Create the console proxy item (BLOCKED-ON livespec-orchestrator-beads-fabro/<id>) and make this item depends_on it so the dispatcher refuses to run it. Hand the maintainer the path as a comment on the epic — an ASK left in a backlog is NOT a handoff. A recorded deviation with no linked proxy is a defect. Console feature work is HELD until orchestrator b1–b3 land. Full postmortem and the mechanical rules: https://github.com/thewoolleyman/livespec-console-beads-fabro/blob/master/plan/retire-overseer-and-redesign-control-plane-around-console/research/never-work-around-upstream-dependencies.md

## Upstream-dependency proxies — the mechanical contract

The guard above is the reminder layer. These are the layers that REFUSE, and
the convention every work item in this tenant follows so they can:

- **A proxy item** stands in for one upstream orchestrator dependency. Label
  `upstream-dep:livespec-orchestrator-beads-fabro`; title
  `BLOCKED-ON orchestrator <bd-ib-id>: <what it is>`; metadata
  `upstream_work_item_id` (the `bd-ib-*` id) and `plan_ref:
  livespec-orchestrator-beads-fabro/<slug>`; status `blocked`. It closes ONLY
  when the upstream item closes — never by hand, never by `resolve-blocked`.
- **A held console item `depends_on` its proxy** (`bd dep add <item> <proxy>`).
  The dispatcher then refuses to admit it (`not in the ready set`) — the hold
  is a refusal, not a memory. Proven 2026-09-04 on `-npr2gw`.
- **A recorded deviation links a proxy.** An admitted item whose description
  carries a `deviations:` line other than `deviations: none`, or a workaround
  phrase (hand-bridge, because pinned, workaround, in place of a projection,
  literal prepare/value) beside an upstream reference, MUST depend on a proxy.
  A filing at `backlog`/`open` is not yet a shipped deviation.
- **The gate:** `crates/console-upstream-dep-check` (pure over the
  `bd list --status all --json -n 0` array; general — any item, epic or not)
  run by `just gate-upstream-deps` from the pre-push hook on the host. It
  FAILS CLOSED when the ledger is unreachable. It is deliberately outside the
  `check:` aggregate and not `check-`-prefixed: CI has no tenant secret and
  the reconciler adopts every `check-*` slug. A sandbox checkout
  (`livespec.sandboxExempt`) has no ledger by design; there the dispatcher's
  pre-dispatch refusal is the gate. Failure modes:
  `upstream-dep-proxy-not-blocked`, `upstream-dep-proxy-title`,
  `upstream-dep-proxy-metadata-missing`, `upstream-dep-deviation-without-proxy`,
  `upstream-dep-held-item-dispatchable`.
- **Visibility:** proxies surface in the console's `blocked` lane
  (`list-work-items`, label `upstream-dep:*`). Surfacing them as a
  needs-attention inbox row needs an orchestrator gather fact — upstream work,
  referenced from the console epic, never built here.
- **ASK ≠ handoff.** Filing the orchestrator item is one step of four: file
  it, create the proxy, wire the edge, name the path on the epic.

# Agent instructions

This repo is a LiveSpec-family peer for the Beads/Fabro operator console.
The authoritative design is the live specification under `SPECIFICATION/`,
which now carries a revision history (`SPECIFICATION/history/v001/`). A Rust
workspace under `crates/` implements the console against that spec; ongoing
implementation work is tracked in the Beads ledger, not in this file.

## Repository scope

`livespec-console-beads-fabro` is a separate product from:

- `livespec` core, which owns the spec lifecycle and `/livespec:*`
  contract.
- `livespec-orchestrator-beads-fabro`, which owns Beads work-items,
  Dispatcher, and Fabro dispatch mechanics.
- `fabro`, which owns workflow execution, run state, human gates, logs,
  and sandbox UI.

This repo owns the operator console: event ingestion, canonical events,
commands, projections, TUI/GUI presentation, and human-attention routing.

## Agent interaction (maintainer working style)

- **Decisions → AskUserQuestion with a recommendation.** When a choice is
  genuinely the maintainer's to make, present it via the AskUserQuestion tool
  with 2–4 concrete options and a clearly-marked **"(Recommended)"** first
  option — never as a freeform prose question. Put load-bearing framing inside
  the question / option text.
  - **Batch pending decisions into ONE call** (up to 4 questions) rather than
    serial round-trips; the maintainer answers them together and expects
    everything approved to then be executed end-to-end, with each outcome
    VERIFIED (not assumed) before it is reported.
  - **An "Other" free-text answer may redirect the question's premise, not
    pick among your options — re-verify the redirected target before acting.**
    Observed 2026-07-22: a question about which items to review was answered
    by naming five specific "stale pending-approval records" from a handoff;
    the ledger showed all five already `done`. The right response was fresh
    verification and a report back, not executing the literal instruction.
- **Don't stop to ask what you should just do.** Execute the agreed plan and the
  obvious next steps yourself; reserve questions for genuinely maintainer-owned
  choices you cannot resolve from the request, the code, or sensible defaults —
  and even then lead with a recommendation.
- **Overseer tracks: never sit silently idle.** Long-running sessions on this
  host run under an overseer daemon, one track per plan thread, with a state
  file at `tmp/overseer/<thread>/.overseer-state` (untracked). When every
  remaining action is genuinely human-gated, write
  `blocked: <one-line reason listing the concrete pending decisions>` to that
  file so the operator is alerted out-of-band; remove the file when working
  again. On an overseer nudge, FIRST refresh repo + ledger state (both move
  under concurrent sessions — plan-handoff status claims have measurably
  rotted within hours), then do real in-scope work if any exists, and only
  then write the blocked marker.
  - **Write the marker LAST, every gated turn, and verify it with `cat`.**
    The daemon deletes the marker the moment it sees the session take a
    turn — including short background-notification turns (PR-merge
    cleanups). A marker written mid-turn, or in an earlier turn, is GONE
    by the time anyone looks. Measured 2026-07-26: three separate reports
    of a "live" marker while the file was absent, which is precisely the
    looks-stopped-while-gated failure these tracks exist to prevent. So:
    on EVERY turn that ends human-gated, the final tool action is write
    marker → `cat` marker, and any claim that it exists quotes that
    `cat`'s bytes from the same turn.
- **Durable agent memory lives in-repo.** Persist durable agent guidance and
  learned preferences in this file (or a file it references), NOT in ephemeral
  per-session agent memory. The repo's hook that blocks `~/.claude` memory writes
  is a signal to capture the memory HERE, not to drop it.
  - Topic-scoped durable knowledge lives under `.ai/`, loaded on demand via
    these references:
    - [`.ai/spec-check-and-ci-discipline.md`](.ai/spec-check-and-ci-discipline.md)
      — why a "spec-only" change can break Rust CI (the `console-spec-check`
      spec-ground-truth coupling), reading CI logs (incl. the empty
      `gh run view --log-failed` gotcha), telling a WEDGED self-hosted runner
      apart from real saturation when jobs sit queued (opposite fixes; capacity
      signals cannot see a wedge), and verifying the CI'd commit before
      trusting a local test run.
    - [`.ai/factory-dispatch-and-merge-coupling.md`](.ai/factory-dispatch-and-merge-coupling.md)
      — dispatching a work-item to the factory MERGES it; `ready` does not
      authorize dispatch when a merge must wait. Why the `do-not-merge`
      label, `merge_on_review_cap`, and `factory_safety` all fail to hold
      it, why you cannot just go off-factory instead, and why the
      workflow-edit refusal does not cover `.fabro/`.
    - [`.ai/fleet-repo-naming.md`](.ai/fleet-repo-naming.md)
      — never use bare "beads-fabro" (two sibling repos end in it); the
      repo ↔ tenant ↔ ID-prefix map, and targeting repos by full
      `/data/projects/<full-name>` path for destructive / tenant ops.
    - [`.ai/livespec-plugin-currency.md`](.ai/livespec-plugin-currency.md)
      — keeping the livespec plugins current: per-project pins go stale,
      `claude plugin update <name>@<marketplace> --scope project` (why the
      `name@marketplace` form is required) vs. host-wide `codex plugin
      marketplace upgrade`, "latest" = the `vX.Y.Z` tag, and the beads
      self-heal landing in orchestrator ≥ 0.4.0 (and how that qualifies the
      "Access denied ⇒ outside the wrapper" rule below).
    - [`.ai/linkable-references.md`](.ai/linkable-references.md)
      — every maintainer-facing reference must be web-linkable (GitHub
      URL for git-side content; a beads id must carry title + substance
      until the dolt-server `plan/beads-web-ui/` UI lands); the
      maintainer refuses unlinkable handoffs.
    - [`.ai/coverage-region-testability-discipline.md`](.ai/coverage-region-testability-discipline.md)
      — "genuinely unreachable" is not a disposition: an uncovered region is
      always resolved into deleted / tested / refactored-then-tested, never
      annotated, and never `.expect()`-relabelled. The hidden-global heuristic
      (a fallible call on an injected dependency is testable; one on a hidden
      global — clock, env, fs — is an injectability smell to inject away), with
      the discarded-timestamp latent bug as the worked example.
- **Handoffs: update the living plan-thread handoff; NEVER print one inline.**
  Session handoffs live at `plan/<topic>/handoff.md` (one durable thread per
  topic; resume via `/livespec-orchestrator-beads-fabro:plan <topic>`); UPDATE
  it in place and print its PATH. Completed threads archive to
  `plan/archive/<topic>/`; legacy prompt handoffs live in `archive/prompts/`.
  Do not print a handoff body in the chat, and do not proliferate new handoff
  files.
- **NO SHADOW LEDGER — never duplicate in git what the ledger already holds.**
  Status, progress, "X merged", "Y filed / reparented", next actions and
  who-owns-what live in the beads ledger (epic timelines, children, item
  comments and metadata) and are READ from it with one cached
  `bd list --status all --json -n 0`. A git change to a plan file is justified
  only by NEW REASONING — a decision record, a measured finding, an analysis —
  never by "something happened". `plan/<slug>/research/` notes and any
  "program board" hold structure and reasoning, not status; the
  `associated_work_item_id` anchor changes only when the anchor changes.
  Measured 2026-09-01: a plan session opened a worktree to append "v046
  ratified / child filed / anchor owed" to a research note minutes after
  writing the same facts into the epic's handoff; the maintainer stopped it
  before push. The test before opening any worktree for a plan file: "could a
  fresh session derive this from the ledger?" If yes, write nothing.
- **Charter-detector changes require three-way control.** Before changing a
  charter to satisfy a known-defect detector, verify the suspect form, the same
  thing written differently, and a known-real defect. If the suspect form and
  the equivalent alternate form disagree, the DETECTOR is wrong; fix the
  detector or report the detector defect rather than rewriting already-correct
  charter code to appease a broken check.
- **Legitimate charter counter-examples use indented literal blocks.** The
  charter detectors read fenced code blocks only, so a legitimate example that
  must preserve the exact bytes of a detector-shaped snippet belongs in an
  indented literal block. Do not add a self-declared "skip this block" marker;
  it becomes another convention to maintain and is not part of the detector
  contract.

## Codex dogfooding (OpenAI Codex CLI/TUI)

This repo's `/livespec:*` and orchestrator surfaces can be dogfooded from
OpenAI Codex CLI/TUI, not just Claude Code. Unlike the Claude path (plugins
enabled PER PROJECT via a committed `.claude/settings.json`), Codex plugin
enablement is **HOST-WIDE**: each registration persists in `~/.codex/config.toml`
and applies to every project on the host. Codex offers no project-scoped plugin
enablement, so there is no committed-settings analogue for the Codex path.

Install the three family plugins host-wide: livespec CORE (the artifact carrier
that ships the spec-side prose and wrappers), the `livespec-driver-codex` Codex
Driver (which supplies the `/livespec:*` operation surface over core's prose),
and the selected orchestrator plugin:

```bash
# livespec CORE (spec-side prose + wrappers; no skills of its own):
codex plugin marketplace add thewoolleyman/livespec
codex plugin add livespec@livespec

# The Codex Driver (supplies the spec-side /livespec:* operation surface):
codex plugin marketplace add thewoolleyman/livespec-driver-codex
codex plugin add livespec@livespec-driver-codex

# The selected orchestrator plugin (ships its own Codex skills):
codex plugin marketplace add thewoolleyman/livespec-orchestrator-beads-fabro
codex plugin add livespec-orchestrator-beads-fabro@livespec-orchestrator-beads-fabro
```

Once installed, Codex operations are driven via `codex exec` and NAME-selected as
`<plugin>:<op>` (for example, `livespec:next`,
`livespec-orchestrator-beads-fabro:list-work-items`) rather than as
`/`-prefixed slash commands. The distributed Drivers resolve their prose at
runtime; no `AGENTS.md` skill-to-prose mapping is required. See
`livespec/SPECIFICATION/contracts.md` §"Plugin distribution" and
`livespec/SPECIFICATION/non-functional-requirements.md` §"Codex dogfooding
contracts" for the authoritative install and resolution contracts.

The Codex TUI picker displays skills by short name with the plugin as context.
In `/skills` → `List skills` (or the `@` picker), search the operation name,
for example `orchestrate`; the row renders as
`orchestrate (livespec-orchestrator-beads-fabro)` with kind `Skill`. The
colon-qualified form `livespec-orchestrator-beads-fabro:orchestrate` is still
valid for prompt / `codex exec` name selection and model-visible skill
references, but it is not the picker row operators should expect.

## Beads runtime prerequisites

This repo's work-item store is a per-repo beads/Dolt TENANT
(`livespec-console-beads-fabro`) on the shared family dolt-server — NOT JSONL
files. Installing the plugin does NOT provision the backend; a clone connects to
its tenant only when ALL of the following are present:

- **`bd` CLI, pinned (v1.0.5)**, at an absolute path (`/usr/local/bin/bd`, NEVER
  the mise shim), with `LIVESPEC_BD_PATH` pointing at it (the impl-beads wrappers
  shell out to `$LIVESPEC_BD_PATH`).
- **A running Dolt `sql-server`** reachable over **TCP `127.0.0.1:3307`**. Family
  tenants force TCP (not the unix socket); `.beads/config.yaml` carries `dolt.*`
  host/port keys with NO `socket` key.
- **The tenant password** in env as a single **bare `BEADS_DOLT_PASSWORD`** —
  injected by THIS project's configured env wrapper. This is a FAMILY tenant: it
  shares the one family password via the family 1Password Environment wrapper
  `with-livespec-env.sh` (canonical copy at
  `/data/projects/1password-env-wrapper/with-livespec-env.sh`). `bd` consumes the
  bare var — there is NO per-tenant `BEADS_DOLT_PASSWORD_<tenant>` variable and
  NO per-tenant→bare mapping. Real isolation comes from the per-tenant SQL user +
  DB-scoped grant, not from password distinctness. Secrets are probe-only —
  `printenv NAME | wc -c`, never echo values — and NEVER committed to
  `.livespec.jsonc` or `.beads/`.
- **The `.beads/` pointer files**: `config.yaml` (committed; the `dolt.*` server
  keys) and `metadata.json` (gitignored, regenerable). NEVER run `bd init` inside
  a primary checkout or worktree — it auto-commits and clobbers `.beads/`.

**Run beads commands from the target repo root.** Per-command `bd` resolves its
connection from the current directory's `.beads/config.yaml` (auto-discovery), so
run from this repo's root, or `bd` silently operates on the wrong tenant.

**Wrap every `bd`-touching command under the env wrapper** — there is no "session
launched under the wrapper"; the wrapper applies per command. The canonical
invocation is `with-livespec-env.sh -- <command>`, e.g.

    /data/projects/1password-env-wrapper/with-livespec-env.sh -- bd list --status all -n 0

which injects the bare `BEADS_DOLT_PASSWORD` for that one command. **`bd list`
silently truncates TWICE, and the two truncations need two different flags.**

- **The row cap.** `bd list` stops at 50 rows, `--json` included, with nothing in
  the output saying the set was cut. `-n 0` (or `--limit 0`) removes it.
- **The status filter.** Without `--status all`, `bd list` returns only
  NON-CLOSED items and reports that truncated set as a normal, plausible,
  non-empty result. **`-n 0` does not fix this** — it lifts the row cap only.
  Measured in this tenant 2026-08-21: 254 records with `--status all`, of which
  209 are closed, so the bare form shows 45 — under a fifth of the ledger.

Either read alone under-reports and can make an existing item look absent. Use
`--status all -n 0` for any prior-art scan, any "have we already built this"
check, and any completeness review — the item you are looking for is usually
closed, which is exactly why it is invisible.

The wrapper rule extends to the orchestrator `capture-work-item` /
`list-work-items` / `next` skills: run the `bd`/python commands they drive under
the wrapper too. An **"Access denied" / "no beads database found" failure almost
always means you ran OUTSIDE the wrapper** (the password is absent), not that a
secret is missing — re-run under the wrapper.
Never hand-hunt the secret or reach around the seam with raw `mysql` / `dolt` /
`sudo`, and never rely on a `!`-prefixed one-off in a Claude prompt (it does not
persist env into later tool-call shells). A `CALL DOLT_BACKUP … command denied`
warning is correct-by-design (tenant users lack SUPER) — ignore it.

**The wrapper execs in a CLEAN environment — set variables INSIDE it, not in
front of it.** Anything exported ahead of `with-livespec-env.sh` is dropped
before the wrapped program runs, and this is not limited to `LIVESPEC_*`:
sentinel variables of any name vanish. The failure is SILENT — the program
runs, exits 0, and behaves as though you never set the variable.

    # WRONG — silently dropped, no error, exit 0
    LIVESPEC_CONSOLE_REPO_PATH=/data/projects/foo \
      with-livespec-env.sh -- livespec-console-beads-fabro serve

    # RIGHT — set it inside the wrapper's environment
    with-livespec-env.sh -- env \
      LIVESPEC_CONSOLE_REPO_PATH=/data/projects/foo \
      livespec-console-beads-fabro serve

The wrapper also supplies only a minimal system `PATH`
(`/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin`), so anything
in `~/.local/bin` (e.g. `fabro`) is NOT on `PATH` inside it — pass an absolute
path for such programs. Found during the B8 release acceptance (2026-07-21),
where a console pointed at another repo appeared to ignore its env override;
`docs/installing.md` carries the user-facing form of the same rule.

**Budget your wrapper calls.** Each one is an `op run` against a 1Password daily
quota that is shared **account-wide across every tenant**, not per-repo — a session
that spends it blocks `git push` and every ledger write fleet-wide, for other
sessions too. Batch: make one `bd list --status all --json -n 0` and parse the
cached result locally as often as needed, and loop multi-item work inside a
single wrapper invocation rather than wrapping each command. Do not narrate into
the ledger. Detail: the `livespec` repo's fleet agent-disciplines reference,
§"Ledger-write economy under a shared secret wrapper".

## Decision authority — when to ask, proceed, or self-resolve

Fleet-standard guidance, ported from
`livespec/AGENTS.md` §"When to ask, proceed, or self-resolve" and
`livespec-orchestrator-beads-fabro/AGENTS.md` §"Drive authorized work to
completion; do not over-ask". The default is to decide and report, not to
escalate.

**Why every governed member carries this.** On 2026-08-20 a track in this
fleet sat roughly sixteen hours parked on a picker whose option 1 was its own
recorded next action, and five self-decidable engineering calls were escalated
as standing maintainer questions. The investigation found the guidance was
real but partial: `AGENTS.md` is authored per repo and nothing propagates it,
so sessions in the repos that lacked it were reading a file that never told
them what they were allowed to decide.

- **Drive authorized work to completion; do not over-ask.** When the maintainer
  names a goal and says to finish or continue it, execute the WHOLE arc —
  implement, dispatch, PR, merge, iterate, archive — without pausing to confirm
  each already-authorized step. An operator-flow step that says "present
  options and let the user select" is satisfied by a standing directive once
  the goal is named; do not re-prompt. Default to acting, then reporting
  outcomes.
- **A recorded next action is an instruction, not a menu.** When a handoff, a
  work-item, or a plan timeline names exactly one next action, take it.
  Re-presenting it as option 1 of a picker is the stall shape above.
- **Research before gating.** If a question is answerable by reading the code,
  the spec, the docs, or by testing on a live system, do that, decide,
  implement, and report for objection. Reserve gates for genuine product or
  values calls, irreversible or outward-facing actions, and secret or
  host-mutation authorization.
- **Only ask on genuine doubt, one thing at a time.** Self-resolve trivial
  wording fixes, internal-consistency repairs, and items clearly aligned with
  established preferences, presenting each with its disposition. When a gate is
  warranted, ask exactly one question per turn.
- **One investigation, one finding, one question.** When a focused
  investigation surfaces unrelated discrepancies, finish the original question
  first and surface only the load-bearing finding; log side observations
  briefly. Cosmetic drift never blocks on its own.
- **Prescribed destructive ops are pre-authorized.** When a destructive git
  operation is the codified mechanism of an adopted workflow — the
  `git commit --amend` of the Red→Green step, for instance — the adoption is
  the authorization. Keep per-instance gating for ad-hoc `--amend`,
  force-push, `reset --hard`, or `branch -D` on unmerged branches.
- **An unratified filter inside a check is conformance, not ratification.**
  Narrowing, excluding, or filtering inside an enforcement check to match what
  the ratified spec already says is a conformance fix — implement it and report
  it. It only becomes a ratification question when the change would make the
  check assert something the spec does not.
- **A question you can answer with a recommendation is a finding, not a
  maintainer question.** If you can state the options, the costs, and which one
  you would pick, you have already done the deciding work. Decide it, record
  the reasoning where the work is tracked, and report it as decided.

### Standing maintainer directives (binding until countermanded)

Recorded on the foreman epic and binding on EVERY session in this repo. They
are reproduced here because CLAUDE.md is what a session actually reads at
kickoff: on 2026-08-21 a plan-resume session was staffed without them, raised a
two-question picker, and the foreman had to hand the directives back mid-thread.

1. **Get the TUI into a usable state.** Whatever does not work perfectly gets
   fixed later. Ship, do not polish.
2. **The absolute priority is momentum.** Keep everything moving. Say yes to
   everything possible.
3. **Find the bugs by dogfooding, ASAP.** Real usage at the real TUI is the
   bug-finding plan.
4. **Do not bring decisions to the maintainer.** A decision with a sound
   recommendation is DECIDED — execute and report. Groom cuts, closures,
   acceptance amendments, and scope splits are all included. The ONLY escalation
   is a security concern a consensus panel cannot resolve. A four-question batch
   is what prompted this rule, and it was answered with explicit displeasure at
   being asked.
5. **Recorded next actions are instructions, not menus.** Take them.

Directive 4 narrows the "when to ask" guidance above rather than sitting beside
it: where that section says to reserve gates for genuine product calls and
irreversible actions, directive 4 says that in THIS repo, in this phase, even
those are yours when you can state a sound recommendation. This does not touch
the confirm-first rule for destructive or outward-facing ACTIONS in the
mutation protocol — deciding is yours, but an irreversible act still gets its
confirmation.

## Repository mutation protocol

Every repo change uses a worktree → PR → merge → cleanup path. Treat leaving
dirty state, committing on the primary checkout, or asking the user whether to
commit as failures of the workflow, not as acceptable stopping points. The
commit-refuse hook — the canonical STRUCTURAL body REUSED from
livespec-dev-tooling, installed by `just bootstrap` (which delegates to `just
install-commit-refuse-hooks`) as `pre-commit`/`pre-push`/`commit-msg` — refuses
any commit or push at the primary checkout and delegates to lefthook everywhere
else. It detects the primary structurally (refuses when `git rev-parse
--git-dir` equals `git rev-parse --git-common-dir`, unless
`livespec.sandboxExempt` is set), so it is ARMED ON INSTALL with no
`livespec.primaryPath` arming step to miss. This is the `baseline` profile of
livespec's Conformance Pattern (concern #1, Worktree-discipline); `just
check-baseline` is the fail-closed verifier wired into `just check`.

1. Confirm the primary checkout before editing (the primary is where `git
   rev-parse --git-dir` equals `git rev-parse --git-common-dir`):

   ```bash
   git rev-parse --git-dir; git rev-parse --git-common-dir
   git status --short --branch
   ```

2. If the change will modify tracked files, create a dedicated worktree from the
   primary checkout's `master` and do all edits there. Every worktree lives under
   the per-user root `~/.worktrees/livespec-console-beads-fabro/<branch>` — NEVER
   as a peer of the clones under `/data/projects`. **Create it with the recipe,
   NOT with `git worktree add`:**

   ```bash
   mise exec -- just worktree-create <branch> master
   ```

   `worktree-create` provisions the worktree-discipline pack into `dev-tooling/`
   and hydrates. Raw `git worktree add` does neither, and **a worktree without
   that pack can neither commit a `.py` change nor push at all** —
   `check-primary-checkout-commit-refuse-hook-installed` fails with
   `worktree_pack_absent` in both the pre-commit and pre-push aggregates. **A
   docs-only branch is NOT exempt**; do not assume a prose change takes a fast
   path around it.

   Two properties make this expensive to learn by hitting it:

   - The check is only reachable through a full `just check`, so it fires at
     COMMIT or PUSH time — after the work is done — not at worktree-creation
     time.
   - A hook-rejected `git commit` leaves the change **STAGED**, so a following
     `git log` shows some other track's commit at HEAD and reads as success.
     **After a hook-gated commit, `git status` is the check that tells the
     truth, not `git log`.**

   `just install-worktree-pack` rescues a worktree that already exists without
   the pack, and it must also be re-run in any worktree created across a
   `livespec-dev-tooling` pin bump, because `worktree-create` provisions by
   copying from the primary checkout. Prefer either over `just bootstrap`, which
   reconciles the claude-plugins row and **advances the local plugin install** —
   the thing that turns `check-fork-drift` red on clean `master`. The rest of the
   lifecycle has recipes too: `just worktree-hydrate`,
   `just worktree-land [base_ref]`, and `just worktree-reap [--execute]` for
   orphans. `dev-tooling/*` is gitignored and byte-verified against the package
   source — never hand-edit the installed copy.

3. Use `mise exec -- git commit ...` and `mise exec -- git push ...` so the
   mise-managed lefthook hooks actually run. Never pass `--no-verify`; if a hook
   fails, fix the cause or halt with the failure.
4. Open a PR, wait for required checks, and merge through the PR using the repo's
   rebase-merge discipline.
5. After merge, refresh the primary checkout to `origin/master`, remove the
   feature worktree, delete the local branch, and verify the primary checkout is
   clean on `master`. Do not leave orphaned worktrees.

Rust product changes follow Red-Green-Replay. The standalone checker exists as
`crates/console-red-green-replay-check` and IS wired into the `just check`
aggregate as `check-red-green-replay` (stage 2, landed 2026-08-21 via PR #804),
so the ritual is enforced on every `just check` run. What is NOT yet enforced is
per-commit gating: `lefthook.yml` still carries only `pre-commit` and `pre-push`,
with no `commit-msg` section. That hook is stage 3
(`livespec-console-beads-fabro-mvu22t.3`), which is now `ready`: its hold was
lifted 2026-08-22 once the field evidence it asked for was recorded on the item
— stage 2's range check exercised in BOTH directions on real branches (a
product-Rust branch with earned trailers passing, and a trailer-less control
failing with `red-green-replay-range-missing-trailers`). Stage 3 has now landed:
`lefthook.yml` carries a `commit-msg` section, so the checker runs at COMMIT
time, decides Red / Green / SuiteGreen from the staged content, and writes the
TDD trailers itself once the tests it requires pass. A commit staging no product
Rust is untouched.

The two layers are complementary, not redundant. The hook governs commits made
while it is active; the range check in `just check` catches product-Rust commits
that entered history another way — merged in from a branch predating the hook,
cherry-picked, or rebased in.
Follow the checker contract in
`crates/console-red-green-replay-check/src/lib.rs`: "Content is the trigger" and
"Subject prefixes never exempt product Rust from the ritual." A changeset
staging no product Rust passes regardless of subject, so docs / spec / config
commits are exempt in practice by content; use `docs(...)`, `chore(...)`, or
`chore(spec):` subjects as a convention, not as an exemption criterion. Keep the
specification cohesive; do not import orchestrator-only concerns except through
explicit contracts.

## Post-merge janitor: Rust toolchain on the mise PATH

The factory Dispatcher's post-merge janitor re-runs `mise exec -- just check` in a
fresh detached worktree under a scrubbed, non-interactive PATH. rustup (not mise)
owns the Rust toolchain — pinned by `rust-toolchain.toml` — and installs
`cargo`/`rustc`/`rustfmt`/`clippy` under `~/.cargo/bin`. An interactive shell gets
that directory from rustup's profile snippet, but the janitor's minimal env does
not, so `.mise.toml` exposes it via `[env] _.path = ["~/.cargo/bin"]`. Removing
that entry reintroduces `cargo: not found` (exit 127) in the janitor even when the
PR merged green.

## GitHub CLI: JSON polling for `gh pr checks`

Use the JSON form when polling PR checks:

```bash
gh pr checks <n> --json name,bucket
```

The `bucket` field groups each check into `pass`, `fail`, `pending`, `skipping`,
or `cancel`. A PR with pending checks exits with code **8**, so a non-zero exit
status is not automatically a hard error; preserve stdout/stderr and inspect both
the exit code and the captured JSON.

**This bites hardest inside a polling loop.** A CI monitor that hides stderr or
replaces failures with fallback JSON can make a broken command look like "no
checks yet" or "all green". Two rules follow:

- **Never silence stderr on a command a loop depends on.** A hard error and
  "nothing has happened yet" can produce identical stdout; only stderr
  distinguishes them. Run the exact command once in the foreground before arming
  any Monitor.
- **Never treat empty captured output as success, and never add a `|| echo "[]"`
  style fallback.** The fallback swallows real command errors and can also hide
  the expected exit 8 while checks are pending. Gate on parsed, non-empty JSON,
  then evaluate the `bucket` values explicitly; `jq "all(...)"` over an empty
  array is `true`, which turns a broken fetch into a false "all green".

Separately, `gh pr edit --body` fails against this repo with a Projects-classic
GraphQL deprecation error (`repository.pullRequest.projectCards`). Update PR
bodies through REST instead:

```bash
gh api -X PATCH repos/<owner>/<repo>/pulls/<n> --input body.json   # {"body": "..."}
```

## CI queued/slow on the self-hosted pool: run the wedge scan BEFORE you explain it

**Action-gate, unconditional. Before ANY claim about WHY CI jobs are queued,
stuck, or slow to start on the self-hosted ARC k3s pool
(`livespec-console-beads-k3s`), you MUST run the `was not found` runner-log scan
from [`.ai/spec-check-and-ci-discipline.md`](.ai/spec-check-and-ci-discipline.md)
§"Jobs queued with nothing starting" and quote its output. No "saturation",
"pool is busy", or "throttled to `nominalQuota`/quota-1" claim may be stated
without that scan's result in hand.** This is an action you take before you
speak, not a reference you may open — it is here in the always-read file
precisely so a plausible capacity story cannot close the question before the
check runs.

The confirming scan, on the cluster host (`poweredge-xubuntu`,
`KUBECONFIG=/etc/rancher/k3s/k3s.yaml`), over each `Running` non-`-workflow`
runner pod:

```bash
kubectl logs <pod> -n arc-runners --tail=40 | grep "was not found"
```

Any hit is a wedged (zombie) runner with certainty — it is `Running`/`ready` to
Kubernetes but permanently dead to GitHub, so ARC never scales up and jobs sit
queued with headroom to spare. Wedge and real saturation present IDENTICALLY
(jobs queued, nothing starting) and have OPPOSITE fixes, so the scan is the only
thing that tells them apart. A THIRD presentation — runner pods created but never
coming up (PVCs `Pending`, `FailedScheduling ... VolumeBinding`, containerd
`failed to create inotify fd`) — is neither, has a third fix, and is diagnosed
by the two counts in the same note's third-case section; and none of it applies
while `CI_RUNNER_LABELS` is absent from the repo (jobs are then on GitHub-hosted
runners), so read the variable first.

Two signals are INVALID here and must not be used to reach a capacity verdict —
using them is exactly how this gate was earned (2026-08-30, PR #891 misdiagnosed
as quota-1 saturation when the true cause was a wedged runner; refuted by peak
concurrency 7 and 16 runners spinning up at once):

- **The GitHub REST/CLI runner list** (`gh api .../actions/runners`, "N runners
  busy"). Ephemeral ARC rows linger `offline`/stale; it reads like an outage
  while checks run fine. Measure the JOBS, not the runners.
- **`SchedulingGated` pod counts as "consumed capacity".** A wedge shows zero
  gated pods with spare capacity; gated-pod counts do not distinguish the two
  cases and quota-floor reasoning fits a wedge's evidence just as well.

The shape also discriminates: real saturation TRICKLES jobs in serially; a wedge
creates nothing for many minutes, then a BURST once cleared (the fleet's
5-minute auto-clearer, `livespec-s43svm.30`). The runner pool is fleet-owned —
report what the scan shows; do not resize anything from this repo.
