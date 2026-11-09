---
topic: python-helper-coverage-config
author: claude:dev-host-test-gate-load
created_at: 2026-11-09T19:38:31Z
---

## Proposal: The Python helper tree's coverage is gated and combines under any pytest coverage spelling

### Target specification files

- SPECIFICATION/non-functional-requirements.md

### Summary

State, in the inner-loop quality gate, the 100% line and branch coverage the `check-per-file-coverage` recipe already holds the Python helper tree under `dev-tooling/` to. Require that pyproject.toml's coverage configuration governs every pytest coverage run, so a bare `pytest --cov` spelling still combines branch data. Add a Contributor Scenario C scenario for it.

### Motivation

Work item livespec-console-beads-fabro-olhsgx is an execution mirror of livespec-dev-tooling-wj45qt (plan livespec-dev-tooling-6vuiwe, dev-host-test-gate-load). It is HELD because no H2 heading here governs this repository's Python coverage run. The spec's coverage clauses (the inner-loop list and Contributor Scenario C) govern Rust llvm-cov line and region coverage only. The justfile's `check-per-file-coverage` recipe already holds `dev-tooling/` to 100% line and branch coverage (pyproject `[tool.coverage.run] branch = true`, `[tool.coverage.report] fail_under = 100`), with no ratified clause behind it.

The defect it must exclude, measured in livespec-dev-tooling on 2026-11-09: pytest-cov hands its test subprocesses the coverage configuration by path only when `--cov-config` names an existing file. With a bare `pytest --cov` the default `.coveragerc` is assumed, and it does not exist. A subprocess whose cwd has no pyproject in view then measures statements only, and pytest-cov's combine step fails ("Can't combine statement coverage data with branch data") after the whole suite has passed. This repository's pytest section carries no `addopts`, so it has the same latent gap. Its own recipe passes `--cov-config=pyproject.toml` and does not meet it; a gate Job or ad-hoc bare spelling would. The livespec-dev-tooling fix (livespec-dev-tooling#3895) pins `--cov-config=pyproject.toml` in pytest addopts, and that is the expected means here. The clause states the behaviour, not that means.

After this revise, olhsgx's References line is re-pointed to "## Contributor Scenario C -- Quality gate enforces the inner and merge loops", and its hold is lifted.

### Proposed Changes

Two co-edits in SPECIFICATION/non-functional-requirements.md. No H2 heading is added, renamed or removed, so tests/heading-coverage.json is unchanged.

1. §"Contracts" → "### Quality Gate" → the **Inner loop -- `just check`** list. Insert this bullet after the "- architecture checks (see `## Constraints` -> Architecture Tests)" bullet:

```
- 100% line and branch coverage of every measured file of the Python
  helper tree under `dev-tooling/`, measured by its pytest suite
  (`check-per-file-coverage`). The coverage configuration in
  `pyproject.toml` MUST govern every pytest coverage run in this
  repository, whether or not the run names that configuration, so that
  a run's branch data and the data its test subprocesses record always
  combine. A run that is given `--cov` without `--cov-config` MUST end
  with pytest's own exit status and a coverage report, never with a
  failure to combine statement data with branch data.
```

2. §"Contributor Scenario C -- Quality gate enforces the inner and merge loops" → its gherkin Feature. Insert this scenario after "Scenario: The inner loop runs the fast deterministic checks":

```gherkin
  Scenario: The Python helper tree's coverage combines under any pytest coverage spelling
    Given pyproject.toml configures branch coverage for the dev-tooling helper tree
    And a helper test starts a Python subprocess whose working directory has
      no pyproject.toml in view
    When pytest runs the helper suite with --cov and no --cov-config argument
    Then the run ends with pytest's own exit status
    And it prints a coverage report
    And it does not fail combining statement coverage data with branch data
```
