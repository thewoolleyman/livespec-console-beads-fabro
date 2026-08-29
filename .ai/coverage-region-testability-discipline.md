# Coverage-region testability discipline (console)

Durable, learned agent knowledge for `livespec-console-beads-fabro`,
loaded on demand from the `.ai/` list in `CLAUDE.md` / `AGENTS.md`.
Captured after a session where a residual uncovered region under the
region-coverage gate was labelled "genuinely unreachable" and dispositioned
as *restructure-later, do not test* — which would have buried a latent bug
behind a coverage label. The maintainer corrected the framing; this file is
that correction, generalised.

## There is no "genuinely-unreachable" resting state

"Genuinely unreachable" is **not a disposition**. In well-designed code:

- If a region is **genuinely unreachable**, it has no reason to exist —
  **delete it** (the code, not the coverage of it). This is what
  `SPECIFICATION/non-functional-requirements.md`'s no-exclusions clause
  means by "un-executable and therefore deletable": delete the construct,
  never annotate around it.
- If a region is **reachable**, it is **always testable**. Reach it with a
  test.
- If a region is reachable **but you cannot test it**, that is a **design
  defect** (poor coupling/cohesion), not a fact about the region. **Refactor
  for unit-level testability** — inject the dependency, or use an
  already-injected value — then test it.

So a residual uncovered region is never "tolerated as unreachable." It is
always resolved into one of: *deleted*, *tested*, or *refactored-then-tested*.
Recording "genuinely unreachable → leave it" is the failure mode this file
exists to prevent.

Two anti-patterns that only *look* like a fix and are forbidden:

- **Coverage annotation / exclusion.** Forbidden outright by the
  no-exclusions clause (re-ratified 2026-08-21). Not an option.
- **`.expect()` / `.unwrap()` on the "impossible" error.** This does NOT
  remove the dead branch — it **relabels the dead `?` region as a dead panic
  region**, which is still uncovered. It moves the problem, it does not solve
  it.

## The hidden-global heuristic (how to tell which case you are in)

A production `?`-arm's Err path is:

- **Testable-by-injection** when the fallible call is on an **injected
  dependency** — e.g. `store.command_count()?`, `WorkItemSnapshot::new(...)?`.
  Drive the Err arm by constructing a store / input that FAILS at that call.
  This is the region-coverage lane's normal, correct technique (children that
  close `?`-arms on the injected store).

- **An injectability smell** when the fallible call is on a **hidden global**
  — the system clock, `std::env`, the filesystem, "now", process state —
  reached directly rather than through an injected seam. You cannot make a
  hidden global fail from a unit test, which is *exactly why the region looks
  "unreachable."* The fix is not a test and not deletion-in-place; it is to
  **inject the dependency, or use a value the caller already injected**, which
  makes the site deterministic and usually deletes the fallible call entirely.

Rule of thumb: **"unreachable" almost always means "the fallible thing is a
hidden global that should have been injected."** Check what the fallible call
operates on before you ever write the word "unreachable."

## Worked example: the discarded-timestamp latent bug

`crates/console-cli/src/lib.rs`, `persist_tui_runtime_effects`:

- The function takes `_requested_at: &str` — **underscore-prefixed, i.e.
  deliberately unused**.
- It ignores that parameter and calls `current_command_requested_at()?`,
  which reaches the **global clock**:
  `OffsetDateTime::now_utc().format(&Rfc3339).map_err(...)`.
- `current_command_requested_at()` has **exactly one caller** (that site).
- Every real caller already injects a timestamp (a production call passes
  `observed_at`; tests pass a fixed `"2026-06-23T00:00:02Z"`) — and it is
  **silently discarded** in favour of wall-clock time.

The uncovered Err arm existed **solely because an injected value was thrown
away for a hidden global.** That is the injectability smell, and it was
masking a real defect: the persisted `command_requested_at` did not match the
caller's supplied timestamp.

**The fix** (this is what "restructuring disposition" means here): use the
injected parameter — drop the underscore, thread `requested_at` into the
append — and **delete `current_command_requested_at()`**. The uncovered
region disappears (no `?`), the timestamp becomes deterministic and
caller-controlled (existing tests now assert it), and the discarded-timestamp
bug is corrected. It IS a behaviour change, so it carries Red-Green-Replay and
a timestamp assertion. It is NOT a test bolted onto dead code, and it is NOT
`.expect()`.

## Prior dispositions that got it right (what "restructure" should look like)

Two earlier region dispositions in this lane honoured the principle and are
good models:

- **`civil_from_days` (console-red-green-replay-check).** Its
  `i32::try_from(year).unwrap_or(i32::MAX)` (and month/day) fallbacks are a
  **total** function: no uncovered region, no panic arm, and the fallback is
  genuinely reachable for pathological `i64` inputs. Totality via `unwrap_or`
  is a legitimate resolution — it is not a masked dead branch.

- **The seed `?`-arms (`source_polls_from_seed`, console-cli).** Restructured
  so the **injected seed's fields** flow into the fallible `::new(...)?`
  constructors, making the Err arms reachable and testable; a
  `source_seed_builder_rejects_invalid_static_identity_fields` test exercises
  the rejection.

Both mean the same thing the maintainer's principle means: **restructure to
delete-or-make-testable, never to annotate or tolerate.**
