use console_application::source_adapters::{
    NeedsAttentionReadOutcome, NeedsAttentionSnapshotPort, ProbeNeedsAttentionPort, SourceProbe,
    SourceProbeOutcome, parse_needs_attention_snapshot,
};

struct StubProbe {
    outcome: SourceProbeOutcome,
}

impl SourceProbe for StubProbe {
    fn read_file(&self, _path: &str) -> SourceProbeOutcome {
        SourceProbeOutcome::Unavailable {
            reason: "unused".to_owned(),
        }
    }

    fn run_command(&self, _program: &str, _args: &[&str]) -> SourceProbeOutcome {
        self.outcome.clone()
    }
}

fn observed(stdout: &str) -> StubProbe {
    StubProbe {
        outcome: SourceProbeOutcome::observed(stdout, true),
    }
}

#[test]
fn mixed_needs_attention_envelope_keeps_unknown_kind_and_surfaces_skipped_item()
-> Result<(), String> {
    let mixed = r#"{"attention":[{"id":"wi-unknown","kind":"new-orchestrator-kind","urgency":"high","summary":"Unknown kind survives","source_ref":{"repo":"console","work_item":"wi-unknown"},"handoff":{"kind":"inspect","command":"inspect:wi-unknown"}},{"id":"wi-bad","kind":"human-valve","urgency":7,"summary":"Malformed item","source_ref":{"repo":"console","work_item":"wi-bad"},"handoff":{"kind":"approve","command":"approve:wi-bad"}}]}"#;

    let items = parse_needs_attention_snapshot(mixed)?;

    assert_eq!(items.len(), 2);
    assert!(
        items
            .iter()
            .any(|item| item.id() == "wi-unknown" && item.kind() == "new-orchestrator-kind")
    );
    let skipped = items
        .iter()
        .find(|item| item.id().contains("wi-bad"))
        .ok_or_else(|| "malformed item skip signal should name wi-bad".to_owned())?;
    assert_eq!(skipped.kind(), "needs-attention-malformed-item");
    assert!(skipped.summary().contains("wi-bad"));
    Ok(())
}

#[test]
fn mixed_needs_attention_envelope_is_observed_not_unavailable() {
    let mixed = r#"{"attention":[{"id":"wi-unknown","kind":"new-orchestrator-kind","urgency":"high","summary":"Unknown kind survives","source_ref":{"repo":"console","work_item":"wi-unknown"},"handoff":{"kind":"inspect","command":"inspect:wi-unknown"}},{"id":"wi-bad","kind":"human-valve","urgency":7,"summary":"Malformed item","source_ref":{"repo":"console","work_item":"wi-bad"},"handoff":{"kind":"approve","command":"approve:wi-bad"}}]}"#;
    let probe = observed(mixed);
    let port = ProbeNeedsAttentionPort::new(&probe, "needs-attention", &["--json"]);

    assert!(matches!(
        port.read_snapshot(),
        NeedsAttentionReadOutcome::Observed(items) if !items.is_empty()
    ));
}

#[test]
fn genuinely_unreadable_needs_attention_envelope_degrades_to_unavailable() {
    let probe = observed("{}");
    let port = ProbeNeedsAttentionPort::new(&probe, "needs-attention", &["--json"]);

    assert!(matches!(
        port.read_snapshot(),
        NeedsAttentionReadOutcome::Unavailable(reason)
            if reason == "needs-attention output is not a JSON object with an attention array"
    ));
}
