# Changelog

## [0.4.0](https://github.com/thewoolleyman/livespec-console-beads-fabro/compare/v0.3.0...v0.4.0) (2026-08-22)


### Features

* add CI parity check gate ([8d01872](https://github.com/thewoolleyman/livespec-console-beads-fabro/commit/8d01872ebec4ce672d6e8363d3d8d7ebfb805913))
* add red-green-replay checker port ([34ab473](https://github.com/thewoolleyman/livespec-console-beads-fabro/commit/34ab473ea02878e7f83c3b69733a08d4b7f81667))
* **check:** gate the committed .fabro fork against upstream drift ([842a316](https://github.com/thewoolleyman/livespec-console-beads-fabro/commit/842a3162a3dcf214e331a85f2490c2d2ad8a94b6))
* **ci:** wire CI run telemetry export to Honeycomb ([eafe0fb](https://github.com/thewoolleyman/livespec-console-beads-fabro/commit/eafe0fb4ab9d02a6d3c40a1f14f8373a203a8d9c))
* cover driver handoff overlay ([4d98f6e](https://github.com/thewoolleyman/livespec-console-beads-fabro/commit/4d98f6eb62939796b0268e12f5a350e26726de2d))
* derive detail actions from registry ([1cbddf1](https://github.com/thewoolleyman/livespec-console-beads-fabro/commit/1cbddf1b6b05982ebeca9c588297dd95c2f1ed25))
* derive Status-line hints from a new operator action registry ([5ad4de9](https://github.com/thewoolleyman/livespec-console-beads-fabro/commit/5ad4de97a869bc323d64bf2f8190b4b38ee502f2))
* dispatch selected ready item ([f4e1261](https://github.com/thewoolleyman/livespec-console-beads-fabro/commit/f4e1261e1190a0f2256a4d627edd3887109e44df))
* distinguish unreconciled finished active runs ([9184d04](https://github.com/thewoolleyman/livespec-console-beads-fabro/commit/9184d0427ec56234c0f8bd9f843ac033c5d88b77))
* drill detail commands into explainer ([fa95d80](https://github.com/thewoolleyman/livespec-console-beads-fabro/commit/fa95d8094405ca8df735a89011c636153bfb8909))
* gate the unnameable-miss signature explicitly instead of --fail-under-lines ([82ed394](https://github.com/thewoolleyman/livespec-console-beads-fabro/commit/82ed39409deb1c69f6a2ecd64c8ebe256de1867f))
* generate operator action reference ([e780bb1](https://github.com/thewoolleyman/livespec-console-beads-fabro/commit/e780bb12f8bcfaaa1433bf1856f00502b64fa946))
* guard first-party Python import supply ([be5a6b7](https://github.com/thewoolleyman/livespec-console-beads-fabro/commit/be5a6b7eeda56885d95de5995cc04acd3ea4be85))
* **hooks:** wire the Red-Green-Replay commit-msg hook ([e01ecc4](https://github.com/thewoolleyman/livespec-console-beads-fabro/commit/e01ecc4a3e5f99a1d60b0f35578b6bd9edfaaea0))
* keep the menu bar visible ([da7a9b0](https://github.com/thewoolleyman/livespec-console-beads-fabro/commit/da7a9b0aa8caa786bf45b4b669a02c59f3434e0b))
* kill the vacuous factory-safety arm and seam in the real refusal signal ([f627dfb](https://github.com/thewoolleyman/livespec-console-beads-fabro/commit/f627dfbece082daea12a38f1ae12dbfe46f08e57))
* pin per-state verb suppression ([2d5ce11](https://github.com/thewoolleyman/livespec-console-beads-fabro/commit/2d5ce1141bdf5e0d00e9379b50f3761495b3ea68))
* **registry:** chords, so the structural keys dispatch THROUGH the registry ([80d2cc6](https://github.com/thewoolleyman/livespec-console-beads-fabro/commit/80d2cc688d1d104ac32975a38aa2bd2a5b3b6755))
* render ledger plan pages ([23b7209](https://github.com/thewoolleyman/livespec-console-beads-fabro/commit/23b72092da9d47deb860c6dbd336090581269a81))
* route hotkeys, valve staging, and Help rosters through the registry ([c3bff4a](https://github.com/thewoolleyman/livespec-console-beads-fabro/commit/c3bff4ab70eac237567af12168fc24574564eb5f))
* slice A — the cross-repo parity gate and the set-workflow-scope-override valve ([52f4b94](https://github.com/thewoolleyman/livespec-console-beads-fabro/commit/52f4b9455a74051c72b85bf267266af1ab1f80b7))
* surface a refused action instead of discarding it at the port boundary ([67c58d4](https://github.com/thewoolleyman/livespec-console-beads-fabro/commit/67c58d4140a6cbfe5d18dda8c445861c5d9f496f))
* surface dispatcher journal refusals ([dbb5052](https://github.com/thewoolleyman/livespec-console-beads-fabro/commit/dbb5052654ddf358ee18091cd6299f5ebf73519c))
* the action invoker — every registered action reachable without a hotkey ([d9fc67d](https://github.com/thewoolleyman/livespec-console-beads-fabro/commit/d9fc67d683f30533e10f6d1888e737e6784d3765))
* **tui:** generate the menu bar and submenus from the registry taxonomy ([bb9702c](https://github.com/thewoolleyman/livespec-console-beads-fabro/commit/bb9702c25d09dc9065587a545dbfee39fc17ce1f))


### Bug Fixes

* address red-green-replay checker review ([a72e775](https://github.com/thewoolleyman/livespec-console-beads-fabro/commit/a72e7757e294d43887734be0c779be3202a0785d))
* align lanes help with handoff verbs ([21ff727](https://github.com/thewoolleyman/livespec-console-beads-fabro/commit/21ff7273f0518926105e6c2bbd67685ed212be71))
* anchor lane selection by work-item id ([5aee161](https://github.com/thewoolleyman/livespec-console-beads-fabro/commit/5aee1617e156c1b1b677c3f4189aab5fc3b269ce))
* **arch-check:** follow in-tree symlinks instead of skipping them ([17c9b04](https://github.com/thewoolleyman/livespec-console-beads-fabro/commit/17c9b040327154e173bcfcf86065af08f73de363))
* **arch-check:** stop flagging symlinks that cannot leave the tree ([d20d1bd](https://github.com/thewoolleyman/livespec-console-beads-fabro/commit/d20d1bdf45f833151cc0c46940055ceb99e96e22))
* **ci:** point the k3s proof job at the scale set that exists ([5a8ce9b](https://github.com/thewoolleyman/livespec-console-beads-fabro/commit/5a8ce9b0e09d1e10a0b6896a7f1758deff3d447c))
* cite the Rust pin's authority instead of restating it ([1bb4561](https://github.com/thewoolleyman/livespec-console-beads-fabro/commit/1bb4561151498aee71b0da520ca69d8dd76ccaa5))
* close arch-check workspace source coverage ([abdd2d1](https://github.com/thewoolleyman/livespec-console-beads-fabro/commit/abdd2d11f79dad2c67e881de941027651d51ff32))
* consume published scope override signal ([148cf85](https://github.com/thewoolleyman/livespec-console-beads-fabro/commit/148cf850c86ce22aa3789f51bb8b64d54f97f345))
* cover closed menu bar entry ([ce9e245](https://github.com/thewoolleyman/livespec-console-beads-fabro/commit/ce9e2453400802902b128471224867d40f899587))
* derive status hints from per-item verbs ([514a326](https://github.com/thewoolleyman/livespec-console-beads-fabro/commit/514a326e48cbafad5e135e9a2bc75f1ce8b1d88a))
* disambiguate action registry cap labels ([cd71cfc](https://github.com/thewoolleyman/livespec-console-beads-fabro/commit/cd71cfc7e58c3681097e292b689ddc111de34809))
* distinguish claimed active lane items ([23a7555](https://github.com/thewoolleyman/livespec-console-beads-fabro/commit/23a7555e0e0402d96633f2e2c0a594d1ea3749f2))
* distinguish every repeatable operator action, not only move ([940647b](https://github.com/thewoolleyman/livespec-console-beads-fabro/commit/940647b99ed72f645bde65bb9e75e81acc13d15e))
* **e2e+docs:** slice-A CI red — walkthrough hints ride this slice; test navigation bug; live assertions ([77ed854](https://github.com/thewoolleyman/livespec-console-beads-fabro/commit/77ed854cb4ed84ea50b397a51b888f427f22f192))
* enforce zero-Beads-knowledge arch guard ([2a48407](https://github.com/thewoolleyman/livespec-console-beads-fabro/commit/2a48407f8f6036bbf3d9aee8f04faaef1db49efc))
* epoch source availability transitions ([29628cc](https://github.com/thewoolleyman/livespec-console-beads-fabro/commit/29628cc670aaba51a01996a602a0c2b915a55c31))
* **fabro:** sync the fork's behavioural drift back to the orchestrator ([bb53f53](https://github.com/thewoolleyman/livespec-console-beads-fabro/commit/bb53f53fd63b3f5c08c96c3894ed4bd1b418059b))
* **fabro:** sync the forked pr.md publish leg from upstream ([6b3c434](https://github.com/thewoolleyman/livespec-console-beads-fabro/commit/6b3c43417a86a5e22ee17de114329c830f9e760e))
* **factory:** port upstream's checkpoint commit_timeout into the fork ([da2d1eb](https://github.com/thewoolleyman/livespec-console-beads-fabro/commit/da2d1ebd91914634d4980b63f617e037fda323c2))
* **gate:** re-pin fork digests to this host's resolved plugin build ([5a6148a](https://github.com/thewoolleyman/livespec-console-beads-fabro/commit/5a6148aa2884c6a7f8b5dfda0abc07c4c2d4ef64))
* **gate:** seventh drift firing — upstream docker pin bump, not ported, re-pinned ([301b9da](https://github.com/thewoolleyman/livespec-console-beads-fabro/commit/301b9dab25fe1c9cd37c879aaab5dfc940cd4f2b))
* **gates:** ground-truth count for the v038 clauses + sixth drift firing ([57f2a5f](https://github.com/thewoolleyman/livespec-console-beads-fabro/commit/57f2a5fbffcac85aec46ac6723e967f7aea3aea4))
* give the command queue single-consumer claim semantics ([2665cad](https://github.com/thewoolleyman/livespec-console-beads-fabro/commit/2665cade7e215bad316275a08cabedd5e74d9b56))
* guard fabro sandbox rust toolchain ([eeede46](https://github.com/thewoolleyman/livespec-console-beads-fabro/commit/eeede4637d67f227348c6f844ddecc9ecdeb2bc2))
* guard rust version lockstep ([9fa005e](https://github.com/thewoolleyman/livespec-console-beads-fabro/commit/9fa005e260fec7edd016711d4a1ba2564ecfa4d2))
* honor factory drain request limits ([cd0e002](https://github.com/thewoolleyman/livespec-console-beads-fabro/commit/cd0e0028479c48af6e8ecf1f05f062caec2c9c8f))
* keep control commands off drain lane ([fb9ed6d](https://github.com/thewoolleyman/livespec-console-beads-fabro/commit/fb9ed6d745f93ef989f0c238216516e5bb60f174))
* make menu unavailable actions honest ([370a91f](https://github.com/thewoolleyman/livespec-console-beads-fabro/commit/370a91f16dbe333a49641adb4fce50707e4b1634))
* name ranked drain target before dispatch ([d9c14e3](https://github.com/thewoolleyman/livespec-console-beads-fabro/commit/d9c14e38d99d28b69bf769d1317d072a883ebf57))
* narrow move-status picker tests ([57e94a4](https://github.com/thewoolleyman/livespec-console-beads-fabro/commit/57e94a493e7c6c6dd7ff714b53db298ed3714c3d))
* narrow workflow fork drift docker pin comparison ([0fc0f97](https://github.com/thewoolleyman/livespec-console-beads-fabro/commit/0fc0f97c98c48c011c484a7be9c9f27a478d7478))
* page help overlay by focused pane ([886011d](https://github.com/thewoolleyman/livespec-console-beads-fabro/commit/886011d2219f8f9bd23a8b6fb593d91ec8e92bd2))
* persist dispatcher execution payloads ([180a542](https://github.com/thewoolleyman/livespec-console-beads-fabro/commit/180a542f5c58a3cfff837913d1950e61f8df7761))
* persist failed command diagnostics ([ceb3c03](https://github.com/thewoolleyman/livespec-console-beads-fabro/commit/ceb3c0383e2d22d88c7220a5011db6fe248625a3))
* pin keyless action menu accelerators ([347ab6a](https://github.com/thewoolleyman/livespec-console-beads-fabro/commit/347ab6a0fbeca4acd347765ca93dc78983a3d38f))
* preserve active move command target ([46783ad](https://github.com/thewoolleyman/livespec-console-beads-fabro/commit/46783adf17c5186a67e63dd273c448dab8817233))
* preserve lane item selection on menu entry ([99d19e7](https://github.com/thewoolleyman/livespec-console-beads-fabro/commit/99d19e72102db425c10369866ac925dfce1d1dba))
* preserve transient header status under width pressure ([9621fb6](https://github.com/thewoolleyman/livespec-console-beads-fabro/commit/9621fb6520083356161dc3d647b09734adafd8a7))
* preserve transient header status under width pressure ([c75b05b](https://github.com/thewoolleyman/livespec-console-beads-fabro/commit/c75b05b69de05f3ca69eaf075f4dd27c4b3a4af0))
* refresh lanes from impl attention rows ([00ed7e1](https://github.com/thewoolleyman/livespec-console-beads-fabro/commit/00ed7e17bc6e8c1bff34ffc3c01ae869c43ec413))
* register ready dispatch action ([e10e035](https://github.com/thewoolleyman/livespec-console-beads-fabro/commit/e10e03507d3d4707e51c65c4b25519aabcb9bc63))
* reject active move command targets ([f5efdab](https://github.com/thewoolleyman/livespec-console-beads-fabro/commit/f5efdab9d16c8f3d0e071187fa774020fd2d7468))
* **release:** auto-enable release PR merge ([a2a5e13](https://github.com/thewoolleyman/livespec-console-beads-fabro/commit/a2a5e13cf098eb47798fe6988b49236e02ac56a5))
* render non-outcome dispatcher journals as progress ([ce738ab](https://github.com/thewoolleyman/livespec-console-beads-fabro/commit/ce738ab86c80c0717d254c68960f79307c546e9d))
* report parked drain outcomes honestly ([26f3dc8](https://github.com/thewoolleyman/livespec-console-beads-fabro/commit/26f3dc81c5e45b08fa526fe40ffbbaf03be819a7))
* require explicit TODO coverage tier ([2132155](https://github.com/thewoolleyman/livespec-console-beads-fabro/commit/2132155ee4b25cf76a98c90742c2abb3e928c48d))
* reserve dispatcher backlog bounce for real bounces ([7b2e244](https://github.com/thewoolleyman/livespec-console-beads-fabro/commit/7b2e2442f85a3c1cac30d0da0b8b0cb30536f1d5))
* resolve newest orchestrator plugin record ([25f1f9f](https://github.com/thewoolleyman/livespec-console-beads-fabro/commit/25f1f9fe6e8f0e40997f9b1dc29e458882a7b5f9))
* restrict driver handoff to host-only safety ([7b2ddd6](https://github.com/thewoolleyman/livespec-console-beads-fabro/commit/7b2ddd644c034f2189408fb0c45b4269b29ad030))
* retry failed approve valve commands ([c540a96](https://github.com/thewoolleyman/livespec-console-beads-fabro/commit/c540a96278473d88119bc11dac6b946861334250))
* run cockpit commands off the tui thread ([f5cc3da](https://github.com/thewoolleyman/livespec-console-beads-fabro/commit/f5cc3da1dd379923bdcb15ce5824300679244e10))
* stamp command requests per append ([86c1499](https://github.com/thewoolleyman/livespec-console-beads-fabro/commit/86c14996a436a5df0517d4191851cda88b52da02))
* surface unavailable menu refusal ([111134c](https://github.com/thewoolleyman/livespec-console-beads-fabro/commit/111134c622dbe219f1827260d4b6f988015b37e8))
* **test:** bound the drive-log wait so the lifecycle walk stops racing its own side effect ([1f7cf9c](https://github.com/thewoolleyman/livespec-console-beads-fabro/commit/1f7cf9c3de3bc04ea011fe7054d8b570b5147af1))
* **test:** wait on the asserted board state, not on the view label ([854622b](https://github.com/thewoolleyman/livespec-console-beads-fabro/commit/854622b8100bc525ae6c87fff4f16f2ced8cbebc))
* validate heading coverage test ids ([5505b76](https://github.com/thewoolleyman/livespec-console-beads-fabro/commit/5505b764a2b9c5b5465e8f4a16e35ad843ebc9e3))
* warn on mid-dispatch acceptance arming ([2626dd0](https://github.com/thewoolleyman/livespec-console-beads-fabro/commit/2626dd0fceea1174d565e357a025cf53e2806e54))
* wire selected factory dispatch ([46b8cce](https://github.com/thewoolleyman/livespec-console-beads-fabro/commit/46b8cce3492662774ad3e3cba64d34e8c8953aea))


### Refactoring

* collapse console-tui test coverage sites ([27ede6b](https://github.com/thewoolleyman/livespec-console-beads-fabro/commit/27ede6bbc00877a8ed84101d5db30fc63d7e9011))

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
