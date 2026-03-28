#!/bin/bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
LOG_DIR="$ROOT_DIR/logs/transaction_load_test"
NODE_LOG="$LOG_DIR/node.log"
PORT="${1:-7001}"
CLIENTS="${2:-4}"
TX_PER_CLIENT="${3:-1000}"
AMOUNT="${4:-42}"

REMOTE_SEED_NODE="${PRIMS_REMOTE_SEED_NODE:-}"
SKIP_BUILD="${PRIMS_LOAD_SKIP_BUILD:-0}"

NODE_PID=""
CLIENT_PIDS=""
DISPATCH_STABLE_CHECKS=0
LAST_DISPATCHED_COUNT=-1

mkdir -p "$LOG_DIR"
rm -f "$NODE_LOG" "$LOG_DIR"/client_*.log

cleanup() {
  for pid in $CLIENT_PIDS; do
    if kill -0 "$pid" 2>/dev/null; then
      kill "$pid" 2>/dev/null || true
      wait "$pid" 2>/dev/null || true
    fi
  done

  if [[ -n "$NODE_PID" ]] && kill -0 "$NODE_PID" 2>/dev/null; then
    kill "$NODE_PID" 2>/dev/null || true
    wait "$NODE_PID" 2>/dev/null || true
  fi
}
trap cleanup EXIT INT TERM

cd "$ROOT_DIR"

if [[ "$SKIP_BUILD" != "1" ]]; then
  echo "Compilation du nœud et du CLI..."
  cargo build --release --bin prims --bin prims-cli >/dev/null
fi

if [[ -n "$REMOTE_SEED_NODE" ]]; then
  SEED_NODE="$REMOTE_SEED_NODE"
  echo "Mode distant activé."
  echo "Seed node distant : $SEED_NODE"
else
  if lsof -nP -iTCP:"$PORT" -sTCP:LISTEN >/dev/null 2>&1; then
    echo "Le port $PORT est déjà utilisé. Libère-le avant ce test."
    exit 1
  fi

  SEED_NODE="/ip4/127.0.0.1/tcp/$PORT"

  echo "Démarrage du nœud cible sur le port $PORT..."
  (
    export RUST_LOG=info
    export PRIMS_LISTEN_ADDRESS="$SEED_NODE"
    export PRIMS_SEED_NODES=""
    unset PRIMS_PUBLISH_MESSAGE || true
    target/release/prims >"$NODE_LOG" 2>&1
  ) &
  NODE_PID="$!"

  sleep 5
fi

STARTED_AT_MS="$(python3 -c 'import time; print(int(time.time() * 1000))')"

echo "Lancement de $CLIENTS clients concurrents avec $TX_PER_CLIENT transactions chacun..."
for i in $(seq 1 "$CLIENTS"); do
  CLIENT_LOG="$LOG_DIR/client_${i}.log"
  START_NONCE=$(( (i - 1) * TX_PER_CLIENT + 1 ))

  (
    cd "$ROOT_DIR"
    target/release/prims-cli flood \
      --count "$TX_PER_CLIENT" \
      --start-nonce "$START_NONCE" \
      --amount "$AMOUNT" \
      --listen-address "/ip4/127.0.0.1/tcp/0" \
      --seed-nodes "$SEED_NODE" \
      >"$CLIENT_LOG" 2>&1
  ) &
  CLIENT_PIDS="$CLIENT_PIDS $!"
done

FAILED_CLIENTS=0
for pid in $CLIENT_PIDS; do
  if ! wait "$pid"; then
    FAILED_CLIENTS=$((FAILED_CLIENTS + 1))
  fi
done
CLIENT_PIDS=""

TOTAL_REQUESTED=$((CLIENTS * TX_PER_CLIENT))

if [[ -n "$REMOTE_SEED_NODE" ]]; then
  DISPATCHED_COUNT="n/a"
  TPS="n/a"
else
  while true; do
    DISPATCHED_COUNT="$(awk '/Dispatched transaction/ {count++} END {print count+0}' "$NODE_LOG")"

    if [[ "$DISPATCHED_COUNT" -ge "$TOTAL_REQUESTED" ]]; then
      break
    fi

    if [[ "$DISPATCHED_COUNT" -eq "$LAST_DISPATCHED_COUNT" ]]; then
      DISPATCH_STABLE_CHECKS=$((DISPATCH_STABLE_CHECKS + 1))
    else
      DISPATCH_STABLE_CHECKS=0
      LAST_DISPATCHED_COUNT="$DISPATCHED_COUNT"
    fi

    if [[ "$DISPATCH_STABLE_CHECKS" -ge 2 ]]; then
      break
    fi

    sleep 0.2
  done

  TPS_PLACEHOLDER_END=1
fi

ENDED_AT_MS="$(python3 -c 'import time; print(int(time.time() * 1000))')"
ELAPSED_MS=$((ENDED_AT_MS - STARTED_AT_MS))
ELAPSED_SECS="$(awk -v ms="$ELAPSED_MS" 'BEGIN { if (ms <= 0) print "0.001"; else printf "%.3f", ms / 1000 }')"
PUBLISHED_COUNT="$(awk '/Published transaction nonce/ {count++} END {print count+0}' "$LOG_DIR"/client_*.log)"

if [[ -z "$REMOTE_SEED_NODE" ]]; then
  DISPATCHED_COUNT="$(awk '/Dispatched transaction/ {count++} END {print count+0}' "$NODE_LOG")"
  TPS="$(awk -v dispatched="$DISPATCHED_COUNT" -v elapsed="$ELAPSED_SECS" 'BEGIN { if (elapsed <= 0) print "0.00"; else printf "%.2f", dispatched / elapsed }')"
fi

echo
echo "===== Résumé du test de charge transactions ====="
if [[ -n "$REMOTE_SEED_NODE" ]]; then
  echo "Mode : distant"
  echo "Seed node distant : $SEED_NODE"
else
  echo "Mode : local"
  echo "Port cible : $PORT"
fi
echo "Clients concurrents : $CLIENTS"
echo "Transactions par client : $TX_PER_CLIENT"
echo "Transactions demandées : $TOTAL_REQUESTED"
echo "Transactions publiées par les clients : $PUBLISHED_COUNT"
echo "Transactions dispatchées dans la mempool : $DISPATCHED_COUNT"
echo "Clients en échec : $FAILED_CLIENTS"
echo "Durée totale (s) : $ELAPSED_SECS"
echo "TPS observé : $TPS"

if [[ -z "$REMOTE_SEED_NODE" ]]; then
  echo
  echo "===== Extrait du log du nœud ====="
  grep -E "Listening on|Connection established|Dispatched transaction|Failed to deserialize" "$NODE_LOG" | tail -n 40 || true

  echo
  echo "Test de charge terminé. Arrêt propre du nœud cible."
  echo "Log nœud : $NODE_LOG"
else
  echo
  echo "Mode distant : pas de log nœud local à analyser."
fi

echo "Logs clients : $LOG_DIR/client_*.log"
