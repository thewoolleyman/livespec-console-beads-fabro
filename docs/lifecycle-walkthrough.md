# Lifecycle walkthrough, key by key

This walks one work-item from waiting-for-a-human all the way to shipped, one
keystroke at a time, naming what you should see on screen after each one.

Every step below is executed against the real TUI by
`tmux_tui_e2e_lifecycle_walkthrough_two_repos`
(`crates/console-cli/tests/tmux_tui_e2e.rs`), for two different repositories, on
every CI run. If this page and the binary disagree, that test fails — so what
you read here is what the console does.

## Who drives what

The operator does not drive every transition, and the TUI is deliberate about
this. The `move` action refuses `acceptance`, `done`, and `pending-approval` —
the **ship-guard**. Those lanes are reached by the factory finishing work or by
a human valve, never by an operator relocating an item.

| Transition | Driven by |
|---|---|
| `backlog` → `ready` / `active` / `blocked` | operator — `s` move-to-status |
| `pending-approval` → `ready` | operator — **`p` approve** |
| `ready` → `active` → … | the factory |
| `active` → `acceptance` | the factory |
| `acceptance` → `done` | operator — **`c` accept** |
| any → `blocked` | operator — `r` reject, or the factory |

So the two moments the factory genuinely needs you are **admission** (approve)
and **ship** (accept). This walkthrough crosses both.

## Before you start

Launch the console against the repository you want to operate:

```bash
just tui
```

or, from an installed binary, under the credential wrapper — see
[Installing](installing.md).

The walkthrough assumes one work-item sitting in `pending-approval`. If your
board has none, the same keys work on any item; only the starting lane differs.

---

## Step 1 — See what is waiting

The console opens on the **Attention** view, which lists exactly the items
waiting on a human. You should see:

```
fleet: livespec | mode: tui | repo: <your-repo> | view: Attention | attention: 1
```

and, in the middle pane:

```
> Pending approval
```

The **Detail** pane on the right names the item — `Repo:`, `Work item:`,
`Fabro run:`, `Attach:`, and a `Timeline:` of the events that produced it.

> If `attention: 0` and the list is empty, the factory does not need you. That
> is the normal resting state.

## Step 2 — Move focus into the list

Focus starts on the left **Views** menu. Press:

**`Enter`**

The middle pane's title becomes `Attention [focus]`, and the Status line at the
bottom changes to offer the per-item keys:

```
up/down move | enter open | p/c/r approve/accept/reject | m/n set-admission/acceptance | ? help | q quit
```

Use `↑` / `↓` here if you have more than one item; the walkthrough acts on the
selected row.

## Step 3 — Open the approve valve

Press:

**`p`**

A modal titled `Valve` opens on top of the screen:

```
Approve work-item
Target: <your-work-item-id>
Enter to confirm | Esc to cancel
```

The Status line switches to the modal's own hints:

```
up/down change | enter confirm | esc cancel
```

Check the `Target:` line names the item you meant. `Esc` backs out with nothing
sent.

## Step 4 — Confirm the approval

Press:

**`Enter`**

The console issues `approve:<work-item-id>` through the orchestrator's `drive`
API — it never writes the ledger itself — and the item is admitted to `ready`.

The inbox empties: the header returns to `attention: 0` and the Detail pane
reads `No attention item selected`.

## Step 5 — Let the factory work

Nothing to press. The item is now the factory's: it moves `ready` → `active`,
does the work, and parks the result in `acceptance` for a human to judge.

The console polls its sources every 2 seconds, so the change appears on its own.
When it does, the item is back in your inbox:

```
> Acceptance review
```

and the header counts it again — `attention: 1`.

> This is the step you cannot drive from the keyboard, and the ship-guard is
> why: `s` will not offer you `acceptance` as a target.

## Step 6 — Open the accept valve

With the item selected, press:

**`c`**

The `Valve` modal opens again, this time:

```
Accept work-item
Target: <your-work-item-id>
Enter to confirm | Esc to cancel
```

## Step 7 — Ship it

Press:

**`Enter`**

The console issues `accept:<work-item-id>`. The item moves to `done`, and the
inbox empties again — `attention: 0`, `No attention item selected`.

`done` is terminal: it is reached *only* by `accept`, and a shipped item offers
no onward move.

## Step 8 — Confirm on the board

Press:

**`Esc`** to step back to the Views menu, then **`↓` `↓`** to reach **Lanes**.

The board shows the item where you left it:

```
> backlog (0)
  pending-approval (0)
  ready (0)
  active (0)
  acceptance (0)
  blocked (0)
  done (1)
```

The item has left `pending-approval` and is counted in `done`.

---

## If something needs rework instead

At either valve, `r` rejects rather than accepts. The modal warns
`dangerous / use with caution`, and `↑` / `↓` choose the mode:

- **`rework`** — send it back for another pass
- **`regroom`** — the slice was wrong; it needs re-cutting

Rejecting moves the item to `blocked`. From a drilled-in lane you can then use
`s` to drive it back to `ready` or `backlog` once the blocker clears.

## Driving an item that is not in your inbox

The Attention view only lists items *waiting on a human*. To act on any other
item, go through **Lanes**:

1. `↓` `↓` from the Views menu to reach **Lanes**, then `Enter` to focus the
   board.
2. `↑` / `↓` to a lane, then `Enter` to drill in. The pane title becomes
   `Lane: <name>`.
3. `↑` / `↓` to select an individual work-item. Only now do the per-item keys
   act — on the lane *overview* they are inert, and the Status line does not
   offer them.
4. `Enter` opens the selected item's full record; `Esc` steps back.
5. **`s`** offers the statuses this item may legally be driven to, given the
   lane it is in. `↑` / `↓` change the target, `Enter` confirms.

`s` is the one valve that works *only* in a drilled-in lane: it needs the item's
current lane to know which targets are legal, so it is inert in the Attention
view.

## Related

- [Detailed usage](detailed-usage.md) — a section per pane, the full keybinding
  reference, and the dispatcher settings.
- [Overview and quick start](overview-quickstart.md) — the shortest path to
  acting on an item.
