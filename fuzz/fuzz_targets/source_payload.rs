#![no_main]

//! Fuzz the payload-JSON READ path (livespec-console-beads-fabro-txtzn5.9).
//!
//! Every function here parses a payload string that arrives on a
//! `ConsoleEvent` envelope, and every one of them is TOTAL by contract: it
//! returns `Option`/`Result` and MUST NOT panic on any input, because the
//! payload originates outside the console. A replayed event log can carry a
//! payload written by an older schema, a truncated write, or a source that
//! changed shape — none of which the reader may treat as unreachable.
//!
//! The oracle is deliberately weak: no assertion on the parsed VALUE, only
//! that the call returns. Asserting a value would encode this fuzzer's guess
//! at the schema and make the target fail on legitimate change; not panicking
//! is the property that actually holds for all inputs.

use console_application::source_adapters::{
    attention_item_snapshot_from_payload_json, attention_resolved_id_from_payload_json,
    dispatcher_journal_from_payload_json, fabro_run_snapshot_from_payload_json,
    work_item_snapshot_from_payload_json,
};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let payload = String::from_utf8_lossy(data);
    let payload_ref = payload.as_ref();

    let _work_item = work_item_snapshot_from_payload_json(payload_ref);
    let _journal = dispatcher_journal_from_payload_json(payload_ref);
    let _fabro_run = fabro_run_snapshot_from_payload_json(payload_ref);
    let _attention = attention_item_snapshot_from_payload_json(payload_ref);
    let _resolved = attention_resolved_id_from_payload_json(payload_ref);
});
