# What landed, verified against merged code — and one premise of the cut that was weak

**Ledger anchor:** `livespec-console-beads-fabro-thu6gp`

Three of this thread's four dispatchable items are implemented and merged. This
note records the acceptance VERIFICATION (done against the merged code, not
inferred from a green run) and re-examines one premise of the `-mcj` cut that a
sibling thread has since shown to be a weak instrument.

## All three guards are wired and non-vacuous

`run_checks` (`crates/console-arch-check/src/main.rs`) now runs SIX families,
up from the three this thread was chartered against:

```
check_crate_graph
check_crate_sources          (per scanned crate)
check_zero_beads_knowledge               <- -p4bvrt
check_tmux_socket_scoping
check_workspace_rust_version_matches_toolchain   <- -mcj.2
check_fabro_image_rust_toolchain                 <- -mcj.1
```

`just check-arch` is green on master and the crate's suite is 86 tests, 0 failed.

### `-p4bvrt` avoided the vacuity trap it was filed to warn about

Its record said the obvious implementation — grepping for `Command::new("bd")` —
protects nothing, because the one process-spawn site takes a RUNTIME VALUE and
no literal exists to match. The merged code does the right thing instead:

- It asserts a **closed allow-list**, `ALLOWED_BACKING_CLI_DEFAULT_TOKENS`, and
  flags any default token not on it — suspect-by-default, so adding a backing CLI
  forces a deliberate decision rather than silently widening the resolvable set.
- It refuses to pass vacuously: `check_zero_beads_source_paths` carries the
  `paths.is_empty()` guard ("refusing to pass without having read anything").
- It states its honest limit in the doc comment — `from_environment` applies
  runtime program overrides, so no static check can promise "the console can
  never invoke bd".

That is the full shape the item specified, including the part that is easy to
skip.

### `-mcj.2` handled the version-comparison trap

`1.92` and `1.92.0` agree, so string equality fails on correct input; and a naive
textual prefix would wrongly accept `1.92` against `1.920.0`. The merged code
parses both into components and uses `is_component_prefix_of`, which is
`starts_with` over parsed NUMBERS — `[1,92]` against `[1,920,0]` is correctly
false. Test `workspace_rust_version_two_component_prefix_matches_toolchain` pins
the legitimate case.

## The sizing warning that motivated the cut is a weak instrument

`7d8558f` (a sibling thread, on `-4jb3kl`) established that the Dispatcher's
`item-sizing` warning measures the wrong thing: it uses DESCRIPTION LENGTH as a
proxy for work size, and since the fix for "the sandbox never sees comments"
moved every load-bearing constraint out of comments and into descriptions, a long
description now means MORE GUIDANCE, not heavier work. On that thread's slices
the correlation was outright inverted — the smallest work carried the longest
description.

**That warning is the first reason I gave for cutting `-mcj`.** Being honest
about it: the instrument was weak, and the inversion applies to `-mcj` too. Its
3548 characters were carrying guidance about four copies of one pin — light work,
heavily documented — not four pieces of heavy work.

**The cut was still right, on its other grounds, and the outcome shows it.**

- The reasoning that actually carried it never depended on length. The recorded
  rationale says outright that "the four enumerated parts are four COPIES of one
  pin, not four pieces of work", and cuts by ENFORCEMENT MECHANISM instead —
  probe an image / compare two declarations cargo acts on / edit prose. That
  argument is untouched by the sizing finding.
- It isolated the only contended file. `.github/workflows/ci.yml` is owned by
  `plan/test-adequacy-gates/`, and all of that contention went into `-mcj.3`
  alone.
- **The isolation then paid off for an unrelated reason.** When the hp factory
  went under a dispatch hold, `-mcj.1` and `-mcj.2` had already merged and only
  `-mcj.3` was left held. Uncut, the whole of `-mcj` would now be blocked
  behind a prose edit.

The lesson to carry is not "do not cut" but **do not cite the sizing warning as
evidence of size**. Read the actual work — site counts, distinct mechanisms,
contended files. Dispatcher mechanics belong to
`livespec-orchestrator-beads-fabro` per `AGENTS.md` "Repository scope", so this is
recorded here rather than filed as a defect.

## What remains

> **SUPERSEDED 2026-08-21 by `004-the-gate-that-was-not-one.md`. BOTH bullets
> below are now false.** `-mcj.3` closed (PR 780). And `-mvu22t` was never a
> maintainer gate — the two claims inflating it ("fleet-wide", "exempt
> docs/chore") were both measured false, and the decision was the session's to
> make under standing directive 4. The section is left in place rather than
> rewritten because it is the record of what this thread believed at the time,
> and that belief surviving eight consecutive handoffs is itself the finding.

- `-mcj.3` — HELD at `backlog` by the hp dispatch hold, not by anything about
  its content. Returns to `ready` when the hold lifts.
- `-mvu22t` — the one genuine maintainer gate in this thread: its commit-msg hook
  would gate every later commit fleet-wide, and `lefthook.yml` still has no
  `commit-msg` section at all.
