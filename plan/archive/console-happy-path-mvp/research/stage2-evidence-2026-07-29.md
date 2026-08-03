# Stage-2 evidence — 2026-07-29

Three pieces of evidence that were produced live during the 2026-07-29 session and
kept being re-requested because they existed only in session chat. **That is the
whole reason this file exists**: evidence that lives in a transcript is evidence
that has to be re-derived by whoever asks next. Durable beats repeated.

Everything below was measured, not recalled. Exit codes are read UNPIPED throughout,
because a piped `$?` is the last command's.

---

## 1. The four TUI approve-valve admissions — the captured hints

`cxu4eu`, `cyixzi`, `vwxyj4`, `zvnjef` were admitted at the REAL TUI `p` valve on a
fresh cockpit built from current master, each verified on the ledger after its own
keypress. `ectqye` was deliberately NOT admitted (its routing is undecided pending
reconciliation with `-k0w`).

**Without the hints these are lane moves, not walk legs.** The hint is the evidence
that the operator surface — not `drive.py` — was exercised, and that the surface
advertised the right vocabulary before the key was pressed.

Captured at every one of the five pending-approval rows BEFORE any valve keypress,
by reading the Status band out of a full pane capture:

```
up/down move | enter open | p approve | r reject | m set-admission | ? help | q quit
```

| work-item | captured in the identification sweep | re-captured immediately before its own `p` |
|---|---|---|
| `livespec-console-beads-fabro-cxu4eu` | yes | yes |
| `livespec-console-beads-fabro-cyixzi` | yes | yes |
| `livespec-console-beads-fabro-vwxyj4` | yes | yes |
| `livespec-console-beads-fabro-zvnjef` | yes | pressed from the sweep position — **no second capture** |
| `livespec-console-beads-fabro-ectqye` | yes | not pressed (deliberately) |

All five rows rendered the string **identically**. It matches
`attention_item_footer_hint`'s `PendingApproval` arm exactly as it stood at the time:
`p` / `r` / `m` present, and **no `c accept`** — the correct suppression for a
pending-approval item.

**The one gap, stated rather than glossed:** `zvnjef` was pressed from the position
where the sweep had just captured its hint, so it has one capture and not two. The
other three have two independent captures each.

Each valve rendered `Approve work-item` with `Target: <exact work-item id>`, and the
id was read back before `Enter` was sent. No silent failure on any press — which is
the specific defect (`-ectqye`, `-u3w3er`) that made the earlier `-sreeqc` attempt
fail and forced the `drive.py` workaround this walk exists to retire.

**Note for a future reader:** these hint strings are now HISTORICAL. PR #505
(`514a326`) rewrote the hint tables to derive from `per_item_verb_is_state_valid`,
and the pending-approval hint gained the policy dials. The capture above documents
what the surface advertised on 2026-07-29, not what it advertises now.

---

## 2. `console-fork-drift-check` — the red demo, and what it does NOT catch

The guard (crate `console-fork-drift-check`, `just check-fork-drift`) was demonstrated
red before it was accepted in PR #479. This is a fresh four-arm re-demonstration on
current master, run because the guard protects the file class that let the `.fabro`
fork drift for three weeks and a guard nobody has seen fail is a guard nobody should
trust.

```
ARM 1 — BASELINE (unmodified tree)
  RC=0
ARM 2 — OUR OWN FORK EDITED (review_adapter changed in the committed workflow.toml)
  RC=0  <-- BLIND: our own edit does not trip it
ARM 3 — UPSTREAM DIGEST MOVED (pin for prompts/pr.md zeroed)
  RC=1  <-- CATCHES IT
  console-fork-drift-check: the committed .fabro fork drifted from its pins
    - prompts/pr.md: UPSTREAM MOVED since this pin was taken
      (pinned 000000000000, live 02a9430cf61c). Recorded divergence: SYNCED VERBATIM
      from upstream ...
ARM 4 — RESTORED
  RC=0
```

Tree verified clean afterwards; both mutations were reverted from backups.

### What this proves, and the half that matters more

**It catches upstream movement.** Arm 3 is the failure the guard exists for, and the
message is genuinely actionable: it names the file, the pinned digest, the live
digest, and replays the recorded `reason` so the reader can judge whether to port the
change or re-pin.

**It is BLIND to our own edits — arm 2 is the important arm.** The fixture
(`tests/fixtures/fabro-fork-upstream-pins.json`) stores `upstream_sha256`, so the gate
answers exactly one question: *did UPSTREAM move since we pinned it?* It does **not**
answer *did we change our fork without recording why?* A green `check-fork-drift` is
therefore not a statement about our side of the diff at all.

Two consequences a successor should not have to rediscover:

- Editing our committed `.fabro` files does **not** red this gate and does **not**
  force a pins refresh. If someone argues against such an edit on the grounds that
  "it would red the drift guard", that premise is false — measured in arm 2. The real
  reasons to avoid casual edits are review and provenance, which are better reasons.
- Nothing mechanical watches our own divergence. That remains a human responsibility,
  and the `reason` field is where it is discharged.

---

## 3. The first Codex review — did it review, or rubber-stamp?

Context: the Claude subscription hit its weekly ceiling (re-tested and confirmed —
four failures, byte-identical, an hour apart). Maintainer decision: point
`review_adapter` at the Codex ACP adapter. `mbohw3` was the first slice reviewed that
way (run `01KYQGW4G9KHGZR3ME4TMG03K4`, PR #505, merged `514a326`).

### Routing form — checked FIRST, and deliberately

```
{"preferred_next_label": "approve"}
```

Well-formed, valid value, on the last line as `prompts/review.md` requires.

This was checked before reading a word of the prose, because **`review -> pr` is the
UNCONDITIONAL fallback edge** (`workflow.fabro:285`) while only `review -> review_fix`
carries a guard. A malformed verdict therefore does not fail — it falls through and
PUBLISHES. Swapping the reviewer's vendor is exactly the event that would expose that
latent asymmetry. It did not fire, but the check is cheap and silent when it breaks:
**re-run it on every remaining slice.**

### Substance — it genuinely reviewed

The decisive test is not whether it returned findings but whether it demonstrably
read the diff. It described the implementation as

> "the implementation derives verb sets from the predicate, but then maps those
> bitsets back to a finite string table"

and the merged code is exactly that: `per_item_hint_bits(lane, ...)` builds a `u16`
mask by consulting `per_item_verb_is_state_valid`, and
`const fn attention_item_footer_hint_for_bits(bits: u16)` maps the mask to the
literal (`lib.rs:1625-1670`). That structure is non-obvious, is not guessable from the
work-item title, and exists to work around a `const fn` constraint. **Only a reader
of the diff writes that sentence.** It is not a rubber stamp.

It also reported its own tooling limits honestly (`rg` absent; some piped
line-numbering blocked by the sandbox) rather than silently degrading.

### Where it fell short

- **261 words, ~2 minutes, mostly process narration, one finding.**
- It noticed that the new docs harness *"hardcodes the required contexts rather than
  deriving them"*, explicitly weighed "advisory or blocker", and chose advisory.
  That is **this item's own core concern recurring** — `mbohw3` exists to stop the
  verb vocabulary being typed independently in a second place, and the new verifier
  types the required-context list independently in a third. Arguably blocking.
- It **never tested whether the new verifier can fail.** No engagement with the
  mutation proof the implement stage claimed, the coverage gate, or a red
  demonstration. For an unbound-encoding item that is the single most load-bearing
  check, and it is the exact defect class this repo has been bitten by: the six
  `docs_*_lockstep` gates stayed green through two recorded doc rots.

### The finding it did raise — RESOLVED, and it goes against the reviewer

> `[ADVISORY] docs/detailed-usage.md:266 — The "Attention, backlog work-item selected"
> row appears unreachable under requires_attention_from_lane; it documents the derived
> source arm, but not a context the operator currently meets.`

**MEASURED SAME DAY: the row IS reachable, five times over, and the reviewer's
advisory is REFUTED.** `needs_attention.py --json` for this repo returns five standing
`hygiene:untriaged-backlog:<id>` items whose `source_ref` carries a work-item:

```json
{"path": null, "repo": "livespec-console-beads-fabro",
 "work_item": "livespec-console-beads-fabro-9ts"}
```

— likewise `-htp`, `-mvu22t`, `-oqm`, `-topr34`. All five are `lane=backlog`, verified
against the ledger.

The chain, end to end:

1. a `hygiene:untriaged-backlog` row enters the Attention list as an
   `AttentionEntry::NeedsAttention`. It is **not** filtered by
   `requires_attention_from_lane` — that predicate gates only the valve-actionable
   LANE FOLD, which is the step the advisory (and I) over-read;
2. `source_ref.work_item` is exactly what `AttentionItem::work_item_id()` reads;
3. `selected_work_item_lane()` resolves that id through `work_item_by_id` — from the
   LANE collection, not from the attention snapshot — yielding `Lane::Backlog`;
4. `attention_item_footer_hint(Lane::Backlog)` therefore renders.

**So the docs row is CORRECT and necessary** — do not remove it on the strength of the
advisory. And the pre-fix state was a GENUINE operator-visible defect rather than a
structural risk: with five backlog-resolving rows live, `m set-admission` was admitted
by the predicate and never advertised. The work-item's original rider, and its re-tier
to a correctness fix, were right.

**The audit's own corroboration was the weakest link, and it was mine.** Derivation (1)
above cited "a live 61-row Attention list holding no `Backlog` row" as support. That
list was dominated by worktree-hygiene rows whose `source_ref` carries a `path` and
**no** `work_item` — and I never checked whether any row carried a `work_item`
RESOLVING to backlog. Absence of a Backlog *lane* row is not absence of a
backlog-*resolving* row; I conflated the two and treated the result as confirmation.
This is the track's own named lesson landing on the person quoting it: **an absence
never announces itself in a check aimed at the wrong field.**

Derivation (2) — the implement stage's red TUI regression — was the one that had it
right all along, and it was right for the right reason: it had to build a fixture with
an Attention row whose source reference names a Backlog work-item *because that is what
production emits*, not as a contrivance.

### The verdict on the vendor swap

Written at n=1 as **"adequate, not good"** — it caught the right thing and graded it too
gently. **Revised at n=2, upward.** The second slice (`nvflph`) returned
`{"preferred_next_label": "fix"}` with a genuinely blocking, operator-visible finding:

> `[BLOCKING] crates/console-tui/src/lib.rs:1737 — The in-app Lanes help still documents
> the old move-status vocabulary as "any pre-terminal status" including active, plus
> approve/accept routes. That contradicts the ratified picker doors implemented by this
> branch.`

That is this thread's defect class exactly — a second, unbound encoding of the verb
vocabulary, here in help text — and it lands on `vwxyj4`'s clause ("the picker MUST NOT
offer `active`"): the branch enforces the door while the help still advertises it.

So the reviewer **discriminates**: approve on one slice, block on the next, both
well-formed. The honest summary is that it is a capable reviewer that under-grades
advisories, not a rubber stamp — and one advisory it did raise turned out to be wrong
in the safe direction (flagging a row that is in fact reachable), which is the better
way for a reviewer to be wrong.
