#!/usr/bin/env bash
# Export this CI run's per-job timings to Honeycomb as OTLP/HTTP-JSON spans,
# and FAIL the job if Honeycomb does not accept them.
#
# This is a CLOSED-LOOP self-verification, not an external monitor: a CI run
# deterministically knows its own telemetry should exist, so the run that
# produced the data confirms Honeycomb received it. If the ingest key is
# missing/revoked, the network is down, or the payload is rejected, this exits
# non-zero -> the job goes red -> the existing CI-failure notification fires.
# That makes a broken telemetry pipeline impossible to die silently.
#
# Wired to run on push(master)/merge_group only (never PRs) by default; set
# EXPORT_FAIL_SOFT=1 in the workflow for pull_request events to enable PR-run
# emission without the closed-loop fail property. Emits one `ci.run` root span
# + one `ci.job.<name>` child per completed job. Span/trace ids are derived
# from the GitHub run/job integer ids (deterministic, unique, valid hex).
#
# After emitting the job-level spans, the script scans each critical-path job's
# steps for steps named "Phase: compile", "Phase: test", or "Phase: fuzz" and
# emits build-telemetry spans conforming to the shared attribute scheme
# (plan/optimize-console-builds/telemetry-attribute-scheme.md):
#   build.env=ci, build.phase, repo, git.commit.sha, toolchain.version
# These phase steps are provided by the companion workflow diff (item 577xhi);
# until that diff lands the phase-span loop is a no-op (0 matching steps).
# Phase span failures are fail-hard on push, fail-soft when EXPORT_FAIL_SOFT=1.
#
# Required environment:
#   REPO    - owner/name (e.g. thewoolleyman/livespec-console-beads-fabro)
#   RUN_ID  - this workflow run's id (github.run_id)
#   GH_TOKEN - gh auth token with actions:read (the workflow's GITHUB_TOKEN)
#   HONEYCOMB_GITHUB_CI_INGEST_KEY_LIVESPEC - write-only Honeycomb ingest key
#
# Optional environment:
#   EXPORT_FAIL_SOFT - set to 1 for PR runs: ingest/phase-span failures are
#     logged as warnings rather than aborting the job.  Master-path (push)
#     leaves this unset to retain the closed-loop fail-hard property.
set -euo pipefail

: "${REPO:?REPO required}"
: "${RUN_ID:?RUN_ID required}"
KEY="${HONEYCOMB_GITHUB_CI_INGEST_KEY_LIVESPEC:?HONEYCOMB_GITHUB_CI_INGEST_KEY_LIVESPEC required}"

DATASET="github-ci"            # OTLP service.name -> Honeycomb dataset
NAMESPACE="livespec-family"
ENDPOINT="https://api.honeycomb.io/v1/traces"
SCOPE_NAME="livespec.github-ci-export"
SCOPE_VERSION="1.0.0"

iso_to_nanos() { date -u -d "$1" +%s%N; }   # GNU date (ubuntu runners)
hex32() { printf '%032x' "$1"; }            # 16-byte trace id
hex16() { printf '%016x' "$1"; }            # 8-byte span id

run_json="$(gh run view "$RUN_ID" --repo "$REPO" \
  --json databaseId,headSha,headBranch,event,displayTitle,conclusion,createdAt,startedAt,updatedAt,jobs)"

trace_id="$(hex32 "$RUN_ID")"
run_span_id="$(hex16 "$RUN_ID")"
run_start="$(iso_to_nanos "$(jq -r '.startedAt // .createdAt' <<<"$run_json")")"
run_end="$(iso_to_nanos "$(jq -r '.updatedAt' <<<"$run_json")")"
run_concl="$(jq -r '.conclusion // ""' <<<"$run_json")"
run_code=2; [ "$run_concl" = "success" ] && run_code=1

# `$run_json` carries the whole `jobs` array and MUST reach jq on stdin, never
# as a `--argjson` value. Passing it on argv died with "jq: Argument list too
# long" (E2BIG, exit 126) on runs 30052264761 and 30052356388, reddening master
# and with it every dark-factory dispatch gated on `check-master-ci-green`.
#
# The exact runner-side threshold was NOT reproducible off-runner: the failing
# payload is ~84 KB (63 jobs) against a ~83 KB green run (62 jobs), both far
# under MAX_ARG_STRLEN, and the same call succeeds locally on the exact failing
# payload — so step env size or a runner-image change is likely also in play.
# What is certain is that this value grows without bound as the job count grows
# and it is the only such argv entry here. stdin has no argv limit, so routing
# it there removes the whole class rather than moving the threshold.
run_span="$(jq -c \
  --arg trace "$trace_id" --arg span "$run_span_id" \
  --arg start "$run_start" --arg end "$run_end" \
  --arg repo "$REPO" --argjson run_id "$RUN_ID" --argjson code "$run_code" '
  {traceId:$trace, spanId:$span, name:"ci.run", kind:1,
   startTimeUnixNano:$start, endTimeUnixNano:$end,
   attributes:[
     {key:"repo",value:{stringValue:$repo}},
     {key:"ci.run_id",value:{intValue:($run_id|tostring)}},
     {key:"ci.conclusion",value:{stringValue:(.conclusion // "")}},
     {key:"ci.title",value:{stringValue:(.displayTitle // "")}},
     {key:"git.commit.sha",value:{stringValue:(.headSha // "")}},
     {key:"git.branch",value:{stringValue:(.headBranch // "")}},
     {key:"ci.event",value:{stringValue:(.event // "")}}
   ],
   status:{code:$code}}' <<<"$run_json")"

job_spans="[]"
while IFS=$'\t' read -r jid jname jconcl jstart_iso jend_iso; do
  [ -n "$jstart_iso" ] && [ "$jstart_iso" != "null" ] || continue
  [ -n "$jend_iso" ] && [ "$jend_iso" != "null" ] || continue
  jspan_id="$(hex16 "$jid")"
  jstart="$(iso_to_nanos "$jstart_iso")"
  jend="$(iso_to_nanos "$jend_iso")"
  jcode=2; [ "$jconcl" = "success" ] && jcode=1
  span="$(jq -nc \
    --arg trace "$trace_id" --arg span "$jspan_id" --arg parent "$run_span_id" \
    --arg name "ci.job.$jname" --arg start "$jstart" --arg end "$jend" \
    --arg repo "$REPO" --argjson run_id "$RUN_ID" \
    --arg jname "$jname" --arg jconcl "$jconcl" --argjson code "$jcode" '
    {traceId:$trace, spanId:$span, parentSpanId:$parent, name:$name, kind:1,
     startTimeUnixNano:$start, endTimeUnixNano:$end,
     attributes:[
       {key:"repo",value:{stringValue:$repo}},
       {key:"ci.run_id",value:{intValue:($run_id|tostring)}},
       {key:"ci.job.name",value:{stringValue:$jname}},
       {key:"ci.job.conclusion",value:{stringValue:$jconcl}}
     ],
     status:{code:$code}}')"
  job_spans="$(jq -c ". + [$span]" <<<"$job_spans")"
done < <(jq -r '.jobs[] | [.databaseId, .name, (.conclusion // ""), (.startedAt // ""), (.completedAt // "")] | @tsv' <<<"$run_json")

# Same argv-limit class as the run span above: `$job_spans` grows with the job
# count, so it goes on stdin. `$run_span` stays a `--argjson` value because it
# is a single fixed-shape span (seven attributes) and cannot grow with the run.
payload_file="$(mktemp)"
jq -c \
  --argjson run "$run_span" \
  --arg svc "$DATASET" --arg ns "$NAMESPACE" \
  --arg scope "$SCOPE_NAME" --arg ver "$SCOPE_VERSION" '
  {resourceSpans:[{
     resource:{attributes:[
       {key:"service.name",value:{stringValue:$svc}},
       {key:"service.namespace",value:{stringValue:$ns}}
     ]},
     scopeSpans:[{
       scope:{name:$scope, version:$ver},
       spans:([$run] + .)
     }]
   }]}' <<<"$job_spans" > "$payload_file"

span_count="$(jq '.resourceSpans[0].scopeSpans[0].spans | length' "$payload_file")"
resp_file="$(mktemp)"
http="$(curl -sS -o "$resp_file" -w '%{http_code}' "$ENDPOINT" \
  -H "x-honeycomb-team: $KEY" -H "Content-Type: application/json" \
  --data-binary @"$payload_file" || echo 000)"
rejected="$(jq -r '.partialSuccess.rejectedSpans // 0' "$resp_file" 2>/dev/null || echo unknown)"

echo "Honeycomb ingest: HTTP=$http spans=$span_count rejected=$rejected dataset=$DATASET trace=$trace_id"
main_ok=0
if [ "$http" = "200" ] && [ "$rejected" = "0" ]; then
  main_ok=1
else
  echo "--- Honeycomb response ---" >&2
  cat "$resp_file" >&2 || true
  if [ "${EXPORT_FAIL_SOFT:-0}" = "1" ]; then
    echo "CI telemetry main ingest failed (HTTP=$http rejected=$rejected) — non-fatal (PR fail-soft mode)." >&2
  else
    echo "::error::CI telemetry export to Honeycomb FAILED (HTTP=$http rejected=$rejected). The telemetry pipeline is broken; fix it rather than ignore this." >&2
    exit 1
  fi
fi

# --- Phase-level spans for critical-path jobs ---
# For each of the four critical-path jobs, scan the job's steps for steps whose
# name starts with "Phase: compile", "Phase: test", or "Phase: fuzz".  Each
# matching step produces one build-telemetry OTLP span (via emit-build-telemetry.sh)
# carrying the shared scheme attributes: build.env=ci, build.phase, repo,
# git.commit.sha, toolchain.version.
#
# The naming convention is provided by the companion workflow diff (item 577xhi);
# until that diff lands this loop emits 0 spans (no matching step names exist).
# Phase-span failures are fail-hard on push (EXPORT_FAIL_SOFT unset) and
# fail-soft on PR runs (EXPORT_FAIL_SOFT=1), matching the main ingest contract.
commit_sha="$(jq -r '.headSha // ""' <<<"$run_json")"
toolchain_ver="$(grep '^channel' rust-toolchain.toml 2>/dev/null \
  | sed 's/channel *= *"\(.*\)"/\1/' || echo unknown)"
phase_span_count=0

while IFS=$'\t' read -r phj_id phj_name; do
  case "$phj_name" in
    check-nextest|check-coverage|check-clippy|check-fuzz) ;;
    *) continue ;;
  esac

  while IFS=$'\t' read -r step_name step_start_iso step_end_iso; do
    [ -n "$step_start_iso" ] && [ "$step_start_iso" != "null" ] || continue
    [ -n "$step_end_iso" ] && [ "$step_end_iso" != "null" ] || continue

    build_phase=""
    case "$step_name" in
      "Phase: compile"*) build_phase="compile" ;;
      "Phase: test"*)    build_phase="test" ;;
      "Phase: fuzz"*)    build_phase="fuzz" ;;
      *) continue ;;
    esac

    BUILD_EMIT_FAIL_SOFT="${EXPORT_FAIL_SOFT:-0}" \
    HONEYCOMB_BUILD_INGEST_KEY="$KEY" \
    BUILD_ENV="ci" \
    BUILD_PHASE="$build_phase" \
    BUILD_REPO="$REPO" \
    BUILD_GIT_COMMIT_SHA="$commit_sha" \
    BUILD_TOOLCHAIN_VER="$toolchain_ver" \
    BUILD_SPAN_NAME="build.${phj_name}.${build_phase}" \
    BUILD_START_NANO="$(iso_to_nanos "$step_start_iso")" \
    BUILD_END_NANO="$(iso_to_nanos "$step_end_iso")" \
      bash "$(dirname "$0")/emit-build-telemetry.sh"

    phase_span_count=$((phase_span_count + 1))
  done < <(jq -r --argjson jid "$phj_id" \
    '.jobs[] | select(.databaseId == $jid) | (.steps // [])[] | [.name, (.startedAt // ""), (.completedAt // "")] | @tsv' \
    <<<"$run_json")
done < <(jq -r '.jobs[] | [.databaseId, .name] | @tsv' <<<"$run_json")

if [ "$phase_span_count" -gt 0 ]; then
  echo "Phase span emission: $phase_span_count build-telemetry phase spans emitted."
else
  echo "Phase span emission: 0 phase spans (companion workflow diff not yet applied — expected)."
fi

if [ "$main_ok" = "1" ]; then
  echo "CI telemetry exported and confirmed received by Honeycomb ($span_count spans, trace $trace_id)."
fi
