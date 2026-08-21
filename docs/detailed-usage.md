# Detailed usage

## The screen

A single screen in three horizontal bands:

- a **header** band (3 rows), titled `LiveSpec Console`;
- a **body** band, which fills the remaining height;
- a **Status** band (3 rows) along the bottom.

The body is split into columns. Most views show three — a **Views**
navigation list, a **content** pane, and a **Detail** pane. The **Lanes** view
shows only two, since the lane board uses the full width and has no Detail
pane.

Four panes can hold focus: **Views**, **content**, **Detail**, and the
**header**. The Status band is never focusable. The focused pane's title
carries a `[focus]` tag.

### Moving focus

`Tab` cycles focus forward, `Shift-Tab` backward. The ring is:

```
Views → content → Detail → header → Views
```

On the Lanes view, which has no Detail pane, the ring is
`Views → content → header → Views`.

`←` and `→` also walk focus, but only across the *body* panes — they never
reach the header, and they stop at the ends rather than wrapping. Neither
`Tab` nor the arrows change which **view** you are looking at; use `↑`/`↓` on
the Views menu for that.

Both `Tab` and `Shift-Tab` are inert while any overlay is open.

## Panes

### Header pane

The header renders a single status line:

```
fleet: livespec | mode: tui | repo: <repo> | view: <view> | attention: <N>
```

with a `sources: <N> unavailable (…)` segment appended whenever a backing
source could not be observed, so a blind cockpit is never mistaken for an idle
factory. A source appears there only while its **most recent** observation
failed; as soon as it is observed again it drops off, and a source that has
never failed never appears. When nothing is unavailable the segment is absent
entirely — there is no phantom `sources: 0`.

On a narrow terminal the line degrades one step at a time, re-measuring after
each, and stopping as soon as it fits:

1. drop `mode: tui`
2. drop `fleet: livespec`
3. shorten the sources segment to `(<first>, +N more)`
4. shorten it further to a bare `sources: N unavailable`
5. drop `view: <view>`
6. drop `attention: <N>`

`repo` is never dropped, and the unavailable **count** survives every step —
the two things you cannot afford to lose. Fields are dropped whole; none is
ever truncated mid-value.

**The header is focusable and scrolls.** Give it focus and it renders the
full, un-degraded line, which you pan with `←` and `→` — 8 columns per press,
clamped at both ends. This is how you read content that a narrow viewport
clipped. On blur the scroll resets, so the header always returns to its
left-justified default.

### Views pane

The navigation list. Six views in this order: **Attention**, **Spec**,
**Lanes**, **Events**, **Repos**, **Settings**. The active view is marked with
a leading `>`.

`↑`/`↓` move between views. `Enter` or `→` moves focus into the content pane.
Changing view resets the Detail pane's scroll position.

### Attention pane

The list of items waiting on a human. Each row is the item's title, with its
available action in brackets where it has one:

```
> Pending approval [approve]
> Acceptance review [accept]
> Blocked: <reason>
```

Selecting a row fills the **Detail** pane with `Repo:`, `Work item:`, and
`Fabro run:` lines, and a `Timeline:` of the events that produced it. With
nothing selected the Detail pane reads `No attention item selected`.

What those lines hold depends on which kind of row it is — and the kind is
**not** decided by which source emitted it. The inbox merges work-item rows with
needs-attention rows, and where both describe the same work-item the
needs-attention one is dropped in favour of the richer work-item row.

- **Work-item rows** — a work-item resting on a human step: `pending-approval`
  under `manual` admission, `blocked`/`needs-human`, or `acceptance` whose
  policy carries a human leg. `Fabro run:` names the run when one is observed
  and reads `-` otherwise, and `Attach:` appears **only when a Fabro run is
  attached** — when a human-gate observation matches the row's repo and
  work-item. An item that has not reached Fabro has neither, so a
  `pending-approval` row normally shows `Fabro run: -` and **no** `Attach:`
  line. A `Timeline:` of the events that produced it follows.
- **Needs-attention rows** — everything the needs-attention program reports that
  no work-item row already claims: plan threads, hygiene findings, spec-revise
  items, and any `valve:<verb>:<id>` row whose work-item is not itself flagged.
  `Fabro run:` is **always** `-`, and `Attach:` is **always** present, carrying
  that row's handoff command (for example `open:plan/<topic>`). `Work item:`
  shows the row's work-item if its source reference names one, otherwise the
  path, otherwise the row id — so a plan-thread row displays a *path* there.

The practical consequence runs both ways. `Attach:` present is **not** evidence
a Fabro run exists — on a needs-attention row it is just the handoff command,
with `Fabro run: -` directly above it. And `Attach:` absent on a valve row is
**not** a missing observation — it means that row is the richer work-item
projection, which shows the line only for a real Fabro attach.

`Enter` on a row opens **the source work-item's full record**, the same modal a
drilled-in lane row opens. `Esc` closes it. A row that names no work-item —
a plan thread or a hygiene finding, for example — has nothing to open, so
`Enter` is inert there and the Status line stops offering it.

`/` opens search, which filters this list — a lowercased substring match over
the summary, id, kind, repo, work-item, and path. An empty query matches
everything.

#### Rows that appear and disappear on their own

The inbox is not driven only by you. The console reads the orchestrator's
published Dispatcher journal, and each **auto-disposition** it finds there
moves the inbox without any keystroke:

| Journal disposition | Effect on the inbox | Governed by |
|---|---|---|
| `auto-approve` | the item's `approve` valve row **is resolved and leaves** | `auto_approve_ready` |
| `ai-auto-accept` | the item's `accept` valve row **is resolved and leaves** | `acceptance_mode` |
| `ai-fail-auto-rework` | as above — resolved on the `accept` valve | `acceptance_mode` |
| `ship-on-cap` | as above — resolved on the `accept` valve | `merge_on_review_cap` |
| `cap-exceeded-escalation` | a needs-human row **appears** | `review_fix_cap` |

So a row vanishing from Attention is normal: the factory disposed of it under
a setting, and the console reflected that. Equally, a row you never triggered
may appear — that is the orchestrator escalating something no setting was
allowed to auto-dispose. Only `cap-exceeded-escalation` arrives this way;
the other four only ever remove.

The console **observes** these; it never re-derives them. The journal names
both the disposition and the settings that governed it, and the console takes
both verbatim — which is why the table's right-hand column is the
orchestrator's claim, not the console's inference. Reflection is idempotent,
so a decision already reflected on an earlier run is skipped rather than
double-counted, and a journal line the console cannot parse — malformed, or
carrying a disposition outside the five above — is **skipped silently** rather
than surfaced as a phantom row.

The governing settings themselves are described under
[Dispatcher settings](#dispatcher-settings); this section is only about what
they do to the inbox.

### Spec pane

Two rows:

```
LiveSpec next snapshots: <count>
Revise required: <count>
```

Spec lifecycle status is projected from LiveSpec adapter observations — the
console does not read the spec tree itself, so these counts reflect what the
adapter last observed rather than the state of your working copy.

Revise-required events stay visible here until they are resolved. A non-zero
`Revise required` means the spec has pending proposed changes awaiting a
`/livespec:revise` pass; it will not clear on its own.

### Lanes pane

The seven-lane board, in pipeline order: `backlog`, `pending-approval`,
`ready`, `active`, `acceptance`, `blocked`, `done`.

The overview shows each lane as a header with its count, followed by up to
three preview items:

```
> ready (4)
    - <id> [<status>]  <title>(<reason>)
```

`Enter` on a lane drills into it, giving one row per work-item:

```
> <id>  rank <rank>  [<status>]  <title>  repo <repo>(<reason>)
```

Both rows carry the work-item's **title**; an item whose record has none reads
`(untitled)`. In the drill-in row the repo sits *after* the title, prefixed
with a literal `repo `, so that a narrow pane clips the repo rather than the
title. `(<reason>)` is a suffix present only when the lane supplies one.

`↑`/`↓` then select an individual item, and `Esc` steps back out to the
overview before it steps back to the Views menu. An empty lane reads
`No work-items in this lane`.

With an item selected, `Enter` opens its **full record** — every field the
orchestrator emits for that item, not just the row's summary. `Esc` closes it.
`Enter` therefore means two different things in this view: *drill into a lane*
on the overview, and *open the selected item's record* once you are inside one.
The Status line says which one applies.

Drilling in matters: the `s` **move-to-status** valve needs the item's current
lane to know which statuses it may be driven to, so it works **only** on a
drilled-in lane selection, and is inert in the Attention view.

### Events pane

```
Stored events: <count>
Latest event    <type> from <source> on <stream_id>
```

The latest-event row reads `none` when the store is empty.

The event log is the canonical source for projections. Every pane in the
console is derived from it, which is why the projections can be rebuilt from
the log alone, and why `backfill` and `snapshot` are meaningful operations —
see [CLI options](cli-options.md).

### Repos pane

```
Repos observed: <count>
```

with the sorted, de-duplicated repo ids in the Detail pane.

### Settings pane

Titled `Settings > Dispatcher settings`. See
[Dispatcher settings](#dispatcher-settings) below.

### Detail pane

Shows the expansion of whatever the content pane has selected. It wraps long
lines, scrolls with `↑`/`↓` when focused, and shows a scrollbar on its right
border only when the content actually overflows. With no projection rows it
reads `No projection rows`.

### Status pane

The bottom band, titled `Status`. It shows the shortcuts that apply **right
now**: the hints change as focus moves between panes and as overlays open and
close, so it always describes the current context rather than a fixed summary.

| Context | Hint |
|---|---|
| Header focused | `left/right scroll \| esc/tab leave \| ? help \| q quit` |
| Attention, backlog work-item selected | `up/down move \| enter open \| m set-admission \| g merge cap \| f fix cap \| n set-acceptance \| k rework cap \| ? help \| q quit` |
| Attention, pending-approval work-item selected | `up/down move \| enter open \| p approve \| r reject \| m set-admission \| g merge cap \| f fix cap \| n set-acceptance \| k rework cap \| ? help \| q quit` |
| Attention, dispatcher-admitted pending-approval work-item selected | `up/down move \| enter open \| r reject \| m set-admission \| g merge cap \| f fix cap \| n set-acceptance \| k rework cap \| ? help \| q quit` |
| Attention, acceptance work-item selected | `up/down move \| enter open \| c accept \| r reject \| ? help \| q quit` |
| Attention, blocked work-item selected | `up/down move \| enter open \| ? help \| q quit` |
| Attention, no work-item selected | `? help \| q quit` |
| Lanes, lane overview | `up/down move \| enter drill \| ? help \| q quit` |
| Lanes, drilled into a backlog item | `up/down move \| enter item \| esc lane list \| h handoff \| s move-status \| m set-admission \| g merge cap \| f fix cap \| n set-acceptance \| k rework cap \| ? help \| q quit` |
| Lanes, drilled into a pending-approval item | `up/down move \| enter item \| esc lane list \| s move-status \| p approve \| r reject \| m set-admission \| g merge cap \| f fix cap \| n set-acceptance \| k rework cap \| ? help \| q quit` |
| Lanes, drilled into a dispatcher-admitted pending-approval item | `up/down move \| enter item \| esc lane list \| s move-status \| r reject \| m set-admission \| g merge cap \| f fix cap \| n set-acceptance \| k rework cap \| ? help \| q quit` |
| Lanes, drilled into a ready item | `up/down move \| enter item \| esc lane list \| s move-status \| g merge cap \| f fix cap \| n set-acceptance \| k rework cap \| ? help \| q quit` |
| Lanes, drilled into a factory-unsafe ready item | `up/down move \| enter item \| esc lane list \| h handoff \| s move-status \| g merge cap \| f fix cap \| n set-acceptance \| k rework cap \| ? help \| q quit` |
| Lanes, drilled into an active item | `up/down move \| enter item \| esc lane list \| n set-acceptance \| k rework cap \| ? help \| q quit` |
| Lanes, drilled into an acceptance item | `up/down move \| enter item \| esc lane list \| s move-status \| c accept \| r reject \| ? help \| q quit` |
| Lanes, drilled into a blocked item | `up/down move \| enter item \| esc lane list \| s move-status \| ? help \| q quit` |
| Lanes, drilled into a done item | `enter item \| esc lane list \| ? help \| q quit` |
| Lanes, drilled into an empty lane | `esc lane list \| ? help \| q quit` |
| Settings | `up/down move \| enter/space edit row \| ? help \| q quit` |
| Spec, Events, Repos | `up/down move \| left/right focus \| / search \| ? help \| q quit` |
| Search open | `type to search \| esc cancel` |
| Command palette open | `type a command \| esc cancel` |
| Action invoker open | `up/down select \| enter stage \| esc cancel` |
| Command modal open | `up/down select action \| enter explain \| esc cancel` |
| Valve confirm open | `up/down change \| enter confirm \| esc cancel` |
| Work-item record open | `up/down scroll \| PgUp/PgDn page \| esc close item` |
| Help open | `left/right pane \| up/down act \| PgUp/PgDn page \| esc close help` |

**The Status line never advertises a key that would do nothing.** The per-item
valves act on a *selected work-item*, so they are absent on the lane overview
(which selects a lane, not an item), in an empty drilled-in lane, and whenever
Attention has no work-item-backed row selected — an empty inbox, **or a
populated one sitting on a row that names no work-item**. `up`/`down` drop out
too when there are no rows to move over. `Enter` opens a selected work-item's
record from Attention or a drilled-in lane, so it is absent when there is no
selected work-item.

Availability is derived from the selected item's full record, not its lane
alone. `p approve` renders only while the item's **effective admission policy
is `manual`** — a dispatcher-admitted (`auto`) item omits it, because the
approve valve cannot fire there (`m set-admission` is the operator's route to
reclaim the door). `h handoff` renders only where the driver-handoff verb
applies: a drilled-in **backlog** item (groom), or a drilled-in **ready** item
that carries a factory-safety marking (implement). Any marking counts — the
factory refuses a marked item whatever the marking says, so an attended host
session is the route forward.

An open overlay's hint wins over the focused pane's. The pane hints key on the
active **view** plus what is selected in it — not on which of the body panes
holds focus — so Views, content, and Detail focus within the same view and the
same selection share a hint.

The Status band deliberately sits *below* the Help modal's bottom margin, so
its hints stay readable while the modal is open.

## Keys

### Global

| Key | Effect |
|---|---|
| `Ctrl-C` | Quit, at any time, including from inside an overlay. |
| `q` | Quit — only when no overlay is open. With search or the command palette open it types a literal `q` into the query. |
| `/` | Open search. |
| `:` | Open the command palette (`drain`, `actions`). |
| `?` | Open the Help modal. |
| `Tab` / `Shift-Tab` | Cycle focus forward / backward. Inert while an overlay is open. |

### By focus, with no overlay open

| Focus | `↑`/`↓` | `←` | `→` | `Enter` | `Esc` |
|---|---|---|---|---|---|
| Views | previous / next view | — | focus content | focus content | — |
| content | move the selection | drilled lane → overview, else focus Views | focus Detail | in Lanes: drill into a lane, or open the selected item's record once inside one; in Attention: open the selected row's work-item record; in Settings: edit the row; elsewhere: inert | as `←` |
| Detail | scroll | focus content | — | — | focus content |
| header | — | scroll left | scroll right | — | focus Views |

### Per-item valves

These act on the **selected work-item** — the selected row in **Attention**,
or the selected item in a **drilled-in lane**. They are inert when no
work-item is selected, which is the whole of the Spec, Events, Repos, and
Settings views, the lane overview (which selects a lane, not an item), and an
empty drilled-in lane.

**Not every Attention row carries a work-item.** A row is valve-eligible only
when its source reference names one — the `valve:<verb>:<work-item>` rows do.
A plan-thread row (`plan:<topic>`), a hygiene finding
(`hygiene:stale-worktree:<path>`), and a spec-revise row
(`spec:revise:<path>`) name a path or a topic instead, so the valves are inert
on them even though the inbox is populated and the row is selected. The Status
line is the reliable tell: it drops the valve keys exactly when they would do
nothing.

| Key | Valve |
|---|---|
| `p` | approve |
| `c` | accept |
| `r` | reject — warned as dangerous |
| `m` | set admission (`auto` / `manual`) |
| `n` | set acceptance (`ai-then-human` / `ai-only` / `human-only`) |
| `g` | override `merge_on_review_cap` |
| `f` | override `review_fix_cap` |
| `k` | override `acceptance_rework_cap` |
| `s` | move to a status — **drilled-in lane only** |

Each opens a confirmation modal showing the valve, the target work-item, and
where the valve takes a value, the current option with `↑`/`↓` to change it.
`Enter` confirms; `Esc` cancels. Rejection additionally prints
`dangerous / use with caution`.

The statuses `s` offers depend on the lane the item is in:

| From | May be driven to |
|---|---|
| `backlog` | ready, blocked |
| `pending-approval` | backlog, blocked |
| `ready` | backlog, blocked |
| `active` | *nothing* — active is entered by the factory |
| `acceptance` | backlog, blocked |
| `blocked` | backlog, ready — through resolve-blocked |
| `done` | *nothing* — a shipped item offers no onward move |

### Driver handoff

`h` opens the full-width **Driver Handoff** overlay only where the
driver-handoff verb applies: a drilled-in `backlog` item renders the groom
invocation, and a drilled-in `ready` item with a non-null `factory_safety`
marking renders the driver-implement invocation. The verb is suppressed
everywhere else.

The rendered command carries only the selected work-item id. It does not write
a prompt file, execute the driver, spawn a process, monitor a driver session,
or wait for one. The console is only handing the command text to the operator;
the driver session records its own work through the orchestrator surface.

Press `Enter` in the overlay to send the command to the terminal copy path;
press `Esc` to cancel. The overlay wording is normative: it says
`copy sent to terminal`, describing what the console attempted, and MUST NOT
claim an unobservable result such as `Copied!`.

### The command palette

`:` opens it. It accepts exactly three commands: `drain` and
`drain ready queue` (both drain the ready queue), and `actions`, which opens
the action invoker. Anything else is reported as an unknown command.

### The action invoker

`:` then `actions` opens a roster of EVERY registered operator action for the
current selection, in one list — including the actions that carry no hotkey at
all (today: `Set workflow scope override`, the recorded remedy a
factory-safety refusal names). An available action stages its normal confirm
modal on `Enter`; an unavailable one renders marked `(unavailable here)` and
stays inert. The roster is deliberately minimal scaffolding for the menu
shell: it exists so every action is reachable before menus land.

### When an action is refused

An operator action that fails does not fail silently. Whatever the
orchestrator's action surface emitted is captured, carried on the item's
`work_item.action.failed` event, and rendered on the item's own record — open
the record with `Enter` and its last line reads `Last action: …`:

| what the surface emitted | the line reads |
|---|---|
| a `--json` payload with `domain_error` and `summary` | `Last action: <id> refused — <domain_error>: <summary>` |
| only one of those two fields | `Last action: <id> refused — <that field>` |
| output that does not parse as that shape | `Last action: <id> failed — <raw output>` |
| nothing at all | `Last action: <id> failed (no diagnostic emitted)` |

So `refused` means the surface explained itself and the explanation is quoted;
`failed` means it did not, and you are seeing either its raw output or an honest
admission that there was none. Nothing is invented to fill the gap.

Only the LATEST failure per work-item is kept, and it is cleared the moment a
later action against that same item completes — so a refusal you have already
recovered from never lingers on the record.

## The Help modal

`?` opens it, and — this is the point — it opens **focused on the section for
the pane you were just on**. Help from the header lands on the header's
section; Help from anywhere in the body lands on the current view's section.

The modal is a window over the main screen, inset 3 characters on all four
sides. Its left column is a menu of sections; its right pane is the help text
for the selected one.

There are eight sections: **Global actions** first, then one per view
(Attention, Spec, Lanes, Events, Repos, Settings), and **Header** last.

| Key | Effect |
|---|---|
| `←` / `→` | Focus the section menu / text pane. |
| `↑` / `↓` | With the section menu focused, move between sections; with the text pane focused, scroll by one line. Section movement stops at the ends and resets the text pane to the top. |
| `PgUp` / `PgDn` | Page the text pane by the current viewport height, regardless of which help pane is focused. Vertical only — the text wraps rather than scrolling sideways. |
| `Esc` | Close. |

`Esc` is the **only** way out: `?` is inert while Help is open, so it does not
toggle. A row reading `esc to exit` is always printed along the bottom of the
modal, whichever section you are on and however far you have scrolled.

## Dispatcher settings

The factory's routine autonomy is governed by six **dispatcher policy
settings**. The orchestrator owns every one of them. The console only commands
and observes: it holds no setting state, derives every value from the
orchestrator's published read surface, and issues every write through the
`drive` API.

There is **no autonomous-mode master switch**. Each setting is an independent
dial, and turning on a dangerous one is an ordinary recorded write — there is
no type-the-repo-name ceremony.

Open the **Settings** view and press `Enter` or `Space` on a row to edit it:
a bool toggles, an enum cycles, an integer increments and wraps (1 through 9).
A setting whose non-default value lets the factory act without a human is
labelled **dangerous / use with caution** wherever it appears.

| Setting | Type | Dangerous? | Per-item override |
|---|---|---|---|
| `auto_approve_ready` | bool | **yes** — auto-approves a ready item with no human | `m` set-admission (`auto`/`manual`) |
| `merge_on_review_cap` | bool | **yes** — ships past the review cap with no sign-off | `g` (`on`/`off`/`clear`) |
| `acceptance_mode` | enum `ai-then-human` \| `ai-only` \| `human-only` | **yes** when `ai-only` — the AI auto-accepts | `n` set-acceptance |
| `review_fix_cap` | int | no | `f` (value, or `clear`) |
| `acceptance_rework_cap` | int | no | `k` (value, or `clear`) |
| `wip_cap` | int | no | **none** — a per-repo ceiling, structurally not per-item |

Every overridable setting has a per-item valve, so you can depart from the
global default on a single work-item without changing the default. Setting an
override to `clear` returns that item to inheriting the global value. Only
`wip_cap` admits no override: it is a per-repo concurrency ceiling, so a
per-item value would be meaningless.

When the console has no trustworthy read of the orchestrator's settings, the
Settings view says so — `Dispatcher settings not observed` — rather than
showing stale or invented values.
