---
topic: out-of-band-edit-2026-07-12t07-04-33z
author: livespec-doctor
created_at: 2026-07-12T07:04:33Z
---

## Proposal: out-of-band-edit-2026-07-12t07-04-33z

doctor detected drift between HEAD-active spec content and the
HEAD-history-vN snapshot; this auto-backfill records the active
state as the new canonical version.

### Proposed Changes

```diff
--- history/vN/contracts.md
+++ active/contracts.md
@@ -322,6 +322,13 @@
 call Fabro, Beads, LiveSpec, Dispatcher, or GitHub directly. Work-item state
 enters the console ONLY through the orchestrator-CLI port: no console code --
 adapter, application, or UI -- invokes `bd` or reads the Beads tenant directly.
+When a console run needs orchestrator-owned backing CLIs, it MUST resolve and
+validate the orchestrator plugin entry points before invoking them: explicit
+per-program overrides win, then an explicit plugin-root override, then the
+selected repo checkout's `.claude-plugin/scripts/bin/`, then the installed
+Claude plugin cache; a malformed selected plugin root fails loudly, while an
+absent plugin degrades through named not-observed findings rather than
+fabricating source state.
 
 ```mermaid
 flowchart TB
```
