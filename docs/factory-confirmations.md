# Factory Confirmations

This file records narrow, dated operational confirmations that exist to prove
factory behavior rather than product-facing console behavior.

## 2026-07-23 - python-rust-agent sandbox dispatch

Work-item `livespec-console-beads-fabro-x9o` was created as a throwaway
factory confirmation for `livespec-dev-tooling-a46`: prove that the
Fabro dispatch path for this repository can host a live run in the
`ghcr.io/thewoolleyman/livespec-fabro-sandbox:python-rust-agent-*` consumer
image shape and carry a minimal docs-only change through the normal PR,
review, merge, and post-merge janitor path.

The confirmation acceptance evidence is the dispatch observation itself: the
Fabro run id, the exact sandbox image tag, review result, merge result, and
janitor result are recorded in the work-item journal.
