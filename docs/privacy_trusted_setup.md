# Trusted setup simplifié du circuit zk de Prims

## Objectif

Cette étape 7.6 génère des paramètres Groth16 pour le circuit ZkTransferCircuit de Prims, avec une cérémonie participative simplifiée locale.

Le but est de :
- tracer les contributions à la cérémonie ;
- dériver un transcript final ;
- générer une clé de proving et une clé de verifying ;
- documenter clairement les limites du prototype.

## Outil utilisé

Le binaire dédié est :

    cargo run --bin prims-setup -- --help

Sous-commandes disponibles :
- init : initialise une nouvelle cérémonie ;
- contribute : ajoute une contribution au transcript ;
- finalize : génère les paramètres Groth16 à partir du transcript final ;
- inspect : affiche l'état courant de la cérémonie.

## Emplacement des artefacts

Par défaut, les artefacts sont écrits dans :

    artifacts/zk-setup

Fichiers produits :
- ceremony_state.json
- groth16_proving_key.bin
- groth16_verifying_key.bin
- setup_metadata.json

## Procédure locale de référence

### 1. Initialiser la cérémonie

    cargo run --bin prims-setup -- init --out-dir artifacts/zk-setup --organizer manu225

### 2. Ajouter une ou plusieurs contributions

    cargo run --bin prims-setup -- contribute --out-dir artifacts/zk-setup --contributor participant-local-2

### 3. Finaliser et produire les paramètres

    cargo run --bin prims-setup -- finalize --out-dir artifacts/zk-setup

### 4. Inspecter l'état de la cérémonie

    cargo run --bin prims-setup -- inspect --out-dir artifacts/zk-setup

## Fonctionnement simplifié

Cette cérémonie est un prototype local documenté.

Chaque contribution :
- ajoute de l'entropie locale ;
- met à jour un digest SHA-256 de transcript ;
- n'écrit pas l'entropie brute sur disque.

Le transcript final est ensuite condensé en une graine déterministe utilisée pour exécuter le trusted setup Groth16.

## Métadonnées du circuit de référence

Les paramètres actuellement générés sont liés à une forme de circuit de référence :
- 1 entrée privée ;
- 2 sorties privées ;
- profondeur de preuve de Merkle correspondant à l'arbre de référence actuel ;
- frais de référence 5.

Cette contrainte est importante : si la forme du circuit change plus tard, les paramètres devront être régénérés.

## Limites importantes

Ce setup est simplifié et sert de base de travail pour Prims.

Il ne remplace pas une vraie cérémonie MPC robuste de production, car :
- les contributions sont locales ;
- le transcript est géré dans un seul environnement ;
- il n'y a pas encore de vérification externe indépendante des contributions ;
- il n'y a pas encore de destruction attestée d'un éventuel "toxic waste".

## Sécurité

Bonnes pratiques :
- ne jamais exposer de clé privée, mot de passe ou token ;
- conserver une copie des artefacts de setup et de leur documentation ;
- ne pas supposer que ce setup est suffisant pour un mainnet ;
- considérer groth16_proving_key.bin et groth16_verifying_key.bin comme des artefacts sensibles du protocole à sauvegarder proprement.

## État actuel

Le transcript final et les paramètres générés sont décrits dans :

    artifacts/zk-setup/setup_metadata.json

Le fichier contient :
- le digest final du transcript ;
- le nombre de contributions ;
- la liste des contributeurs ;
- la forme du circuit de référence ;
- des notes sur les limites du prototype.
