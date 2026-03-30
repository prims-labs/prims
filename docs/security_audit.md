# Audit de sécurité Prims

## Objectif

Ce document prépare l étape 10.7 de la roadmap :
- contacter des auditeurs externes, ou
- ouvrir un appel a la communaute pour audit,
puis suivre les failles identifiees jusqu a leur correction.

L objectif est de faire auditer le prototype testnet avant les etapes finales de documentation, whitepaper et preparation mainnet.

## Perimetre prioritaire

Les composants a auditer en priorite sont :
- reseau P2P et seed node public
- consensus Proof of Stake et logique de finalisation
- sharding et transactions cross-shard
- API JSON-RPC
- CLI `prims-cli`
- explorateur `prims-explorer`
- stockage RocksDB
- confidentialite optionnelle et logique zk
- VM Wasm et execution de contrats
- scripts, workflows CI/CD et configuration de testnet

## Types de failles recherchees

En priorite :
- execution de code non autorisee
- creation ou destruction non autorisee de fonds
- double depense
- corruption d etat
- contournement du consensus ou de la validation
- fuite d informations sensibles
- deni de service reproductible
- contournement des limites reseau, RPC ou VM
- faiblesse de configuration CI/CD ou secrets

## Modalites d audit

Deux modes sont acceptes :
1. audit externe cible par auditeur ou petite equipe
2. appel a la communaute avec remontée responsable

Dans les deux cas :
- travailler a partir d un commit ou tag explicite
- documenter clairement l environnement de test
- ne jamais demander ni transmettre de cle privee, mot de passe ou token
- realiser les tests sur environnement local, testnet ou infrastructure autorisee
- fournir des reproductions minimales et verifiables

## Livrables attendus

Chaque rapport doit contenir :
- titre
- composant concerne
- severite estimee
- preconditions
- etapes de reproduction
- impact
- preuve de concept minimale
- proposition de mitigation ou de correctif si possible
- commit, branche ou version concernee

## Classification de severite

### Critique
- execution de code arbitraire
- creation de fonds non autorisee
- contournement du consensus
- fuite de secret critique

### Haute
- double depense realiste
- corruption d etat importante
- deni de service durable
- faille majeure sur VM, RPC, reseau ou confidentialite

### Moyenne
- crash reproductible a impact limite
- contournement partiel d une regle metier
- fuite d information non critique

### Faible
- faiblesse defensive mineure
- validation incomplete sans impact majeur direct
- ecart documentaire securite

## Regles de confidentialite

- ne jamais publier une faille avant analyse et correctif
- ne jamais inclure de secret dans un rapport
- reduire la preuve de concept au strict necessaire
- coordonner la divulgation apres correction ou mitigation
- conserver une trace des decisions dans `journal_avancement.md`

## Suivi interne

Pour chaque faille retenue :
1. enregistrer la date de reception
2. qualifier la severite
3. reproduire localement
4. corriger ou mitiger
5. valider le correctif
6. documenter le resultat
7. preparer la divulgation coordonnee si approprie

## Etat actuel

A ce stade, ce document sert de base pour :
- ouvrir un appel a audit
- transmettre un perimetre propre a des auditeurs externes
- suivre les constats de securite pendant la phase testnet
