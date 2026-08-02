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
    /// never the sole route once menus exist.
    pub hotkey: char,
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
        hotkey: 'h',
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
        hotkey: 's',
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
            let to = status_move_targets(ctx.lane).first().copied()?;
            Some(PendingValve::MoveStatus { from: ctx.lane, to })
        }),
    },
    ActionSpec {
        id: "approve",
        label: "Approve work-item",
        hint_token: "p approve",
        hotkey: 'p',
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
        hotkey: 'c',
        menu_path: &["Work item", "Lifecycle"],
        parameter: None,
        availability: |ctx| per_item_verb_is_state_valid(ctx.lane, PendingValve::Accept),
        staging: ActionStaging::Valve(|_ctx| Some(PendingValve::Accept)),
    },
    ActionSpec {
        id: "reject",
        label: "Reject work-item",
        hint_token: "r reject",
        hotkey: 'r',
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
        hotkey: 'm',
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
        hotkey: 'g',
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
        hotkey: 'f',
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
        hotkey: 'n',
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
        hotkey: 'k',
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
];

/// The registered action bound to `hotkey`, if any.
#[must_use]
pub fn action_for_hotkey(hotkey: char) -> Option<&'static ActionSpec> {
    ACTION_REGISTRY.iter().find(|spec| spec.hotkey == hotkey)
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
        .filter(|spec| (spec.availability)(ctx))
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
