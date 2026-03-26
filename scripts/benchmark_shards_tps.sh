#!/bin/bash
set -euo pipefail

export LC_ALL=C
export LANG=C

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
LOG_ROOT="$ROOT_DIR/logs/benchmark_shards_tps"
RUN_ID="$(date +%Y%m%d_%H%M%S)"
RUN_DIR="$LOG_ROOT/$RUN_ID"
RESULTS_FILE="$RUN_DIR/results.csv"

MAX_SHARDS="${1:-3}"
CLIENTS_PER_SHARD="${2:-2}"
TX_PER_CLIENT="${3:-200}"
AMOUNT="${4:-42}"

CLIENT_PIDS=""

cleanup() {
  for pid in $CLIENT_PIDS; do
    if kill -0 "$pid" 2>/dev/null; then
      kill "$pid" 2>/dev/null || true
      wait "$pid" 2>/dev/null || true
    fi
  done
  docker compose down --remove-orphans >/dev/null 2>&1 || true
}
trap cleanup EXIT INT TERM

if [[ "$MAX_SHARDS" -lt 1 || "$MAX_SHARDS" -gt 3 ]]; then
  echo "MAX_SHARDS doit être compris entre 1 et 3."
  exit 1
fi

mkdir -p "$RUN_DIR"

echo "scenario_shards,services,ports,clients_per_shard,tx_per_client,total_requested,published_count,dispatched_count,failed_clients,elapsed_secs,published_tps,dispatched_tps,shard1_dispatched,shard2_dispatched,shard3_dispatched" > "$RESULTS_FILE"

cd "$ROOT_DIR"

echo "Compilation du CLI de charge..."
cargo build --release --bin prims-cli >/dev/null

echo "Construction des images Docker..."
docker compose build >/dev/null

for shard_count in $(seq 1 "$MAX_SHARDS"); do
  echo
  echo "===== Benchmark scénario ${shard_count} shard(s) ====="

  services=()
  ports=()

  if [[ "$shard_count" -ge 1 ]]; then
    services+=("shard1")
    ports+=("7001")
  fi
  if [[ "$shard_count" -ge 2 ]]; then
    services+=("shard2")
    ports+=("7002")
  fi
  if [[ "$shard_count" -ge 3 ]]; then
    services+=("shard3")
    ports+=("7003")
  fi

  SCENARIO_DIR="$RUN_DIR/shards_${shard_count}"
  mkdir -p "$SCENARIO_DIR"

  docker compose down --remove-orphans >/dev/null 2>&1 || true

  SCENARIO_STARTED_AT_ISO="$(date -u +"%Y-%m-%dT%H:%M:%SZ")"
  docker compose up -d "${services[@]}" >/dev/null

  READY_COUNT=0
  for _ in $(seq 1 20); do
    docker compose ps "${services[@]}" > "$SCENARIO_DIR/compose.ps.txt"
    docker compose logs --no-color --since "$SCENARIO_STARTED_AT_ISO" "${services[@]}" > "$SCENARIO_DIR/compose.startup.log" || true
    READY_COUNT="$(grep -c 'Listening on' "$SCENARIO_DIR/compose.startup.log" || true)"
    if [[ "$READY_COUNT" -ge "$shard_count" ]]; then
      break
    fi
    sleep 1
  done

  if [[ "$READY_COUNT" -lt "$shard_count" ]]; then
    echo "Les shards ne semblent pas prêts. Vérifie $SCENARIO_DIR/compose.startup.log"
    exit 1
  fi

  STARTED_AT_MS="$(python3 -c 'import time; print(int(time.time() * 1000))')"

  CLIENT_PIDS=""
  CLIENT_LOG_INDEX=0
  TOTAL_CLIENTS=$((shard_count * CLIENTS_PER_SHARD))
  TOTAL_REQUESTED=$((TOTAL_CLIENTS * TX_PER_CLIENT))

  for port in "${ports[@]}"; do
    for _ in $(seq 1 "$CLIENTS_PER_SHARD"); do
      CLIENT_LOG_INDEX=$((CLIENT_LOG_INDEX + 1))
      START_NONCE=$(( (CLIENT_LOG_INDEX - 1) * TX_PER_CLIENT + 1 ))
      CLIENT_LOG="$SCENARIO_DIR/client_${CLIENT_LOG_INDEX}_port_${port}.log"

      (
        cd "$ROOT_DIR"
        target/release/prims-cli flood \
          --count "$TX_PER_CLIENT" \
          --start-nonce "$START_NONCE" \
          --amount "$AMOUNT" \
          --listen-address "/ip4/127.0.0.1/tcp/0" \
          --seed-nodes "/ip4/127.0.0.1/tcp/$port" \
          > "$CLIENT_LOG" 2>&1
      ) &
      CLIENT_PIDS="$CLIENT_PIDS $!"
    done
  done

  FAILED_CLIENTS=0
  for pid in $CLIENT_PIDS; do
    if ! wait "$pid"; then
      FAILED_CLIENTS=$((FAILED_CLIENTS + 1))
    fi
  done
  CLIENT_PIDS=""

  DISPATCH_STABLE_CHECKS=0
  LAST_DISPATCHED_COUNT=-1

  while true; do
    docker compose logs --no-color --since "$SCENARIO_STARTED_AT_ISO" "${services[@]}" > "$SCENARIO_DIR/compose.current.log" || true
    DISPATCHED_COUNT="$(awk '/Dispatched transaction/ {count++} END {print count+0}' "$SCENARIO_DIR/compose.current.log")"

    if [[ "$DISPATCHED_COUNT" -ge "$TOTAL_REQUESTED" ]]; then
      break
    fi

    if [[ "$DISPATCHED_COUNT" -eq "$LAST_DISPATCHED_COUNT" ]]; then
      DISPATCH_STABLE_CHECKS=$((DISPATCH_STABLE_CHECKS + 1))
    else
      DISPATCH_STABLE_CHECKS=0
      LAST_DISPATCHED_COUNT="$DISPATCHED_COUNT"
    fi

    if [[ "$DISPATCH_STABLE_CHECKS" -ge 3 ]]; then
      break
    fi

    sleep 0.5
  done

  ENDED_AT_MS="$(python3 -c 'import time; print(int(time.time() * 1000))')"
  ELAPSED_MS=$((ENDED_AT_MS - STARTED_AT_MS))
  ELAPSED_SECS="$(awk -v ms="$ELAPSED_MS" 'BEGIN { if (ms <= 0) print "0.001"; else printf "%.3f", ms / 1000 }')"

  docker compose logs --no-color --since "$SCENARIO_STARTED_AT_ISO" "${services[@]}" > "$SCENARIO_DIR/compose.final.log" || true

  PUBLISHED_COUNT="$(awk '/Published transaction nonce/ {count++} END {print count+0}' "$SCENARIO_DIR"/client_*.log)"
  DISPATCHED_COUNT="$(awk '/Dispatched transaction/ {count++} END {print count+0}' "$SCENARIO_DIR/compose.final.log")"

  SHARD1_DISPATCHED="$(awk '/^shard1-1[[:space:]]+\|/ && /Dispatched transaction/ {count++} END {print count+0}' "$SCENARIO_DIR/compose.final.log")"
  SHARD2_DISPATCHED="$(awk '/^shard2-1[[:space:]]+\|/ && /Dispatched transaction/ {count++} END {print count+0}' "$SCENARIO_DIR/compose.final.log")"
  SHARD3_DISPATCHED="$(awk '/^shard3-1[[:space:]]+\|/ && /Dispatched transaction/ {count++} END {print count+0}' "$SCENARIO_DIR/compose.final.log")"

  PUBLISHED_TPS="$(awk -v published="$PUBLISHED_COUNT" -v elapsed="$ELAPSED_SECS" 'BEGIN { if (elapsed <= 0) print "0.00"; else printf "%.2f", published / elapsed }')"
  DISPATCHED_TPS="$(awk -v dispatched="$DISPATCHED_COUNT" -v elapsed="$ELAPSED_SECS" 'BEGIN { if (elapsed <= 0) print "0.00"; else printf "%.2f", dispatched / elapsed }')"

  SERVICES_JOINED="$(printf '%s;' "${services[@]}")"
  SERVICES_JOINED="${SERVICES_JOINED%;}"
  PORTS_JOINED="$(printf '%s;' "${ports[@]}")"
  PORTS_JOINED="${PORTS_JOINED%;}"

  printf '%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s\n' \
    "$shard_count" \
    "$SERVICES_JOINED" \
    "$PORTS_JOINED" \
    "$CLIENTS_PER_SHARD" \
    "$TX_PER_CLIENT" \
    "$TOTAL_REQUESTED" \
    "$PUBLISHED_COUNT" \
    "$DISPATCHED_COUNT" \
    "$FAILED_CLIENTS" \
    "$ELAPSED_SECS" \
    "$PUBLISHED_TPS" \
    "$DISPATCHED_TPS" \
    "$SHARD1_DISPATCHED" \
    "$SHARD2_DISPATCHED" \
    "$SHARD3_DISPATCHED" \
    >> "$RESULTS_FILE"

  echo "Scénario ${shard_count} shard(s) : published=$PUBLISHED_COUNT / dispatched=$DISPATCHED_COUNT / published_tps=$PUBLISHED_TPS / dispatched_tps=$DISPATCHED_TPS"

  docker compose down --remove-orphans >/dev/null 2>&1 || true
done

echo
echo "===== Résultats benchmark multi-shards ====="
cat "$RESULTS_FILE"
echo
echo "Fichier résultats : $RESULTS_FILE"
