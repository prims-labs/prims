#!/bin/bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
LOG_DIR="$ROOT_DIR/logs/local_cluster"
PIDS=""

mkdir -p "$LOG_DIR"
rm -f "$LOG_DIR"/node*.log

cleanup() {
  for pid in $PIDS; do
    if kill -0 "$pid" 2>/dev/null; then
      kill "$pid" 2>/dev/null || true
    fi
  done
  wait 2>/dev/null || true
}

trap cleanup EXIT INT TERM

run_node() {
  local name="$1"
  local listen_address="$2"
  local seed_nodes="$3"
  local publish_message="${4:-}"
  local log_file="$LOG_DIR/${name}.log"

  (
    cd "$ROOT_DIR"
    export RUST_LOG=info
    export PRIMS_LISTEN_ADDRESS="$listen_address"
    export PRIMS_SEED_NODES="$seed_nodes"

    if [[ -n "$publish_message" ]]; then
      export PRIMS_PUBLISH_MESSAGE="$publish_message"
    else
      unset PRIMS_PUBLISH_MESSAGE || true
    fi

    cargo run --bin prims >"$log_file" 2>&1
  ) &

  PIDS="$PIDS $!"
  echo "$name démarré (pid $!) -> $log_file"
}

NODE1="/ip4/127.0.0.1/tcp/7001"
NODE2="/ip4/127.0.0.1/tcp/7002"
NODE3="/ip4/127.0.0.1/tcp/7003"

cd "$ROOT_DIR"

echo "Compilation du nœud réseau..."
cargo build --bin prims

run_node "node1" "$NODE1" ""
sleep 3

run_node "node2" "$NODE2" "$NODE1"
sleep 3

run_node "node3" "$NODE3" "$NODE1,$NODE2" "hello-prims-cluster"
sleep 8

echo
echo "===== Extrait des logs ====="
for file in "$LOG_DIR"/node*.log; do
  echo
  echo "----- $(basename "$file") -----"
  tail -n 20 "$file" || true
done

echo
echo "Cluster local lancé."
echo "Vérifie dans les logs la découverte des pairs et la propagation du message 'hello-prims-cluster'."
echo "Appuie sur Ctrl+C pour arrêter les 3 nœuds."
while true; do
  sleep 1
done
