//! The four v048 fenced-block defects, as fixtures.
//!
//! Across the five ratification-review rounds of the v048 amendment, four of
//! the defects found lived in fenced blocks -- which `extract_rules` skips, so
//! no clause and no gap-id covers them and CI could not see the
//! contradiction. Each was caught only by a reviewer sweeping every block by
//! hand, and the sweep kept failing. The fixtures below are those defect
//! states, taken from this repository's own history: the prose of the
//! amendment as it was rewritten, against blocks that still said the old
//! thing.
//!
//! `data/staleness/v048-nightly-filing-*.md` carry the round-1/2 shape (the
//! rank-order removal and the capture-surface removal, whose stale blocks are
//! the Contributor Scenario C Gherkin and the quality-gate pyramid mermaid);
//! `data/staleness/v048-credential-claim-*.md` carry the round-4 shape, where
//! the repaired credential claim left a mermaid EDGE asserting that the family
//! secret flows to the ingress host.

use console_spec_check::{StaleFencedBlock, stale_fenced_blocks};

const NFR: &str = "non-functional-requirements.md";

const NIGHTLY_PREVIOUS: &str = include_str!("data/staleness/v048-nightly-filing-previous.md");
const NIGHTLY_CURRENT: &str = include_str!("data/staleness/v048-nightly-filing-current.md");
const CREDENTIAL_PREVIOUS: &str = include_str!("data/staleness/v048-credential-claim-previous.md");
const CREDENTIAL_CURRENT: &str = include_str!("data/staleness/v048-credential-claim-current.md");

/// The stale Contributor Scenario C Gherkin step: the nightly still files "at
/// the top of the rank order", "through the orchestrator's capture surface".
const STALE_GHERKIN_STEP: &str =
    "And a chore work-item is filed at the top of the rank order in the";
/// The stale quality-gate pyramid node: the finding still becomes a
/// top-ranked work-item.
const STALE_PYRAMID_NODE: &str = "finding -> top-ranked chore work-item; never fail master";
/// A block that is merely PRESENT: the same diagram one section earlier, whose
/// node says only that a finding opens a work-item.
const UNAFFECTED_PYRAMID_NODE: &str = "finding -> open work-item (never fail master)";
/// The round-4 defect: an EDGE carrying the family secret to the host-side
/// `bd`, in a diagram whose node labels all read fine in isolation.
const STALE_EDGE: &str = "OP --> Bare --> HostBd --> Beads";
/// The same edge with the host-side `bd` off it -- the node DECLARATION that
/// names `HostBd` is untouched.
const REPAIRED_EDGE: &str = "OP --> Bare --> Beads";

#[test]
fn v048_rank_order_removal_flags_the_scenario_c_gherkin_and_the_pyramid_mermaid() {
    let findings = stale_fenced_blocks(NFR, NIGHTLY_PREVIOUS, NIGHTLY_CURRENT);

    let mermaid = block_range(NIGHTLY_CURRENT, STALE_PYRAMID_NODE);
    let gherkin = block_range(NIGHTLY_CURRENT, STALE_GHERKIN_STEP);
    let unaffected = block_range(NIGHTLY_CURRENT, UNAFFECTED_PYRAMID_NODE);
    assert!(
        mermaid.is_some() && gherkin.is_some() && unaffected.is_some(),
        "fixture must carry all three blocks"
    );

    assert!(
        flags(&findings, mermaid, "top rank"),
        "the pyramid mermaid still says top-ranked: {findings:?}"
    );
    assert!(
        flags(&findings, gherkin, "rank order"),
        "the Scenario C gherkin still says top of the rank order: {findings:?}"
    );
    assert_eq!(
        flagged_blocks(&findings),
        vec![mermaid, gherkin],
        "exactly the two stale blocks are flagged, and the block that merely \
         mentions a finding is not"
    );
    assert!(
        findings.iter().all(|finding| finding.spec_file == NFR),
        "every finding names its file"
    );
}

#[test]
fn v048_capture_surface_removal_flags_the_gherkin_filing_step() {
    let findings = stale_fenced_blocks(NFR, NIGHTLY_PREVIOUS, NIGHTLY_CURRENT);

    let gherkin = block_range(NIGHTLY_CURRENT, STALE_GHERKIN_STEP);
    assert!(gherkin.is_some(), "fixture must carry the gherkin block");
    assert!(
        flags(&findings, gherkin, "capture surface"),
        "the filing step still routes through the orchestrator's capture \
         surface: {findings:?}"
    );
}

#[test]
fn v048_credential_claim_removal_flags_the_scenario_e_mermaid_edge() {
    let findings = stale_fenced_blocks(NFR, CREDENTIAL_PREVIOUS, CREDENTIAL_CURRENT);

    let mermaid = block_range(CREDENTIAL_CURRENT, STALE_EDGE);
    assert!(
        mermaid.is_some(),
        "fixture must carry the Scenario E diagram"
    );
    assert!(
        flags(&findings, mermaid, "host bd"),
        "the Scenario E edge still runs the family secret to the host-side \
         bd: {findings:?}"
    );
    assert_eq!(
        flagged_blocks(&findings),
        vec![mermaid],
        "the gherkin the same change co-edited is not stale"
    );
}

#[test]
fn the_scenario_e_finding_is_read_off_the_edge_not_off_a_node_label() {
    let previous = CREDENTIAL_PREVIOUS.replace(STALE_EDGE, REPAIRED_EDGE);
    let current = CREDENTIAL_CURRENT.replace(STALE_EDGE, REPAIRED_EDGE);
    assert!(
        current.contains(r#"HostBd["dolt-server write ingress"]"#),
        "the node declaration naming HostBd is still in the diagram"
    );

    let findings = stale_fenced_blocks(NFR, &previous, &current);

    assert!(
        findings.is_empty(),
        "taking the host-side bd off the EDGE clears the finding, so the \
         finding was read off the edge: {findings:?}"
    );
}

#[test]
fn a_spec_file_whose_prose_did_not_change_yields_no_findings() {
    let unchanged = stale_fenced_blocks(NFR, NIGHTLY_CURRENT, NIGHTLY_CURRENT);
    assert!(
        unchanged.is_empty(),
        "the check is silent on blocks that are merely present: {unchanged:?}"
    );

    let block_only_edit = NIGHTLY_CURRENT.replace(STALE_PYRAMID_NODE, UNAFFECTED_PYRAMID_NODE);
    let edited = stale_fenced_blocks(NFR, NIGHTLY_CURRENT, &block_only_edit);
    assert!(
        edited.is_empty(),
        "no prose was removed, so nothing can be stale: {edited:?}"
    );
}

#[test]
fn a_block_the_amendment_co_edited_is_not_stale() {
    let co_edited = NIGHTLY_CURRENT
        .replace(
            STALE_PYRAMID_NODE,
            "finding -> chore filed unranked at beads open; never fail master",
        )
        .replace(
            STALE_GHERKIN_STEP,
            "And a chore work-item is filed in the livespec-console-beads-fabro",
        )
        .replace(
            "      livespec-console-beads-fabro tenant through the orchestrator's\n      capture surface",
            "      tenant through the tailnet beads write ingress",
        );

    let findings = stale_fenced_blocks(NFR, NIGHTLY_PREVIOUS, &co_edited);

    assert!(
        findings.is_empty(),
        "the amendment that co-edited its blocks -- which is what v048 \
         ratified -- is silent: {findings:?}"
    );
}

/// Whether some finding names the block at `range` and the term `term`.
fn flags(findings: &[StaleFencedBlock], range: Option<(usize, usize)>, term: &str) -> bool {
    findings
        .iter()
        .any(|finding| Some(block_of(finding)) == range && finding.term == term)
}

/// The blocks the findings name, deduplicated; findings arrive in block order.
fn flagged_blocks(findings: &[StaleFencedBlock]) -> Vec<Option<(usize, usize)>> {
    let mut blocks: Vec<Option<(usize, usize)>> = findings
        .iter()
        .map(|finding| Some(block_of(finding)))
        .collect();
    blocks.dedup();
    blocks
}

const fn block_of(finding: &StaleFencedBlock) -> (usize, usize) {
    (finding.start_line, finding.end_line)
}

/// The 1-based fence line range of the fenced block containing `needle`.
fn block_range(text: &str, needle: &str) -> Option<(usize, usize)> {
    let mut open: Option<usize> = None;
    let mut carries = false;
    for (index, line) in text.split('\n').enumerate() {
        let number = index + 1;
        if line.trim_start().starts_with("```") {
            match open.take() {
                Some(start) if carries => return Some((start, number)),
                Some(_) => carries = false,
                None => open = Some(number),
            }
            continue;
        }
        if open.is_some() && line.contains(needle) {
            carries = true;
        }
    }
    None
}
