# Proposed Changes

This directory holds in-flight proposed changes to the
specification. Each file is named `<topic>.md` and contains
one or more `## Proposal: <name>` sections with target
specification files, summary, motivation, and proposed
changes (prose or unified diff). Files are processed by
`livespec revise` in creation-time order (YAML `created_at`
front-matter field) and moved into
`../history/vNNN/proposed_changes/` when revised. After a
successful `revise`, this directory is empty.

This README is tracked so the directory exists on a clean
checkout (git does not track empty directories); its presence
keeps `livespec doctor` green, since the doctor requires
`proposed_changes/` to be present. It is ignored by the
revise lifecycle, which only processes `<topic>.md` files.
