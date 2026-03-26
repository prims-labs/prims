#!/bin/bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
LOG_DIR="$ROOT_DIR/logs/dos_test"
NODE_LOG="$LOG_DIR/node_dos.log"
PORT="${1:-7001}"
ATTEMPTS="${2:-60}"
NODE_PID=""

mkdir -p "$LOG_DIR"
rm -f "$NODE_LOG" "$LOG_DIR"/success.count "$LOG_DIR"/failure.count
touch "$LOG_DIR"/success.count "$LOG_DIR"/failure.count

cleanup() {
  if [[ -n "$NODE_PID" ]] && kill -0 "$NODE_PID" 2>/dev/null; then
    kill "$NODE_PID" 2>/dev/null || true
    wait "$NODE_PID" 2>/dev/null || true
  fi
}
trap cleanup EXIT INT TERM

if lsof -nP -iTCP:"$PORT" -sTCP:LISTEN >/dev/null 2>&1; then
  echo "Le port $PORT est déjà utilisé. Libère-le avant ce test."
  exit 1
fi

cd "$ROOT_DIR"

echo "Compilation du nœud réseau..."
cargo build --bin prims >/dev/null

echo "Démarrage du nœud cible sur le port $PORT..."
(
  export RUST_LOG=info
  export PRIMS_LISTEN_ADDRESS="/ip4/127.0.0.1/tcp/$PORT"
  export PRIMS_SEED_NODES=""
  unset PRIMS_PUBLISH_MESSAGE || true
  cargo run --bin prims >"$NODE_LOG" 2>&1
) &
NODE_PID="$!"

sleep 4

echo "Envoi de $ATTEMPTS tentatives de connexion TCP vers 127.0.0.1:$PORT ..."
for i in $(seq 1 "$ATTEMPTS"); do
  if nc -w 1 -z 127.0.0.1 "$PORT" >/dev/null 2>&1; then
    echo 1 >> "$LOG_DIR/success.count"
  else
    echo 1 >> "$LOG_DIR/failure.count"
  fi
done

sleep 3

SUCCESS_COUNT="$(wc -l < "$LOG_DIR/success.count" | tr -d ' ')"
FAILURE_COUNT="$(wc -l < "$LOG_DIR/failure.count" | tr -d ' ')"

echo
echo "===== Résumé du test DoS ====="
echo "Port cible : $PORT"
echo "Tentatives demandées : $ATTEMPTS"
echo "Connexions TCP réussies : $SUCCESS_COUNT"
echo "Connexions TCP échouées : $FAILURE_COUNT"

echo
echo "===== Extrait du log du nœud ====="
grep -E "Listening on|IncomingConnection|IncomingConnectionError|Connection established|OutgoingConnectionError|Behaviour event|Swarm event" "$NODE_LOG" | tail -n 40 || true

echo
echo "Test DoS terminé. Arrêt propre du nœud cible."
echo "Log complet : $NODE_LOG"
