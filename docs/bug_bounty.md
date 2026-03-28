# Bug Bounty Prims

## Objectif

Ce document définit un programme de bug bounty simple pour le testnet Prims.
Le but est d encourager la remontée responsable de failles de sécurité avant les étapes plus avancées du testnet et du mainnet.

## Canal de signalement

Pour toute faille, ouvrir d abord une remontée privée au mainteneur du projet.
Ne pas divulguer publiquement la vulnérabilité avant confirmation, analyse et correctif.

Informations attendues dans le signalement :
- titre clair
- composant concerné
- impact attendu
- étapes de reproduction
- preuve de concept si possible
- version ou commit concerné

## Périmètre

Le programme couvre en priorité :
- nœud principal `prims`
- réseau P2P et seed node public
- API JSON-RPC
- CLI `prims-cli`
- explorateur `prims-explorer`
- stockage RocksDB
- consensus et validation des transactions / blocs
- confidentialité optionnelle et logique zk
- VM Wasm et exécution de contrats
- workflows GitHub Actions et scripts du dépôt

## Hors périmètre

Sauf accord explicite préalable, sont hors périmètre :
- attaques de phishing, social engineering ou compromission de comptes personnels
- attaques physiques sur une machine
- dépendances tierces déjà connues sans preuve d exploitabilité réelle dans Prims
- problèmes purement théoriques sans scénario reproductible
- rapports de faible qualité sans étapes de reproduction
- tests agressifs sur une infrastructure non autorisée
- divulgation publique avant correction

## Règles de test

- tester en priorité sur environnement local ou testnet autorisé
- ne jamais viser une infrastructure tierce sans autorisation
- ne jamais exfiltrer, publier ou conserver des secrets, clés privées, mots de passe ou données sensibles
- limiter l impact des preuves de concept au strict nécessaire
- arrêter immédiatement en cas de risque pour l intégrité des données ou la disponibilité du service

## Catégories de sévérité et récompenses proposées

### Critique
Impact typique :
- exécution de code arbitraire
- création de fonds non autorisée
- contournement du consensus
- corruption d état à grande échelle
- fuite de secret critique

Récompense proposée :
- `50000 PRIMS` à `100000 PRIMS`

### Haute
Impact typique :
- déni de service réseau durable
- contournement important d authentification ou de validation
- double dépense réaliste
- faille majeure sur la VM, le RPC ou la logique de confidentialité

Récompense proposée :
- `10000 PRIMS` à `50000 PRIMS`

### Moyenne
Impact typique :
- crash reproductible avec impact limité
- fuite d information non critique
- contournement partiel de règle métier
- faille nécessitant des conditions particulières

Récompense proposée :
- `1000 PRIMS` à `10000 PRIMS`

### Faible
Impact typique :
- problème mineur de robustesse
- validation incomplète sans impact direct majeur
- documentation sécurité incomplète avec risque limité

Récompense proposée :
- `100 PRIMS` à `1000 PRIMS`

## Décision finale

La sévérité finale et la récompense finale sont déterminées par le mainteneur du projet selon :
- impact réel
- facilité d exploitation
- qualité du rapport
- clarté de la reproduction
- existence d un correctif ou d une mitigation
- caractère inédit du signalement

## Processus de traitement

1. réception du signalement
2. confirmation de réception
3. reproduction
4. qualification de la sévérité
5. correctif ou mitigation
6. validation du correctif
7. attribution de la récompense
8. divulgation coordonnée si approprié

## Remarques

- Ce programme est prévu pour le contexte testnet actuel de Prims.
- Les montants en tokens pourront être ajustés plus tard par gouvernance ou politique officielle du projet.
- Aucun secret ne doit être demandé ni transmis dans un rapport.
