# Parametres du bloc genesis de Prims

## Statut
Document de reference pour l etape 11.2 de la roadmap.
Ce document formalise les parametres cibles du bloc genesis avant lancement mainnet.
Il ne modifie pas la branche `mainnet` gelee ; il prepare le lancement depuis `main`.

## Offre totale
- Offre totale cible : 1_000_000_000 PRIMS

## Repartition initiale cible
- 60% recompenses de staking = 600_000_000 PRIMS
- 20% tresorerie DAO = 200_000_000 PRIMS
- 10% premiers contributeurs = 100_000_000 PRIMS
- 10% fondateur = 100_000_000 PRIMS

## Principes de formalisation
- La somme des allocations initiales doit etre egale a l offre totale.
- Les adresses de destination doivent etre definies explicitement avant tout lancement.
- Les allocations genesis doivent etre traçables et documentees.
- La branche `mainnet` reste gelee ; toute preparation documentaire se fait sur `main`.

## Adresses initiales a definir avant lancement
- adresse_reserve_recompenses_staking : f05cc9b4dc41201ed051c41a9949e0287b04298214db828e6f32f8ec1aa5ca06
- adresse_tresorerie_dao : d403157cba50b5eea5952aa89a6a37f78fa7424616d7a987f9acc0d72135dd78
- adresse_fondateur : 6a205fb96318fc7027f827608a91908910b81945b0f6843acabccf0556f3efb3
- adresses_premiers_contributeurs :
  - contributors_pool = a16b8019212710fdde06fc3456aa9cc910a5338bfacbd2df08441d13943530e0
  - repartition_interne : A_DEFINIR
- adresses_validateurs_initiaux :
  - validator_01 = a6f4c49dd4f505bc267b838685abfb30d6273ed2ade3752f090fe86e32be51b3

- tresorerie DAO sous garde temporaire du fondateur jusqu a l implementation de la gouvernance on-chain.

## Points restant a verrouiller
- justification et liste des premiers contributeurs
- liste exacte des validateurs de confiance du lancement initial
- format technique final du fichier ou module genesis
- procedure de verification publique des allocations
