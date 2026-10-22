---
topic: factory-routing-petri-default
author: claude-opus-5-5
created_at: 2026-10-22T22:09:35Z
---

## Proposal: Default factory names petri

### Target specification files

- SPECIFICATION/constraints.md
- SPECIFICATION/scenarios.md

### Summary

Change the ratified default factory from `legacy` to `petri`. constraints.md §"Factory routing" will require `dispatcher.default_factory` to name `petri`, and Scenario 34 will assert that the default names `petri` and that a dispatch with no explicit factory lands on the server the host registry maps `petri` to. The registry-backed, no-machine rule for every committed entry is unchanged.

### Motivation

Coordinator ruling R668 (relayed 2026-10-22), under the maintainer's direction to finish the Fabro upgrade, cuts the fleet's default dispatch target over from the `legacy` factory (the hp fork-era engine) to the Petri-era `petri` factory, which the fleet factory registry binds to gmktec. The orchestrator cut over in its PR #3461 under ledger item bd-ib-iud52v. The v054 clause pins `legacy` by name, so this repository cannot follow without a revision. The registry-backed rule still holds: the `petri` entry declares `"registry": true` and names no server and no docker_host. The engine-specific keys it carries (`bin`, `secret_channel`, `environment`) name no machine. `legacy` stays declared, so in-flight items keep their recorded factory pin and rollback is a config revert.

### Proposed Changes

In SPECIFICATION/constraints.md §"Factory routing", replace the sentence "`dispatcher.default_factory` MUST name `legacy`." with "`dispatcher.default_factory` MUST name `petri`."

In SPECIFICATION/scenarios.md §"Scenario 34 -- A dispatch resolves its factory through the host's factory registry":
- In the mermaid flowchart, rename the node `Legacy["legacy entry: registry true, no server, no docker_host"]` to `Petri["petri entry: registry true, no server, no docker_host"]`. Change `Default["dispatcher.default_factory names legacy"]` to `Default["dispatcher.default_factory names petri"]` and `Server["Fabro server the registry maps legacy to"]` to `Server["Fabro server the registry maps petri to"]`. Re-point the edges to `Petri`.
- In the first scenario, change `And \`dispatcher.default_factory\` names \`legacy\`` to `And \`dispatcher.default_factory\` names \`petri\``.
- Rename the second scenario to `Scenario: A dispatch with no explicit factory lands where the registry maps petri`, and change its Then step to `Then its Fabro run is created on the server address that file maps \`petri\` to`.

No heading changes, so tests/heading-coverage.json needs no edit.
