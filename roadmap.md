



# 🌈 ROADMAP PRIMS – Version 1000/1000 (Absolue)
## Le plan de construction ultime pour une blockchain nouvelle génération

**Durée totale estimée : 34 mois** (avec marges de sécurité)
**Objectifs de performance :**
- **Scalabilité :** 10 000 TPS par shard, extensible à l'infini.
- **Latence :** Finalité < 2 secondes.
- **Confidentialité :** Transactions anonymes optionnelles (zk-SNARKs) avec génération de preuve < 10s, vérification < 100ms.
- **Smart contracts :** Wasm, multi-langages (Rust, AssemblyScript, C).
- **Modèle de compte hybride :** Account-based pour les transactions standards, UTXO pour les transactions anonymes, avec passerelle de conversion.

---

## 📋 PHASE 0 : Fondations (Semaine 1-4)
**Objectif :** Installer l'environnement, poser les bases, versionner, sécuriser les dépendances.

- [ ] **0.1** Installer Rust, Cargo, Git, VS Code (extensions rust-analyzer, CodeLLDB, GitLens, Even Better TOML).
- [ ] **0.2** Créer un dépôt GitHub public `prims` avec README, .gitignore (Rust), licence MIT.
- [ ] **0.3** Initialiser le projet Rust (`cargo new prims --bin`) et structurer les dossiers :
  ```
  src/
    bin/          (exécutables : prims-node, prims-cli, prims-explorer)
    lib.rs        (bibliothèque principale)
    network/      (module réseau avec libp2p)
    blockchain/   (cœur : blocs, transactions, état)
    crypto/       (cryptographie : signatures, hash, zk)
    consensus/    (consensus PoS)
    storage/      (stockage RocksDB)
    vm/           (machine virtuelle Wasm)
    api/          (API JSON-RPC)
    sharding/     (gestion des shards)
    privacy/      (transactions anonymes)
    utils/        (utilitaires)
  tests/          (tests d'intégration)
  benches/        (benchmarks avec criterion)
  docs/           (documentation)
  scripts/        (scripts d'automatisation)
  ```
- [ ] **0.4** Ajouter les dépendances de base dans `Cargo.toml` :
  - `anyhow`, `thiserror` (gestion d'erreurs)
  - `serde`, `serde_json`, `bincode` (sérialisation)
  - `log`, `env_logger` (logging)
  - `tokio` (async)
  - `criterion` (benchmarks)
  - `clap` (CLI)
- [ ] **0.5** Vérifier `cargo build` et `cargo run`.
- [ ] **0.6** **Sécurité :** Installer `cargo-audit` et exécuter `cargo audit` ; ajouter un workflow GitHub pour les audits automatiques.
- [ ] **0.7** Rédiger un README détaillé avec vision, objectifs, roadmap simplifiée.

**Livrable :** Base de code propre, versionnée, sécurisée.

---

## 📋 PHASE 1 : Réseau P2P (Semaine 5-12)
**Objectif :** Implémenter la couche réseau robuste avec libp2p.
**Choix technique :** libp2p (gossipsub, kadmelia, noise, mplex).

- [ ] **1.1** Ajouter `libp2p` avec les features nécessaires : `tcp`, `noise`, `mplex`, `mdns`, `gossipsub`, `kadmelia`, `request-response`.
- [ ] **1.2** Créer un nœud basique : `Swarm` avec transport TCP+noise, multiplexage, identité.
- [ ] **1.3** Implémenter la découverte de pairs :
  - MDNS pour le réseau local.
  - Seed nodes via configuration (liste de multiadresses).
  - Kademlia DHT pour le stockage distribué des pairs.
- [ ] **1.4** Mettre en place un protocole de gossip (Gossipsub) pour diffuser les messages (transactions, blocs, votes).
- [ ] **1.5** Définir les messages personnalisés (enum `Message`) avec sérialisation bincode :
  - `Ping`, `Pong`
  - `NewTransaction(Transaction)`
  - `NewBlock(Block)`
  - `GetBlocks(u64)` (demande de blocs à partir d'une hauteur)
  - `Blocks(Vec<Block>)`
  - `Vote(Vote)`
- [ ] **1.6** Gérer les connexions : limites, reconnexion, bannissement temporaire des pairs malveillants (timeout, messages invalides).
- [ ] **1.7** **Tests :** Script `scripts/run_local_cluster.sh` pour lancer 3 nœuds et vérifier la découverte, la propagation d'un message.
- [ ] **1.8** **Benchmark :** Mesurer la latence de propagation d'un message entre nœuds (< 500 ms).
- [ ] **1.9** **Sécurité :** Tester la résistance au spam de connexions (DoS).

**Livrable :** Réseau P2P fonctionnel et résilient.

---

## 📋 PHASE 2 : Structure des données et stockage (Semaine 13-20)
**Objectif :** Définir les blocs, transactions, et persister les données avec RocksDB.
**Choix technique :** RocksDB pour les performances, sérialisation bincode.

- [ ] **2.1** Définir les structures de base (transactions, blocs, comptes) en sérialisable.
  - `Transaction` : `from`, `to`, `amount`, `fee`, `nonce`, `signature`, `data` (optionnel).
  - `BlockHeader` : `version`, `previous_hash`, `merkle_root`, `timestamp`, `height`, `validator`, `signature`.
  - `Block` : `header`, `transactions`.
  - `Account` : `balance`, `nonce` (pour compte standard), éventuellement `code_hash` (pour contrat).
- [ ] **2.2** Implémenter le calcul du hash (SHA-256 via `sha2`) et du Merkle root.
- [ ] **2.3** Ajouter la dépendance `rocksdb` et créer un module `storage` avec une abstraction.
- [ ] **2.4** Définir les préfixes de clés :
  - `b:{height}` → bloc
  - `h:{hash}` → height (index inverse)
  - `t:{hash}` → transaction
  - `a:{address}` → compte
  - `s:{address}` → stake (pour validateurs)
  - `c:{address}` → code Wasm (pour contrats)
  - `m:{address}:{key}` → storage des contrats
- [ ] **2.5** Implémenter les fonctions de base : `put`, `get`, `delete`, `iter` avec gestion d'erreurs.
- [ ] **2.6** Implémenter `save_block`, `get_block`, `save_transaction`, `get_transaction`, `update_account`, `get_account`.
- [ ] **2.7** **Tests :** Test unitaire de persistance et de reprise après redémarrage.
- [ ] **2.8** **Benchmark :** Mesurer le temps d'écriture/lecture de 10 000 blocs (< 100 ms par bloc).
- [ ] **2.9** **Sécurité :** Ajouter des checksums sur les données critiques.

**Livrable :** Base de données performante et fiable.

---

## 📋 PHASE 3 : Cryptographie et sécurité (Semaine 21-28)
**Objectif :** Signer, vérifier, hasher, et protéger contre les attaques de base.
**Choix technique :** `ed25519-dalek` pour signatures, `sha2` pour hash.

- [ ] **3.1** Génération de paires de clés (ed25519) avec `rand`.
- [ ] **3.2** Implémenter `sign_transaction` et `verify_transaction` (inclure tous les champs sauf signature).
- [ ] **3.3** Implémenter `hash` (SHA-256) et `merkle_root`.
- [ ] **3.4** Validation des blocs : vérifier `previous_hash`, `merkle_root`, signature du validateur.
- [ ] **3.5** Anti-replay : stocker le dernier nonce utilisé pour chaque compte ; vérifier que le nonce est strictement supérieur.
- [ ] **3.6** Vérifier le solde suffisant et les montants positifs.
- [ ] **3.7** Limites de taille (transactions < 1 Mo, blocs < 10 Mo).
- [ ] **3.8** **Tests :** Tests unitaires de signature, vérification, corruption.
- [ ] **3.9** **Sécurité :** Ajouter des tests de résistance aux collisions (non nécessaire pour SHA-256 mais vérifier l'absence de failles).

**Livrable :** Transactions et blocs sécurisés.

---

## 📋 PHASE 4 : Consensus Proof of Stake (Semaine 29-40)
**Objectif :** Implémenter un consensus PoS avec finalité par votes (tendance BFT).
**Choix technique :** Sélection aléatoire pondérée, votes 2/3, slashing.

- [ ] **4.1** Ajouter les structures pour les validateurs : `Validator { address, stake, locked_until }` dans la base.
- [ ] **4.2** Transactions spéciales : `Stake(amount, duration)` et `Unstake` (après verrouillage).
- [ ] **4.3** Sélection du proposant : à chaque hauteur, `seed = hash(previous_block_hash + height)`, choisir parmi les validateurs avec probabilité proportionnelle au stake.
- [ ] **4.4** Proposition de bloc : le proposant crée un bloc à partir des transactions en attente (mempool) et le diffuse.
- [ ] **4.5** Votes : chaque validateur envoie un vote signé (pour/contre). Collecte et agrégation.
- [ ] **4.6** Finalisation : un bloc est finalisé s'il reçoit >2/3 des votes pondérés.
- [ ] **4.7** Gestion des forks : règle de la chaîne la plus lourde (en stake cumulé).
- [ ] **4.8** Slashing : détection des double-votes (preuve) et pénalité (perte d'une partie du stake).
- [ ] **4.9** Récompenses : distribution des frais + inflation (2% annuel) aux validateurs ayant voté.
- [ ] **4.10** **Tests :** Simulation de 4 validateurs, vérification de la sélection, fork, slashing.
- [ ] **4.11** **Benchmark :** Temps de finalisation < 2 secondes.
- [ ] **4.12** **Sécurité :** Tester attaque de 33% (byzantine) – le consensus doit tenir.

**Livrable :** Consensus fonctionnel pour un seul shard.

---

## 📋 PHASE 5 : Mempool parallélisé et pré-sharding (Semaine 41-48)
**Objectif :** Permettre le traitement parallèle des transactions avant le sharding complet.

- [ ] **5.1** Introduction de partitions logiques basées sur l'adresse (mod N, N = nombre de cœurs).
- [ ] **5.2** Chaque partition a sa propre file d'attente (mempool) gérée par une tâche asynchrone.
- [ ] **5.3** Répartiteur (dispatcher) qui aiguille les transactions entrantes vers la bonne partition.
- [ ] **5.4** Les proposants de blocs (pour l'instant un seul shard) peuvent piocher dans toutes les partitions.
- [ ] **5.5** Pas de priorité par frais : frais fixes (0.001 PRIMS).
- [ ] **5.6** Rate limiting : max 100 transactions par adresse par bloc.
- [ ] **5.7** **Tests :** Script de charge avec milliers de transactions concurrentes.
- [ ] **5.8** **Benchmark :** Atteindre 10 000 TPS en local (sur machine multi-cœurs).
- [ ] **5.9** Vérifier l'impossibilité des attaques sandwich (pas d'ordre visible).

**Livrable :** Traitement parallèle efficace.

---

## 📋 PHASE 6 : Sharding complet (Semaine 49-66)
**Objectif :** Diviser le réseau en shards avec une beacon chain.
**Choix technique :** Architecture proche d'Ethereum 2.0 mais simplifiée.

- [ ] **6.1** Définir le nombre initial de shards (ex: 64) évolutif via gouvernance.
- [ ] **6.2** Beacon chain :
  - Gère l'ensemble des validateurs.
  - Assigne aléatoirement les validateurs aux shards (comités) pour une période (epoch).
  - Stocke les racines d'état des shards (pour les preuves cross-shard).
- [ ] **6.3** Chaque shard a son propre consensus (similaire à la phase 4) avec son comité de validateurs.
- [ ] **6.4** Transactions cross-shard :
  - Une transaction peut concerner plusieurs shards (ex: envoi de A (shard 1) vers B (shard 2)).
  - Protocole de verrouillage à deux phases : préparation sur shard source, validation sur shard destination, puis commit.
  - Inclusion de "receipts" (preuves) dans les blocs.
- [ ] **6.5** La beacon chain valide les preuves cross-shard et assure la finalité globale.
- [ ] **6.6** Mise à jour de l'état des shards sur la beacon chain.
- [ ] **6.7** **Tests :** Simulation de plusieurs shards avec Docker.
- [ ] **6.8** **Benchmark :** Mesurer le TPS global en fonction du nombre de shards (objectif linéaire : 10k TPS par shard).
- [ ] **6.9** **Sécurité :** Test de compromission d'un shard – vérifier que les autres ne sont pas affectés.

**Livrable :** Réseau shardé scalable.

---

## 📋 PHASE 7 : Confidentialité optionnelle (zk-SNARKs) (Semaine 67-82)
**Objectif :** Permettre des transactions anonymes avec un modèle UTXO coexistant avec le modèle account-based.
**Choix technique :** `arkworks` pour les zk-SNARKs, courbe BLS12-381, circuit personnalisé.

- [ ] **7.1** Ajouter les dépendances `ark-*` (ff, ec, groth16, bls12-381, std).
- [ ] **7.2** Étudier le modèle UTXO (comme Zcash) : chaque transaction consomme des entrées (UTXOs) et produit des sorties.
- [ ] **7.3** Définir une `Note` anonyme : contient un engagement (commitment) sur la valeur et le destinataire (clé publique).
- [ ] **7.4** Construire un arbre de Merkle des notes (Merkle tree) pour prouver l'appartenance sans révéler.
- [ ] **7.5** Créer un circuit zk-SNARK qui vérifie :
  - Les entrées existent dans l'arbre (Merkle proof).
  - L'expéditeur connaît la clé privée correspondant à la note (signature).
  - La somme des entrées = somme des sorties + frais.
  - Les engagements sont corrects.
- [ ] **7.6** Générer les paramètres du circuit (trusted setup) avec une cérémonie participative simplifiée (documentée).
- [ ] **7.7** Implémenter la génération de preuve (hors chaîne) et la vérification (dans le nœud).
- [ ] **7.8** Créer un type de transaction `AnonTransaction` contenant la preuve et les engagements.
- [ ] **7.9** Gestion de l'état anonyme : chaque compte peut avoir un solde public (account-based) et un ensemble de notes privées (UTXO) accessibles via une clé de vision. La clé de vision permet au destinataire de détecter les notes qui lui sont destinées. Les notes sont stockées dans un arbre de Merkle global (ou par shard).
- [ ] **7.10** Implémenter la conversion entre les deux modèles :
  - `PublicToAnon(amount)` : brûle des tokens publics et crée une note anonyme (insertion dans l'arbre).
  - `AnonToPublic(amount)` : consume une note (preuve de possession) et crédite le solde public.
- [ ] **7.11** **Tests :** Créer une transaction anonyme, vérifier la preuve, s'assurer qu'on ne peut pas tracer.
- [ ] **7.12** **Benchmark :** Temps de génération de preuve (< 10s) et vérification (< 100 ms).
- [ ] **7.13** **Sécurité :** Vérifier l'impossibilité de double-dépense et de fausses preuves.

**Livrable :** Transactions anonymes optionnelles, coexistence des deux modèles.

---

## 📋 PHASE 8 : API RPC et outils (Semaine 83-94)
**Objectif :** Fournir des interfaces pour utilisateurs et développeurs.
**Choix technique :** JSON-RPC avec `jsonrpsee`, explorateur web avec `axum`, CLI avec `clap`.

- [ ] **8.1** Ajouter `jsonrpsee` pour le serveur RPC.
- [ ] **8.2** Implémenter les méthodes RPC de base :
  - `get_block(height/hash)`
  - `get_transaction(hash)`
  - `send_transaction(hex_tx)`
  - `get_balance(address)`
  - `get_info()`
  - `get_validators()`
  - `get_note_commitments()` (pour les portefeuilles anonymes)
- [ ] **8.3** Ajouter le rate limiting avec `governor`.
- [ ] **8.4** Créer un explorateur web minimal avec `axum` et des templates (ou une SPA simple en JS).
  - Page d'accueil : derniers blocs, dernières transactions.
  - Pages de détail pour bloc, transaction, adresse.
  - Recherche par hauteur/hash/adresse.
- [ ] **8.5** Développer le CLI `prims-cli` avec les commandes :
  - `generate-key`
  - `balance <address>`
  - `send <to> <amount> [--anon]`
  - `stake <amount>`
  - `unstake`
  - `list-validators`
  - `create-contract <wasm_file>`
  - `call-contract <address> <method> <params>`
- [ ] **8.6** Gérer le stockage sécurisé des clés (fichier chiffré avec mot de passe).
- [ ] **8.7** Documenter l'API (OpenAPI ou Markdown) avec exemples.
- [ ] **8.8** **Tests d'intégration :** Tester les appels RPC et CLI.
- [ ] **8.9** **Sécurité :** Vérifier que l'API n'expose pas d'infos sensibles.

**Livrable :** Interfaces complètes pour interagir avec Prims.

---

## 📋 PHASE 9 : Machine virtuelle et smart contracts (Semaine 95-112)
**Objectif :** Exécuter des contrats intelligents en Wasm.
**Choix technique :** `wasmtime` pour le runtime, support de Rust et AssemblyScript.

- [ ] **9.1** Ajouter `wasmtime` et `wasmtime-wasi` (pour les imports).
- [ ] **9.2** Créer un module `vm` avec une structure `WasmVM`.
- [ ] **9.3** Définir les host functions accessibles aux contrats :
  - `get_balance(address) -> u64`
  - `transfer(to, amount) -> bool`
  - `get_caller() -> [u8;32]`
  - `get_block_height() -> u64`
  - `set_storage(key, value)`
  - `get_storage(key) -> Vec<u8>`
  - `emit_event(topic, data)`
- [ ] **9.4** Implémenter ces fonctions en Rust, en vérifiant les droits (seul le contrat peut modifier son propre storage).
- [ ] **9.5** Stockage des contrats : table `contracts` avec code Wasm et storage root (Merkle Patricia Trie inspiré d'Ethereum).
- [ ] **9.6** Implémenter un moteur de gaz : limiter les instructions avec le fuel de `wasmtime` (coût par instruction).
- [ ] **9.7** Transaction `DeployContract` : prend le bytecode Wasm, crée une adresse (déterminée par le hash du code et l'envoyeur), stocke le code.
- [ ] **9.8** Transaction `CallContract` : adresse du contrat, méthode (string), paramètres (sérialisés), limite de gaz.
- [ ] **9.9** Exécution : charger le code, instancier, lier les host functions, exécuter la fonction, mettre à jour le storage.
- [ ] **9.10** Gérer les erreurs (panne, dépassement de gaz) et les rollbacks.
- [ ] **9.11** **Tests :** Déployer un contrat simple (ERC20), l'appeler, vérifier les soldes.
- [ ] **9.12** **Benchmark :** Coût d'exécution d'un contrat simple (< 10 ms).
- [ ] **9.13** **Sécurité :** Tester des contrats malveillants (boucle infinie, accès mémoire) – le runtime doit les stopper.

**Livrable :** Support de smart contracts Wasm.

---

## 📋 PHASE 10 : Testnet et sécurité intensive (Semaine 113-128)
**Objectif :** Tester en conditions réelles, corriger les bugs, auditer.

- [ ] **10.1** Automatiser les builds pour Linux, Mac, Windows avec GitHub Actions.
- [ ] **10.2** Créer un site web simple pour le testnet avec instructions, faucet (service web qui distribue des tokens de test).
- [ ] **10.3** Lancer un seed node public.
- [ ] **10.4** Organiser un bug bounty avec récompenses en tokens (catégories de sévérité).
- [ ] **10.5** Tests de charge : déployer des nœuds sur des VPS, lancer des scripts de transactions massives (milliers de TPS).
- [ ] **10.6** Simuler des pannes (arrêt de nœuds, partitions réseau) avec Toxiproxy.
- [ ] **10.7** Audits de sécurité : contacter des auditeurs externes (ou ouvrir un appel à la communauté) et corriger les failles.
- [ ] **10.8** Rédiger le whitepaper complet (vision, technique, tokenomics).
- [ ] **10.9** Documenter le code (rustdoc) et créer des tutoriels (vidéo/texte) pour développeurs et utilisateurs.
- [ ] **10.10** **Sécurité :** Mener une dernière série d'audits et de tests de pénétration.

**Livrable :** Testnet stable, bugs corrigés, documentation prête.

---

## 📋 PHASE 11 : Mainnet et gouvernance (Semaine 129-144)
**Objectif :** Lancer officiellement et décentraliser.

- [ ] **11.1** Geler le code sur une branche `mainnet`.
- [ ] **11.2** Définir les paramètres du bloc genesis :
  - Offre totale : 1 milliard PRIMS.
  - Répartition (proposée) : 60% récompenses de staking, 20% trésorerie DAO, 10% premiers contributeurs (à justifier), 10% fondateur.
  - Adresses initiales : trésorerie, premiers validateurs, etc.
- [ ] **11.3** Lancer le réseau avec un petit groupe de validateurs de confiance (dont toi).
- [ ] **11.4** Ouvrir au public : annonces sur réseaux sociaux, forums, etc.
- [ ] **11.5** Demander le listing sur CoinMarketCap, CoinGecko.
- [ ] **11.6** Implémenter la gouvernance on-chain (DAO) :
  - Module de vote : propositions, votes pondérés par le stake.
  - Paramètres gouvernables : frais, durée de verrouillage, inflation, etc.
  - Trésorerie contrôlée par la DAO.
- [ ] **11.7** Transférer progressivement le contrôle à la DAO (toi, tu deviens un membre de la communauté).
- [ ] **11.8** Maintenir et améliorer : corriger les bugs critiques, préparer les futures mises à jour.

**Livrable :** Blockchain vivante, décentralisée, avec communauté.

---

## 🧠 Cohérence et gestion des modèles (clarification)

- **Modèle de compte :** Par défaut, Prims utilise un modèle account-based (comme Ethereum) pour sa simplicité. Les transactions anonymes utilisent un modèle UTXO (notes) pour préserver la confidentialité. Les deux coexistent via des transactions de conversion. L'état global est partitionné : chaque adresse a un solde public (account) et éventuellement un ensemble de notes privées (référencées par des engagements). Les notes sont stockées dans un arbre de Merkle global (ou par shard) et accessibles via une clé de vision. La clé de vision permet au destinataire de scanner les notes qui lui sont destinées. Cette approche est similaire à Zcash mais intégrée à un système account-based.
- **Sharding et confidentialité :** Les notes anonymes peuvent être réparties sur plusieurs shards, mais les preuves cross-shard deviennent complexes. On peut initialement concentrer les notes dans un shard dédié à la vie privée, puis étendre. La roadmap phase 7 inclut cette réflexion.

---

## 🧠 Gestion des risques et points de contrôle

À chaque fin de phase :
- Tous les tests unitaires et d'intégration passent (commande `cargo test`).
- Les benchmarks atteignent les objectifs (commande `cargo bench`).
- Les audits de sécurité (`cargo audit`, revue manuelle) ne détectent rien de critique.
- La documentation est à jour.
- On ne passe à la phase suivante qu'après validation.

---

## 📈 Estimations finales

- Phase 0 : 1 mois
- Phase 1 : 2 mois
- Phase 2 : 2 mois
- Phase 3 : 2 mois
- Phase 4 : 3 mois
- Phase 5 : 2 mois
- Phase 6 : 4 mois
- Phase 7 : 4 mois
- Phase 8 : 3 mois
- Phase 9 : 4 mois
- Phase 10 : 4 mois
- Phase 11 : 3 mois

**Total : 34 mois** (2 ans 10 mois). C'est une estimation prudente avec marges pour un développeur solo apprenant en cours de route. On peut accélérer certaines phases si tout se passe bien.

