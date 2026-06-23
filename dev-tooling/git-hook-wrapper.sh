#!/bin/sh
# livespec commit-refuse hook — refuses commits/pushes at the primary checkout,
# and delegates to mise-managed lefthook everywhere else.

primary_path="$(git config --get livespec.primaryPath || true)"
toplevel="$(git rev-parse --show-toplevel)"
if [ -n "$primary_path" ] && [ "$toplevel" = "$primary_path" ]; then
  echo "livespec: refusing commit/push at primary checkout ($toplevel); use a worktree" >&2
  exit 1
fi

HOOK_NAME="$(basename "$0")"
unset GIT_DIR GIT_INDEX_FILE GIT_WORK_TREE GIT_PREFIX
exec mise exec lefthook -- lefthook run --no-auto-install "$HOOK_NAME" "$@"
