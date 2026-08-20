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
    /// Whether the item is waiting on a workflow-scope override, consumed from
    /// the ledger.
    ///
    /// A SEPARATE dimension from [`has_driver_handoff`] on purpose, even
    /// though both concern factory safety. The dispatcher refuses on three
    /// ordered arms and **the override only clears the third**; the first
    /// (a non-null `factory_safety`) refuses before the override is ever
    /// consulted. So an item that admits the driver handoff is precisely one
    /// the override CANNOT help, and deriving one from the other would offer
    /// the action exactly where it is useless — the `-0uw` defect wearing a
    /// different hat.
    pub awaits_scope_override: bool,
    /// Count of ready work-items on the current board.
    ///
    /// Selection-local actions ignore this, but the factory dispatch action is
    /// selection-less and must be gated by the same ready-work fact the command
    /// handler's [`crate::FactoryDrainPolicy`] enforces at execution time.
    pub ready_work_item_count: usize,
    /// Which per-item surface the selection lives on.
    pub surface: ActionSurface,
}

impl ActionContext {
    /// Build the availability context for a selected work-item on a surface.
    #[must_use]
    pub fn for_item(
        item: &LaneWorkItem,
        surface: ActionSurface,
        ready_work_item_count: usize,
    ) -> Self {
        Self {
            lane: item.lane(),
            admission_policy: item.admission_policy(),
            acceptance_policy: item.acceptance_policy(),
            has_driver_handoff: driver_handoff_command(item).is_some(),
            awaits_scope_override: item.detail().awaits_scope_override,
            ready_work_item_count,
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
    /// Persist a factory drain command for ready work.
    FactoryDrain,
    /// Perform a global action, which needs no selection.
    Global(GlobalAction),
}

/// A key chord: the key itself plus whether Control is held.
///
/// # Why a chord and not a bare `char`
///
/// `hotkey: Option<char>` conflated two things and could express neither
/// cleanly. It could not carry `Ctrl-C` at all, and it forced the structural
/// keys (`/`, `:`, `?`, `q`) to stay outside the registry — matched ahead of
/// the registry lookup in the key handler, which is a SECOND ENCODING of the
/// key-to-action mapping and the exact defect the action registry exists to
/// retire.
///
/// The chord also removes a collision that was previously only avoided by
/// keeping `Ctrl-C` out: `c` accepts a work-item and `Ctrl-C` quits. As bare
/// chars those are the same key; as chords they are distinct.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct KeyChord {
    /// Whether Control is held.
    pub ctrl: bool,
    /// The key itself.
    pub key: char,
}

impl KeyChord {
    /// The chord for `key` pressed on its own.
    #[must_use]
    pub const fn plain(key: char) -> Self {
        Self { ctrl: false, key }
    }

    /// The chord for `key` pressed with Control held.
    #[must_use]
    pub const fn ctrl(key: char) -> Self {
        Self { ctrl: true, key }
    }
}

impl core::fmt::Display for KeyChord {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        if self.ctrl {
            write!(formatter, "ctrl-{}", self.key)
        } else {
            write!(formatter, "{}", self.key)
        }
    }
}

/// A global action: one that needs no work-item selection.
///
/// The per-item verbs stage a valve or the driver handoff against a selection.
/// These four are reachable with nothing selected, which is why they were
/// handled outside the registry before chords existed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GlobalAction {
    /// Open the search overlay.
    OpenSearch,
    /// Open the command palette.
    OpenCommandPalette,
    /// Open the modal Help overlay.
    OpenHelp,
    /// Open the menu bar.
    OpenMenu,
    /// Quit the console.
    Quit,
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
    /// The chords that invoke this action — a power-user convenience only,
    /// never the sole route once menus exist. EMPTY is a menu/invoker-only
    /// action: reachable without any key, the living proof that hotkeys are
    /// additional.
    ///
    /// A SLICE rather than one chord because an action can honestly have more
    /// than one accelerator: `q` and `Ctrl-C` are both quit. Modelling that as
    /// two registry entries would render quit twice in every generated menu.
    pub hotkeys: &'static [KeyChord],
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
        hotkeys: &[KeyChord::plain('h')],
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
        hotkeys: &[KeyChord::plain('s')],
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
        hotkeys: &[KeyChord::plain('p')],
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
        hotkeys: &[KeyChord::plain('c')],
        menu_path: &["Work item", "Lifecycle"],
        parameter: None,
        availability: |ctx| per_item_verb_is_state_valid(ctx.lane, PendingValve::Accept),
        staging: ActionStaging::Valve(|_ctx| Some(PendingValve::Accept)),
    },
    ActionSpec {
        id: "reject",
        label: "Reject work-item",
        hint_token: "r reject",
        hotkeys: &[KeyChord::plain('r')],
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
        hotkeys: &[KeyChord::plain('m')],
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
        hotkeys: &[KeyChord::plain('g')],
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
        hotkeys: &[KeyChord::plain('f')],
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
        hotkeys: &[KeyChord::plain('n')],
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
        hotkeys: &[KeyChord::plain('k')],
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
        hotkeys: &[],
        menu_path: &["Work item", "Factory safety"],
        parameter: Some(ActionParameter {
            name: "scope",
            choices: &["citation-only"],
        }),
        // Gated on the item ACTUALLY awaiting an override, which only the
        // orchestrator can know: the refusal this clears is computed at
        // dispatch time from the item's own text, and re-deriving that regex
        // here would violate the consume-don't-re-derive rule. The
        // orchestrator publishes `awaits_scope_override` as the materialized
        // signal for this case; the console consumes it directly and stays
        // inert everywhere else.
        availability: |ctx| ctx.lane == Lane::Ready && ctx.awaits_scope_override,
        staging: ActionStaging::Valve(|_ctx| Some(PendingValve::SetWorkflowScopeOverride)),
    },
    ActionSpec {
        id: "dispatch-ready",
        label: "Dispatch ready work",
        hint_token: "",
        hotkeys: &[],
        menu_path: &["Factory", "Dispatch"],
        parameter: None,
        availability: |ctx| ctx.ready_work_item_count > 0,
        staging: ActionStaging::FactoryDrain,
    },
    // THE GLOBAL ACTIONS. Registered 2026-08-19 on the maintainer's chord
    // ruling, closing the menu-completeness gate's five red names.
    //
    // These were handled outside the registry until chords existed, matched
    // ahead of the registry lookup in `key_event_to_terminal_input`. That was a
    // SECOND ENCODING of the key-to-action mapping, and it made the four keys
    // unreachable from a generated menu — the plain contradiction of "menus are
    // the PRIMARY navigation mechanism".
    //
    // They introduce the FIRST top-level menu nodes beyond `Work item`, so the
    // menu bar becomes real rather than a single-node degenerate case.
    ActionSpec {
        id: "open-search",
        label: "Search",
        hint_token: "/ search",
        hotkeys: &[KeyChord::plain('/')],
        menu_path: &["View", "Search"],
        parameter: None,
        availability: |_ctx| true,
        staging: ActionStaging::Global(GlobalAction::OpenSearch),
    },
    ActionSpec {
        id: "open-command-palette",
        label: "Command palette",
        hint_token: ": palette",
        hotkeys: &[KeyChord::plain(':')],
        menu_path: &["View", "Command palette"],
        parameter: None,
        availability: |_ctx| true,
        staging: ActionStaging::Global(GlobalAction::OpenCommandPalette),
    },
    // The menu bar's own opener. It is a REGISTRY entry rather than a literal
    // key arm so it obeys the same rule as everything else: no behaviour is
    // reachable outside the registry.
    //
    // `v` is a compromise, recorded rather than hidden. The conventional menu-bar
    // keys are F10 and Alt, and `KeyChord` can express neither — it carries a
    // `char` plus Control only. Widening it further is deliberately NOT bundled
    // into this slice; the binding can move without touching the menu tree.
    ActionSpec {
        id: "open-menu",
        label: "Menu bar",
        hint_token: "v menu",
        hotkeys: &[KeyChord::plain('v')],
        menu_path: &["View", "Menu bar"],
        parameter: None,
        availability: |_ctx| true,
        staging: ActionStaging::Global(GlobalAction::OpenMenu),
    },
    ActionSpec {
        id: "open-help",
        label: "Help",
        hint_token: "? help",
        hotkeys: &[KeyChord::plain('?')],
        menu_path: &["Help", "Keys and actions"],
        parameter: None,
        availability: |_ctx| true,
        staging: ActionStaging::Global(GlobalAction::OpenHelp),
    },
    // TWO chords, and this is the case that forced `hotkeys` to be a slice:
    // `q` and `Ctrl-C` are the same action. `Ctrl-C` is also why the chord type
    // exists at all — `Option<char>` could not express it.
    ActionSpec {
        id: "quit",
        label: "Quit",
        hint_token: "q quit",
        hotkeys: &[KeyChord::plain('q'), KeyChord::ctrl('c')],
        menu_path: &["File", "Quit"],
        parameter: None,
        availability: |_ctx| true,
        staging: ActionStaging::Global(GlobalAction::Quit),
    },
];

/// The registered action bound to `chord`, if any.
#[must_use]
pub fn action_for_chord(chord: KeyChord) -> Option<&'static ActionSpec> {
    ACTION_REGISTRY
        .iter()
        .find(|spec| spec.hotkeys.contains(&chord))
}

/// One group inside a top-level menu, e.g. `Lifecycle` under `Work item`.
#[derive(Debug, Clone)]
pub struct MenuGroup {
    /// The group's label — the SECOND element of its members' `menu_path`.
    pub label: &'static str,
    /// The actions in this group, in registry order.
    pub actions: Vec<&'static ActionSpec>,
}

/// One top-level menu — a node of the menu BAR.
#[derive(Debug, Clone)]
pub struct MenuTop {
    /// The bar label — the FIRST element of its members' `menu_path`.
    pub label: &'static str,
    /// The groups under this bar node, in registry order.
    pub groups: Vec<MenuGroup>,
}

/// The menu tree, DERIVED from `menu_path`.
///
/// # Why this is derived and not authored
///
/// Menus are the PRIMARY navigation mechanism, and the registry is the single
/// source of truth for what an operator can do. A hand-authored menu tree would
/// be a SECOND encoding of the same taxonomy: it could drift from the registry
/// silently, and the completeness gate would be quantifying over the wrong
/// thing. Deriving the tree means a new registry entry appears in the menus by
/// construction, and cannot be forgotten.
///
/// Ordering is REGISTRY ORDER throughout — first appearance wins for both bar
/// nodes and groups — so the menus inherit the registry's canonical order rather
/// than imposing a second one.
#[must_use]
pub fn menu_tree() -> Vec<MenuTop> {
    let mut tops: Vec<MenuTop> = Vec::new();
    for spec in ACTION_REGISTRY {
        // A one-element path degenerates to a group of its own name rather than
        // being dropped: an action must never vanish from the menus because its
        // taxonomy is shallow.
        let top_label = spec.menu_path.first().copied().unwrap_or(spec.label);
        let group_label = spec.menu_path.get(1).copied().unwrap_or(top_label);
        // Resolved as an INDEX rather than a second `find` after the push: a
        // fallible re-lookup would need an unreachable fallback arm, and an
        // unreachable arm is a line no test can ever name.
        // The position is bound BEFORE the fallback so the closure may borrow
        // `tops` mutably: an `Option<usize>` holds no borrow, while matching on
        // `tops.iter().position(..)` directly would keep the immutable borrow
        // alive across the push.
        let existing = tops.iter().position(|top| top.label == top_label);
        let top_index = existing.unwrap_or_else(|| {
            tops.push(MenuTop {
                label: top_label,
                groups: Vec::new(),
            });
            tops.len() - 1
        });
        let top = &mut tops[top_index];
        if let Some(group) = top
            .groups
            .iter_mut()
            .find(|group| group.label == group_label)
        {
            group.actions.push(spec);
        } else {
            top.groups.push(MenuGroup {
                label: group_label,
                actions: vec![spec],
            });
        }
    }
    tops
}

/// The actions under one bar node, flattened in registry order.
///
/// The menu renders group HEADERS but selection moves over actions only, so the
/// selection index addresses this flattened list rather than the rendered rows.
#[must_use]
pub fn menu_actions(top_index: usize) -> Vec<&'static ActionSpec> {
    menu_tree()
        .get(top_index)
        .map(|top| {
            top.groups
                .iter()
                .flat_map(|group| group.actions.iter().copied())
                .collect()
        })
        .unwrap_or_default()
}

/// The GLOBAL action bound to `chord`, if the chord is bound to one at all.
///
/// Answering the global question here rather than at the call site keeps the
/// caller free of a `Valve | DriverHandoff => None` arm that nothing can reach:
/// no per-item verb carries a Control chord, so such an arm would be
/// structurally dead code in the TUI. Here it is reachable and tested — a plain
/// `p` resolves to the approve VALVE, which is not a global.
#[must_use]
pub fn global_action_for_chord(chord: KeyChord) -> Option<GlobalAction> {
    match action_for_chord(chord)?.staging {
        ActionStaging::Global(action) => Some(action),
        ActionStaging::Valve(_) | ActionStaging::DriverHandoff | ActionStaging::FactoryDrain => {
            None
        }
    }
}

/// How this action's accelerators render beside its label.
///
/// `menu` marks an action with no accelerator at all — reachable only through
/// a menu or the invoker, which is the point of allowing an empty chord set.
#[must_use]
pub fn accelerator_display(spec: &ActionSpec) -> String {
    if spec.hotkeys.is_empty() {
        return "menu".to_owned();
    }
    spec.hotkeys
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("/")
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
        //
        // Globals are excluded because this composes the PER-ITEM hint row.
        // Their tokens are carried on their specs and exposed through
        // `global_status_hint_tokens` for the Status band's global suffix.
        .filter(|spec| {
            !matches!(spec.staging, ActionStaging::Global(_))
                && !spec.hotkeys.is_empty()
                && (spec.availability)(ctx)
        })
        .map(|spec| spec.hint_token)
        .collect()
}

/// The global shortcut tokens the Status band carries for every pane.
///
/// The permanent menu bar makes the menu taxonomy continuously visible, so this
/// row carries only the always-live modal/exit shortcuts that stay useful beside
/// pane-local hints. It still derives from [`ACTION_REGISTRY`], not a parallel
/// Status-band string.
#[must_use]
pub fn global_status_hint_tokens() -> Vec<&'static str> {
    ACTION_REGISTRY
        .iter()
        .filter(|spec| {
            matches!(
                spec.staging,
                ActionStaging::Global(GlobalAction::OpenHelp | GlobalAction::Quit)
            )
        })
        .map(|spec| spec.hint_token)
        .collect()
}

/// The joined global Status-band suffix, derived from the registry.
#[must_use]
pub fn global_status_hint() -> String {
    global_status_hint_tokens().join(" | ")
}

/// The Status-line hint for a selected work-item, derived from the registry.
///
/// The navigation prefix is pane context; the per-item action tokens derive from
/// [`available_hint_tokens`], and the global suffix derives from
/// [`global_status_hint_tokens`]. A drilled-in lane whose selection admits no
/// action renders without the up/down fragment, reproducing the pinned
/// terminal-lane hint exactly.
#[must_use]
pub fn selected_item_hint(ctx: &ActionContext) -> String {
    let tokens = available_hint_tokens(ctx);
    let suffix = global_status_hint();
    let prefix = match ctx.surface {
        ActionSurface::Attention => "up/down move | enter open",
        ActionSurface::LaneDrill if tokens.is_empty() => {
            return format!("enter item | esc lane list | {suffix}");
        }
        ActionSurface::LaneDrill => "up/down move | enter item | esc lane list",
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
        ActionStaging::FactoryDrain => Some(StagedAction::FactoryDrain),
        ActionStaging::Global(action) => Some(StagedAction::Global(action)),
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
                    // Both scope-override states, or an action gated on it
                    // would read as "offered nowhere" on every surface.
                    [false, true].iter().any(|awaiting| {
                        (spec.availability)(&ActionContext {
                            lane: *lane,
                            admission_policy: *admission,
                            acceptance_policy: AcceptancePolicy::AiThenHuman,
                            has_driver_handoff: *handoff,
                            awaits_scope_override: *awaiting,
                            ready_work_item_count: 1,
                            surface,
                        })
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
    /// Persist a factory drain command.
    FactoryDrain,
    /// Perform a global action, which needed no selection to stage.
    Global(GlobalAction),
}

#[cfg(test)]
mod tests {
    use super::{
        ACTION_REGISTRY, ActionContext, ActionStaging, ActionSurface, GlobalAction, KeyChord,
        action_for_chord, action_for_id, action_offered_on_surface, global_action_for_chord,
        global_status_hint_tokens, menu_actions, menu_tree,
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
            let keyed = !spec.hotkeys.is_empty();
            assert_eq!(spec.hint_token.is_empty(), !keyed, "{}", spec.id);
            assert!(!spec.menu_path.is_empty(), "{}", spec.id);
            assert!(ids.insert(spec.id), "{}", spec.id);
            for chord in spec.hotkeys {
                assert!(hotkeys.insert(*chord), "{chord}");
                // Space is still refused as a registry chord: the key handler
                // matches it ahead of the registry lookup, so a registry claim
                // on it would be a lie. `/ : ? q` USED to be refused for the
                // same reason and are now genuinely registry-dispatched, so
                // they are no longer forbidden — the handler arms that shadowed
                // them are gone.
                assert!(chord.key != ' ', "{}", spec.id);
            }
        }
    }

    #[test]
    fn hotkey_and_id_lookups_round_trip_every_entry() {
        for spec in ACTION_REGISTRY {
            for chord in spec.hotkeys {
                assert_eq!(
                    action_for_chord(*chord).map(|found| found.id),
                    Some(spec.id)
                );
            }
            assert_eq!(
                action_for_id(spec.id).map(|found| found.hotkeys),
                Some(spec.hotkeys)
            );
        }
        assert!(action_for_chord(KeyChord::plain('z')).is_none());
        // The chord's whole point: `c` accepts, `ctrl-c` quits. As bare chars
        // these were the same key, which is why `ctrl-c` could not be
        // registered at all.
        assert_eq!(
            action_for_chord(KeyChord::plain('c')).map(|found| found.id),
            Some("accept")
        );
        assert_eq!(
            action_for_chord(KeyChord::ctrl('c')).map(|found| found.id),
            Some("quit")
        );
        assert!(action_for_id("no-such-action").is_none());
    }

    #[test]
    fn the_menu_tree_reaches_every_registered_action_exactly_once() {
        // THE COMPLETENESS PROPERTY, quantified GENERICALLY over the registry
        // rather than a hand-listed set. A hand-listed expectation would be the
        // same second-encoding defect the derived tree exists to retire, and it
        // would go stale the moment an entry is added — which is exactly the
        // case this must catch.
        let mut reached: Vec<&str> = menu_tree()
            .iter()
            .flat_map(|top| top.groups.iter())
            .flat_map(|group| group.actions.iter())
            .map(|spec| spec.id)
            .collect();
        let mut registered: Vec<&str> = ACTION_REGISTRY.iter().map(|spec| spec.id).collect();
        let reached_count = reached.len();
        reached.sort_unstable();
        registered.sort_unstable();
        assert_eq!(reached, registered);
        // EXACTLY once: a duplicated entry would still satisfy set equality
        // while rendering the same action twice in the menus.
        assert_eq!(reached_count, ACTION_REGISTRY.len());
    }

    #[test]
    fn the_menu_bar_carries_every_top_level_node_in_registry_order() {
        let tree = menu_tree();
        let labels: Vec<&str> = tree.iter().map(|top| top.label).collect();
        // Derived from the registry, not asserted as a fixed list: the point is
        // that first appearance in ACTION_REGISTRY orders the bar.
        let mut expected: Vec<&str> = Vec::new();
        for spec in ACTION_REGISTRY {
            let top = spec.menu_path.first().copied().unwrap_or(spec.label);
            if !expected.contains(&top) {
                expected.push(top);
            }
        }
        assert_eq!(labels, expected);
        // The bar must be a REAL bar, not a degenerate single node — the design
        // basis the 2026-08-03 ruling predicted once the globals were registered.
        assert!(tree.len() >= 2, "{labels:?}");
    }

    #[test]
    fn flattened_menu_actions_match_the_tree_for_every_bar_node() {
        for (index, top) in menu_tree().iter().enumerate() {
            let flattened: Vec<&str> = menu_actions(index).iter().map(|spec| spec.id).collect();
            let walked: Vec<&str> = top
                .groups
                .iter()
                .flat_map(|group| group.actions.iter())
                .map(|spec| spec.id)
                .collect();
            assert_eq!(flattened, walked, "{}", top.label);
        }
        // Out of range is empty, not a panic: the renderer clamps, and a
        // panicking accessor would turn a clamp bug into a crash.
        assert!(menu_actions(menu_tree().len()).is_empty());
    }

    #[test]
    fn only_global_staged_actions_answer_the_global_chord_lookup() {
        assert_eq!(
            global_action_for_chord(KeyChord::ctrl('c')),
            Some(GlobalAction::Quit)
        );
        assert_eq!(
            global_action_for_chord(KeyChord::plain('?')),
            Some(GlobalAction::OpenHelp)
        );
        // A per-item VALVE is bound to `p`, so the global lookup declines it.
        assert_eq!(global_action_for_chord(KeyChord::plain('p')), None);
        // And the driver handoff, the other non-global staging.
        assert_eq!(global_action_for_chord(KeyChord::plain('h')), None);
        assert_eq!(global_action_for_chord(KeyChord::plain('z')), None);
    }

    #[test]
    fn global_status_hint_tokens_derive_from_the_registry() {
        let tokens = global_status_hint_tokens();
        let expected: Vec<&str> = ACTION_REGISTRY
            .iter()
            .filter(|spec| {
                matches!(
                    spec.staging,
                    ActionStaging::Global(GlobalAction::OpenHelp | GlobalAction::Quit)
                )
            })
            .map(|spec| spec.hint_token)
            .collect();
        assert_eq!(tokens, expected);
        assert_eq!(tokens, ["? help", "q quit"]);
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
                            for awaiting in [false, true] {
                                let ctx = ActionContext {
                                    lane: *lane,
                                    admission_policy: admission,
                                    acceptance_policy: AcceptancePolicy::AiThenHuman,
                                    has_driver_handoff: handoff,
                                    awaits_scope_override: awaiting,
                                    ready_work_item_count: 1,
                                    surface,
                                };
                                if (spec.availability)(&ctx) {
                                    staged_somewhere |= stage_action(spec, &ctx).is_some();
                                }
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
            awaits_scope_override: false,
            ready_work_item_count: 1,
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
            awaits_scope_override: false,
            ready_work_item_count: 1,
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
        // The scope override needs the awaiting-an-override dimension, not the
        // lane alone.
        assert!(!valve_is_available(
            PendingValve::SetWorkflowScopeOverride,
            &ready_drill
        ));
        let awaiting_override = ActionContext {
            awaits_scope_override: true,
            ..ready_drill
        };
        assert!(valve_is_available(
            PendingValve::SetWorkflowScopeOverride,
            &awaiting_override
        ));
        // THE DEFECT THIS CATCHES, on real producer output: the override is
        // gated on the DEDICATED signal and NOT on the driver-handoff /
        // factory-safety dimension. An item marked factory-unsafe is refused
        // by the dispatcher's FIRST arm, which runs before the override label
        // is consulted — so offering the override there would advertise an
        // action that provably cannot clear the refusal. Deriving one from the
        // other is the mistake; this pins that they are independent.
        let host_only_ready = ActionContext {
            has_driver_handoff: true,
            awaits_scope_override: false,
            ..ready_drill
        };
        assert!(!valve_is_available(
            PendingValve::SetWorkflowScopeOverride,
            &host_only_ready
        ));
    }
}
