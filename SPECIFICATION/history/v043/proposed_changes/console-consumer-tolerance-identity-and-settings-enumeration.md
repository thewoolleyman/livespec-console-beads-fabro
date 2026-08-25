---
topic: console-consumer-tolerance-identity-and-settings-enumeration
author: claude-opus-5
created_at: 2026-08-25T16:01:48Z
---

## Proposal: Per-item tolerance for the needs-attention envelope; one bad item never blinds the inbox

### Target specification files

- SPECIFICATION/contracts.md
- SPECIFICATION/scenarios.md

### Summary

The needs-attention adapter contract gains a per-item tolerance paragraph requiring the adapter to ingest every well-formed item, skip only the items it cannot parse, and surface each skipped item with a reason; the adapter-honesty rule is narrowed so an uninterpretable ENVELOPE, not an uninterpretable item, is what reserves a not-observed finding. An unknown kind is declared a well-formed item that MUST be ingested rather than skipped. Three new Gherkin scenarios cover the mixed envelope, the unknown kind, and the converse genuinely-unreadable envelope.

### Motivation

The orchestrator ratified the consumer-tolerance posture at v077 and RELEASED it in v0.72.8, declaring that a consumer whose parse discards the whole envelope on one bad item is non-conforming and that this posture binds the downstream consumers who pin it. This console is such a consumer and is non-conforming today. Re-verified at master 2c40e31: parse_needs_attention_snapshot (crates/console-application/src/source_adapters.rs:2882) fails the whole envelope on one bad item along two independent axes -- serde_json::from_str deserializes the attention Vec in one shot so a single malformed field aborts the entire parse, and the id loop returns an error on the first item with an empty id -- and the caller turns that error into a not-observed finding, so one bad item blinds the entire inbox including the detection-staleness backstop. Nothing surfaces what was skipped because nothing is skipped: everything is dropped. This is exactly the per-item field-stability failure the console-fable review (finding 5) predicted. The current spec permits the defect because the adapter-honesty rule reserves not-observed for 'an uninterpretable payload' without saying whether one bad item makes the payload uninterpretable; this proposal answers that question before the code change lands. Filed as the spec-first leg of livespec-console-beads-fabro-2fr7hy under plan epic livespec-console-beads-fabro-ddfbcx; the code leg is livespec-console-beads-fabro-e55mov.

### Proposed Changes

In `SPECIFICATION/contracts.md` §"Adapter Contract", the adapter-honesty bullet that reserves a not-observed finding for genuine unreachability MUST be narrowed so a single unparseable ITEM inside an otherwise-readable envelope is not promoted into an uninterpretable payload for the whole source:

```diff
   MUST treat it as observed-and-idle and MUST NOT emit a not-observed
   finding for it. A not-observed finding is reserved for GENUINE
   unreachability: an unresolvable program, a non-zero command exit, an
-  unreadable or absent required file, or an uninterpretable payload, or
-  the preceding bullet's simulated / unimplemented (no real source I/O)
-  case. This is the cockpit-blind-vs-idle distinction (`scenarios.md`
+  unreadable or absent required file, or an uninterpretable ENVELOPE, or
+  the preceding bullet's simulated / unimplemented (no real source I/O)
+  case. An envelope the adapter CAN read is OBSERVED even when some of
+  its items are unparseable: a malformed item MUST NOT be promoted into
+  an uninterpretable payload for the whole source, and an adapter that
+  reports one MUST still report the source as observed. This is the
+  cockpit-blind-vs-idle distinction (`scenarios.md`
```

In `SPECIFICATION/contracts.md` §"Initial Adapters", the needs-attention adapter clause MUST gain a per-item tolerance paragraph immediately after the sentence ending "explicitly absent rather than be fabricated.":

**Per-item tolerance and the skipped-item surface.** The field guarantees above hold PER ITEM. The adapter MUST parse the `attention[]` array per item: it MUST ingest every well-formed item, and it MUST skip an item it cannot parse — malformed fields, or a missing or empty stable `id` — while consuming the rest of the envelope. A parse that discards the WHOLE envelope because one item is bad is non-conforming; one malformed item MUST NOT blind the inbox, the detection-staleness backstop included. The adapter MUST surface what it skipped, carrying a human-readable reason per skipped item, so a silently-shrunken inbox is impossible: skipping MUST be visible, not merely tolerated. An unknown `kind` is a WELL-FORMED item and MUST NOT be skipped as malformed — `kind` is an open string set on the wire, and the console MUST render or list an unknown-kind item rather than drop it. A not-observed finding for this source is reserved for an envelope that is genuinely unreadable — output that is not a JSON object carrying an `attention` array at all — never for the items inside a readable one.

This posture is the producer-declared contract this console pins from the orchestrator plugin (repo `thewoolleyman/livespec-orchestrator-beads-fabro`, `SPECIFICATION/contracts.md` §"Per-item field stability and the consumer-tolerance posture", ratified v077 and released in v0.72.8), which states that a consumer whose parse discards the whole envelope on one bad item is non-conforming. The console MUST NOT re-derive or relax it.

In `SPECIFICATION/scenarios.md` §"Scenario 12 -- needs-attention snapshot diffed at ingest into attention_item events", three scenarios MUST be appended to the existing `Feature: needs-attention snapshot diffed at ingest` block, after its current scenario. They are added under that existing heading deliberately: the parse that feeds the diff is the same adapter ingest behavior the heading already governs, and the behavioral-coverage gate binds a registered test per scenario HEADING, so a new heading would be untested until its code leg lands. The clauses added by this proposal MUST be linked to this heading in `tests/heading-coverage.json`.

```gherkin
Scenario: A malformed item is skipped and named while the rest of the envelope is ingested
  Given a readable needs-attention envelope carrying one well-formed item and one malformed item
  When the adapter parses the envelope
  Then it ingests the well-formed item
  And it skips the malformed item and surfaces it with a human-readable reason
  And it does not emit a not-observed finding for the needs-attention source

Scenario: An item of an unknown kind is well-formed and is ingested
  Given a readable needs-attention envelope carrying one item whose kind the console does not recognize
  When the adapter parses the envelope
  Then it ingests that item rather than skipping it as malformed
  And the item is reachable in the needs-attention inbox

Scenario: A genuinely unreadable envelope still degrades the source honestly
  Given output that is not a JSON object carrying an attention array
  When the adapter parses it
  Then it emits a not-observed finding for the needs-attention source carrying a human-readable reason
  And it ingests no attention items
```


## Proposal: Console principal resolution replaces the hardcoded operator constant on commands and action-port invocations

### Target specification files

- SPECIFICATION/contracts.md
- SPECIFICATION/scenarios.md

### Summary

A new Operator principal resolution subsection fixes how the console resolves the acting principal -- an explicit --invoker argument, else the LIVESPEC_INVOKER environment variable, else a derived unattributed:<os-user>@<hostname> fallback mark -- and requires the resolved principal to populate requested_by on every command and to be forwarded on every orchestrator action-port invocation, with console:<principal> as the recommended asserted form. Four new Gherkin scenarios cover each resolution branch and the forwarding obligation.

### Motivation

The console's spec today carries requested_by only as a Command Envelope payload field described as 'user-or-agent', fixing no resolution order and no principal form; the implementation consequently hardcodes a constant. Re-verified at master 2c40e31: crates/console-cli/src/main.rs:236 passes the string literal \"operator\" as requested_by into run_store_backed_tui_session, and no --invoker flag and no LIVESPEC_INVOKER read exist anywhere in the CLI, so every act performed through the console is attributed to the same constant regardless of who ordered it. The orchestrator has since ratified the matching contract at v073 and RELEASED it in v0.72.4, fixing the three-step resolution order and naming console:<principal> as an example identity form, so the console can adopt a settled upstream contract rather than invent one. Adopting it verbatim keeps the two repositories' attribution vocabulary aligned, which matters because the console's acts are journaled by the orchestrator it invokes. A resolution order is load-bearing behavior and must be specified before it is implemented. Filed as part of livespec-console-beads-fabro-2fr7hy; the code leg is livespec-console-beads-fabro-ets4om.

### Proposed Changes

In `SPECIFICATION/contracts.md`, a new subsection MUST be added immediately after §"Command Envelope", specifying how the console resolves the principal that populates `requested_by`:

```markdown
### Operator principal resolution

The console MUST resolve the acting principal ONCE per session and MUST NOT substitute a constant for it. Resolution proceeds in this order, and the first match wins:

1. An explicit `--invoker <id>` argument on the console invocation.
2. Otherwise the `LIVESPEC_INVOKER` environment variable, when set and non-empty.
3. Otherwise a derived fallback of the form `unattributed:<os-user>@<hostname>`.

The resolved principal MUST populate `requested_by` on every command the console persists, and MUST be forwarded on every orchestrator action-port invocation the console makes, so an act performed through the console is attributable to whoever ordered it. The fallback is a MARK, not an identity: it records that no caller asserted who acted, and a command carrying it MUST remain distinguishable from one carrying an asserted principal. The console's asserted principal SHOULD take the form `console:<principal>`.

This resolution order and the recommended form are consumed verbatim from the orchestrator plugin's ratified journal-invoker attribution contract (repo `thewoolleyman/livespec-orchestrator-beads-fabro`, `SPECIFICATION/contracts.md` §"Journal invoker attribution", ratified v073 and released in v0.72.4), which names `console:<principal>` as an example identity. The console MUST NOT invent a second, divergent resolution order.
```

In `SPECIFICATION/scenarios.md` §"Scenario 2 -- Factory drain command", four scenarios MUST be appended to the existing `Feature: Factory drain command` block, after its current scenario. That heading is the canonical command-issue path this behavior attaches to — the console persists a command and invokes the orchestrator through its port — and, as above, the coverage gate binds a registered test per scenario HEADING, so the attribution scenarios ride the existing heading rather than opening an untested one. The clauses added by this proposal MUST be linked to this heading in `tests/heading-coverage.json`.

```gherkin
Scenario: An explicitly supplied invoker wins over the environment and the fallback
  Given the console is invoked with an explicit invoker argument
  And the invoker environment variable is also set to a different value
  When the operator issues a command
  Then the persisted command's requested_by carries the explicitly supplied invoker

Scenario: The environment supplies the principal when no argument is given
  Given the console is invoked with no explicit invoker argument
  And the invoker environment variable is set and non-empty
  When the operator issues a command
  Then the persisted command's requested_by carries the environment-supplied invoker

Scenario: With no asserted identity the console marks the act unattributed
  Given the console is invoked with no explicit invoker argument
  And the invoker environment variable is unset
  When the operator issues a command
  Then the persisted command's requested_by carries the derived unattributed fallback mark
  And that mark is distinguishable from an asserted principal

Scenario: The resolved principal reaches the orchestrator action port
  Given a resolved acting principal for the session
  When the console invokes the orchestrator through an action port
  Then the invocation carries that resolved principal
  And it does not carry a hardcoded constant in its place
```


## Proposal: Settings enumeration is illustrative, not normative, and excludes keys the orchestrator declares non-API-configurable

### Target specification files

- SPECIFICATION/contracts.md

### Summary

The frozen six-key settings enumeration is rewritten as an explicitly illustrative, non-exhaustive list that grows with the orchestrator's declarations, so it cannot contradict the binding rule beside it that the console MUST read the published declaration; and a converse obligation is added requiring that a dispatcher key the orchestrator declares NOT API-configurable MUST NOT appear as an editable row on the console Settings surface or any remote API the console exposes.

### Motivation

The contract states 'The six settings the console commands and observes are ...' immediately before binding the console to read the orchestrator's published declaration 'so a key the orchestrator adds needs no console spec change to appear.' Those two sentences are already in tension, and the tension becomes a live contradiction now that the orchestrator has ratified dispatcher.drift_capture_merge_threshold as API-configurable at v078 and released it in v0.72.9: the console must surface a seventh key while its own spec says there are six. Leaving the frozen count in place invites an implementer to treat the enumeration as normative and hardcode the list, which is precisely what the adjacent clause forbids. The converse obligation closes the matching hole in the other direction: the orchestrator's v073 contract declares dispatcher.require_invoker deliberately NOT API-configurable and forbids it from being editable through the console Settings surface or any remote API, on the grounds that a dial which relaxes attribution must not be reachable over the surface whose acts it attributes -- but this console's spec never states that exclusion, so a completeness sweep could read the key's absence as drift and 'fix' it by adding the forbidden row. Filed as part of livespec-console-beads-fabro-2fr7hy; the settings-lockstep code leg is livespec-console-beads-fabro-evasgx.

### Proposed Changes

In `SPECIFICATION/contracts.md` §"Dispatcher Policy Settings", the frozen six-key enumeration MUST be replaced so the illustrative list cannot contradict the binding read-the-declaration rule beside it, and so a key the orchestrator declares NOT API-configurable is kept off the console surface:

```diff
-The six settings the console commands and observes are `auto_approve_ready`,
-`merge_on_review_cap`, `acceptance_mode`, `review_fix_cap`,
-`acceptance_rework_cap`, and `wip_cap`. The console MUST NOT hardcode that
-list: it MUST read the orchestrator's published declaration of its
-API-configurable keys, so a key the orchestrator adds needs no console spec
-change to appear.
+The console commands and observes exactly those `dispatcher.*` settings the
+orchestrator publishes as API-configurable. That set GROWS as the orchestrator
+ratifies further keys; at the time of writing it included
+`auto_approve_ready`, `merge_on_review_cap`, `acceptance_mode`,
+`review_fix_cap`, `acceptance_rework_cap`, and `wip_cap`. That enumeration is
+illustrative and MUST NOT be read as normative or exhaustive: the console MUST
+NOT hardcode it, and MUST read the orchestrator's published declaration of its
+API-configurable keys, so a key the orchestrator adds needs no console spec
+change to appear. Conversely, a `dispatcher.*` key the orchestrator declares
+NOT API-configurable MUST NOT appear on the console Settings surface or any
+remote API the console exposes, even when the console can otherwise read its
+effective value; such a key MAY be displayed only as read-only context, never
+as an editable row.
```

No new scenario accompanies this change: §"Scenario 14 -- Settings surface stays in lockstep with the orchestrator's declared keys" already carries the lockstep behavior, and this proposal repairs a stale enumeration and adds the converse exclusion rather than introducing a new observable behavior. The converse exclusion is a negative obligation on the same completeness check Scenario 14 already governs.

