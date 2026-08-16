# The overseer/foreman layer is orthogonal to this console — do not conflate

`livespec-overseer` (the `foreman` and `overseer` daemons/skills) and
`livespec-console-beads-fabro` (this repo) are two **separate, independent**
mechanisms for driving factory work. Neither configures, gates, or observes
the other. This is now recorded normatively in `SPECIFICATION/spec.md` ->
Scope Boundary (landed as `v040`); this note exists so a session grepping
`.ai/` first finds the same answer without re-deriving it.

**Why this note exists.** A session investigated whether the console TUI
needed new UI to surface foreman/overseer autonomy config levers — for
example `livespec-overseer.foreman_valve_disposition` in `.livespec.jsonc`,
which governs whether the foreman may convene a consensus panel on a
blocked session. The maintainer clarified: they are orthogonal, not
layered. Don't re-investigate this.

**What that means concretely:**

- The console MUST NOT be treated as a control surface for the
  overseer/foreman layer.
- The console's Settings UI (the generic, orchestrator-declared
  `dispatcher.*` levers surface -- see `contracts.md` -> Dispatcher Policy
  Settings) has no analog for `foreman_valve_disposition` or any other
  `livespec-overseer` config key, and SHOULD NOT gain one.
- `spec_governance.*` levers (`propose_change_mode`, `revise_decision_mode`,
  `ratification_review`, etc.) are a *third*, also-separate namespace --
  Spec Plane territory the console explicitly disclaims owning per
  `spec.md` -> Scope Boundary ("`/livespec:*` spec mutation semantics").
  Don't conflate this with the foreman/overseer question either.

If a future session is asked to make foreman/overseer state visible
*inside* the console, that is a deliberate architecture change, not a
gap-fill -- it should go through a fresh `/livespec:propose-change`
that explicitly revisits this boundary, not a silent UI addition.
