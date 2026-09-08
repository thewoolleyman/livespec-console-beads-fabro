# TUI UX rubric — the report every dogfood pass writes

Maintainer ruling 2026-09-08: the console is being dogfooded at the real TUI while fixes land through the factory, and every pass that touches the TUI must leave behind a usability report on the work item it exercised. The reports feed a parallel track, the TUI UX improvements epic (label `track:tui-ux`, one of this plan's exit criteria): when a dogfooded item closes, a Fable-model agent reviews its report and files improvement items under that epic, which are dispatched as they are filed and reported on at least every two hours.

## Dimensions (grade each 1–5, 5 best, with one sentence of evidence quoting the screen)

1. **Intuitiveness** — could a first-time operator guess the next key without help?
2. **Keystrokes to goal** — count the keys from the Attention view to completing the task under test; note the minimum you can imagine.
3. **Responsiveness** — time from keypress to visible change; stalls, input dropped under refresh, blank frames.
4. **Readability** — contrast, alignment, truncation, density, whether the important field is visible at 105 and 211 columns.
5. **Simplicity** — how many concepts the screen asks the operator to hold; redundant chrome; noise rows.
6. **Understandability** — do labels, hints, dialogs and error text say what will happen and why something failed?
7. **Discoverability** — are available actions shown where they apply, and only there?
8. **Feedback** — does every action produce a visible, honest outcome (success, failure with cause, in progress)?
9. **Consistency** — same keys mean the same thing in every view; layout and wording match across views and the Help overlay.
10. **Error recovery** — can a mistake be cancelled or undone; is the way back obvious (Escape, q)?
11. **Help** — does `?` answer the question the operator has at that moment?

## Report format (a comment on the dogfooded work item, first line exactly `tui-ux-report`)

    tui-ux-report
    item: <work item id>   build: <sha>   pane: <cols>x<rows>   date: <UTC>
    task under test: <one line>
    keystrokes: <observed> (minimum imaginable: <n>)
    intuitiveness: <1-5> — <evidence>
    responsiveness: <1-5> — <evidence>
    readability: <1-5> — <evidence>
    simplicity: <1-5> — <evidence>
    understandability: <1-5> — <evidence>
    discoverability: <1-5> — <evidence>
    feedback: <1-5> — <evidence>
    consistency: <1-5> — <evidence>
    error-recovery: <1-5> — <evidence>
    help: <1-5> — <evidence>
    top-3 improvements: 1. <what, why, which dimension>  2. ...  3. ...

## How the track consumes a report

When the dogfooded item closes, the plan session spawns a Fable-model agent with the report and the captures; it files one item per improvement worth the work under the TUI UX epic (label `track:tui-ux`, description naming the source item and dimensions, gradeable acceptance criteria in the field), skips duplicates against the epic's open children, and the plan session dispatches them. The two-hourly UX report lists items filed, items landed, and the re-graded dimensions after landing.
