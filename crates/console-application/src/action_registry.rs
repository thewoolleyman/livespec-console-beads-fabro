//! The operator ACTION REGISTRY: the single source of truth for every per-item
//! operator action the console offers.
//!
//! Each entry carries the action's stable id, its human label, its parameter
//! shape, its availability predicate, its staging handler, its menu taxonomy
//! (consumed by the menu shell, which GENERATES menus from it), and the hotkey
//! that invokes it as a power-user convenience. The Status-line hint tokens,
//! the key bindings, and the Help-modal rosters are all DERIVED from this
//! table; a second, independently-maintained encoding of the action set is the
//! defect class this module exists to retire.
//!
//! Availability is ONE derivation consumed by BOTH presentation (is the action
//! offered in the hints?) and invocation (does the key stage it?). A surface
//! that offers an action which cannot fire — or fires one it does not offer —
//! is the dishonesty the Status-line contract forbids.

use crate::source_adapters::{AcceptancePolicy, AdmissionPolicy, Lane};
use crate::{
    DispatcherOverride, LaneWorkItem, OverrideBool, OverrideInt, PendingValve, RejectMode,
    driver_handoff_command, per_item_verb_is_state_valid, status_move_targets,
};

/// The per-item surface a selection lives on.
///
/// The two surfaces offer slightly different action sets: the move-status and
/// driver-handoff verbs act only on the individually-selected item of a
/// drilled-in lane, never on an Attention row (whose selection resolves a
/// work-item by id but is not a lane cursor).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionSurface {
    /// The needs-attention inbox with a work-item-backed row selected.
    Attention,
    /// A drilled-in lane with an individual work-item selected.
    LaneDrill,
}

/// Everything an availability predicate may depend on for the selection.
///
/// Built from the full [`LaneWorkItem`] record, so a predicate can consume any
/// dimension the action needs — lifecycle lane AND effective admission policy
/// at minimum — instead of the lane-only slice that let the Status band
/// advertise `p approve` on a dispatcher-admitted item.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActionContext {
    /// The selected item's lifecycle lane, consumed from the ledger.
    pub lane: Lane,
    /// The selected item's admission policy, consumed from the ledger.
    ///
    /// The ingest defaults an absent policy to `manual`, matching the
    /// orchestrator's effective-admission default. CONDITION this depends on:
    /// the orchestrator's `effective_admission_policy` also consults the
    /// global `auto_approve_ready` lever, which the per-item record does not
    /// carry; this predicate is exact while that lever remains `false` (its
    /// default). The durable fix is an orchestrator-emitted effective field.
    pub admission_policy: AdmissionPolicy,
    /// The selected item's acceptance policy, consumed from the ledger.
    pub acceptance_policy: AcceptancePolicy,
    /// Whether the selected item admits the driver-handoff verb (its lane and
    /// factory-safety marker admit it, per [`driver_handoff_command`]).
    pub has_driver_handoff: bool,
    /// Which per-item surface the selection lives on.
    pub surface: ActionSurface,
}

impl ActionContext {
    /// Build the availability context for a selected work-item on a surface.
    #[must_use]
    pub fn for_item(item: &LaneWorkItem, surface: ActionSurface) -> Self {
        Self {
            lane: item.lane(),
            admission_policy: item.admission_policy(),
            acceptance_policy: item.acceptance_policy(),
            has_driver_handoff: driver_handoff_command(item).is_some(),
            surface,
        }
    }
}

/// How an available action is staged when invoked.
#[derive(Debug, Clone, Copy)]
pub enum ActionStaging {
    /// Open the valve-confirm modal on the staged valve.
    Valve(fn(&ActionContext) -> Option<PendingValve>),
    /// Open the driver-handoff overlay for the selected item.
    DriverHandoff,
}

/// One registered operator action.
#[derive(Debug, Clone, Copy)]
pub struct ActionSpec {
    /// The stable action id. Where the action rides a single orchestrator
    /// `drive` verb this is that verb, so cross-repo parity can key on it.
    pub id: &'static str,
    /// The human label the confirm modal and menus render.
    pub label: &'static str,
    /// The exact Status-line hint fragment for this action.
    pub hint_token: &'static str,
    /// The hotkey that invokes this action — a power-user convenience only,
    /// never the sole route once menus exist. `None` is a menu/invoker-only
    /// action: reachable without any key, the living proof that hotkeys are
    /// additional.
    pub hotkey: Option<char>,
    /// The menu path this action lives under. Menus are GENERATED from this
    /// taxonomy; it is carried from day one so the menu shell inherits no
    /// schema migration.
    pub menu_path: &'static [&'static str],
    /// The parameter the confirm dialog cycles, described for menu/dialog
    /// generation. Payload-free actions carry no parameter.
    pub parameter: Option<ActionParameter>,
    /// Whether this action is offered and live for the given selection.
    pub availability: fn(&ActionContext) -> bool,
    /// How the action is staged when invoked.
    pub staging: ActionStaging,
}

/// The parameter shape a registered action's dialog cycles through.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActionParameter {
    /// The parameter's name as the dialog labels it.
    pub name: &'static str,
    /// The cycled choice values, as operator-facing labels. Dynamic ranges
    /// (the integer cap dials) describe their shape rather than enumerating
    /// every value.
    pub choices: &'static [&'static str],
}

/// The registry, in CANONICAL HINT ORDER.
///
/// The table order is the order action tokens render in every Status-line hint
/// row. Reordering it reorders the hints, which the docs lockstep gate pins —
/// treat the order as part of each entry.
pub static ACTION_REGISTRY: &[ActionSpec] = &[
    ActionSpec {
        id: "driver-handoff",
        label: "Driver handoff",
        hint_token: "h handoff",
        hotkey: Some('h'),
        menu_path: &["Work item", "Hand off"],
        parameter: None,
        availability: |ctx| {
            ctx.has_driver_handoff && matches!(ctx.surface, ActionSurface::LaneDrill)
        },
        staging: ActionStaging::DriverHandoff,
    },
    ActionSpec {
        id: "move",
        label: "Move status",
        hint_token: "s move-status",
        hotkey: Some('s'),
        menu_path: &["Work item", "Lifecycle"],
        parameter: Some(ActionParameter {
            name: "target status",
            choices: &["backlog", "ready", "blocked"],
        }),
        availability: |ctx| {
            matches!(ctx.surface, ActionSurface::LaneDrill)
                && status_move_targets(ctx.lane).first().is_some_and(|to| {
                    per_item_verb_is_state_valid(
                        ctx.lane,
                        PendingValve::MoveStatus {
                            from: ctx.lane,
                            to: *to,
                        },
                    )
                })
        },
        staging: ActionStaging::Valve(|ctx| {
            status_move_targets(ctx.lane)
                .first()
                .map(|to| PendingValve::MoveStatus {
                    from: ctx.lane,
                    to: *to,
                })
        }),
    },
    ActionSpec {
        id: "approve",
        label: "Approve work-item",
        hint_token: "p approve",
        hotkey: Some('p'),
        menu_path: &["Work item", "Lifecycle"],
        parameter: None,
        // Lane AND effective admission policy: the approve valve fires only on
        // an effective-manual pending-approval item (the orchestrator's
        // `can_approve_item` rule), so a dispatcher-admitted (`auto`) item
        // neither advertises nor stages it.
        availability: |ctx| {
            per_item_verb_is_state_valid(ctx.lane, PendingValve::Approve)
                && ctx.admission_policy == AdmissionPolicy::Manual
        },
        staging: ActionStaging::Valve(|_ctx| Some(PendingValve::Approve)),
    },
    ActionSpec {
        id: "accept",
        label: "Accept work-item",
        hint_token: "c accept",
        hotkey: Some('c'),
        menu_path: &["Work item", "Lifecycle"],
        parameter: None,
        availability: |ctx| per_item_verb_is_state_valid(ctx.lane, PendingValve::Accept),
        staging: ActionStaging::Valve(|_ctx| Some(PendingValve::Accept)),
    },
    ActionSpec {
        id: "reject",
        label: "Reject work-item",
        hint_token: "r reject",
        hotkey: Some('r'),
        menu_path: &["Work item", "Lifecycle"],
        parameter: Some(ActionParameter {
            name: "mode",
            choices: &["rework", "regroom"],
        }),
        availability: |ctx| {
            per_item_verb_is_state_valid(ctx.lane, PendingValve::Reject(RejectMode::Rework))
        },
        staging: ActionStaging::Valve(|_ctx| Some(PendingValve::Reject(RejectMode::Rework))),
    },
    ActionSpec {
        id: "set-admission",
        label: "Set admission",
        hint_token: "m set-admission",
        hotkey: Some('m'),
        menu_path: &["Work item", "Policy dials"],
        parameter: Some(ActionParameter {
            name: "policy",
            choices: &["manual", "auto"],
        }),
        availability: |ctx| {
            per_item_verb_is_state_valid(
                ctx.lane,
                PendingValve::SetAdmission(AdmissionPolicy::Manual),
            )
        },
        staging: ActionStaging::Valve(|_ctx| {
            Some(PendingValve::SetAdmission(AdmissionPolicy::Manual))
        }),
    },
    ActionSpec {
        id: "set-merge-on-review-cap",
        label: "Set override",
        hint_token: "g merge cap",
        hotkey: Some('g'),
        menu_path: &["Work item", "Policy dials"],
        parameter: Some(ActionParameter {
            name: "merge_on_review_cap",
            choices: &["on", "off", "clear"],
        }),
        availability: |ctx| {
            per_item_verb_is_state_valid(
                ctx.lane,
                PendingValve::SetOverride(DispatcherOverride::MergeOnReviewCap(
                    OverrideBool::Clear,
                )),
            )
        },
        staging: ActionStaging::Valve(|_ctx| {
            Some(PendingValve::SetOverride(
                DispatcherOverride::MergeOnReviewCap(OverrideBool::Clear),
            ))
        }),
    },
    ActionSpec {
        id: "set-review-fix-cap",
        label: "Set override",
        hint_token: "f fix cap",
        hotkey: Some('f'),
        menu_path: &["Work item", "Policy dials"],
        parameter: Some(ActionParameter {
            name: "review_fix_cap",
            choices: &["1..=9", "clear"],
        }),
        availability: |ctx| {
            per_item_verb_is_state_valid(
                ctx.lane,
                PendingValve::SetOverride(DispatcherOverride::ReviewFixCap(OverrideInt::Clear)),
            )
        },
        staging: ActionStaging::Valve(|_ctx| {
            Some(PendingValve::SetOverride(DispatcherOverride::ReviewFixCap(
                OverrideInt::Clear,
            )))
        }),
    },
    ActionSpec {
        id: "set-acceptance",
        label: "Set acceptance",
        hint_token: "n set-acceptance",
        hotkey: Some('n'),
        menu_path: &["Work item", "Policy dials"],
        parameter: Some(ActionParameter {
            name: "policy",
            choices: &["ai-then-human", "ai-only", "human-only"],
        }),
        availability: |ctx| {
            per_item_verb_is_state_valid(
                ctx.lane,
                PendingValve::SetAcceptance(AcceptancePolicy::AiThenHuman),
            )
        },
        staging: ActionStaging::Valve(|_ctx| {
            Some(PendingValve::SetAcceptance(AcceptancePolicy::AiThenHuman))
        }),
    },
    ActionSpec {
        id: "set-acceptance-rework-cap",
        label: "Set override",
        hint_token: "k rework cap",
        hotkey: Some('k'),
        menu_path: &["Work item", "Policy dials"],
        parameter: Some(ActionParameter {
            name: "acceptance_rework_cap",
            choices: &["1..=9", "clear"],
        }),
        availability: |ctx| {
            per_item_verb_is_state_valid(
                ctx.lane,
                PendingValve::SetOverride(DispatcherOverride::AcceptanceReworkCap(
                    OverrideInt::Clear,
                )),
            )
        },
        staging: ActionStaging::Valve(|_ctx| {
            Some(PendingValve::SetOverride(
                DispatcherOverride::AcceptanceReworkCap(OverrideInt::Clear),
            ))
        }),
    },
    // Menu/invoker-only, DELIBERATELY: the first action with no hotkey, the
    // living proof that every action is reachable without one. It records the
    // selected item's declared `.github/workflows/` path as citation-only —
    // the documented remedy a factory-safety host-only refusal names — so it
    // is offered exactly where it unblocks: a ready item the factory refused.
    ActionSpec {
        id: "set-workflow-scope-override",
        label: "Set workflow scope override",
        hint_token: "",
        hotkey: None,
        menu_path: &["Work item", "Factory safety"],
        parameter: Some(ActionParameter {
            name: "scope",
            choices: &["citation-only"],
        }),
        availability: |ctx| ctx.lane == Lane::Ready && ctx.has_driver_handoff,
        staging: ActionStaging::Valve(|_ctx| Some(PendingValve::SetWorkflowScopeOverride)),
    },
];

/// The registered action bound to `hotkey`, if any.
#[must_use]
pub fn action_for_hotkey(hotkey: char) -> Option<&'static ActionSpec> {
    ACTION_REGISTRY
        .iter()
        .find(|spec| spec.hotkey == Some(hotkey))
}

/// The registered action with the given stable id, if any.
#[must_use]
pub fn action_for_id(id: &str) -> Option<&'static ActionSpec> {
    ACTION_REGISTRY.iter().find(|spec| spec.id == id)
}

/// The hint tokens of every action offered and available for `ctx`, in
/// canonical order. This is the SAME derivation the key handlers consult, so
/// hidden hints and inert keys cannot diverge.
#[must_use]
pub fn available_hint_tokens(ctx: &ActionContext) -> Vec<&'static str> {
    ACTION_REGISTRY
        .iter()
        // Hint tokens are KEY hints: a menu/invoker-only action has no key to
        // hint, so it renders in menus and the invoker roster instead.
        .filter(|spec| spec.hotkey.is_some() && (spec.availability)(ctx))
        .map(|spec| spec.hint_token)
        .collect()
}

/// The Status-line hint for a selected work-item, derived from the registry.
///
/// The navigation prefix and the trailing help/quit keys are context data, not
/// registered actions; the action tokens between them derive from
/// [`available_hint_tokens`]. A drilled-in lane whose selection admits no
/// action renders without the up/down fragment, reproducing the pinned
/// terminal-lane hint exactly.
#[must_use]
pub fn selected_item_hint(ctx: &ActionContext) -> String {
    let tokens = available_hint_tokens(ctx);
    let (prefix, suffix) = match ctx.surface {
        ActionSurface::Attention => ("up/down move | enter open", "? help | q quit"),
        ActionSurface::LaneDrill if tokens.is_empty() => {
            return "enter item | esc lane list | ? help | q quit".to_owned();
        }
        ActionSurface::LaneDrill => (
            "up/down move | enter item | esc lane list",
            "? help | q quit",
        ),
    };
    if tokens.is_empty() {
        return format!("{prefix} | {suffix}");
    }
    format!("{prefix} | {} | {suffix}", tokens.join(" | "))
}

/// Whether the registered action may be invoked for `ctx`, staged as the
/// interaction intent it opens. `None` when the action is unavailable — the
/// hotkey is inert exactly where the hint is suppressed.
#[must_use]
pub fn stage_action(spec: &ActionSpec, ctx: &ActionContext) -> Option<StagedAction> {
    if !(spec.availability)(ctx) {
        return None;
    }
    match spec.staging {
        ActionStaging::Valve(stager) => stager(ctx).map(StagedAction::Valve),
        ActionStaging::DriverHandoff => Some(StagedAction::DriverHandoff),
    }
}

/// Whether `spec` is ever offered on `surface`, for any selection state.
///
/// Reference surfaces (the Help modal, menu generation) use this to list the
/// actions a surface can offer without fixing a selection; the per-selection
/// answer stays [`ActionSpec::availability`]'s alone.
#[must_use]
pub fn action_offered_on_surface(spec: &ActionSpec, surface: ActionSurface) -> bool {
    Lane::all().iter().any(|lane| {
        [AdmissionPolicy::Manual, AdmissionPolicy::Auto]
            .iter()
            .any(|admission| {
                [false, true].iter().any(|handoff| {
                    (spec.availability)(&ActionContext {
                        lane: *lane,
                        admission_policy: *admission,
                        acceptance_policy: AcceptancePolicy::AiThenHuman,
                        has_driver_handoff: *handoff,
                        surface,
                    })
                })
            })
    })
}

/// Whether the staged valve is a registered action available for `ctx`.
///
/// The invocation-side half of the one-derivation rule: the reducer refuses
/// to stage, and the confirm refuses to resolve, a valve the registry does
/// not offer for the selection, so hidden hints, inert keys, and non-firing
/// confirms cannot diverge.
#[must_use]
pub fn valve_is_available(valve: PendingValve, ctx: &ActionContext) -> bool {
    let id = match valve {
        PendingValve::Approve => "approve",
        PendingValve::Accept => "accept",
        PendingValve::Reject(_mode) => "reject",
        PendingValve::SetAdmission(_policy) => "set-admission",
        PendingValve::SetAcceptance(_policy) => "set-acceptance",
        PendingValve::MoveStatus { from, .. } => {
            if from != ctx.lane {
                return false;
            }
            "move"
        }
        PendingValve::SetOverride(DispatcherOverride::MergeOnReviewCap(_value)) => {
            "set-merge-on-review-cap"
        }
        PendingValve::SetOverride(DispatcherOverride::ReviewFixCap(_value)) => "set-review-fix-cap",
        PendingValve::SetOverride(DispatcherOverride::AcceptanceReworkCap(_value)) => {
            "set-acceptance-rework-cap"
        }
        PendingValve::SetWorkflowScopeOverride => "set-workflow-scope-override",
    };
    action_for_id(id).is_some_and(|spec| (spec.availability)(ctx))
}

/// The staged result of invoking an available registered action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StagedAction {
    /// Open the valve-confirm modal on this valve.
    Valve(PendingValve),
    /// Open the driver-handoff overlay.
    DriverHandoff,
}

#[cfg(test)]
mod tests {
    use super::{
        ACTION_REGISTRY, ActionContext, ActionSurface, action_for_hotkey, action_for_id,
        action_offered_on_surface,
    };
    use crate::source_adapters::{AcceptancePolicy, AdmissionPolicy, Lane};

    #[test]
    fn registry_entries_are_complete_unique_and_off_the_system_keys() {
        // no-orphan-hotkeys, registry half: every entry carries a stable id, a
        // hint token, a menu path (the taxonomy menus are generated from), and
        // a UNIQUE hotkey that cannot shadow a system key.
        let mut hotkeys = std::collections::BTreeSet::new();
        let mut ids = std::collections::BTreeSet::new();
        for spec in ACTION_REGISTRY {
            assert!(!spec.id.is_empty());
            assert!(!spec.label.is_empty());
            // A hint token is a KEY hint: keyed actions carry one, and a
            // menu/invoker-only action carries none.
            // Bound as a local so the assert fits on ONE line: rustfmt's
            // fn_call_width would otherwise break the args across lines and put
            // the failure-only `spec.id` message on a line llvm-cov counts as
            // never executed. Same pincer PR #573 restructured out.
            let keyed = spec.hotkey.is_some();
            assert_eq!(spec.hint_token.is_empty(), !keyed, "{}", spec.id);
            assert!(!spec.menu_path.is_empty(), "{}", spec.id);
            assert!(ids.insert(spec.id), "{}", spec.id);
            if let Some(key) = spec.hotkey {
                assert!(hotkeys.insert(key), "{}", key);
                assert!(!['/', ':', '?', 'q', ' '].contains(&key), "{}", spec.id);
            }
        }
    }

    #[test]
    fn hotkey_and_id_lookups_round_trip_every_entry() {
        for spec in ACTION_REGISTRY {
            if let Some(key) = spec.hotkey {
                assert_eq!(action_for_hotkey(key).map(|found| found.id), Some(spec.id));
            }
            assert_eq!(
                action_for_id(spec.id).map(|found| found.hotkey),
                Some(spec.hotkey)
            );
        }
        assert!(action_for_hotkey('z').is_none());
        assert!(action_for_id("no-such-action").is_none());
    }

    #[test]
    fn surface_offering_matches_the_documented_surface_split() {
        // The move-status and driver-handoff verbs are drilled-lane-only;
        // every other action is offered on both per-item surfaces.
        for spec in ACTION_REGISTRY {
            let lane_drill = action_offered_on_surface(spec, ActionSurface::LaneDrill);
            let attention = action_offered_on_surface(spec, ActionSurface::Attention);
            assert!(lane_drill, "{}", spec.id);
            let drill_only = spec.id == "move" || spec.id == "driver-handoff";
            assert_eq!(attention, !drill_only, "{}", spec.id);
        }
    }

    #[test]
    fn every_registered_action_stages_from_some_admitting_context() {
        // Every staging closure runs against a context its availability
        // admits: an action the registry offers somewhere must stage there —
        // a roster entry that can never stage is a phantom.
        use super::stage_action;
        for spec in ACTION_REGISTRY {
            let mut staged_somewhere = false;
            for surface in [ActionSurface::Attention, ActionSurface::LaneDrill] {
                for lane in Lane::all() {
                    for admission in [AdmissionPolicy::Manual, AdmissionPolicy::Auto] {
                        for handoff in [false, true] {
                            let ctx = ActionContext {
                                lane: *lane,
                                admission_policy: admission,
                                acceptance_policy: AcceptancePolicy::AiThenHuman,
                                has_driver_handoff: handoff,
                                surface,
                            };
                            if (spec.availability)(&ctx) {
                                staged_somewhere |= stage_action(spec, &ctx).is_some();
                            }
                        }
                    }
                }
            }
            assert!(staged_somewhere, "{}", spec.id);
        }
    }

    #[test]
    fn approve_availability_needs_the_manual_admission_dimension() {
        let manual = ActionContext {
            lane: Lane::PendingApproval,
            admission_policy: AdmissionPolicy::Manual,
            acceptance_policy: AcceptancePolicy::AiThenHuman,
            has_driver_handoff: false,
            surface: ActionSurface::Attention,
        };
        let auto = ActionContext {
            admission_policy: AdmissionPolicy::Auto,
            ..manual
        };
        let approve = action_for_id("approve");
        assert_eq!(approve.map(|spec| (spec.availability)(&manual)), Some(true));
        assert_eq!(approve.map(|spec| (spec.availability)(&auto)), Some(false));
    }

    #[test]
    fn valve_availability_maps_every_valve_shape_onto_its_registry_entry() {
        use super::valve_is_available;
        use crate::{DispatcherOverride, OverrideBool, OverrideInt, PendingValve, RejectMode};
        let ready_drill = ActionContext {
            lane: Lane::Ready,
            admission_policy: AdmissionPolicy::Manual,
            acceptance_policy: AcceptancePolicy::AiThenHuman,
            has_driver_handoff: false,
            surface: ActionSurface::LaneDrill,
        };
        // A move staged from a lane the item is NOT in is refused outright.
        let stale_from = PendingValve::MoveStatus {
            from: Lane::Backlog,
            to: Lane::Ready,
        };
        assert!(!valve_is_available(stale_from, &ready_drill));
        let live_from = PendingValve::MoveStatus {
            from: Lane::Ready,
            to: Lane::Backlog,
        };
        assert!(valve_is_available(live_from, &ready_drill));
        // Every valve shape resolves to its registry entry's availability.
        assert!(!valve_is_available(PendingValve::Approve, &ready_drill));
        assert!(!valve_is_available(PendingValve::Accept, &ready_drill));
        assert!(!valve_is_available(
            PendingValve::Reject(RejectMode::Rework),
            &ready_drill
        ));
        assert!(!valve_is_available(
            PendingValve::SetAdmission(AdmissionPolicy::Manual),
            &ready_drill
        ));
        assert!(valve_is_available(
            PendingValve::SetAcceptance(AcceptancePolicy::AiOnly),
            &ready_drill
        ));
        assert!(valve_is_available(
            PendingValve::SetOverride(DispatcherOverride::MergeOnReviewCap(OverrideBool::On)),
            &ready_drill
        ));
        assert!(valve_is_available(
            PendingValve::SetOverride(DispatcherOverride::ReviewFixCap(OverrideInt::Value(3))),
            &ready_drill
        ));
        assert!(valve_is_available(
            PendingValve::SetOverride(DispatcherOverride::AcceptanceReworkCap(OverrideInt::Clear)),
            &ready_drill
        ));
        // The scope override needs the factory-safety dimension, not the lane
        // alone.
        assert!(!valve_is_available(
            PendingValve::SetWorkflowScopeOverride,
            &ready_drill
        ));
        let refused_ready = ActionContext {
            has_driver_handoff: true,
            ..ready_drill
        };
        assert!(valve_is_available(
            PendingValve::SetWorkflowScopeOverride,
            &refused_ready
        ));
    }
}
