# Appel a audit de securite - Prims Testnet

## Contexte

Prims entre dans une phase de revue de securite testnet.
Le projet recherche :
- des auditeurs externes independants
- ou des contributeurs de la communaute capables de relire, tester et signaler des failles de maniere responsable

## Objectif

Identifier au plus tot les vulnerabilites importantes avant les etapes finales de documentation, whitepaper et preparation mainnet.

## Perimetre

Les zones prioritaires sont :
- reseau P2P et seed node public
- consensus et finalisation
- sharding et logique cross-shard
- API JSON-RPC
- CLI `prims-cli`
- explorateur `prims-explorer`
- stockage RocksDB
- confidentialite optionnelle et logique zk
- VM Wasm et execution de contrats
- scripts et workflows CI/CD

## Ce qui est attendu

Un rapport utile doit contenir :
- un titre clair
- le composant concerne
- la severite estimee
- les preconditions
- les etapes de reproduction
- l impact
- une preuve de concept minimale si possible
- une proposition de mitigation ou de correctif si possible

## Regles importantes

- ne pas divulguer publiquement une faille avant correction ou mitigation
- ne pas transmettre de cle privee, mot de passe, token ou secret
- tester uniquement sur environnement local, testnet ou infrastructure autorisee
- limiter les preuves de concept au strict necessaire
- signaler de maniere responsable

## Publication effective

- Issue GitHub publique : https://github.com/prims-labs/prims/issues/1
- Branche de reference : `main`
- Commit de reference : `c9feea8a34b3d04b4200d46dffcd8cd7eb0ad525`

## Reference de travail

Le perimetre detaille, les regles internes et le suivi des constats sont documentes dans :
- `docs/security_audit.md`
- `docs/bug_bounty.md`

## Contact

Le mainteneur du projet centralise les retours de securite.
Utiliser un canal prive et transmettre :
- le commit ou la branche concernee
- le composant vise
- la reproduction
- l impact observe

## Remarque

Ce document constitue une base d appel a audit pour la phase testnet actuelle.
Il pourra etre complete plus tard avec un canal officiel detaille, une fenetre d audit, ou une liste de commits cibles.
