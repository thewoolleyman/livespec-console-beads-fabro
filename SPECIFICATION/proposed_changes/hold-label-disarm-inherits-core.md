---
topic: hold-label-disarm-inherits-core
author: claude-opus-5-5
created_at: 2026-11-11T08:36:55Z
---

## Proposal: This repository carries the hold-label disarm workflow livespec core makes inherited fleet policy

### Target specification files

- SPECIFICATION/non-functional-requirements.md

### Summary

Add a `### Hold-Label Disarm` subsection under `## Contracts` in `non-functional-requirements.md`, binding this repository, which carries `auto-enable-merge.yml`, to also carry `disarm-on-hold-label.yml`, citing livespec core for the behaviour rather than restating it, and add `## Contributor Scenario M` as the clause's scenario.

### Motivation

livespec core's proposal `hold-label-disarm-inherited-policy` (livespec#3162, ratified in livespec v240, livespec#3163) makes the `auto-enable-merge.yml` / `disarm-on-hold-label.yml` pair inherited fleet policy in livespec `non-functional-requirements.md` §"Shared content sync — copier template", and says a non-template member carries its own copy and cites that section rather than restating it. This repository runs `.github/workflows/auto-enable-merge.yml` and carries no `disarm-on-hold-label.yml`, so a `do-not-merge` label added after the factory bot armed a pull request leaves auto-merge on: the gap measured on livespec-dev-tooling#4019, #4031 and #4033. No clause in this specification governs auto-merge or hold labels today, so the carrier work item has no honest References line. The four other non-template members (livespec-overseer v094, livespec-runtime v034, livespec-driver-claude v017, livespec-driver-codex v023) took the same clause. Unlike livespec-runtime, this repository's normative clauses must each link to a contributor scenario (`### Behavioral Coverage`), so the clause is given one. This repository's `auto-enable-merge.yml` mints its App token with `actions/create-github-app-token@v3` and the `client-id:` input, the same input livespec's root `disarm-on-hold-label.yml` uses, so the copy can be byte-identical. Plan ci-runtime-and-cost (livespec-dev-tooling-ib3oe3).

### Proposed Changes

1. In `SPECIFICATION/non-functional-requirements.md`, under `## Contracts`, after `### Delegated Pre-Push Gate` and before `## Scenarios`, add this subsection:

### Hold-Label Disarm

Because this repository carries `.github/workflows/auto-enable-merge.yml`, it MUST also carry `.github/workflows/disarm-on-hold-label.yml`, byte-identical to livespec's root copy, which turns a pull request's auto-merge off when the `do-not-merge` label is added to it. The behaviour, and the rule that every fleet member carrying `auto-enable-merge.yml` carries it, are livespec's `non-functional-requirements.md` §"Shared content sync — copier template" and are not restated here. The clause links to `## Contributor Scenario M -- A hold label added to an armed pull request turns its auto-merge off`.

2. In the same file, after `## Contributor Scenario L -- A delegating host's push gates on the remote executor and refuses on no verdict`, add:

## Contributor Scenario M -- A hold label added to an armed pull request turns its auto-merge off

```gherkin
Feature: A hold label disarms an armed pull request
  As a LiveSpec maintainer holding a pull request
  I want adding the do-not-merge label to turn the pull request's auto-merge off
  So that a pull request the factory bot armed does not merge while it is held

Scenario: A hold label added to an armed pull request turns its auto-merge off
  Given a pull request in this repository whose auto-merge is armed
  When the do-not-merge label is added to it
  Then the disarm-on-hold-label workflow runs for that pull request
  And the pull request's auto-merge is turned off
```

3. The same revise MUST add `§"Shared content sync — copier template"` to the `livespec` entry of `.livespec.jsonc`'s `external_references` allowlist, in the bare heading form that file requires, and MUST register `## Contributor Scenario M` and the new clause in `tests/heading-coverage.json` (a spec-first `"test": "TODO"` row naming its tier and owning carrier work item).

Ordering: livespec's revise of `hold-label-disarm-inherited-policy` has merged (livespec v240), so the cited behaviour exists when this clause ratifies.
