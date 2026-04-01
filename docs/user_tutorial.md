# Tutoriel utilisateur Prims

Ce guide explique comment demarrer le prototype Prims en local et utiliser les outils principaux sans exposer de secret.

## 1. Prerequis

- macOS avec zsh
- Depot local Prims a jour
- Rust et Cargo installes
- Ne jamais partager une cle privee, un mot de passe ou un token

## 2. Demarrer le noeud principal

Dans un premier terminal :

    cargo run --bin prims

Par defaut, le noeud demarre :
- le reseau P2P sur `127.0.0.1:7001`
- le serveur RPC sur `127.0.0.1:7002`

## 3. Demarrer le site web testnet

Dans un second terminal :

    cargo run --bin prims-explorer

Par defaut, le site est disponible sur :

    http://127.0.0.1:7003

## 4. Verifier le site web

Tu peux ouvrir le site depuis le terminal :

    open http://127.0.0.1:7003

La page affiche notamment :
- les informations du noeud
- les validateurs
- les commitments anonymes
- la recherche de solde
- le faucet de test si une cle faucet locale est configuree

## 5. Verifier un solde avec le CLI

Dans un troisieme terminal :

    cargo run --bin prims-cli -- balance <adresse>

Remplace `<adresse>` par l adresse a verifier.

## 6. Lister les validateurs

    cargo run --bin prims-cli -- list-validators

## 7. Generer une paire de cles de test

    cargo run --bin prims-cli -- generate-key

Utilise seulement des cles de test jetables.
Ne colle jamais une vraie cle privee dans le terminal, dans le code ou sur GitHub.

## 8. Utiliser le faucet web

Le faucet n est disponible que si une cle locale de test est configuree cote explorateur.
Sans cle configuree, le bouton faucet doit refuser proprement la demande.

## 9. Consulter la documentation utile

- `docs/api_rpc.md`
- `docs/whitepaper.md`
- `README.md`

## 10. Bonnes pratiques

- garder les secrets hors du depot Git
- verifier `git status` avant toute modification volontaire
- sauvegarder les fichiers importants apres une mise a jour majeure
- ne jamais utiliser de fonds reels dans ce prototype
