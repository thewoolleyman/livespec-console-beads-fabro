#!/usr/bin/env bash
# emit-local-build-telemetry — emit a fail-soft build-telemetry span for local
# just check / pre-push aggregate runs (build.env=local).
#
# Silently skips when HONEYCOMB_BUILD_INGEST_KEY is absent (offline / keyless).
# Never fails the calling build: exits 0 in all error paths.
#
# Required environment variables (set by the calling justfile recipe):
#   BUILD_START_NANO  — span start time, Unix nanoseconds
#   BUILD_END_NANO    — span end time, Unix nanoseconds
#   BUILD_SPAN_NAME   — span name, e.g. build.just-check
#   BUILD_PHASE       — build phase: fetch | compile | test | fuzz | link
#
# Optional:
#   HONEYCOMB_BUILD_INGEST_KEY — write-only ingest key from the family env
#                                wrapper. Absent → emit is skipped silently.
set -euo pipefail

if [ -z "${HONEYCOMB_BUILD_INGEST_KEY:-}" ]; then
  echo "build-telemetry(local): no ingest key — skipping." >&2
  exit 0
fi

: "${BUILD_START_NANO:?BUILD_START_NANO required}"
: "${BUILD_END_NANO:?BUILD_END_NANO required}"
: "${BUILD_SPAN_NAME:?BUILD_SPAN_NAME required}"
: "${BUILD_PHASE:?BUILD_PHASE required}"

repo="$(git remote get-url origin 2>/dev/null \
  | sed 's|.*github\.com[:/]\(.*\)\.git$|\1|; s|.*github\.com[:/]\(.*\)$|\1|' \
  || echo unknown)"
commit_sha="$(git rev-parse HEAD 2>/dev/null || echo unknown)"
toolchain_ver="$(grep '^channel' rust-toolchain.toml 2>/dev/null \
  | sed 's/channel *= *"\(.*\)"/\1/' || echo unknown)"

HONEYCOMB_BUILD_INGEST_KEY="$HONEYCOMB_BUILD_INGEST_KEY" \
BUILD_EMIT_FAIL_SOFT=1 \
BUILD_ENV=local \
BUILD_PHASE="$BUILD_PHASE" \
BUILD_REPO="$repo" \
BUILD_GIT_COMMIT_SHA="$commit_sha" \
BUILD_TOOLCHAIN_VER="$toolchain_ver" \
BUILD_SPAN_NAME="$BUILD_SPAN_NAME" \
BUILD_START_NANO="$BUILD_START_NANO" \
BUILD_END_NANO="$BUILD_END_NANO" \
  bash "$(dirname "$0")/emit-build-telemetry.sh"
