# Tests de charge Prims

Ce document décrit les scripts de tests de charge disponibles pour Prims, en mode local Docker et en mode distant de type VPS.

## Objectif

Valider l envoi massif de transactions vers un ou plusieurs nœuds Prims, mesurer le volume publié, et préparer des essais de charge proches d un déploiement testnet réel.

## Scripts disponibles

- `scripts/test_transaction_load.sh`
- `scripts/benchmark_shards_tps.sh`

## Mode local simple

Le script `scripts/test_transaction_load.sh` peut démarrer un nœud local, lancer plusieurs clients concurrents, puis afficher un résumé.

Exemple :

```bash
bash scripts/test_transaction_load.sh 7001 4 1000 42
```

Paramètres :
- port cible local
- nombre de clients concurrents
- transactions par client
- montant par transaction

Résumé attendu :
- transactions demandées
- transactions publiées par les clients
- transactions dispatchées dans la mempool
- clients en échec
- durée totale
- TPS observé

## Mode distant de type VPS

Pour viser un seed node déjà démarré ailleurs, utiliser la variable d environnement `PRIMS_REMOTE_SEED_NODE`.

Exemple :

```bash
PRIMS_REMOTE_SEED_NODE="/ip4/203.0.113.10/tcp/7001" \
PRIMS_LOAD_SKIP_BUILD=1 \
bash scripts/test_transaction_load.sh 7001 4 1000 42
```

Comportement :
- le script ne démarre pas de nœud local
- le script lance seulement les clients de charge
- la métrique `Transactions dispatchées dans la mempool` devient `n/a`
- la métrique `TPS observé` devient `n/a` si aucun log serveur local n est analysé

Variables utiles :
- `PRIMS_REMOTE_SEED_NODE` : multiadresse du seed node distant
- `PRIMS_LOAD_SKIP_BUILD=1` : évite la recompilation locale si les binaires sont déjà prêts

## Benchmark multi-shards en mode Docker

Le script `scripts/benchmark_shards_tps.sh` peut lancer un benchmark sur 1 à 3 shards Docker locaux.

Exemple :

```bash
bash scripts/benchmark_shards_tps.sh 3 2 200 42
```

Paramètres :
- nombre maximal de shards à tester
- clients par shard
- transactions par client
- montant par transaction

Le script produit :
- un CSV de résultats
- des logs par scénario
- des métriques de publication et de dispatch

## Benchmark multi-shards en mode distant

Pour viser des seed nodes déjà déployés sur des VPS, utiliser `PRIMS_BENCH_REMOTE_SEEDS`.

Exemple avec un seul seed :

```bash
PRIMS_BENCH_REMOTE_SEEDS="/ip4/203.0.113.10/tcp/7001" \
PRIMS_BENCH_SKIP_BUILD=1 \
PRIMS_LOAD_SKIP_BUILD=1 \
bash scripts/benchmark_shards_tps.sh 1 2 1000 42
```

Exemple avec trois seeds :

```bash
PRIMS_BENCH_REMOTE_SEEDS="/ip4/203.0.113.10/tcp/7001,/ip4/203.0.113.11/tcp/7001,/ip4/203.0.113.12/tcp/7001" \
PRIMS_BENCH_SKIP_BUILD=1 \
PRIMS_LOAD_SKIP_BUILD=1 \
bash scripts/benchmark_shards_tps.sh 3 2 1000 42
```

Comportement :
- le mode devient `remote` dans le CSV
- `services_or_seeds` contient les seeds distants
- `published_count` est calculé depuis les résumés clients
- `dispatched_count` reste `n/a` tant qu aucun log serveur distant n est collecté localement

Variables utiles :
- `PRIMS_BENCH_REMOTE_SEEDS` : liste de multiadresses séparées par des virgules
- `PRIMS_BENCH_SKIP_BUILD=1` : évite compilation et rebuild Docker inutiles
- `PRIMS_LOAD_SKIP_BUILD=1` : évite recompilation côté script client

## Emplacement des résultats

Résultats typiques :
- `logs/transaction_load_test/`
- `logs/benchmark_shards_tps/<timestamp>/`

## Bonnes pratiques

- ne jamais exposer de clé privée, token ou mot de passe dans les commandes
- utiliser des nœuds de test dédiés pour les essais de charge
- éviter toute charge agressive sur une infrastructure non autorisée
- conserver les journaux et CSV pour comparer les résultats entre scénarios
- sauvegarder localement les scripts et journaux utiles après une étape importante

## Limites actuelles

- le mode distant mesure surtout les transactions publiées côté clients
- le dispatch serveur distant n est pas compté sans collecte de logs distante
- les essais VPS réels nécessiteront ensuite la préparation d hôtes distants, ports ouverts et supervision adaptée

## Préparation minimale d un nœud VPS

Le script `scripts/run_vps_node.sh` permet de lancer un nœud Prims avec les variables d environnement utiles pour un VPS.

Exemple de lancement d un seed node sur un VPS Linux avec un binaire déjà compilé :

```bash
PRIMS_LISTEN_ADDRESS="/ip4/0.0.0.0/tcp/7001" \
PRIMS_EXTERNAL_ADDRESS="/ip4/203.0.113.10/tcp/7001" \
PRIMS_SEED_NODES="" \
PRIMS_NETWORK_SECRET_KEY_FILE="$HOME/prims_secrets/network_identity.hex" \
PRIMS_DB_PATH="$HOME/prims_data/rocksdb" \
PRIMS_RPC_ADDRESS="127.0.0.1:7002" \
./prims
```

Exemple de lancement d un second nœud rejoignant le premier :

```bash
PRIMS_LISTEN_ADDRESS="/ip4/0.0.0.0/tcp/7001" \
PRIMS_EXTERNAL_ADDRESS="/ip4/203.0.113.11/tcp/7001" \
PRIMS_SEED_NODES="/ip4/203.0.113.10/tcp/7001" \
PRIMS_NETWORK_SECRET_KEY_FILE="$HOME/prims_secrets/network_identity.hex" \
PRIMS_DB_PATH="$HOME/prims_data/rocksdb" \
PRIMS_RPC_ADDRESS="127.0.0.1:7002" \
./prims
```

Conseils VPS :
- ouvrir le port TCP `7001` côté pare-feu pour le P2P
- garder `PRIMS_RPC_ADDRESS` sur `127.0.0.1:7002` si le RPC ne doit pas être public
- ne jamais commiter ni transférer la clé réseau privée dans le dépôt
- utiliser un chemin de base dédié par nœud pour `PRIMS_DB_PATH`
- vérifier que `PRIMS_EXTERNAL_ADDRESS` annonce bien l IP publique réelle du VPS
