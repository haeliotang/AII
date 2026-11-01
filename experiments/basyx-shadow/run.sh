#!/usr/bin/env bash
# BaSyx AAS read-only adapter rehearsal (M9.2) — AI rehearsal on a local container.
# NOT field equipment; does not count toward the M9.2 exit criteria.
#
# Starts a pinned BaSyx AAS Environment container, injects the pump-p1 dataset,
# and drives the runner through: read → docker stop → docker start → re-inject.
# Requires Docker. Not part of scripts/verify-all.sh or `cargo test`.
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"
IMAGE="eclipsebasyx/aas-environment:2.0.0-milestone-15"
NAME="poie-basyx-shadow-$$"
RAW="$HERE/raw"
LOG="$RAW/run.log"
ACK="$RAW/.ack"
WORK="$(mktemp -d)"
FIFO="$WORK/cmd.fifo"
SUBMODEL_ID="urn:poieweave:base-01:pump-p1:operational-data"
RUNNER_PID=""

mkdir -p "$RAW"
rm -f "$RAW"/phase-*.txt "$ACK"
: >"$LOG"
log() { echo "[$(date -u +%H:%M:%S)] $*" | tee -a "$LOG" >&2; }

cleanup() {
  exec 3>&- 2>/dev/null || true
  if [ -n "$RUNNER_PID" ] && kill -0 "$RUNNER_PID" 2>/dev/null; then kill "$RUNNER_PID" 2>/dev/null || true; fi
  docker rm -f "$NAME" >/dev/null 2>&1 || true
  rm -rf "$WORK" "$ACK"
}
trap cleanup EXIT

PORT="$(python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1",0)); print(s.getsockname()[1]); s.close()')"
BASE="http://127.0.0.1:$PORT"

wait_ready() {
  local deadline=$((SECONDS + 240))
  while [ $SECONDS -lt $deadline ]; do
    if [ "$(curl -s -o /dev/null -w '%{http_code}' "$BASE/shells" || true)" = "200" ]; then
      log "ready after start ($BASE/shells → 200)"
      return 0
    fi
    sleep 2
  done
  log "container did not become ready"; docker logs --tail 80 "$NAME" >>"$LOG" 2>&1 || true
  return 1
}

inject() {
  local observed_at="$1" code
  sed "s/__OBSERVED_AT__/$observed_at/" "$HERE/data/submodel.template.json" >"$WORK/submodel.json"
  code="$(curl -s -o "$WORK/resp" -w '%{http_code}' -X POST -H 'Content-Type: application/json' \
    --data-binary @"$WORK/submodel.json" "$BASE/submodels")"
  log "POST /submodels → $code $(head -c 300 "$WORK/resp")"
  [ "$code" = "201" ] || return 1
  code="$(curl -s -o "$WORK/resp" -w '%{http_code}' -X POST -H 'Content-Type: application/json' \
    --data-binary @"$HERE/data/aas.json" "$BASE/shells")"
  log "POST /shells → $code $(head -c 300 "$WORK/resp")"
  [ "$code" = "201" ] || return 1
}

send() { log "runner ← $*"; echo "$*" >&3; }
wait_ack() {
  local deadline=$((SECONDS + 60))
  while [ $SECONDS -lt $deadline ]; do
    if [ -f "$ACK" ] && grep -qx "$1 done" "$ACK"; then return 0; fi
    if ! kill -0 "$RUNNER_PID" 2>/dev/null; then log "runner exited before acking $1"; return 1; fi
    sleep 0.2
  done
  log "timeout waiting for runner ack: $1"; return 1
}
now_utc() { date -u +%Y-%m-%dT%H:%M:%SZ; }

log "build runner"
(cd "$ROOT" && cargo build -q -p poie-basyx-shadow)
RUNNER="$ROOT/target/debug/poie-basyx-shadow"

log "image $IMAGE ($(docker image inspect "$IMAGE" --format '{{index .RepoDigests 0}}' 2>/dev/null || echo 'not pulled yet'))"
log "docker run $NAME on 127.0.0.1:$PORT"
docker run -d --name "$NAME" -p "127.0.0.1:$PORT:8081" "$IMAGE" >>"$LOG"
log "digest $(docker image inspect "$IMAGE" --format '{{index .RepoDigests 0}}')"
wait_ready

OBSERVED_1="$(now_utc)"
inject "$OBSERVED_1" || log "initial injection failed (runner continues; C1 will record it)"

mkfifo "$FIFO"
"$RUNNER" --base-url "$BASE" --out "$HERE" --ack "$ACK" --image "$IMAGE" <"$FIFO" >>"$LOG" 2>&1 &
RUNNER_PID=$!
exec 3>"$FIFO"

phase_ok=1
send "read $OBSERVED_1" && wait_ack read || phase_ok=0

if [ $phase_ok = 1 ]; then
  log "docker stop $NAME"
  docker stop "$NAME" >>"$LOG"
  send "disconnected" && wait_ack disconnected || phase_ok=0
fi

if [ $phase_ok = 1 ]; then
  log "docker start $NAME"
  docker start "$NAME" >>"$LOG"
  wait_ready || phase_ok=0
fi
if [ $phase_ok = 1 ]; then
  send "restarted" && wait_ack restarted || phase_ok=0
fi

if [ $phase_ok = 1 ]; then
  sleep 1
  OBSERVED_2="$(now_utc)"
  if inject "$OBSERVED_2"; then
    send "reinjected $OBSERVED_2" && wait_ack reinjected || phase_ok=0
  else
    log "re-injection failed"; phase_ok=0
  fi
fi

send "finish"
exec 3>&-
status=0
wait "$RUNNER_PID" || status=$?
RUNNER_PID=""
docker logs "$NAME" >"$RAW/container.log" 2>&1 || true
log "runner exit status $status (0 = all checks PASS, 1 = some FAIL, other = runner error)"
log "report: $HERE/REPORT.md"
exit "$status"
