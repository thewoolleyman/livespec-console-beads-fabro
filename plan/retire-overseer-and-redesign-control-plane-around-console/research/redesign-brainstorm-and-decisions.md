# Retire the overseer and redesign the control plane around the console — brainstorm record and decisions

Captured 2026-08-31 from a maintainer ⇄ coordination-session discussion held in
homelab's `steady-state-loop-hardening` seat (Claude Code session
`f83ba18a-b6bb-4dc9-8ad2-84081b1cac67`), immediately after four read-only
investigations of the livespec-overseer foreman's 2026-08-30 five-job report.
The verbatim turns are in `brainstorming.jsonl` beside this note; this note is
the structured digest. Nothing here is ratified: it is the seed for this plan's
charter, for propose-changes in the owning repos, and for the scope events that
will cut it.

This plan's anchor: `associated_work_item_id` at the plan root reads
`unassigned` until the console-tenant epic is created — deliberately using the
convention decided in §6 before it is ratified, as this plan's first dogfood.

## 1. What triggered this

The homelab dogfooding plan had just recovered from a stalled loop (empty-commit
factory runs traced to homelab's own sandbox-write hook, a stale workflow fork,
and a vacuous-pass acceptance gate — all three fixed 2026-08-30). The overseer
foreman then reported starting five worker sessions overnight with several
factory failures. Four Fable investigators (read-only; fabro dumps, daemon logs,
tmux captures, every beads tenant) classified every failure. The measured
result that drove this discussion:

- **Nine of fourteen untracked failures were transport/observation failures,
  not intelligence failures**: an 8-line pane-scrape window that cannot see a
  multi-line picker (`_GATE_TAIL_LINES`, regression from release 1.67.2); a
  Codex seat on a registry row still recording `harness: claude`, so the daemon
  refused its own restart 56 times; an un-clearable `ready` declaration file;
  an env-var marker never set by `foreman-act`; a 48-char note truncation that
  hid the restore instruction; a stale shell-episode stamp; a session vanishing
  with no daemon record; a missing label; and — the biggest — the shared
  Claude weekly usage limit, which killed every Claude seat **including the
  foreman** on 08-23 and left the fleet dead for six days, because the limit
  manifests as an interactive TUI prompt that has to be scraped and answered.
- The deterministic layers held: start-intent journaling worked, the daemon
  classified dead foreman rows correctly, the dispatcher's admission accounting
  was right all along.
- Decision-layer failures were real but secondary and partly intent-delegation:
  dispatching past an item-sizing warning, an unannounced dispatch, writing
  records instead of progress (73 handoffs on one epic; 15 handoffs + 12 notes
  in one stuck day), workers treating a functioning `delegated` revise mode as
  a maintainer act.
- LLM layering **worked** where layers were independent and adversarial with
  evidence discipline (the four investigators overturned two of the foreman's
  own claims), and **failed** where layers shared context and incentive (a
  worker deciding its own next action from its own handoff; an acceptance pass
  judging its own criteria-splitter's fragment).
- The "ACP zero-output hang" label on three timed-out runs was wrong: measured
  live inside the sandbox, the agent was working (57 tool calls, a commit on
  the run branch); the Red-Green-Replay ritual under pre-commit gates
  (lefthook → `just check` → `pytest -n 4 --cov`) on a 4-core quota exceeded
  the 1800 s turn, and fabro's `AgentAcpTimedOut` reports `stdout: ""` by
  construction. Item size was falsified as the cause.

Everything that actually landed that day — the hook fix, the fork refresh, a
real item merged clean through the factory, a spec revision ratified in one
day — was driven by the maintainer, attended, with short-lived subagents,
against the ledger and the deterministic primitives.

## 2. The maintainer's framing (verbatim intent, not paraphrase)

- The other goal of livespec is to find **at what level an "AI Dark Factory"
  can be driven**. Pivoting to human-in-the-loop loses that goal; LLMs keep
  getting smarter; layered LLMs catching each other's failures (as fabro's
  reviewer/fix nodes do) is the target.
- Taking LLMs out of the loop also cripples the maintainer's ability to have
  full-context discussions (spec ⇄ impl auto-synced context is the other
  livespec premise).
- The failure is **on both sides**: LLMs not smart enough, *and* the maintainer
  delegating too much intent and losing the ability to steer (wrong thing
  built, yak-shaving, bad architecture).
- **tmux as the transport layer is where many stalls happen.** Headless agent
  communication with needs-attention bubbled to the top removes the
  "stall on picker" class by construction.
- Lean into **pi** for headless work (configurable, programmable, open); but the
  design must work across the three driver classes (claude, codex, pi).
- The ability to **drop into a worker session to steer** when needed must be
  preserved — but through the option to attach, not through tmux as the
  substrate.
- The **non-factory-dispatchable set must be very thin**: only things that
  actually interact with infra, plus the fallback when the factory is broken.
  Do not reinvent fabro.
- Plans as they exist duplicate the beads epic; the plan operation compensates
  for beads lacking a human surface, which leaves garbage in the ledger nobody
  reads. Lean into beads as the surface.

## 3. Transport research (measured 2026-08-31)

| Need | pi 0.84.4 (`@earendil-works/pi-coding-agent`) | Claude Code | Codex 0.151.0 |
|---|---|---|---|
| Headless bidirectional RPC | `pi --mode rpc` — JSONL over stdio (`docs/rpc.md`); SDK `AgentSession`, `runRpcMode` | `claude -p --input-format stream-json --output-format stream-json`; Agent SDK (TS/Python) streaming-input mode | `codex app-server` JSON-RPC (experimental; schema dumpable); `codex exec --json` |
| Questions / permissions as data | `extension_ui_request` (`select`/`confirm`/`input`/`editor`, id + timeout) — **only fired by extensions**; built-in tools do not prompt | `canUseTool` callback; `AskUserQuestion` and `requiresUserInteraction` MCP tools always reach it (denied under `dontAsk`) | server→client `Item/commandExecution/requestApproval`, `Item/fileChange/requestApproval`, `Item/permissions/requestApproval`, `Item/tool/requestUserInput`, `McpServer/elicitation/request` |
| Steer / interrupt mid-turn | `steer`, `follow_up`, `abort` | queued messages; SDK `interrupt()`; `system/init` advertises `interrupt_receipt_v1` | `Turn/steer`, `Turn/interrupt`; `codex queue --thread <id> --message` |
| Rate/usage limit as data | `auto_retry_start/end` with `errorMessage` | `system/api_retry` with `error: rate_limit \| billing_error \| overloaded …` | `Account/rateLimits/updated` notification |
| Resume by id | `--session <id>`, `--session-id`, session JSONL tree with durable entry cursors | `--resume <id>` from any directory; `--session-id <uuid>` | `Thread/resume`; `codex exec resume <id>` |
| Human attaches to a running session | pause/resume only (TUI and RPC are run modes over one session file); an experimental `@earendil-works/pi-protocol` (CBOR client/server) exists, no compatibility guarantees | Remote Control (`--remote-control` / `/rc`): web + mobile attach concurrently with the terminal; permission prompts and `AskUserQuestion` forwarded and held open; `claude remote-control` server mode hosts many sessions; requires claude.ai login | shared local daemon (`codex app-server daemon`); `codex agents` lists sessions; TUI attaches with `--remote <ADDR>`; `codex remote-control start/pair` |
| Native ACP | **No.** Community adapters only (`@victor-software-house/pi-acp` embeds the SDK; `svkozak/pi-acp` spawns rpc); neither implements `request_permission`; upstream discussion #4444 has a native `--mode acp` proposal with no maintainer response | `@agentclientprotocol/claude-agent-acp` (what fabro uses) | `@agentclientprotocol/codex-acp` (what fabro uses) |

Caveats that bite: `pi -p` exits 0 on a failed model call and non-interactive
pi ignores project-local settings unless trusted (`--approve`) — fail-open
traps for any headless driver (livespec-driver-pi AGENTS.md).

**fabro (pinned `8de6611`, v0.254.0 fork, and upstream/main):** ACP is internal
between `fabro-workflow` and the adapter subprocess; fabro auto-answers the
adapter's `request_permission` (`AllowAlways` → `AllowOnce` → first non-reject,
`fabro-acp/src/session.rs:313`) and drops ACP tool-call updates. There is no
ACP session to attach to. fabro **does** expose its own control plane for a
running run, present at the pinned build: `fabro run steer <run> "<text>"
[--interrupt]` → `POST /runs/{id}/steer|interrupt` (queued between turns, or
cancel-and-deliver; run-wide pending buffer; `agent.steer.dropped` on
overflow); `fabro run attach` → `GET /runs/{id}/attach` (SSE events only, no
transcript, no per-tool events); interview questions → `GET /runs/{id}/questions`,
`POST /runs/{id}/questions/{qid}/answer`. "Drop into a fabro node" therefore
means steer / interrupt / watch / answer — not a terminal on the agent, whose
session lives inside the sandbox container.

## 4. Decisions reached (each is a proposal until a scope event or ratified clause says otherwise)

**D1 — Retire tmux as the transport.** Sessions become data: questions,
permissions, rate limits, context and turn events are protocol messages
composed into needs-attention. The picker-stall class is eliminated by
construction, not fixed. Every scraping-remediation item found on 2026-08-31
(gate window, stale launch profile, ready file, unattended marker, note
truncation, stamps, session-gone record, usage-limit picker answering) is
**superseded by the transport change**, not fixed in place.

**D2 — The console is the control-plane surface; the orchestrator is its API.**
Strict dependency direction: console → orchestrator through the injected CLI
(JSON envelopes); `livespec-runtime` is a dev-only dependency of the console
(wire-shape tests), never a product dependency; the orchestrator stays
console-unaware. Every capability the overseer's roles provided is reduced to
an orchestrator primitive (§5); the console renders and configures. Per the
ratified routing test (research/010 rt-sol-2 in homelab's archive: a shape
moves to the runtime only when ≥2 Python producers need identical semantics),
new schemas here stay in the orchestrator.

**D3 — No second execution substrate. All automated LLM work is a fabro run;
the only variable is which workflow.** Implementation runs
`implement-work-item`; a spec revise, a gap capture, a review panel, a
plan-authoring step are **named, complete workflow variants** — i.e. the
existing orchestrator plan `pluggable-factory-workflow-configs`
(`bd-ib-yqpdrt`), which this discussion **re-prioritizes to central** after it
had been demoted on 2026-08-30. v092's typed workflow inputs
(`typed-repo-integration-contract`, `bd-ib-vblnq2`) make variants safe to
author. Consent dialogues (revise accept/reject, per-gap consent, panel
unanimity) become fabro **interview questions** on the existing questions API,
surfaced through needs-attention, answered from the console — or auto-answered
where policy is `delegated`. Panels are a workflow with N adapters plus a
disposition rule; the policy enum (majority-autonomous / unanimous-or-block /
report-only) extends the dispatcher's existing valve-policy surface
(`set-acceptance:<id>:ai-only|ai-then-human|human-only`) to attention items.
**Retracted during the discussion:** a `jobs run` runner, a four-driver
`sessions` layer, a `dispose` primitive, a `supervise` session daemon — all
were a second factory. The harness RPC research (§3) still matters, but as
fabro *adapters* (nodes), which they already are.

**D4 — The thin non-dispatchable set, exactly:** (1) maintainer-in-person
infra acts (AWS console, registrar, billing, break-glass), recorded as scope
events; (2) the interactive context session — questions, rulings, research
authoring, routing judgment; (3) the broken-factory fallback — a human + LLM
session driving worktree → PR by hand (the 2026-08-30 hook fix: "the factory
cannot fix the thing that blocks the factory"). All three are the same
interactive skill (§6); none needs a runner.

**D5 — What survives from the overseer, transferred by capability, not by
plan:** account rotation (caam), generalized beyond Anthropic and made
event-driven off the harnesses' rate-limit signals; consensus-panel disposition
with the three policies; and two deterministic rules that become orchestrator
behavior — re-dispatch on `transient_infra`, and starvation (ready-aging fact)
→ dispatch. Dropped outright: pane classification, keystroke answering,
`.overseer-state`/ready files, heartbeat files, tmux-named sessions, the
resident LLM foreman seat, the grooming seat (grooming becomes a dispatchable
workflow), the supervisor seat (review becomes a workflow variant with a
structured verdict). The overseer repo is **frozen whole** rather than triaged
plan-by-plan; every tmux-class plan is marked superseded-by-transport; the
named capabilities transfer into orchestrator/console plans; the rest archives.

**D6 — Retire the plan operation; epics are plans.** The plan operation
existed because beads lacked a human surface; that left garbage in the ledger.
Replace it with:
1. `metadata.plan_slug` **required on every epic**, dash-case, **unique per
   tenant**; the human-readable handle that overseer/console listings and
   tooling anchor to instead of the short id. Non-epics do not duplicate it (a
   child's plan is its parent chain); the one legitimate non-epic use is a
   **cross-repo reference**, which must be tenant-qualified (`plan_ref:
   <tenant>/<slug>`) and only on items that are not children of the referenced
   epic.
2. A **`discuss-work-item <plan_slug | work_item_id>`** skill that loads all
   relevant context for that item and its children (epic, comments, children,
   dependency edges, typed next action, linked research directory, linked spec
   clauses) and **stands by** — it does not auto-act; it answers questions,
   drafts research, records rulings as scope events, and drives only when told,
   through the same primitives. This is the maintainer's day-to-day session
   and the console's future chat pane; the deterministic loader inside it is
   the one genuinely new read primitive (`context`), in the family of
   `list-work-items` / `needs-attention`.
3. **Keep `plan/` directories** across the fleet with the same
   `research/` + `archive/` semantics, but require a file
   **`associated_work_item_id`** at each plan-dir root containing a work-item id
   in that tenant or the literal `unassigned` (research-before-work-items), for
   bidirectional matching.
4. **Doctor checks enforce**: plan_slug present and unique; the anchor file
   present and valid; bidirectional consistency (dir → epic slug == dir name;
   epic slug → if a dir exists it names that epic; `unassigned` allowed only
   while no epic with that slug exists); a live dir with a closed epic or an
   archived dir with an open epic is an error; an epic with a plan dir may
   close only with a `plan-completeness-review-evidence` comment present;
   `next_action` present and well-typed on every open epic with a plan dir;
   comment rate per epic-day under threshold (warn).
5. **Handoffs collapse to typed epic metadata** — `next_action: {kind:
   impl | spec-op | human | none, ref, text}` plus `last_session`, updated in
   place — because resume context is what `discuss-work-item` computes.
   Scope events (rulings, deferrals) stay as comments; they are the part a
   human reads back. Beads' native "N/N complete — eligible for close" is the
   child-disposition gate; the completeness-review evidence comment is the
   other gate.
6. The `plan` skill's remaining mechanics (`bd create --type epic`,
   `mkdir plan/<slug>/research`, write the anchor file, `git mv` to archive on
   close) need no operation: doctor reports what is missing;
   `discuss-work-item` may offer to scaffold. Naming: the interactive skill
   must not be called `plan` (collides with Claude Code's built-in on
   autocomplete); `discuss-work-item` is the chosen name.

This is a fleet contract change (the plan operation is ratified in the
orchestrator's spec; drivers and console consume it) → orchestrator
propose-change with the doctor rules as scenarios, plus a one-shot migration
that writes `associated_work_item_id` from each existing epic's `plan_slug`.

**D7 — This uber plan lives in the console repo** (not the orchestrator, which
gets most of the work). Reasons: deliverables are framed as *consumed and
demonstrated* (the discipline that worked) rather than *shipped* (how
"declared, not demonstrated" happened); dependency direction — a console plan
may name orchestrator plans by reference, an orchestrator plan naming console
phases would put control-plane knowledge in the layer the fleet keeps
scrubbing it out of; and the maintainer's attention is here daily (the
intent-retention failure happened in a plan nobody was looking at). The
orchestrator gets its own execution plans, referenced from here.

**D8 — Console delivery is the dogfooding driver; homelab is on hold as the
exit gate, not retired.** Tooling-on-tooling can run forever and always look
like progress, so the final gate of this plan is: *homelab resumes under the
console-driven loop and moves one real fleet item ready → done*. Homelab's
`steady-state-loop-hardening` stays blocked with that as its far gate.

**D9 — Use the console TUI to drive work as soon as possible.** Until then the
orchestrator's CLIs are used directly — they *are* the console's API. The
maintainer runs a dedicated interactive LLM session (`discuss-work-item`) for
day-to-day questions, status and rulings.

## 5. Capability → primitive reduction (corrected after the "no second factory" pushback)

| Overseer role | Capability | Where it goes | Status |
|---|---|---|---|
| Foreman | Roster (plans × session × work state) | `needs-attention` + `bd list --type epic` on `plan_slug` + fabro `ps` | exists |
| Foreman | Starvation → start work | dispatcher `loop` cadence on the ready-aging fact | rule exists in prose (7ranbh); re-home |
| Foreman | Decide what matters / escalate | **none** — `next` ranking + attention urgency; console renders | role deleted |
| Foreman | Valve disposition by consensus | panel **workflow variant** + valve policy enum on attention items | new variant + existing policy surface |
| Foreman | Ledger bookkeeping | `capture-work-item`, `bd` via wrapper | exists |
| Worker seat | Execute the recorded next action | `drive impl:<id>` / dispatcher `loop`; spec ops as **workflow variants** with interview consent | drive/loop exist; variants via `bd-ib-yqpdrt` |
| Worker seat | Handoffs / scope events | typed `next_action` metadata; scope-event comments | contract change (D6) |
| Supervisor seat | Adversarial review with durable verdict | review **workflow variant** with structured verdict (unifies the completeness reviewer and core's auto-spawn ratification review) | unify |
| Grooming seat | Bounded backlog drain + revise pending proposals | `groom` + core `revise` as dispatchable workflow runs, triggered by the §15 staleness facts | no seat |
| caam loop | Account rotation on limits | orchestrator `accounts status \| rotate`, driven by rate-limit events, multi-provider | generalize |
| overseerd | Context-floor restart, registry, pane classification, ready files | **none** for overseer sessions (they no longer exist); fabro/adapters own node context; re-dispatch on `transient_infra` is dispatcher policy | deleted with transport |
| fabro reviewers / fix nodes | In-workflow quality gates | unchanged, inside the workflow payload | unchanged |

Genuinely new surface after retractions: `context` (read-only loader inside
`discuss-work-item`); workflow variants for revise / gap-capture / panel /
review; interview-question surfacing into needs-attention (mostly exists);
valve policy extended to attention items; `accounts` (caam moved in); typed
`next_action`; the `associated_work_item_id` convention and doctor rules.

## 6. The fabro-side items this design needs

- Forward ACP `request_permission` and user-input as **interview questions**
  on the existing questions API instead of auto-answering `AllowAlways`, so a
  parked node is parked on a typed question (fabro fork; upstream is
  stable-frozen since v0.254.0 — 780 nightly commits, no stable release in 76
  days).
- Steer semantics already exist; "attach" remains steer / interrupt / watch /
  answer unless fabro exposes the adapter session id.
- Known gaps recorded 2026-08-30/31 that survive any transport choice:
  `AgentAcpTimedOut` reports `stdout: ""` unconditionally; no per-tool ACP
  events; unconditional `--allow-empty` checkpoints; the needs-human
  preservation ref collides on `refs/heads/needs-human/unknown-run`
  (`FABRO_RUN_ID` is hook-only for script nodes); sandbox PID 1 is `sleep
  infinity` and reaps nothing (559 defunct processes measured in one live
  container); the Red-Green-Replay ritual under pre-commit gates exceeds the
  1800 s implement turn (`runaway-process-containment`, `bd-ib-wcuauj.2`).

## 7. Phases (proposed; the scope event cuts them)

0. Record the finding in homelab (transport failures dominated; deterministic
   layers held; resident-LLM-foreman role failed; layering worked when
   independent); freeze the overseer whole; ratify charters in each repo — the
   console must first ratify that it absorbs these capabilities (its v040
   boundary currently declares console/overseer orthogonal); RULE ONE's
   equivalent applies in every repo before building.
1. Orchestrator: `plan_slug` / `associated_work_item_id` / doctor rules /
   typed `next_action` propose-change + migration; `discuss-work-item` skill
   over the `context` loader. Console: list plans from the ledger.
2. Orchestrator: workflow variants (`pluggable-factory-workflow-configs`) for
   revise and gap-capture with interview consent; needs-attention carries the
   question + answer route; console answers questions. This kills the picker
   class.
3. Orchestrator: valve policy on attention items + panel variant; `accounts`
   rotation; re-dispatch on `transient_infra`; starvation → dispatch cadence.
4. fabro fork: permission/user-input as interview questions.
5. Overseer archived; capabilities transferred by name.
6. Exit gates (both demonstrated): the console drives its own next slice
   through the new path; homelab moves one real fleet item ready → done under
   the console-driven loop.

## 8. Open questions for the scope event

- Whether plan-authoring (research notes, scope events) may run as a workflow
  variant at all in early phases, or stays in the interactive set until the
  job contract has a track record (authoring-instead-of-progress was the
  decision-layer failure; a present human is the cheap guard).
- The exact `next_action` schema and whether `last_session` is worth keeping
  once sessions are ephemeral.
- Whether the overseer's context-floor wrap-up rule has any remaining subject
  once no overseer-owned sessions exist (fabro adapters own node context).
- How `discuss-work-item` reaches the console chat pane (same loader, in-app
  LLM) and which harness backs it there.
- Migration order for the overseer freeze: the four plans the maintainer is
  finishing (`foreman-fact-consumption`, `session-start-and-registry-integrity`,
  `foreman-codex-pi-runtime-support`, `caam-loop-otel-instrumentation`) run to
  completion first.

## 9. Retractions log (proposals made and withdrawn during the discussion)

- "Freeze the overseer and pivot to human-in-the-loop" → withdrawn: loses the
  dark-factory goal; re-cut as "retire the transport, keep LLM layering where
  layers are independent".
- "The LLM layers survive intact (workers, reviewers, panels, supervisors)" →
  withdrawn as role-preserving; replaced by capability → primitive reduction.
- `jobs run plan-execute`, a four-driver `sessions` layer, `dispose`,
  `supervise` → withdrawn as a second factory; replaced by workflow variants +
  interview questions + existing valve policies.
- Deprioritizing `pluggable-factory-workflow-configs` (2026-08-30) → reversed;
  it is central under D3.
- Splitting the plan skill into `orient` + `thread` → withdrawn; replaced by
  D6 (epics are plans; `discuss-work-item`; `associated_work_item_id`; doctor).
- A new overseer "seat survival" plan for the usage-limit / stale-profile /
  un-clearable-ready cluster → withdrawn; those failures are superseded by the
  transport change.

## 10. Evidence trail

- homelab `plan/steady-state-loop-hardening/research/014` (recovered
  hook/fork/fabro findings), `002` (the sixteen-section matrix), `012`
  (verification rules), `013` (pointer); its epic `homelab/hl-eufbpx` scope
  events of 2026-08-29/30.
- The four investigator reports of 2026-08-31 (recorded in the homelab
  session transcript; classification summary posted in that session).
- Orchestrator plans `typed-repo-integration-contract` (`bd-ib-vblnq2`,
  v092), `pluggable-factory-workflow-configs` (`bd-ib-yqpdrt`),
  `runaway-process-containment` (`bd-ib-wcuauj`),
  `acp-implement-zero-output-hang` (`bd-ib-b5dg` — counter-specimen filed),
  `empty-diff-acceptance-integrity` (`bd-ib-xmom`, closed).
- Overseer plans `foreman-liveness-and-escalation` (`overseer-ll9d`),
  `daemon-row-truth-and-attention-coverage` (`overseer-qt3wvu`),
  `foreman-fact-consumption` (`overseer-7ranbh`),
  `session-start-and-registry-integrity` (`overseer-zidpiu`).
- pi docs (`docs/rpc.md`, `docs/sdk.md`, `docs/sessions.md` in the installed
  package), earendil-works/pi discussion #4444 and issue #175, the two pi-acp
  adapters; Claude Code docs (headless, Agent SDK streaming input and
  permissions, Remote Control); `codex app-server generate-json-schema`;
  fabro source at `8de6611` and `upstream/main`.
