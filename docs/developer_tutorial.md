# Tutoriel développeur Prims

Ce guide explique comment lancer le prototype Prims en local, générer la documentation Rust et tester rapidement les composants principaux sans exposer de secret.

## 1. Prérequis

- macOS avec zsh
- Rust et Cargo installés
- Dépôt local Prims à jour
- Ne jamais utiliser une vraie clé privée dans les tests

## 2. Vérification rapide de l’environnement

    cargo --version
    rustc --version
    git --version

## 3. Compiler le projet

    cargo check

Cette commande vérifie que le projet compile sans produire les binaires finaux.

## 4. Lancer le nœud principal

    cargo run --bin prims

Par défaut, le nœud démarre :
- le réseau P2P sur `127.0.0.1:7001`
- le serveur RPC sur `127.0.0.1:7002`

## 5. Lancer l’explorateur web testnet

Dans un second terminal :

    cargo run --bin prims-explorer

Par défaut, l’explorateur écoute sur `127.0.0.1:7003` et interroge le RPC local.

## 6. Utiliser le CLI

Afficher l’aide :

    cargo run --bin prims-cli -- --help

Exemples utiles :

    cargo run --bin prims-cli -- list-validators
    cargo run --bin prims-cli -- balance <adresse>
    cargo run --bin prims-cli -- generate-key

## 7. Générer le rustdoc

    cargo doc --no-deps
    open target/doc/prims/index.html

Le rustdoc permet d’explorer les modules publics du projet depuis le navigateur.

## 8. Exécuter les tests

Tests complets :

    cargo test -- --nocapture

Tests plus ciblés si besoin :

    cargo test --test rpc_api -- --nocapture
    cargo test --test cli_integration -- --nocapture
    cargo test vm:: -- --nocapture

## 9. Fichiers importants pour un développeur

- `src/lib.rs` : point d’entrée des modules publics
- `src/network/` : couche P2P
- `src/blockchain/` : types, hash, validations
- `src/storage/` : persistance RocksDB
- `src/consensus/` : mempool, votes, finalisation
- `src/sharding/` : beacon chain et cross-shard
- `src/privacy/` : confidentialité et preuves
- `src/api/` : serveur JSON-RPC
- `src/vm/` : exécution Wasm
- `src/bin/` : binaires utilisateur et outils

## 10. Bonnes pratiques

- Ne jamais commiter de clé privée, mot de passe ou token
- Utiliser des clés de test jetables seulement
- Vérifier `git status` avant chaque commit
- Sauvegarder les fichiers importants après chaque mise à jour majeure
- Lancer au minimum `cargo fmt --all --check` et `cargo check` avant un commit

## 11. Documentation existante à consulter

- `docs/api_rpc.md`
- `docs/privacy_trusted_setup.md`
- `docs/load_testing.md`
- `docs/whitepaper.md`
