#![no_main]

//! Fuzz the adapter NORMALIZATION path (livespec-console-beads-fabro-txtzn5.9).
//!
//! These parsers turn a source program's raw stdout into canonical console
//! events. The input is another process's output — `bd list --json`, a
//! Dispatcher journal, a `gh` response — so it is untrusted by construction:
//! the source may be a different version, may have been killed mid-write, or
//! may emit a field in a shape the console has never seen. Each parser returns
//! `Result<ParsedObservation, String>` and MUST resolve every malformed input
//! to the `Err` arm rather than panicking.
//!
//! Fail-soft is the property under test, and it is stronger than "returns".
//! `parse_orchestrator_observation` deliberately reads the DESCRIPTIVE half of
//! each record field-by-field through total helpers so no descriptive field can
//! drop a work-item from the board; a panic there would be that guarantee
//! failing silently under an input nobody wrote a unit test for.
//!
//! The first byte selects the adapter so one corpus can reach every parser,
//! and the remainder is the stdout. Do not assert on the parsed value: an
//! oracle that encodes a schema guess breaks on legitimate schema change,
//! whereas totality holds for every input forever.

use console_application::source_adapters::{
    ObservedSource, SourceAdapterKind, parse_dispatcher_observation, parse_fabro_observation,
    parse_github_observation, parse_livespec_observation, parse_needs_attention_snapshot,
    parse_orchestrator_observation,
};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let selector = match data.first() {
        Some(value) => *value,
        None => 0,
    };
    let stdout_bytes = match data.len() {
        0 => data,
        _ => &data[1..],
    };
    let stdout = String::from_utf8_lossy(stdout_bytes);
    let stdout_ref = stdout.as_ref();

    // Exercised on every input, not just its selector slot: it takes raw
    // stdout directly rather than an ObservedSource, so it costs one extra
    // call and doubles the corpus reaching it.
    let _attention = parse_needs_attention_snapshot(stdout_ref);

    let kind = match selector % 5 {
        0 => SourceAdapterKind::Orchestrator,
        1 => SourceAdapterKind::Dispatcher,
        2 => SourceAdapterKind::Fabro,
        3 => SourceAdapterKind::GitHub,
        _ => SourceAdapterKind::LiveSpec,
    };
    let observed = ObservedSource::new(kind, "livespec-console-beads-fabro", stdout_ref);

    let _parsed = match kind {
        SourceAdapterKind::Orchestrator => parse_orchestrator_observation(&observed),
        SourceAdapterKind::Dispatcher => parse_dispatcher_observation(&observed),
        SourceAdapterKind::Fabro => parse_fabro_observation(&observed),
        SourceAdapterKind::GitHub => parse_github_observation(&observed),
        _ => parse_livespec_observation(&observed),
    };
});
