---
topic: pre-commit-skips-the-cargo-leg
author: claude-opus-5-5:ci-runtime-and-cost
created_at: 2026-11-13T05:16:55Z
---

## Proposal: The pre-commit hook skips its cargo leg when a commit stages no Rust path

### Target specification files

- SPECIFICATION/non-functional-requirements.md

### Summary

Add a pre-commit clause to non-functional-requirements.md ### Quality Gate, and two scenarios to the Gherkin feature of ## Contributor Scenario C. The pre-commit hook's cargo leg is `cargo fmt --check`, `cargo clippy` denying warnings, and the architecture check. The hook MUST skip that leg when no staged path is a Rust path, and the Rust paths are named concretely from this repository's layout. It MUST report the skip as a skip, never as a pass. It MUST run the full leg when the staged path set is empty or unreadable, or includes a symlink, a submodule, a mode change or a type change. Pre-push and CI keep running the full `just check` aggregate whatever the commit staged. The rule mirrors the CI documentation-lane classifier already ratified in the same section: classify by staged path, and fail closed to the full leg.

### Motivation

Measured 2026-11-13 04:57-05:02Z by legacy-coordinator-agent on the VPS vmi3006760.contaboserver.net. A 7-line `.livespec.jsonc` deletion in this repository ran `cargo fmt --all --check` and `cargo clippy --workspace --all-targets --all-features -- -D warnings` in its pre-commit hook. That cost about 402 CPU-s, 7.9% of host CPU, and it was the 4th-largest consumer on a host at CPU PSI ~66-71 with 5% idle. The maintainer has asked for load to come off that host. This repository's pre-push gate already delegates its full aggregate to the cluster (§"Delegated Pre-Push Gate"), so pre-commit is the remaining host-local cargo build on a config-only commit. The ratified text states the CI documentation-only classifier and what `just check` contains, but says nothing about which subset the pre-commit hook runs. Today the hook runs `just check-pre-commit` (check-format, check-clippy, check-arch) on every commit. Work item livespec-console-beads-fabro-p37s4b carries the implementation and is held on this spec-change tier.

### Proposed Changes

In SPECIFICATION/non-functional-requirements.md, under ### Quality Gate, insert the following paragraph after the paragraph that begins "**Code-neutral documentation lane -- CI on classified documentation-only" and before the paragraph that begins "**Nightly -- scheduled run against the canonical branch.**":

**Pre-commit -- the commit-time cargo leg.** The pre-commit hook's cargo leg is `cargo fmt --check`, `cargo clippy` denying warnings, and the architecture check. The hook MUST classify the commit by its staged paths, and MUST NOT run that leg when no staged path is a Rust path. A Rust path is: any path ending in `.rs` (including `build.rs`); any file named `Cargo.toml` or `Cargo.lock`; `rust-toolchain.toml` or `rust-toolchain`; any path beneath `.cargo/`; `rustfmt.toml`, `.rustfmt.toml`, `clippy.toml` or `.clippy.toml`; and `.fabro/workflows/implement-work-item/workflow.toml`, which the architecture check reads to compare the sandbox image's Rust toolchain. Both the source and the destination of a staged rename MUST be classified. When the hook does not run the leg, it MUST print a line naming the skipped leg and the reason that no Rust path is staged, and MUST NOT report that leg as passed. The hook MUST run the full leg when the staged path set is empty or cannot be read, when a staged path is a symlink or a submodule, when a staged change alters a file's mode or type, or when classification fails for any reason. This skip MUST NOT change the pre-push gate or CI: `just check` at pre-push, whether run locally or delegated, and in CI MUST still include `cargo fmt --check`, `cargo clippy` denying warnings, and the architecture checks, whatever paths a commit staged.

In the same file, in the ```gherkin``` feature of ## Contributor Scenario C -- Quality gate enforces the inner and merge loops, insert the following two scenarios after "Scenario: The required aggregate fails closed in either lane" and before "Scenario: A nightly finding opens a chore instead of failing master":

```gherkin
  Scenario: A commit that stages no Rust path skips the pre-commit cargo leg
    Given a commit whose staged paths are all regular files with no mode
      or type change, and none of them is a Rust path
    When the pre-commit hook runs
    Then it does not run cargo fmt --check, cargo clippy, or the
      architecture check
    And it reports the cargo leg as skipped because no Rust path is staged
    And just check at pre-push and in CI still includes fmt --check,
      clippy denying warnings, and the architecture checks

  Scenario: A staged Rust path or classification uncertainty runs the full pre-commit cargo leg
    Given a commit that stages a Rust path, or whose staged path set is
      empty or unreadable, or that stages a symlink, a submodule, or a
      mode or type change
    When the pre-commit hook runs
    Then it runs cargo fmt --check, clippy denying warnings, and the
      architecture check
```

No `## ` heading is added or removed. Both scenarios sit inside the existing ## Contributor Scenario C feature, so tests/heading-coverage.json keeps its existing row for that heading. The implementing item MUST add tests exercising both scenarios to that row's tests.
