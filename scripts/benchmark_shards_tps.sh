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

REMOTE_SEEDS_RAW="${PRIMS_BENCH_REMOTE_SEEDS:-}"
SKIP_BUILD="${PRIMS_BENCH_SKIP_BUILD:-0}"

CLIENT_PIDS=""

cleanup() {
  for pid in $CLIENT_PIDS; do
    if kill -0 "$pid" 2>/dev/null; then
      kill "$pid" 2>/dev/null || true
      wait "$pid" 2>/dev/null || true
    fi
  done

  if [[ -z "$REMOTE_SEEDS_RAW" ]]; then
    docker compose down --remove-orphans >/dev/null 2>&1 || true
  fi
}
trap cleanup EXIT INT TERM

if [[ "$MAX_SHARDS" -lt 1 || "$MAX_SHARDS" -gt 3 ]]; then
  echo "MAX_SHARDS doit être compris entre 1 et 3."
  exit 1
fi

mkdir -p "$RUN_DIR"

echo "scenario_shards,mode,services_or_seeds,clients_per_shard,tx_per_client,total_requested,published_count,dispatched_count,failed_clients,elapsed_secs,published_tps,dispatched_tps,detail_1,detail_2,detail_3" > "$RESULTS_FILE"

cd "$ROOT_DIR"

if [[ "$SKIP_BUILD" != "1" ]]; then
  echo "Compilation du CLI de charge..."
  cargo build --release --bin prims-cli >/dev/null
fi

REMOTE_MODE=0
REMOTE_SEEDS=()
if [[ -n "$REMOTE_SEEDS_RAW" ]]; then
  IFS=',' read -r -a REMOTE_SEEDS <<< "$REMOTE_SEEDS_RAW"
  REMOTE_MODE=1
  if [[ "${#REMOTE_SEEDS[@]}" -lt "$MAX_SHARDS" ]]; then
    echo "PRIMS_BENCH_REMOTE_SEEDS doit contenir au moins $MAX_SHARDS seed(s) pour ce benchmark."
    exit 1
  fi
else
  if [[ "$SKIP_BUILD" != "1" ]]; then
    echo "Construction des images Docker..."
    docker compose build >/dev/null
  else
    echo "Mode Docker local : build des images ignoré (PRIMS_BENCH_SKIP_BUILD=1)."
  fi
fi

for shard_count in $(seq 1 "$MAX_SHARDS"); do
  echo
  echo "===== Benchmark scénario ${shard_count} shard(s) ====="

  services=()
  ports=()
  seeds_for_scenario=()

  if [[ "$REMOTE_MODE" -eq 1 ]]; then
    for idx in $(seq 1 "$shard_count"); do
      seeds_for_scenario+=("${REMOTE_SEEDS[$((idx - 1))]}")
    done
  else
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
  fi

  SCENARIO_DIR="$RUN_DIR/shards_${shard_count}"
  mkdir -p "$SCENARIO_DIR"

  if [[ "$REMOTE_MODE" -eq 0 ]]; then
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
  else
    printf '%s\n' "${seeds_for_scenario[@]}" > "$SCENARIO_DIR/remote_seeds.txt"
  fi

  STARTED_AT_MS="$(python3 -c 'import time; print(int(time.time() * 1000))')"

  CLIENT_PIDS=""
  CLIENT_LOG_INDEX=0
  TOTAL_CLIENTS=$((shard_count * CLIENTS_PER_SHARD))
  TOTAL_REQUESTED=$((TOTAL_CLIENTS * TX_PER_CLIENT))

  if [[ "$REMOTE_MODE" -eq 1 ]]; then
    for seed in "${seeds_for_scenario[@]}"; do
      for _ in $(seq 1 "$CLIENTS_PER_SHARD"); do
        CLIENT_LOG_INDEX=$((CLIENT_LOG_INDEX + 1))
        START_NONCE=$(( (CLIENT_LOG_INDEX - 1) * TX_PER_CLIENT + 1 ))
        CLIENT_LOG="$SCENARIO_DIR/client_${CLIENT_LOG_INDEX}.log"

        (
          cd "$ROOT_DIR"
          PRIMS_REMOTE_SEED_NODE="$seed" \
          PRIMS_LOAD_SKIP_BUILD=1 \
          bash scripts/test_transaction_load.sh 7001 1 "$TX_PER_CLIENT" "$AMOUNT" \
            > "$CLIENT_LOG" 2>&1
        ) &
        CLIENT_PIDS="$CLIENT_PIDS $!"
      done
    done
  else
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
  fi

  FAILED_CLIENTS=0
  for pid in $CLIENT_PIDS; do
    if ! wait "$pid"; then
      FAILED_CLIENTS=$((FAILED_CLIENTS + 1))
    fi
  done
  CLIENT_PIDS=""

  ENDED_AT_MS="$(python3 -c 'import time; print(int(time.time() * 1000))')"
  ELAPSED_MS=$((ENDED_AT_MS - STARTED_AT_MS))
  ELAPSED_SECS="$(awk -v ms="$ELAPSED_MS" 'BEGIN { if (ms <= 0) print "0.001"; else printf "%.3f", ms / 1000 }')"

  if [[ "$REMOTE_MODE" -eq 1 ]]; then
    PUBLISHED_COUNT="$(awk '/Transactions publiées par les clients :/ {sum += $NF} END {print sum+0}' "$SCENARIO_DIR"/client_*.log)"
    DISPATCHED_COUNT="n/a"
    PUBLISHED_TPS="$(awk -v published="$PUBLISHED_COUNT" -v elapsed="$ELAPSED_SECS" 'BEGIN { if (elapsed <= 0) print "0.00"; else printf "%.2f", published / elapsed }')"
    DISPATCHED_TPS="n/a"
    DETAIL_1="${seeds_for_scenario[0]:-}"
    DETAIL_2="${seeds_for_scenario[1]:-}"
    DETAIL_3="${seeds_for_scenario[2]:-}"
    SERVICES_OR_SEEDS="$(printf '%s;' "${seeds_for_scenario[@]}")"
    SERVICES_OR_SEEDS="${SERVICES_OR_SEEDS%;}"
  else
    docker compose logs --no-color --since "$SCENARIO_STARTED_AT_ISO" "${services[@]}" > "$SCENARIO_DIR/compose.final.log" || true

    PUBLISHED_COUNT="$(awk '/Published transaction nonce/ {count++} END {print count+0}' "$SCENARIO_DIR"/client_*.log)"
    DISPATCHED_COUNT="$(awk '/Dispatched transaction/ {count++} END {print count+0}' "$SCENARIO_DIR/compose.final.log")"

    DETAIL_1="$(awk '/^shard1-1[[:space:]]+\|/ && /Dispatched transaction/ {count++} END {print count+0}' "$SCENARIO_DIR/compose.final.log")"
    DETAIL_2="$(awk '/^shard2-1[[:space:]]+\|/ && /Dispatched transaction/ {count++} END {print count+0}' "$SCENARIO_DIR/compose.final.log")"
    DETAIL_3="$(awk '/^shard3-1[[:space:]]+\|/ && /Dispatched transaction/ {count++} END {print count+0}' "$SCENARIO_DIR/compose.final.log")"

    PUBLISHED_TPS="$(awk -v published="$PUBLISHED_COUNT" -v elapsed="$ELAPSED_SECS" 'BEGIN { if (elapsed <= 0) print "0.00"; else printf "%.2f", published / elapsed }')"
    DISPATCHED_TPS="$(awk -v dispatched="$DISPATCHED_COUNT" -v elapsed="$ELAPSED_SECS" 'BEGIN { if (elapsed <= 0) print "0.00"; else printf "%.2f", dispatched / elapsed }')"

    SERVICES_OR_SEEDS="$(printf '%s;' "${services[@]}")"
    SERVICES_OR_SEEDS="${SERVICES_OR_SEEDS%;}"

    docker compose down --remove-orphans >/dev/null 2>&1 || true
  fi

  printf '%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s\n' \
    "$shard_count" \
    "$([[ "$REMOTE_MODE" -eq 1 ]] && echo remote || echo docker)" \
    "$SERVICES_OR_SEEDS" \
    "$CLIENTS_PER_SHARD" \
    "$TX_PER_CLIENT" \
    "$TOTAL_REQUESTED" \
    "$PUBLISHED_COUNT" \
    "$DISPATCHED_COUNT" \
    "$FAILED_CLIENTS" \
    "$ELAPSED_SECS" \
    "$PUBLISHED_TPS" \
    "$DISPATCHED_TPS" \
    "$DETAIL_1" \
    "$DETAIL_2" \
    "$DETAIL_3" \
    >> "$RESULTS_FILE"

  echo "Scénario ${shard_count} shard(s) : published=$PUBLISHED_COUNT / dispatched=$DISPATCHED_COUNT / published_tps=$PUBLISHED_TPS / dispatched_tps=$DISPATCHED_TPS"
done

echo
echo "===== Résultats benchmark multi-shards ====="
cat "$RESULTS_FILE"
echo
echo "Fichier résultats : $RESULTS_FILE"
