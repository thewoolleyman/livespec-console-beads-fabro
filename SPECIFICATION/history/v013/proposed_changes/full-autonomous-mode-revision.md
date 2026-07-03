---
proposal: full-autonomous-mode.md
decision: accept
revised_at: 2026-07-03T00:22:11Z
author_human: thewoolleyman <chad@thewoolleyman.com>
author_llm: claude-opus-4-8
---

## Decision and Rationale

Accept the full-autonomous-mode proposal as authored. It lands the console's operator-facing surface for a per-repo, default-OFF, dangerous autonomous mode: a Full Autonomous Mode section in spec.md (with the Configuration context extended to own autonomous-mode policy), the wire contract in contracts.md (the .livespec.jsonc setting, the config.autonomous_mode_set command with its confirmed guard, the config.autonomous_mode.enabled/disabled audit events, the factory.autonomous_mode_enable/disable_requested commands to the orchestrator plane, and the TUI dangerous-labelled type-to-confirm toggle), the operator-observable Autonomous-Mode Safety constraints, and two Gherkin scenarios. Placement respects all three authoring splits: behavior carries a clause+scenario, functional content stays in the operator-facing files, and the orchestrator-side decision engine is left to the orchestrator repos (noted, not mis-filed here).

## Resulting Changes

- spec.md
- contracts.md
- constraints.md
- scenarios.md
