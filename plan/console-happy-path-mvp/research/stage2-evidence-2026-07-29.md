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

### The finding it did raise, and why it is unresolved

> `[ADVISORY] docs/detailed-usage.md:266 — The "Attention, backlog work-item selected"
> row appears unreachable under requires_attention_from_lane; it documents the derived
> source arm, but not a context the operator currently meets.`

This is the **third independent derivation** of the same reachability point:

1. the 2026-07-29 docs-custody audit (source read, plus a live 61-row Attention list
   holding one `Blocked` and five `PendingApproval` rows and no `Backlog` row);
2. this item's OWN implement stage, whose TUI regression went RED until it built a
   fixture with an Attention row whose SOURCE REFERENCE names a Backlog work-item;
3. the Codex review above.

**The open question, which none of the three answers:** a Backlog lane row cannot
enter the Attention list, so the only route is an ingested needs-attention row whose
`work_item_id` resolves — via `selected_work_item_lane` -> `work_item_by_id` — to a
Backlog item. Does the orchestrator actually EMIT such rows in production, or is that
only synthesizable in a fixture? If the latter, this slice documented a context
operators cannot meet, which is the inverse of the dishonesty `pane_footer_hint`'s own
doc comment forbids ("advertise keys that do nothing"). **Answer this before treating
the row as correct.**

### The verdict on the vendor swap

**Adequate, not good.** The claim this record supports is *"it caught the right thing
and graded it too gently"* — not *"we changed reviewers under duress and it was
fine"*. The distinction matters before four more slices ride on it.
