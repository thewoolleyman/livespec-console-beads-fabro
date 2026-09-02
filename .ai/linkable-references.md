# Every maintainer-facing reference must be web-linkable

**The rule (maintainer directive, 2026-09-02).** Anything a session hands
to the maintainer — a handoff, a report, a question, an ASK, a completion
summary — may only reference things through a URL the maintainer can open
in a browser. The maintainer REFUSES a handoff that leans on an
unlinkable reference; treat that refusal as the contract, not as
friction.

**Why.** During the retire-overseer plan (2026-09-02) a status summary
said "Scenario 30" with no link. The maintainer had no way to know
whether that named a work item, a git file, or something else — and said
so: spraying opaque names is exactly how the fleet falls back into the
not-enough-human-attention trap this plan exists to fix. Attention is
the scarcest resource; a reference that costs the maintainer a lookup is
a reference that does not get looked up.

**How to apply.**

- Git-side content (spec scenarios, contracts, plan research, code):
  cite as a GitHub URL to the file — with the heading anchor or line
  numbers when they exist. "Scenario 30" becomes
  `[Scenario 30 — A needs-human terminal reaches the operator as a
  ledger valve](https://github.com/thewoolleyman/livespec-console-beads-fabro/blob/master/SPECIFICATION/scenarios.md)`.
- PRs, CI runs, releases: the GitHub URL, always.
- Beads work-items are NOT web-linkable today. Until the beads web UI
  lands (planned in the dolt-server repo: `plan/beads-web-ui/`, see that
  repo on GitHub), a work-item reference must carry the id PLUS its
  title PLUS enough inline substance that the maintainer needs no
  lookup. Once the UI exists, link the item's page.
- The rule extends to option text inside AskUserQuestion pickers and to
  ledger comments the maintainer is asked to read.

This is recorded both here and as a standing maintainer directive on the
retire-overseer plan epic (`livespec-console-beads-fabro-pzbdbo`).
