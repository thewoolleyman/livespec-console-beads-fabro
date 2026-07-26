# Strand capture — the four `active`/`fabro` rows from the 2026-07-21 drain

Captured 2026-07-26 on supervisor direction, BEFORE any un-stranding —
these four ledger rows are the surviving reproduction of the stranding
defect, and the orchestrator-side fix will be verified against this
capture. The two companion files are VERBATIM, unedited:

- `dispatch-journal-verbatim.jsonl` — every `tmp/fabro-dispatch-journal.jsonl`
  entry from the 2026-07-21 drain session (loop-pick 03:32:14Z through the
  loop's final entries) for the four stranded items plus `-bamsy3` (the
  same-loop CONTROL case that completed), copied byte-for-byte.
- `ledger-records-verbatim.json` — the four items' full materialized
  ledger records as of 2026-07-26 (`list-work-items --json`), unedited.

## The four rows

`livespec-console-beads-fabro-sreeqc`, `-276inb`, `-qwjfsw`, `-ogpok4` —
all `active`, `assignee: fabro`, unchanged since 2026-07-21.

## What the journal shows (all timestamps 2026-07-21 UTC)

One `:drain` palette command (budget 50 — see `-9ts`) picked five items
at 03:32:14 and dispatched them SERIALLY. Every run used the REPO's own
workflow (`workflow_toml: /data/projects/livespec-console-beads-fabro/
.fabro/workflows/implement-work-item/workflow.toml` in each
`dispatch-id` entry — relevant to `-8i9`). Per item:

| item | fabro run done | PR | merged (forge) | then |
|---|---|---|---|---|
| `-bamsy3` | 04:00:29 | #348-era | 04:0x | `pull-primary` OK ("Updating 7180920..7110eca, Fast-forward, AGENTS.md") → janitor → `ledger-complete` → acceptance. **CONTROL: completed.** |
| `-qwjfsw` | 04:49:19 | **#352** | **04:50:19Z** | `pull-primary` 04:50:24 → **outcome `failed`**, detail = raw fetch output, no fast-forward |
| `-sreeqc` | 05:19:04 | **#354** | **05:20:36Z** | `pull-primary` 05:20:40 → **outcome `failed`**, same shape |
| `-276inb` | 06:16:40 | **#358** | **06:17:49Z** | `pull-primary` 06:18:16 → **outcome `failed`**, same shape |
| `-ogpok4` | 06:43:53 | **#359** | **06:45:07Z** | `pull-primary` 06:45:28 → **outcome `failed`**, same shape |

(#352 merge commit `fd6c622c0`; #354 `2120e6267`; #358 `6262f666f`;
#359's merge commit is reported ambiguously by `gh` — its `mergedAt` is
the authoritative fact here.)

**Every one of the four PRs MERGED, seconds before its failure.** The
implementations have been on master since 2026-07-21. What never ran,
per the control case's trail: the post-merge janitor checkout, the
janitor gate, `ledger-complete`, and acceptance parking. The rows are
post-merge bookkeeping residue, not lost work.

## The trigger — this session's own uncommitted primary-checkout edits

The failing stage is `pull-primary`: the engine fast-forwards the
PRIMARY checkout (`/data/projects/livespec-console-beads-fabro`) after
the merge. From roughly 03:2x on 2026-07-21 that checkout carried
UNCOMMITTED edits made by the console-happy-path-mvp session itself
(the `.livespec.jsonc` human-valve revert, later joined by drifted
`plan/` files) while pin bumps rewrote `.livespec.jsonc` on master — so
`git pull --ff-only` could not update the file and failed. The control
case threaded the needle: `-bamsy3`'s 04:02:05 pull only had to
fast-forward `AGENTS.md` and succeeded. All four later pulls failed.
The dirty state was cleaned on 2026-07-23 (PR #392, maintainer
directive), which is why the 2026-07-23 dispatches (`-ipwtll`, `-x9o`)
completed end-to-end.

Two distinct defects follow, and only the second is this repo's:

1. **Orchestrator-side (the supervised fix, not this thread's work):**
   a post-merge bookkeeping failure leaves the item `active` forever —
   `active` conflates "run executing" with "dead run awaits a human",
   the WIP cap counts both (`_dispatcher_admission.py`:
   `active_count = sum(status == "active")`), and nothing surfaces the
   dead run. Being SUPERSEDED by a directed epic in
   `livespec-orchestrator-beads-fabro`; `-6ma` closes against it when
   the epic id arrives.
2. **Process-side (this thread's own lesson, already landed):** work
   left uncommitted on the primary checkout does not just violate
   worktree discipline — it broke four live dispatches. The mutation
   protocol exists for exactly this.

## Recovery state (do NOT act before supervisor authorization)

The four rows must not be moved until the supervisor authorizes
un-stranding (their brief 03). When authorized, recovery is per item:
verify the merged PR against master, run/settle the post-merge
bookkeeping the engine skipped, and route the item through acceptance —
NOT re-dispatch (the work is merged; a re-dispatch would rebuild landed
code and double-spend the queue).

## Recovery record — 2026-07-26 (supervisor-authorized, briefs 05/06)

All four rows were recovered through the purpose-built guarded valve —
`dispatcher.py reconcile-merged --repo <primary> --item <id>` — which
re-resolved each merged PR from the forge, re-ran only the post-merge
janitor, and entered the normal acceptance path without relaunching
Fabro. Per-item outcome `green` / "merged, post-merge janitor green"
(PRs #352, #354, #358, #359 confirmed in each output). Verified in the
ledger afterwards: all four at `acceptance`, awaiting the human accept
valve under this repo's `ai-then-human` policy. No item was routed
through `backlog` or `ready`; nothing was re-dispatched.

Two findings recorded on supervisor request (brief 06), for the
`plan/dispatch-claim-liveness/` thread in
`livespec-orchestrator-beads-fabro` — recorded here only, nothing filed
in that tenant:

1. **Janitor-red check: NEGATIVE.** None of the four ever reached a
   janitor stage — the only janitor entries in the captured trail
   belong to the `-bamsy3` control (green). All four terminal outcomes
   are `outcome.stage: "pull-primary", status: "failed"`. So this
   tenant's strands are NOT members of that repo's 18-item
   `janitor-post-merge/failed` population; they are a DIFFERENT failed
   terminal stage exhibiting the SAME class that thread has since
   diagnosed (the dispatcher survives and journals a terminal failed
   outcome, but the outcome→transition mapping is partial — no ledger
   transition exists for a post-merge failure, whatever the failed
   stage). A second FLAVOR of the class in a second tenant, not a
   second instance of theirs.
2. **Assignee after recovery: `fabro` retained at `acceptance`** on all
   four — matching the normal factory shape (controls `-bamsy3` and
   `-ipwtll` retain `assignee: fabro` even at `done`), so the
   `move_item` leaves-assignee-behind violation that thread documented
   was NOT triggered here: the guarded reconcile valve was used, never
   `move_item`.
