#![no_main]

use console_application::project_attention;
use console_domain::{ConsoleEvent, EventType, validate_non_empty_identifier};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let candidate = String::from_utf8_lossy(data);
    let _identifier_result = validate_non_empty_identifier(candidate.as_ref());
    let selector = match data.first() {
        Some(value) => *value,
        None => 0,
    };
    let stream_seq = match u64::try_from(data.len()) {
        Ok(value) => value,
        Err(_error) => u64::MAX,
    };
    let event_type = match selector % 4 {
        0 => EventType::DispatcherNeedsRegroomObserved,
        1 => EventType::FabroHumanGateObserved,
        2 => EventType::FactoryDrainRequested,
        _ => EventType::LivespecReviseRequired,
    };
    let event = ConsoleEvent::new(
        candidate.to_string(),
        1,
        "factory".to_owned(),
        event_type,
        "fuzz".to_owned(),
        "factory:fuzz".to_owned(),
        stream_seq,
    );
    let events = [event];
    let _attention = project_attention(&events);
});
