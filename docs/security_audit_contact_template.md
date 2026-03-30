# Template de contact audit securite - Prims

## Mode 1 - message court a un auditeur externe

Sujet propose :
Audit securite Prims testnet

Message :
Bonjour,

Je vous contacte dans le cadre du projet Prims, une blockchain en cours de validation sur testnet.

Nous entrons dans une phase de revue de securite et recherchons un audit cible sur tout ou partie des composants suivants :
- reseau P2P et seed node public
- consensus et finalisation
- sharding et logique cross-shard
- API JSON-RPC
- CLI et explorateur
- stockage RocksDB
- confidentialite optionnelle et logique zk
- VM Wasm et execution de contrats
- scripts et workflows CI/CD

Le perimetre technique et les attentes sont resumes dans :
- `docs/security_audit.md`
- `docs/security_audit_call.md`
- `docs/bug_bounty.md`

Si cela vous interesse, je peux transmettre :
- le depot
- le commit de reference
- le contexte testnet
- les points prioritaires a auditer

Merci.

## Mode 2 - appel a audit pour la communaute

Titre propose :
Appel a audit de securite - Prims Testnet

Texte propose :
Prims ouvre une phase de revue de securite testnet.

Nous recherchons des retours responsables sur :
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

Documents de reference :
- `docs/security_audit.md`
- `docs/security_audit_call.md`
- `docs/bug_bounty.md`

Merci de ne pas divulguer publiquement une faille avant correction ou mitigation.
Ne transmettez jamais de cle privee, mot de passe, token ou autre secret.
Travaillez uniquement sur environnement local, testnet ou infrastructure autorisee.

Pour tout signalement, preparez au minimum :
- composant concerne
- severite estimee
- etapes de reproduction
- impact
- preuve de concept minimale si possible
- commit ou version concernee
