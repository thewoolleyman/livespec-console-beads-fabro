#!/usr/bin/env bash
# emit-build-telemetry — emit one build-telemetry OTLP span to Honeycomb.
#
# Shared helper for the CI, factory, and local build-telemetry emitters.
# All emitters call this script to emit a span conforming to the shared
# attribute scheme in
# plan/optimize-console-builds/telemetry-attribute-scheme.md.
#
# Required environment variables:
#   HONEYCOMB_BUILD_INGEST_KEY  — write-only Honeycomb ingest key
#   BUILD_ENV                   — local | factory | ci
#   BUILD_PHASE                 — fetch | compile | test | fuzz | link
#   BUILD_REPO                  — owner/name
#   BUILD_GIT_COMMIT_SHA        — full 40-char commit SHA
#   BUILD_TOOLCHAIN_VER         — Rust toolchain version, e.g. 1.92.0
#   BUILD_SPAN_NAME             — span name, e.g. build.check-nextest
#   BUILD_START_NANO            — span start time, Unix nanoseconds
#   BUILD_END_NANO              — span end time, Unix nanoseconds
#
# Optional environment variables:
#   BUILD_TRACE_ID              — 32 hex chars; generated randomly when absent
#   BUILD_PARENT_SPAN_ID        — 16 hex chars; omitted from payload when absent
#   BUILD_CACHE_TIER            — e.g. none | registry | target
#   BUILD_CACHE_HIT             — true | false
#   BUILD_EMIT_FAIL_SOFT        — set to 1 for fail-soft (local use)
#   HONEYCOMB_BUILD_ENDPOINT    — override default api.honeycomb.io endpoint
set -euo pipefail

: "${HONEYCOMB_BUILD_INGEST_KEY:?HONEYCOMB_BUILD_INGEST_KEY required}"
: "${BUILD_ENV:?BUILD_ENV required}"
: "${BUILD_PHASE:?BUILD_PHASE required}"
: "${BUILD_REPO:?BUILD_REPO required}"
: "${BUILD_GIT_COMMIT_SHA:?BUILD_GIT_COMMIT_SHA required}"
: "${BUILD_TOOLCHAIN_VER:?BUILD_TOOLCHAIN_VER required}"
: "${BUILD_SPAN_NAME:?BUILD_SPAN_NAME required}"
: "${BUILD_START_NANO:?BUILD_START_NANO required}"
: "${BUILD_END_NANO:?BUILD_END_NANO required}"

DATASET="github-ci"
NAMESPACE="livespec-family"
ENDPOINT="${HONEYCOMB_BUILD_ENDPOINT:-https://api.honeycomb.io/v1/traces}"
SCOPE_NAME="livespec.build-telemetry"
SCOPE_VERSION="1.0.0"

rand_hex() { od -An -N "$1" -tx1 /dev/urandom | tr -d ' \n'; }

trace_id="${BUILD_TRACE_ID:-$(rand_hex 16)}"
span_id="$(rand_hex 8)"

attrs_json="$(jq -nc \
  --arg env "$BUILD_ENV" \
  --arg phase "$BUILD_PHASE" \
  --arg repo "$BUILD_REPO" \
  --arg sha "$BUILD_GIT_COMMIT_SHA" \
  --arg toolchain "$BUILD_TOOLCHAIN_VER" '
  [
    {key:"build.env",value:{stringValue:$env}},
    {key:"build.phase",value:{stringValue:$phase}},
    {key:"repo",value:{stringValue:$repo}},
    {key:"git.commit.sha",value:{stringValue:$sha}},
    {key:"toolchain.version",value:{stringValue:$toolchain}}
  ]')"

if [ -n "${BUILD_CACHE_TIER:-}" ]; then
  attrs_json="$(jq -c \
    --arg tier "$BUILD_CACHE_TIER" \
    '. + [{key:"build.cache.tier",value:{stringValue:$tier}}]' \
    <<<"$attrs_json")"
fi

if [ -n "${BUILD_CACHE_HIT:-}" ]; then
  if [ "$BUILD_CACHE_HIT" = "true" ]; then hit_bool="true"; else hit_bool="false"; fi
  attrs_json="$(jq -c \
    --argjson hit "$hit_bool" \
    '. + [{key:"build.cache.hit",value:{boolValue:$hit}}]' \
    <<<"$attrs_json")"
fi

parent_arg="${BUILD_PARENT_SPAN_ID:-}"
span="$(jq -nc \
  --arg trace "$trace_id" --arg span "$span_id" \
  --arg parent "$parent_arg" \
  --arg name "$BUILD_SPAN_NAME" \
  --arg start "$BUILD_START_NANO" --arg end "$BUILD_END_NANO" \
  --argjson attrs "$attrs_json" '
  {traceId:$trace, spanId:$span, name:$name, kind:1,
   startTimeUnixNano:$start, endTimeUnixNano:$end,
   attributes:$attrs, status:{code:1}}
  | if $parent != "" then . + {parentSpanId:$parent} else . end')"

payload_file="$(mktemp)"
jq -nc \
  --arg svc "$DATASET" --arg ns "$NAMESPACE" \
  --arg scope "$SCOPE_NAME" --arg ver "$SCOPE_VERSION" \
  --argjson span "$span" '
  {resourceSpans:[{
    resource:{attributes:[
      {key:"service.name",value:{stringValue:$svc}},
      {key:"service.namespace",value:{stringValue:$ns}}
    ]},
    scopeSpans:[{
      scope:{name:$scope,version:$ver},
      spans:[$span]
    }]
  }]}' > "$payload_file"

KEY="$HONEYCOMB_BUILD_INGEST_KEY"
resp_file="$(mktemp)"
http="$(curl -sS -o "$resp_file" -w '%{http_code}' "$ENDPOINT" \
  -H "x-honeycomb-team: $KEY" -H "Content-Type: application/json" \
  --data-binary @"$payload_file" || echo 000)"
rejected="$(jq -r '.partialSuccess.rejectedSpans // 0' "$resp_file" 2>/dev/null || echo unknown)"

echo "Honeycomb build-telemetry: HTTP=$http rejected=$rejected dataset=$DATASET env=$BUILD_ENV phase=$BUILD_PHASE span=$span_id"
if [ "$http" != "200" ] || [ "$rejected" != "0" ]; then
  if [ "${BUILD_EMIT_FAIL_SOFT:-0}" = "1" ]; then
    echo "build-telemetry emission failed (HTTP=$http rejected=$rejected) — non-fatal (fail-soft mode)." >&2
    exit 0
  fi
  echo "::error::build-telemetry emission to Honeycomb FAILED (HTTP=$http rejected=$rejected)." >&2
  cat "$resp_file" >&2 || true
  exit 1
fi
echo "build-telemetry span emitted: $BUILD_SPAN_NAME (env=$BUILD_ENV phase=$BUILD_PHASE trace=$trace_id)"
