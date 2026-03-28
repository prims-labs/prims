#!/bin/bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BIN_PATH="${1:-$ROOT_DIR/target/release/prims}"

LISTEN_ADDRESS="${PRIMS_LISTEN_ADDRESS:-/ip4/0.0.0.0/tcp/7001}"
EXTERNAL_ADDRESS="${PRIMS_EXTERNAL_ADDRESS:-}"
SEED_NODES="${PRIMS_SEED_NODES:-}"
NETWORK_SECRET_KEY_FILE="${PRIMS_NETWORK_SECRET_KEY_FILE:-}"
DB_PATH="${PRIMS_DB_PATH:-$HOME/prims_data/rocksdb}"
RPC_ADDRESS="${PRIMS_RPC_ADDRESS:-127.0.0.1:7002}"

if [[ ! -x "$BIN_PATH" ]]; then
  echo "Binaire introuvable ou non exécutable : $BIN_PATH"
  echo "Compile d abord avec : cargo build --release --bin prims"
  exit 1
fi

mkdir -p "$(dirname "$DB_PATH")"

if [[ -n "$NETWORK_SECRET_KEY_FILE" && ! -f "$NETWORK_SECRET_KEY_FILE" ]]; then
  echo "Fichier de clé réseau introuvable : $NETWORK_SECRET_KEY_FILE"
  exit 1
fi

echo "=== Configuration nœud VPS ==="
echo "Binaire : $BIN_PATH"
echo "Listen address : $LISTEN_ADDRESS"
echo "External address : ${EXTERNAL_ADDRESS:-none}"
echo "Seed nodes : ${SEED_NODES:-[]}"
echo "DB path : $DB_PATH"
echo "RPC address : $RPC_ADDRESS"
if [[ -n "$NETWORK_SECRET_KEY_FILE" ]]; then
  echo "Network secret key file : $NETWORK_SECRET_KEY_FILE"
else
  echo "Network secret key file : none (identité réseau éphémère)"
fi

export PRIMS_LISTEN_ADDRESS="$LISTEN_ADDRESS"
export PRIMS_SEED_NODES="$SEED_NODES"
export PRIMS_DB_PATH="$DB_PATH"
export PRIMS_RPC_ADDRESS="$RPC_ADDRESS"

if [[ -n "$EXTERNAL_ADDRESS" ]]; then
  export PRIMS_EXTERNAL_ADDRESS="$EXTERNAL_ADDRESS"
fi

if [[ -n "$NETWORK_SECRET_KEY_FILE" ]]; then
  export PRIMS_NETWORK_SECRET_KEY_FILE="$NETWORK_SECRET_KEY_FILE"
fi

exec "$BIN_PATH"
