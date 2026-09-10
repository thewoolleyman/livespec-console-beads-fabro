# The residual 54 splits: 46 positionally-closable + 8 llvm-cov summary phantoms

Measured 2026-08-29 on `origin/master` `9376bc0` (the current HEAD), one
instrumented workspace run:

```
cargo llvm-cov --workspace --all-features --lib --json --output-path cov.json
python3 -c "import json;t=json.load(open('cov.json'))['data'][0]['totals']['regions'];print(t['covered'],'/',t['count'],'unc',t['count']-t['covered'])"
```

`TOTAL 51794 / 51848 regions, 54 uncovered (99.896%)`. Re-measure before
quoting — this is a measurement, not a rate (the standing instruction of
`006`).

## The headline: the 54 is two different things, and only one is closable by a test

| file | summary uncovered | positionally located | phantom |
|---|---|---|---|
| console-cli/src/lib.rs | 23 | **23** | 0 |
| console-eventstore/src/lib.rs | 21 | 19 | **2** |
| console-red-green-replay-check/src/lib.rs | 2 | **2** | 0 |
| console-application/src/lib.rs | 6 | **2** | 4 |
| console-application/src/source_adapters.rs | 2 | 0 | **2** |
| **total** | **54** | **46** | **8** |

**46 are ordinary production `?`-arm / test-module regions** with an exact
`line:col`. **8 are phantoms**: `files[].summary.regions` counts them
uncovered, but no test can be written at a source position to close them,
because no such position exists in the merged view. This is the same 8 that
`-txtzn5.11`'s 2026-08-22 comment flagged as "unclosable by any test" and that
two independent parties (a human session and a factory agent) correctly refused
to fake. This note explains what they actually are.

## What finally located the 46 — and the method error that hid them before

The prior sessions and the refusing factory agent reported console-application
as **0 of 8 locatable** and console-eventstore as **19 of 21**. Two of those
"unlocatable" have since become locatable, and the method that finds them is
recorded here so it is not re-derived a fourth time.

- **Scalar-max merge over the JSON `functions[].regions`**, grouping by
  `(l0,c0,l1,c1,kind,expanded_file_id)` and taking the max exec count, now finds
  **2** all-zero code regions in console-application/lib.rs that the earlier runs
  found 0 of:
  - `6391:47` — `u32_setting(&by_key, "review_fix_cap")?` — a **production
    `?`-arm** Err path. Failure-injectable: a settings map whose
    `review_fix_cap` value is non-`u32`. This belongs to `-txtzn5.15`.
  - `10728:9` — the implicit-`else` of `if let Some(entry) = hit { … }` in a
    `#[cfg(test)]` helper, immediately after a `check(hit.is_some())`. The
    `.8`-class test-module region; close it with the monomorphic
    `check`/`ok_*` helper discipline `-txtzn5.15`'s HARD RULE already states,
    not by leaving the bare `if let`.

- **`llvm-cov show` with ALL workspace object files and
  `-show-instantiations=false`** renders the merged region view and marks
  exactly the **21** located regions (console-application 2 + console-eventstore
  19) with `^0`. The earlier "`--text` shows no `^0` in console-application"
  finding was a **too-few-objects error**: a per-package or few-object `show`
  does not load the cross-crate instantiations, so the console-application
  regions do not render. The reproduction is:

  ```
  OBJS: every executable in target/llvm-cov-target/debug/deps/ with no '.' in its basename
  llvm-cov show $OBJS -instr-profile=target/llvm-cov-target/livespec-console-beads-fabro.profdata \
    -show-line-counts-or-regions -show-instantiations=false -sources <file>
  ```

The full 46 with exact `line:col` (master `9376bc0`, re-measured — the
`-txtzn5.18` list had shifted, exactly as its description warned it would once
`-txtzn5.17` landed):

- **console-cli/src/lib.rs (23 → `-txtzn5.18`, all above line 808):** 936:20,
  1292:39, 1483:89, 1490:98, 1491:93, 1492:45, 1493:41, 1800:61, 1836:63,
  1882:41, 1891:74, 1894:72, 1899:64, 1902:63, 1923:76, 1931:99, 1963:41,
  1972:87, 1975:63, 2007:89, 2008:89, 2124:63, 2145:96.
- **console-eventstore/src/lib.rs (19 of 21 → `-txtzn5.14`):** 624:54, 625:43,
  672:61, 677:69, 681:29, 723:63, 732:29, 740:43, 742:42, 744:57, 763:43,
  766:58, 778:61, 795:43, 951:58, 952:57, 956:57, 985:77, 1002:20 — all SQLite
  `?`-arms (`Connection::open_in_memory`, `query`, `commit`, `pragma_update`,
  `row.get`), failure-injectable through a failing rusqlite connection.
- **console-red-green-replay-check/src/lib.rs (2 → `-txtzn5.19`):** 227:44,
  309:63 — matches its description exactly.
- **console-application/src/lib.rs (2 of 6 → `-txtzn5.15`):** 6391:47, 10728:9
  (above).

## What the 8 phantoms actually are: uncalled generic monomorphizations

Established this session, labelled by strength:

- **Measured.** The region *set* reconciles exactly: the scalar-max merge over
  `functions[].regions` reproduces `files[].summary.regions.count` to the region
  (14928 for lib.rs, 5958 for source_adapters, 2885 for eventstore). The
  discrepancy is entirely in the *covered* determination — for exactly `4+2+2=8`
  regions the max-across-instantiations is `>0` (covered) while the summary
  counts them uncovered.
- **Measured.** All three files are at **100.00% line coverage** (`cargo
  llvm-cov report --show-missing-lines`: lib.rs 926 lines 0 missed;
  source_adapters 382/0; eventstore 221/0). There is no uncovered *line*.
- **Measured.** The merged `llvm-cov show` view (all objects,
  `-show-instantiations=false`) renders **no** `^0` for the 8 — only the 21
  real ones. They are invisible in every merged view.
- **Measured.** Under `-show-instantiations=true`, source_adapters renders **31**
  per-instantiation `^0` markers, on function-closing braces (L196, 218, 845,
  889, 997, 1007, 1017, 1079, …), that collapse to the summary's 2. These are
  monomorphizations of generic functions that are **instantiated in the binary
  but never executed** — a whole-body-uncovered instantiation.
- **Inferred.** The 8 phantoms are therefore the **production analog of the
  monomorphic-helper problem this thread already solved for test helpers.**
  `-txtzn5.15`'s HARD RULE: "NEVER a generic `ok<T, E>`: each monomorphisation
  carries its own match arms, so N call-site types give N uncovered Err arms
  instead of one." The residual production generics in console-application and
  console-eventstore do exactly this: a generic fn instantiated for a type whose
  error/uncalled arm the suite never drives leaves a per-instantiation uncovered
  region the file summary counts and no merged view can show.

## Why this is the real blocker for `-txtzn5.11`, and the recommendation

`-txtzn5.11` adds `--fail-under-regions 100`, which reads `data[0].totals.regions`
— the summed file summaries, i.e. the number that includes the 8 phantoms.
**Closing all 46 located regions still leaves that number at 8, not 0**, so the
gate as specified cannot flip while the phantoms exist. Closing `-txtzn5.18`/
`.19` (both cleanly closable) does not change this; they are not `.11`'s
blocker. The 8 phantoms are.

Three dispositions, per the entry-29 / `.ai/coverage-region-testability-discipline.md`
principle that "genuinely-unreachable is not a disposition":

1. **De-genericize (or fully exercise) the specific production generics** in
   console-application (lib + source_adapters) and console-eventstore that
   produce the uncalled monomorphizations — the same fix the thread applied to
   test helpers, now on production code. This is **impl work on `-txtzn5.14`/
   `.15`, ratification-free**, and it keeps `.11` feasible exactly as written.
   **Recommended first move.** Identify the offending generics via
   `llvm-cov show -show-instantiations=true` on the two files (the closing-brace
   `^0` instantiations name the monomorphizations).
2. **Reformulate the gate** to assert the *merged* region view (the number
   `llvm-cov show`/scalar-max reports, which is 100% once the 46 close) rather
   than the raw summed summary. This is a **gate-definition change** and is
   **design-human-gated** — it goes through `/livespec:propose-change` and the
   maintainer's ratification pass, exactly as `.11`'s recorded second gate
   already says. Fall back to this **only if** a specific phantom proves to be a
   structurally-uncallable monomorphization with no legitimate owner.
3. **A dispositioned allowance** for the phantom count — **forbidden.** The
   no-exclusions clause at non-functional-requirements.md:122-127 stays as
   ratified (2026-08-21 ruling); this path is closed.

## Consequences for the four residual children

- **`-txtzn5.18` (console-cli 23) and `-txtzn5.19` (console-eventstore-adjacent
  check crate, 2):** fully located, all real `?`-arms, cleanly closable to 0.
  Factory is demonstrably healthy again (see below), so these are routable now.
- **`-txtzn5.14` (console-eventstore) and `-txtzn5.15` (console-application):**
  their "reach 0 uncovered in this crate" acceptance is **unsatisfiable as
  written**, because each carries a phantom tail (2 and 6) no test can close.
  They need a groom: close the located real regions (19 and 2) under a corrected
  acceptance, and split the 8 phantoms into a dedicated child that carries
  disposition (1) — that child, not `.14`/`.15`, is `.11`'s true precondition.

## Factory health (context for routing the closable work)

The `-txtzn5.14` zero-output ACP hang (dispatch journal `blocked:fabro-run` at
2026-08-29T12:15:50Z) is real but **intermittent**: the same factory `hp` ran
`ag0` to **green at 12:32:41Z**, 17 minutes later. So the recorded next-action's
OR-condition — "the factory demonstrably runs a clean implement stage" — is
satisfied. Root-causing the hang is orchestrator territory (plan thread
`acp-implement-zero-output-hang`, epic `bd-ib-b5dg`, opened today, still at the
dossier stage) and is handled by the foreman; do not double-file it. Each hang
still costs ~60 min, so dispatch catch-alive per the lore correction.
