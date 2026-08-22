# Fuzzing

Three targets, wired into CI as a merge gate by
`livespec-console-beads-fabro-txtzn5.9`. `SPECIFICATION/non-functional-requirements.md`
§"Quality Gate" ratifies that `just check` MUST NOT include fuzz runs, so the
gate lives in CI and in `just check-fuzz`, never in the `just check` aggregate.

| target | what it fuzzes | why it is untrusted input |
|---|---|---|
| `event_envelope` | `ConsoleEvent` construction + `project_attention` | identifiers and stream sequences arriving on a replayed log |
| `adapter_normalization` | `parse_*_observation`, `parse_needs_attention_snapshot` | another process's raw stdout — a different source version, a killed mid-write, an unseen field shape |
| `source_payload` | the `*_from_payload_json` readers | payloads written by an older schema or a truncated write |

Every function under test is TOTAL by contract: it returns `Option`/`Result`
and must not panic on any input. That, not the parsed value, is the oracle. An
oracle that asserted on the value would encode this fuzzer's guess at the schema
and would fail on legitimate schema change, whereas totality holds for every
input forever.

## The corpus policy: two directories, on purpose

**`corpus/` is generated and is NOT committed.** libFuzzer writes thousands of
inputs there as it explores; they churn on every run and carry no meaning
outside the machine that produced them.

**`regressions/` is the committed regression corpus.** One file per crash the
fuzzer has actually found. Each is a named past failure that every future run
replays, which is what "fail on any new crash" needs in order to also mean
"never re-introduce an old one".

This is a decision, and the alternative was real: keep one `corpus/<target>/`
directory and commit selected files into it via `.gitignore` exceptions, which
is what this tree did before. It was rejected because generated and curated
inputs then share a directory, so a routine `git add -A` sweeps thousands of
generated files into a commit, and the exception list has to be maintained per
file. Two directories make the distinction structural instead of clerical.

The previous policy could not satisfy the requirement at all: `corpus/*` was
ignored with only `.gitkeep` excepted, so a crash reproducer literally could
not be committed.

## Adding a regression

When a run finds a crash, cargo-fuzz writes the reproducing input to
`fuzz/artifacts/<target>/`. Commit it:

```bash
cp fuzz/artifacts/<target>/crash-<hash> fuzz/regressions/<target>/
git add fuzz/regressions/<target>/crash-<hash>
```

Name it for the defect if you know it. Then fix the panic — a committed
reproducer with no fix is a red gate, not a record.

## Exit codes: a crash and a broken toolchain are not the same thing

| exit | meaning |
|---|---|
| 0 | every target built and ran clean |
| 1 | a target **crashed** — a real finding; the reproducer is in `fuzz/artifacts/` |
| 2 | a target **could not be built** — a tooling error, not a finding |

The distinction is load-bearing and was learned the hard way. `cargo fuzz run`
exits non-zero for both cases, and the first version of this gate conflated
them: CI ran in a container with no C++ compiler — `libfuzzer-sys` builds
libFuzzer from source — and the gate announced `CRASHED targets:` for all three.
A tooling error that reads as a fuzzing finding sends whoever is on the gate
hunting a bug that does not exist.

So `just check-fuzz` builds each target before running it, and reports the two
failure modes separately. If you see exit 2, check for a C++ compiler on PATH
before you look at the fuzz targets at all.

## Running

```bash
just check-fuzz          # the CI gate: >=60s per target, replaying regressions
just check-fuzz-smoke    # 5s on one target, for a fast local sanity check
```
