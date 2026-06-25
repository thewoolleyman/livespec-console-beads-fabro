---
topic: claude-opus-4-8-critique
author: claude-opus-4-8
created_at: 2026-06-25T00:26:59Z
---

## Proposal: Adapter-isolation rule: crate granularity and term scope both under-specified

### Target specification files

- SPECIFICATION/non-functional-requirements.md
- SPECIFICATION/spec.md

### Summary

non-functional-requirements.md §Constraints → Domain-Driven Design states 'Adapter crates MAY depend on application/domain contracts but MUST NOT depend on each other,' and its crate-graph diagram draws five distinct console-adapter-* crates (fabro/beads/livespec/dispatcher/github); §Architecture Tests restates this as 'adapters do not depend on each other.' But no rule actually mandates the one-crate-per-source split: the same isolation could be honored by a single console-adapters crate with per-source modules behind per-source ports, in which case 'MUST NOT depend on each other' is vacuously satisfied and cannot be expressed as a crate-graph check. Separately, spec.md §Architecture's hexagonal diagram labels the SQLite event store and the TUI/web frontends as 'adapters' in the same outer ring as the five source adapters, so the scope of 'adapters' in the no-cross-dependency rule (source adapters only, or also event-store/UI crates?) is left open.

### Motivation

It is ambiguous whether the per-source crate split is an intended hard requirement or an over-constraint, and the 'adapters MUST NOT depend on each other' rule is inconsistent in scope between spec.md (which calls the SQLite and TUI components 'adapters') and non-functional-requirements.md (whose console-adapter-* naming implies only source adapters); a reader cannot tell what crate granularity the rule mandates or which components it binds.

### Proposed Changes

Decide and state explicitly in non-functional-requirements.md §Domain-Driven Design whether per-source adapter crates are REQUIRED or whether a single adapters crate with per-source modules also satisfies the intent. If the per-crate split is not mandatory, re-express the isolation invariant at the level it actually binds (e.g., 'no adapter module may depend on another adapter's internals; each source adapter MUST sit behind its own port'), so the rule is checkable regardless of crate count. Also define the scope of the term 'adapter' in the no-cross-dependency and 'adapters do not depend on each other' rules: clarify whether the SQLite event-store crate and the TUI/web crates are 'adapters' for that rule, and reconcile spec.md §Architecture's outer-ring labeling with non-functional-requirements.md's crate taxonomy. No implementation is proposed.

## Proposal: Architecture-test enforcement: MUST-enforce rules with MAY-only mechanisms are not falsifiable

### Target specification files

- SPECIFICATION/non-functional-requirements.md

### Summary

non-functional-requirements.md §Architecture Tests lists rules the checks 'MUST enforce at least' (no forbidden dependency direction in the workspace crate graph; domain has no dependency on adapters/SQLite/web/TUI/HTTP/subprocess/filesystem; adapters do not depend on each other; UI does not call sources directly; no unwrap/expect outside allowed scopes; event/command types live in domain/application; all use cases return typed Result), but then states only that 'cargo metadata MAY enforce crate graph rules' and 'Source-level checks MAY use Rust syntax parsing where needed.' The mechanism strong enough to verify the crate-graph rules (cargo metadata) and the mechanism strong enough to verify the source-level rules accurately (real Rust parsing) are both optional, so a naive text/substring scan satisfies the letter of every MUST-enforce rule.

### Motivation

The section is internally inconsistent: it mandates a rule set with MUST but leaves every enforcement mechanism permissive (MAY / 'where needed'), so it is undefined whether any rule must be checked falsifiably. A grep-level text scan (false positives on comments/strings, false negatives on re-exports, aliases, and path dependencies) technically discharges the obligation, which makes the 'MUST enforce' unverifiable in practice.

### Proposed Changes

Make the crate-graph dependency check REQUIRED rather than MAY (e.g., 'The workspace crate-graph dependency rules MUST be enforced from cargo metadata or an equivalent structured crate-graph source, not a text scan'). For each source-level rule, state the minimum fidelity that counts as enforcement (e.g., 'the unwrap/expect ban MUST be checked at AST level, distinguishing unwrap/expect calls from substrings in comments, strings, and identifiers such as unwrap_or'). State each enforced rule falsifiably enough that a reviewer can name an input that would make it fail. No implementation is proposed.

## Proposal: Quality gate: coverage threshold is required to be 'declared' but no number is declared

### Target specification files

- SPECIFICATION/non-functional-requirements.md

### Summary

non-functional-requirements.md §Contracts → Quality Gate requires the full check aggregate to include 'coverage with a declared threshold,' but the spec declares no threshold value and does not say where the threshold is declared or that CI fails below it. As written, any coverage level — including 0% — satisfies the requirement, because the rule only requires that a threshold be 'declared,' not that it meet any particular bar.

### Motivation

The phrase 'a declared threshold' is self-referential and unfalsifiable: it is undefined what the number is or where it lives, so a reviewer cannot tell whether a given build passes or fails the coverage gate, leaving the spec silent on the one fact this requirement exists to pin.

### Proposed Changes

State the concrete coverage threshold (e.g., a minimum line/branch percentage) in non-functional-requirements.md §Quality Gate, or name the exact authoritative location where the number is declared and assert that just check / CI fails below it. If different thresholds are intended per layer (domain vs adapters vs UI), state each. No implementation is proposed.

## Proposal: Quality gate: 'where practical' leaves fuzz and mutation graduation criteria undefined

### Target specification files

- SPECIFICATION/non-functional-requirements.md

### Summary

non-functional-requirements.md §Contracts → Quality Gate requires 'fuzz tests for event decoding, adapter normalization, and source payload parsing where practical' and 'mutation testing where practical.' The qualifier 'where practical' has no stated criterion, so the spec does not define when fuzzing or mutation testing must graduate from an optional smoke check to a hard, build-failing gate; any omission can be justified after the fact as 'not practical.'

### Motivation

'Where practical' is unfalsifiable: it is unclear and undefined what makes fuzz or mutation testing practical or impractical, so the requirement can be permanently satisfied by a token smoke check, defeating its purpose as a quality gate.

### Proposed Changes

Replace 'where practical' with concrete graduation criteria in non-functional-requirements.md §Quality Gate: state which targets MUST have fuzz harnesses and what bar gates the build (e.g., a minimum corpus and no new crashes on a fixed iteration budget in CI), and state the mutation-testing bar (e.g., a minimum mutation-kill score over a named module set). If fuzz/mutation are intentionally non-gating for now, say so explicitly and state the condition that flips them to gating. No implementation is proposed.

## Proposal: Missing rule: initial-adapter fidelity and the honest-incompleteness obligation

### Target specification files

- SPECIFICATION/contracts.md
- SPECIFICATION/spec.md

### Summary

Neither spec.md (§Product Shape / §Architecture) nor contracts.md (§Adapter Contract / §Initial Adapters / §Command Handling) resolves whether first-milestone adapters and command ports must perform real source I/O or may be minimal/simulated, nor what 'honest incompleteness' requires of a stub. The Adapter Contract's completeness rule covers only incomplete history ('If a source cannot provide complete historical transitions ... emit ... a completeness finding rather than claiming full history'); nothing governs an adapter/port that performs no real I/O. §Command Handling tells a handler to 'append success/failure/outcome events' with no rule that an outcome event must reflect a real effect, so a simulated drain port can emit factory.drain.completed without acting — an operator-observable fabricated success — even though §Terminology says a command 'is not itself proof that the requested action occurred.'

### Motivation

This is a missing rule: the spec is silent on simulated-vs-real adapter fidelity and on the honesty obligation of a stub, leaving it undefined whether emitting a success or observed-fact event for an action never performed or observed is permitted. That gap is what lets a simulated port fabricate success, which contradicts the spec's own stance that an outcome must be an observed fact rather than an assumed one.

### Proposed Changes

Add an explicit honesty/fidelity rule. State in spec.md §Product Shape (or §Architecture) whether initial-milestone adapters and command ports MUST perform real source I/O or MAY ship simulated/minimal. Add a rule (operator-observable, so likely in constraints.md or contracts.md §Adapter Contract / §Command Handling) that an adapter or port which has not actually performed or observed an action MUST signal 'not observed' / 'simulated' / 'unimplemented' (e.g., via a health/completeness finding or a typed not-observed outcome) and MUST NOT emit a success or observed-fact event asserting an effect it did not achieve or observe. No implementation is proposed; this is the spec-side rule whose absence the separately-tracked simulated-boundary implementation gap depends on.

## Proposal: Post-split coherence: DDD layering and Architecture Tests duplicate the same invariants with divergent enumerations

### Target specification files

- SPECIFICATION/non-functional-requirements.md

### Summary

Within non-functional-requirements.md, §Constraints → Domain-Driven Design and §Constraints → Architecture Tests state the same dependency invariants twice with different wording and different enumerations. DDD says domain 'MUST not depend on infrastructure crates such as web, db, process, HTTP, filesystem adapters, or terminal UI'; Architecture Tests says 'domain has no direct dependency on adapters, SQLite, web server, TUI, HTTP, subprocess, or filesystem APIs.' The two forbidden-set enumerations differ (db vs SQLite, process vs subprocess, terminal UI vs TUI, and DDD omits 'adapters' from the domain list that Arch Tests includes). The UI rule is likewise duplicated: DDD 'UI crates MUST talk to projections and command APIs, not directly to source systems' vs Arch Tests 'UI does not call Beads/Fabro/LiveSpec/GitHub directly.'

### Motivation

The duplicated-but-divergent enumerations are an inconsistency: it is unclear whether the DDD layering list and the Architecture Tests list denote the same normative set or two different ones, and maintaining the rule in two places invites drift where one list is updated and the other is not.

### Proposed Changes

Establish a single source of truth for each dependency invariant in non-functional-requirements.md: either state the forbidden-dependency set once (in §Domain-Driven Design) and have §Architecture Tests reference it as 'the layering rules above,' or align the two enumerations verbatim and add a note that they must stay in lockstep. Reconcile the divergent terms (db/SQLite, process/subprocess, terminal UI/TUI) and clarify whether 'adapters' belongs in domain's forbidden list in both places. No implementation is proposed.

## Proposal: Post-split coherence: load-bearing behaviors specified in prose have no scenario

### Target specification files

- SPECIFICATION/scenarios.md
- SPECIFICATION/contracts.md
- SPECIFICATION/constraints.md

### Summary

Several behaviors specified normatively in prose have no corresponding scenario in scenarios.md, which pins only the happy-path attention inbox, factory drain, adapter backfill, incomplete-history snapshot, and TUI workflow. Unpinned normative behaviors include: the command-rejection path (contracts.md §Command Handling: policy validation → 'command rejected event'); crash-gap recovery (contracts.md §Command Handling step 5: 'Leave recovery to reconciliation/backfill when a crash occurs between an external side effect and outcome event append'); and snapshot/read-model corruption recovery by replay (constraints.md §Event-Sourcing Safety: 'Snapshot/read-model corruption MUST be recoverable by replay'). scenarios.md Scenario 2 covers only the accepted-drain happy path.

### Motivation

Leaving these MUST-level behaviors without scenarios is an under-constraint: the rejection, crash-gap-reconciliation, and corruption-recovery guarantees are stated as prose invariants but have no falsifiable behavioral journey, so it is unclear how each is expected to behave at the observable surface and nothing guards against silent regression.

### Proposed Changes

Add scenarios to scenarios.md (or to non-functional-requirements.md §Scenarios where the behavior is contributor-facing) pinning at least: a rejected command (policy-invalid drain → command.rejected, no side effect), crash-gap recovery (side effect performed but outcome event not appended → reconciliation reconstructs the outcome), and snapshot-corruption recovery (corrupt projection state → drop and rebuild from the event log). Alternatively, record an explicit decision that a given behavior does not warrant a pinned scenario. No implementation is proposed.

## Proposal: Post-split coherence: single-binary runtime shape stated with inconsistent normative force

### Target specification files

- SPECIFICATION/constraints.md
- SPECIFICATION/spec.md

### Summary

The single-binary multi-mode runtime shape is stated with inconsistent force across files. spec.md §Product Shape declares it as settled fact ('The steady-state product is a single Rust executable'); constraints.md §Runtime Shape downgrades it to a recommendation ('The executable SHOULD be a single binary that can run TUI/service/API modes from one artifact'); and non-functional-requirements.md §Boundary refers to it as 'the single-binary multi-mode runtime shape' as though it were a fixed constraint. A reader cannot tell whether shipping the console as multiple binaries would violate the spec.

### Motivation

The modality is contradictory between files: spec.md and the non-functional-requirements.md boundary text read as a firm decision while constraints.md uses SHOULD, so it is unclear and inconsistent whether single-binary is a hard requirement (MUST) or merely preferred (SHOULD).

### Proposed Changes

Pick one normative force for the single-binary multi-mode runtime shape and state it consistently: if it is a hard requirement, use MUST in constraints.md §Runtime Shape and keep spec.md's declarative phrasing; if it is a preference, soften spec.md §Product Shape to match the SHOULD and adjust the non-functional-requirements.md §Boundary wording. No implementation is proposed.
