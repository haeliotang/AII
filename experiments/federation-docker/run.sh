#!/usr/bin/env bash
# Two-container network-partition preview (M12.6 AI preview; see PREREG.md).
# Requires a running Docker daemon. Writes raw/ snapshots and REPORT.md.
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
root="$(cd "$here/../.." && pwd)"
image=poie-federation-docker
net=poie-fed
raw="$here/raw"
log="$raw/orchestration.log"

cleanup() {
  docker rm -f poie-site-a poie-site-b >/dev/null 2>&1 || true
  docker network rm "$net" >/dev/null 2>&1 || true
}
trap cleanup EXIT

note() { echo "$(date -u +%H:%M:%S) $*" | tee -a "$log"; }

status() { docker exec "poie-$1" cat /tmp/poie-status.json; }

snapshot() {
  printf '[%s,\n%s]\n' "$(status site-a)" "$(status site-b)" > "$raw/$1.json"
  note "snapshot $1: $(tr -d '\n' < "$raw/$1.json")"
}

# wait_for <seconds> <grep pattern>: until both sites' status matches.
wait_for() {
  local deadline=$(( $(date +%s) + $1 ))
  while [ "$(date +%s)" -le "$deadline" ]; do
    if status site-a | grep -q "$2" && status site-b | grep -q "$2"; then return 0; fi
    sleep 0.25
  done
  note "timeout waiting for $2"
  return 0
}

cleanup
rm -rf "$raw" && mkdir -p "$raw"
note "docker $(docker version --format '{{.Server.Version}}')"
docker build -q -f "$here/Dockerfile" -t "$image" "$root" | tee -a "$log"
docker network create "$net" >/dev/null
for pair in "site-a site-b" "site-b site-a"; do
  set -- $pair
  docker run -d --name "poie-$1" --hostname "$1" --network "$net" --network-alias "$1" \
    "$image" node --site "$1" --peer-site "$2" --peer "$2:7000" --listen 7000 >/dev/null
done
note "containers started"

# D0: connectivity baseline.
wait_for 15 '"successes":[1-9]'
snapshot D0

# D1: partition by detaching site-b from the bridge network.
docker network disconnect "$net" poie-site-b
note "partition: site-b disconnected"
snapshot D1-before
sleep 3
snapshot D1-after

# D2: both sites write while partitioned.
docker exec poie-site-a touch /tmp/poie-go
docker exec poie-site-b touch /tmp/poie-go
note "go: local writes requested on both sites"
sleep 2
snapshot D2

# D3: reconnect and converge.
docker network connect --alias site-b "$net" poie-site-b
note "reconnect: site-b attached"
wait_for 15 '"records":2,"reservations":2,"leases":2'
snapshot D3

# D4: three more partition/reconnect rounds.
for round in 1 2 3; do
  docker network disconnect "$net" poie-site-b
  note "round $round: partition"
  sleep 2
  docker network connect --alias site-b "$net" poie-site-b
  note "round $round: reconnect"
  # Require at least one fresh successful exchange after reconnect.
  before_a=$(status site-a | sed -E 's/.*"successes":([0-9]+).*/\1/')
  before_b=$(status site-b | sed -E 's/.*"successes":([0-9]+).*/\1/')
  deadline=$(( $(date +%s) + 15 ))
  while [ "$(date +%s)" -le "$deadline" ]; do
    now_a=$(status site-a | sed -E 's/.*"successes":([0-9]+).*/\1/')
    now_b=$(status site-b | sed -E 's/.*"successes":([0-9]+).*/\1/')
    [ "$now_a" -gt "$before_a" ] && [ "$now_b" -gt "$before_b" ] && break
    sleep 0.25
  done
  snapshot "D4-r$round"
done

docker logs poie-site-a > "$raw/site-a.log" 2>&1 || true
docker logs poie-site-b > "$raw/site-b.log" 2>&1 || true
docker run --rm -v "$here:/exp" "$image" report --raw /exp/raw --write /exp
