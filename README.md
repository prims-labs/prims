# PRIMS

Blockchain nouvelle génération conçue pour le parallélisme, la sécurité, la confidentialité optionnelle et l'exécution de smart contracts WebAssembly.

## Vision

Créer une blockchain rapide, sécurisée, modulaire et documentée, capable de supporter un grand nombre de transactions et des applications décentralisées modernes.

## Objectifs

- haut débit
- faible latence
- frais réduits
- sécurité forte
- confidentialité optionnelle
- architecture propre et testable

## Choix techniques

- langage : Rust
- contrats intelligents : WebAssembly
- CLI : clap
- benchmarks : criterion

## Structure actuelle du projet

src/
  bin/
  network/
  blockchain/
  crypto/
  consensus/
  storage/
  vm/
  api/
  sharding/
  privacy/
  utils/
  lib.rs

tests/
benches/
docs/
scripts/

## Roadmap simplifiée

- Phase 0 : fondations et outillage
- Phase 1 : réseau P2P
- Phase 2 : structures blockchain
- Phase 3 : cryptographie
- Phase 4 : consensus
- Phase 5 : mempool parallélisé
- Phase 6 : sharding
- Phase 7 : confidentialité
- Phase 8 : API, CLI, explorateur
- Phase 9 : VM Wasm
- Phase 10 : testnet
- Phase 11 : mainnet

## Démarrage rapide

cargo build
cargo run --bin prims

## Sécurité

- ne jamais stocker de clé privée dans le dépôt
- ne jamais stocker de mot de passe ou token dans le code
- auditer régulièrement les dépendances
- sauvegarder les sources localement après chaque étape importante
## Documentation

- API JSON-RPC : `docs/api_rpc.md`
- Setup de confidentialite : `docs/privacy_trusted_setup.md`
- Bug bounty testnet : `docs/bug_bounty.md`
- Tests de charge : `docs/load_testing.md`


## Site web testnet

Le binaire `prims-explorer` fournit un site web simple pour le testnet Prims.

### Lancement

```bash
cargo run --bin prims-explorer
```

Le site écoute par défaut sur `127.0.0.1:7003` et utilise l'endpoint RPC `http://127.0.0.1:7002`.

Variables d'environnement utiles :
- `PRIMS_EXPLORER_ADDRESS`
- `PRIMS_RPC_URL`
- `PRIMS_FAUCET_AMOUNT`
- `PRIMS_FAUCET_SOURCE_SHARD`
- `PRIMS_FAUCET_DESTINATION_SHARD`

### Faucet

Le faucet est désactivé par défaut.

Pour l'activer, définir soit :
- `PRIMS_SECRET_KEY_FILE`
- `PRIMS_SECRET_KEY_HEX`

Important :
- ne jamais commiter de clé privée dans le dépôt
- préférer un fichier local hors dépôt pour la clé du faucet
- ne jamais partager mot de passe, token GitHub ou fichier chiffré sensible
