---
topic: impl-drift-reconciliation
author: claude-opus-4-8
created_at: 2026-06-25T09:56:05Z
---

## Proposal: Re-reconcile the event envelope and events table to the implemented scalar schema

### Target specification files

- SPECIFICATION/contracts.md

### Summary

The v003 contracts.md events table and envelope (correlation_json object, subject_kind/subject_id columns, causation_event_id on the event envelope) do not match the implemented eventstore schema, which uses scalar correlation_id, causation_id, and aggregate_id with no subject columns and no correlation object. console-domain's ConsoleEvent is a minimal struct (event_id, schema_version, context, event_type, source, stream_id, stream_seq) carrying no subject and no correlation object.

### Motivation

Impl->spec drift: the prior D1 reconciliation aligned the spec to an idealized rich-correlation/subject envelope the implementation never adopted; the impl resolved the envelope-vs-table consistency with scalar columns. The spec must follow the implementation's actual, coherent schema.

### Proposed Changes

Realign contracts.md to the implemented schema: the canonical event envelope carries scalar aggregate_id, stream_id, stream_seq, causation_id, and correlation_id (no subject object, no correlation object, no correlation_json); the events table mirrors it 1:1 (aggregate_id, stream_id, causation_id null, correlation_id not null) -- drop subject_kind/subject_id and correlation_json. Commands keep causation_event_id and scalar correlation_id as implemented. Update the Event Envelope JSONC, the envelope-to-table mapping note, the events-table DDL, and the ER diagram to match.

## Proposal: Enumerate the factory.drain.not_wired honest-signal event

### Target specification files

- SPECIFICATION/contracts.md

### Summary

The implementation emits a factory.drain.not_wired event -- the concrete realization of the spec's not-observed / no-fabricated-success honesty rule -- when the drain port is not actually wired, but contracts.md does not enumerate it among the factory outcome events.

### Motivation

Impl->spec drift: the impl realizes the honesty rule (spec.md Initial-adapter fidelity, contracts.md Command Handling rule 6) with a concrete factory.drain.not_wired event that the canonical event vocabulary omits.

### Proposed Changes

Add factory.drain.not_wired to the factory outcome events in contracts.md (alongside factory.drain.started / failed / completed) as the honest not-wired/not-observed outcome a simulated or unimplemented drain port emits instead of a fabricated success.

## Proposal: Reclassify arch-check as a contributor check, not an operator subcommand

### Target specification files

- SPECIFICATION/spec.md

### Summary

spec.md Product Shape lists `livespec-console-beads-fabro arch-check` as an operator subcommand, but the implementation realizes architecture checking as a separate contributor binary (the console-arch-check crate, run via `just check-arch`), not as a subcommand of the console binary.

### Motivation

Impl->spec drift: arch-check is a contributor-facing quality-gate tool (already owned by non-functional-requirements.md Architecture Tests), not part of the operator-facing command surface; listing it as an operator subcommand misrepresents the product surface.

### Proposed Changes

Remove arch-check from the operator subcommand list in spec.md Product Shape and its diagram; note that architecture checks are a contributor quality-gate concern owned by non-functional-requirements.md Architecture Tests (realized as the console-arch-check binary).

## Proposal: Correct Scenario 2's illustrative drain command

### Target specification files

- SPECIFICATION/scenarios.md

### Summary

scenarios.md Scenario 2's sequence diagram shows the Factory invoking `dispatcher loop --budget 1`, but the implementation invokes a configurable drain program (default `livespec-dispatcher-drain`) through the DispatcherFactoryDrainPort.

### Motivation

Impl->spec drift: the concrete command named in the scenario diverged from the implemented drain-port invocation.

### Proposed Changes

Update Scenario 2's sequence diagram to reference the configurable drain program invoked through the factory drain port, rather than the literal `dispatcher loop --budget 1`, so the scenario matches the implemented mechanism without over-specifying a concrete command line.
