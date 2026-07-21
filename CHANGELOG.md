# Changelog

## [0.3.0](https://github.com/thewoolleyman/livespec-console-beads-fabro/compare/v0.2.0...v0.3.0) (2026-07-21)


### Features

* context-specific Status-line shortcut hints (Scenario 19 / B2) ([15c301a](https://github.com/thewoolleyman/livespec-console-beads-fabro/commit/15c301a097b11e246bff7ada9174feb1096f5c67))
* **docs:** B8 release acceptance — de-gate install, fix two doc bugs, bind the asset glob ([e5be717](https://github.com/thewoolleyman/livespec-console-beads-fabro/commit/e5be717bf7e08d7f030e67c1064abcda448a621a))
* **docs:** key-by-key lifecycle walkthrough, verified against the real TUI (B7) ([b8ff009](https://github.com/thewoolleyman/livespec-console-beads-fabro/commit/b8ff009d35fae78bf3da161f0855581e80ac0a9c))
* focusable, horizontally scrollable top/header pane (Scenario 20 / B3) ([4e8598f](https://github.com/thewoolleyman/livespec-console-beads-fabro/commit/4e8598f9bcf59bbaf4695160dd8793e13f930550))
* navigable pane-specific modal Help overlay (Scenario 18 / B4) ([aa4281c](https://github.com/thewoolleyman/livespec-console-beads-fabro/commit/aa4281cebccc373246804a3803d2865fec9eddfe))
* panes render operational content only — remove baked-in doc prose (Scenario 21 / B5) ([1bfdb41](https://github.com/thewoolleyman/livespec-console-beads-fabro/commit/1bfdb41d41c44a440b635be713b2c368fbd74c34))
* **tui:** drill in from a lane row to a work-item's full record ([e724b9c](https://github.com/thewoolleyman/livespec-console-beads-fabro/commit/e724b9c13ad295ba684abe8a1537e1d8c2b822da))
* user-facing docs live in a docs/ tree, README is a pointer (Scenario 22 / B6) ([7df1ea2](https://github.com/thewoolleyman/livespec-console-beads-fabro/commit/7df1ea219e4def46272aadbb1e3834bb72c53039))


### Bug Fixes

* **adapter:** tolerate a null `detail` on replay instead of dropping the item ([6137d08](https://github.com/thewoolleyman/livespec-console-beads-fabro/commit/6137d08750af0113d135ab62d77af2b92f896773))
* **ci:** resolve E2E release binary via CARGO_TARGET_DIR so check-e2e-tmux passes on the CI runner ([79305bc](https://github.com/thewoolleyman/livespec-console-beads-fabro/commit/79305bcb8423743213adaf697ef124272f680c90))
* distinguish unset from empty in the digest; drop a false justification ([cb32eaf](https://github.com/thewoolleyman/livespec-console-beads-fabro/commit/cb32eafcc12cfceab34c56450b769f6a87d65c1a))
* drill attention rows by source work item ([6262f66](https://github.com/thewoolleyman/livespec-console-beads-fabro/commit/6262f666fa8c6bdff86f51fb21f84a3e4a78771d))
* **fabro:** pin the sandbox to the python-rust-agent layer, not the slim CI image ([fc43f26](https://github.com/thewoolleyman/livespec-console-beads-fabro/commit/fc43f26a4367e1c8bd24e9947a90683f0b8fc918))
* key config-manifest staleness to declared keys ([f5fa99f](https://github.com/thewoolleyman/livespec-console-beads-fabro/commit/f5fa99fb224a336f1bf934330292140b72e02b92))
* length-prefix the record digest; show emitted policies, not defaults ([14499d5](https://github.com/thewoolleyman/livespec-console-beads-fabro/commit/14499d5230a468092b650607fabb3dd045c4b618))
* make the factory drain a repeatable command so every :drain lands ([4241fc3](https://github.com/thewoolleyman/livespec-console-beads-fabro/commit/4241fc3b599e610a401fa5b497de0d13fb598bcb))
* open attention work-item records ([2cd1f28](https://github.com/thewoolleyman/livespec-console-beads-fabro/commit/2cd1f280247d5bcff76a253e5ee083b78b2cf6af))
* page work-item modal by measured viewport ([eb411c5](https://github.com/thewoolleyman/livespec-console-beads-fabro/commit/eb411c51cd2942a115964aa85c2235fcb3a00fca))
* preserve journal escalation attention ([3c0496d](https://github.com/thewoolleyman/livespec-console-beads-fabro/commit/3c0496d4b8cf113d532a911f4e02fe6ae99807ad))
* read orchestrator auto-disposition journal ([5938212](https://github.com/thewoolleyman/livespec-console-beads-fabro/commit/5938212ef60a71252a8a0098a42d71c40221b713))
* render the WHOLE record and stop advertising inert keys ([185426b](https://github.com/thewoolleyman/livespec-console-beads-fabro/commit/185426ba08c57618c46f06bb2fc0afe0405d5cff))
* run backing CLIs from selected repo ([7110eca](https://github.com/thewoolleyman/livespec-console-beads-fabro/commit/7110ecae3f433a538c71d2a6dad39d6f900bc78c))
* show lane item titles in TUI rows ([2120e62](https://github.com/thewoolleyman/livespec-console-beads-fabro/commit/2120e62675d24eca85794cb911dd335e67d3952f))
* source-availability honesty — reachable-but-empty sources are observed-idle, not unavailable (Scenario 13 / B1) ([2bf6841](https://github.com/thewoolleyman/livespec-console-beads-fabro/commit/2bf68412c3b3ee6c4556beb9c0a8712f25d4b683))
* suppress invalid Fabro attach hints ([fd6c622](https://github.com/thewoolleyman/livespec-console-beads-fabro/commit/fd6c622c00a75f5cc03116037e3b78b7bc58080c))
* **test:** expand the lifecycle fixture's repo so valve items resolve under repo-scoped ingest ([e4afef4](https://github.com/thewoolleyman/livespec-console-beads-fabro/commit/e4afef40a228305f838e230092949665766f454f))
* **test:** repair the red check-e2e-tmux gate on master ([5ae23fd](https://github.com/thewoolleyman/livespec-console-beads-fabro/commit/5ae23fd66477ed02321e9713d9b0a1d9c88b6b6e))
* **test:** smoke E2E asserts header priority fields, not degraded mode:tui ([e4d0259](https://github.com/thewoolleyman/livespec-console-beads-fabro/commit/e4d025912cadd1bde3a73209879d2fdf30fc786a))
* **tui:** pin the item modal to the id it was opened on ([8dfaa98](https://github.com/thewoolleyman/livespec-console-beads-fabro/commit/8dfaa9835074b842f1dc06f5f732e813cea4f348))

## [0.2.0](https://github.com/thewoolleyman/livespec-console-beads-fabro/compare/v0.1.0...v0.2.0) (2026-07-17)


### Features

* API-configurable-key completeness check (Settings/help/README lockstep) ([fc581a4](https://github.com/thewoolleyman/livespec-console-beads-fabro/commit/fc581a493794cf3e7fd3c2b23fbd95f325eedbf6))
* generalize the console config port onto the orchestrator API ([dce254f](https://github.com/thewoolleyman/livespec-console-beads-fabro/commit/dce254f4c056b7059875136af2ef2325e862e2e2))
* **tui:** broad pre-terminal status moves + per-item override valves for the three cap settings ([b4304af](https://github.com/thewoolleyman/livespec-console-beads-fabro/commit/b4304afcf8ee0a58980565bab49a7980c866d291))
* **tui:** select an individual work-item and move it to any operator-drivable status ([822d4a7](https://github.com/thewoolleyman/livespec-console-beads-fabro/commit/822d4a745c76eb275b5187acd90044cfe6bf08ee))
* **tui:** the Settings view replaces the autonomous-mode arming surface ([2b3b914](https://github.com/thewoolleyman/livespec-console-beads-fabro/commit/2b3b9149d2c270e85186832dc86a7ba5b9d4ee22))


### Bug Fixes

* cockpit projections update live at runtime (Scenarios 2/3/11 conformance) ([cce5677](https://github.com/thewoolleyman/livespec-console-beads-fabro/commit/cce56776e837b7cfff92f13a469b1a95b4f4649f))
* fold the autonomous-decision reflection into the live refresh sequence ([347906a](https://github.com/thewoolleyman/livespec-console-beads-fabro/commit/347906ae476748cbb4a04414593ae90cd655de99))
* move TUI source polling off the event loop; make interactive MOVE land ([261c5f6](https://github.com/thewoolleyman/livespec-console-beads-fabro/commit/261c5f6abfabd54f5d3d55e50e873db94b5359e4))
* pin-stamp the config-manifest fixture so a pin bump fails the completeness gate ([3d7a3d9](https://github.com/thewoolleyman/livespec-console-beads-fabro/commit/3d7a3d93f6f2d498e69811539eb677e2b056d93b))
