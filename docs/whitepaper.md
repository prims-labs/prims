# Whitepaper Prims

Version de travail : étape 10.8  
Statut du projet couvert par ce document : prototype avancé / préparation testnet-mainnet après clôture 10.7  
Date : 31 mars 2026

---

## 1. Résumé exécutif

> À rédiger en dernier, quand tout le document sera stabilisé.

---

## 2. Avertissement sur l état réel du projet

Ce whitepaper décrit Prims à deux niveaux distincts :
- **État implémenté et validé à ce jour** : ce qui existe déjà dans le code, les tests, les benchmarks et le journal d avancement.
- **Éléments prévus par la roadmap** : ce qui est visé plus tard, mais n est pas encore finalisé ni déployé en production.

Le document ne doit pas présenter comme déjà disponible une fonctionnalité qui reste encore au stade de prototype, de validation locale, de préparation testnet ou de plan roadmap.

---

## 3. Vision et proposition de valeur

### 3.1 Vision

Prims vise la construction d une infrastructure blockchain publique capable de combiner haut debit, finalite rapide, securite, simplicite d usage et confidentialite optionnelle. L ambition long terme est de fournir une base technique suffisamment performante pour supporter des paiements, des applications decentralisees et des usages inter-shards sans reproduire les congestions, les frais eleves et les mecanismes d ordre preferentiel observes sur de nombreux reseaux historiques.

Cette vision doit toutefois etre lue avec une distinction claire entre cible et realite actuelle. Au 31 mars 2026, Prims est un prototype Rust avance, fortement teste en local, avec une preparation testnet deja engagee, mais pas encore un reseau mainnet ouvert ni un systeme declare pret pour la production a grande echelle.

### 3.2 Problèmes visés

Prims cherche a traiter plusieurs limites recurrentes des systemes existants :
- debit insuffisant et congestion lors de la montee en charge ;
- frais variables ou eleves selon la demande ;
- priorisation par frais qui favorise les comportements opportunistes ;
- exposition aux mecanismes de MEV, de front-running et d attaques sandwich ;
- difficultes de passage a l echelle sans degrader l experience developpeur ;
- manque d options natives pour concilier transparence publique et confidentialite selective ;
- fragmentation entre couche de paiement, couche smart contracts et architectures multi-shards.

Dans l etat actuel du projet, tous ces problemes ne sont pas encore resolus a l echelle d un reseau public ouvert. En revanche, plusieurs briques techniques ont deja ete prototypees, testees et benchmarkees pour preparer une reponse coherente a ces limites.

### 3.3 Positionnement de Prims

Prims se positionne aujourd hui comme un projet de blockchain nouvelle generation en cours de maturation, construit en Rust, avec les choix structurants suivants deja presents dans le prototype :
- un reseau P2P base sur libp2p ;
- un stockage RocksDB ;
- un consensus Proof of Stake avec votes pondérés et logique de finalisation ;
- une mempool partitionnee et un travail explicite sur le parallellisme ;
- une architecture de sharding prototypee ;
- une confidentialite optionnelle basee sur des zk-SNARKs ;
- une execution de smart contracts WebAssembly ;
- une interface outillee via JSON-RPC, CLI et explorateur web.

Ce positionnement reste volontairement pragmatique : Prims ne pretend pas, a ce stade, etre deja un reseau de production. Le projet se situe dans une phase de preparation testnet/mainnet, avec un socle technique deja large, des validations locales nombreuses, et une progression encore gouvernee par la roadmap.

### 3.4 Différenciation recherchée

La differenciation recherchee par Prims repose sur plusieurs axes.

Premier axe : le parallellisme. La roadmap vise une architecture shardee et extensible, tandis que le prototype actuel a deja valide des briques de mempool partitionnee, de consensus de shard et de transactions cross-shard.

Deuxieme axe : une reduction des asymetries d execution. Le projet cherche a limiter les effets de priorisation par frais et les abus d ordre de transaction. A ce jour, le prototype applique deja des frais fixes et ne donne pas de priorite par frais dans la mempool.

Troisieme axe : la confidentialite optionnelle. Prims ne vise pas une opacite totale par defaut, mais une coexistence entre transactions publiques et transactions anonymes, avec passerelles entre les deux modeles. Cette orientation est deja prototypee dans le code, meme si elle doit encore etre durcie et eprouvee davantage avant toute mise en production.

Quatrieme axe : l unification de plusieurs couches dans un meme systeme. Prims cherche a reunir paiements, smart contracts Wasm, sharding et outillage utilisateur/developpeur dans une architecture coherente, au lieu d empiler des composants heterogenes sans ligne directrice commune.

Cinquieme axe : une progression prudente et verifiable. Le projet avance par etapes validees, avec tests, benchmarks, audits documentes, commits traces et sauvegardes locales. Cette discipline constitue elle-meme un element de differenciation important par rapport a des annonces purement speculatives.

---

## 4. Principes de conception

### 4.1 Performance

La performance constitue un objectif directeur de Prims, mais elle est abordee comme une construction progressive et verifiable, pas comme une promesse marketing de court terme. La roadmap vise une architecture capable de monter en charge par partitionnement logique puis par sharding complet, avec reduction de la congestion, finalite rapide et execution parallele.

Dans l etat actuel du prototype, cette orientation se traduit deja par plusieurs choix concrets : mempool partitionnee, logique de parallellisme, benchmark local de debit eleve, consensus de shard prototype et transactions cross-shard preparees. Cela ne signifie pas encore que Prims delivre aujourd hui la capacite finale visee sur un reseau public ouvert, mais que la base technique a ete pensee pour evoluer dans cette direction.

### 4.2 Sécurité

La securite prime sur la vitesse de livraison. Prims avance par durcissements successifs : validation cryptographique des transactions, verification des signatures, protections anti-replay, limites de taille, tests de rollback Wasm, controle des votes, protections sur le sharding, checksums de stockage et audits documentes.

Ce principe implique qu une fonctionnalite n est pas consideree mature uniquement parce qu elle compile ou fonctionne dans un cas heureux. Elle doit aussi etre accompagnee de tests cibles, de verifications de non-regression et, lorsque necessaire, d un durcissement documentaire et operationnel. L etape 10.7 a renforce explicitement cette approche avant la redaction du whitepaper.

### 4.3 Simplicité d usage

Prims cherche a rester lisible et utilisable, y compris pour des usages non experts. Cela se traduit par un outillage explicite avec CLI, API JSON-RPC, explorateur web, documentation technique et procedures de validation etape par etape.

La simplicite visee n est pas celle d un systeme minimaliste, mais celle d un systeme complexe rendu exploitable par une structure claire. Dans le prototype actuel, cela passe deja par des commandes dediees, une separation nette des composants et une documentation qui distingue ce qui existe deja de ce qui reste en preparation.

### 4.4 Extensibilité

Le projet est pense comme une architecture modulaire. Les couches reseau, stockage, consensus, confidentialite, API, smart contracts et sharding sont separees en modules, ce qui facilite l evolution progressive sans devoir reconstruire l ensemble a chaque etape.

Cette extensibilite est essentielle parce que Prims ne pretend pas etre fige. Le prototype actuel constitue un socle technique evolutif. Les choix retenus, comme Rust, libp2p, RocksDB, Wasm et JSON-RPC, servent autant la robustesse immediate que la capacite a ajouter ou faire evoluer des fonctions dans les phases suivantes.

### 4.5 Confidentialité optionnelle

Prims adopte un principe de confidentialite optionnelle plutot qu une confidentialite universelle imposee. L objectif est de permettre la coexistence de transactions publiques et de transactions anonymes, avec des mecanismes de conversion entre les deux modeles selon les besoins.

Ce principe est important pour conserver a la fois auditabilite, flexibilite et respect de cas d usage differents. Dans le prototype actuel, cette approche est deja visible avec l introduction d un modele anonyme distinct, de notes, d arbres de Merkle, de preuves zk-SNARKs et de conversions public/anon. Cela reste toutefois une zone qui devra encore etre eprouvee avant tout usage de production.

### 4.6 Décentralisation progressive

La decentralisation est une cible structurelle, mais elle est abordee de facon progressive. Prims ne cherche pas a declarer une decentralisation totale avant que le reseau, les outils, la securite et la gouvernance ne soient suffisamment prepares.

Cette progression prudente se retrouve dans la roadmap : d abord le prototype, ensuite le testnet, puis les audits, la documentation, la preparation du mainnet et enfin la mise en place plus complete de la gouvernance. Le principe est simple : ne pas annoncer comme decentralise et pret un systeme qui est encore en cours de stabilisation.

---

## 5. État actuel du projet au 31 mars 2026

### 5.1 Phases déjà validées

Au 31 mars 2026, les phases 0 a 9 de la roadmap ont ete validees dans le journal d avancement local, ainsi que les etapes 10.1 a 10.7 de la phase testnet et securite intensive. Cela signifie que le projet dispose deja d un socle etendu couvrant l environnement de developpement, le reseau P2P, les structures blockchain, le stockage, la cryptographie, le consensus Proof of Stake, la mempool partitionnee, le sharding prototype, la confidentialite optionnelle, les outils RPC/CLI/explorateur, l execution de smart contracts Wasm et une premiere sequence testnet orientee securite.

La phase 10 n est toutefois pas terminee. Au moment de rediger ce whitepaper, l etape 10.8 est en cours, tandis que 10.9 et 10.10 restent encore a finaliser avant de considerer la preparation du mainnet comme suffisamment documentee et durcie.

### 5.2 Niveau de maturité actuel

Prims se trouve aujourd hui a un niveau de maturite que l on peut qualifier de prototype avance en preparation testnet/mainnet, mais pas encore de reseau de production pret pour un lancement public large. Le code couvre deja de nombreuses briques importantes et a fait l objet de validations locales, de benchmarks et de durcissements successifs.

En pratique, cela signifie que Prims a deja depasse le simple stade conceptuel ou demonstratif. Le projet dispose d une architecture executable, de tests, d outils utilisateurs et developpeurs, d une base documentaire, d une CI multi-OS et d une phase de securite deja engagee. En revanche, il reste encore du travail de formalisation, de documentation, de tutoriels et de derniers audits avant qu un discours de preparation mainnet puisse etre considere comme complet.

### 5.3 Ce qui est démontré par tests et benchmarks

L etat actuel du projet ne repose pas uniquement sur des intentions de roadmap. Plusieurs proprietes ont deja ete demontrees par tests ou benchmarks :
- propagation reseau locale validee avec une latence mesuree de 1 ms sur cluster local pour le message de test ;
- stockage RocksDB valide avec ecriture de 10 000 blocs autour de 81.491 a 87.848 ms et lecture autour de 14.040 a 15.869 ms dans le benchmark dedie ;
- finalisation PoS benchmarkee sous l objectif cible local du prototype ;
- mempool partitionnee et traitement parallele valides localement avec un benchmark au-dessus de 10 000 TPS, mesure a 10497.90 TPS ;
- benchmark multi-shards local sous Docker montrant une montee progressive du debit publie entre 1, 2 et 3 shards ;
- confidentialite optionnelle prototypee avec generation de preuve zk mesuree autour de 1.56 s et verification autour de 1.18 ms sur le benchmark dedie ;
- execution Wasm benchmarkee autour de 0.201 ms pour un appel simple de contrat ;
- validations de securite ciblees sur le reseau, le consensus, le sharding, la confidentialite, l API RPC et la VM Wasm ;
- validation globale de l etape 10.7 avec une suite de tests reussie sur la bibliotheque, le CLI et le RPC.

Ces resultats doivent etre interpretes comme des preuves de faisabilite et de robustesse sur l etat actuel du prototype, non comme des garanties de comportement deja confirmees en environnement mainnet ouvert.

### 5.4 Ce qui reste avant le mainnet

Avant toute preparation finale du mainnet, plusieurs chantiers restent explicitement ouverts dans la roadmap. D abord, le whitepaper doit etre finalise a l etape 10.8. Ensuite, l etape 10.9 doit completer la documentation du code et les tutoriels developpeur/utilisateur. Enfin, l etape 10.10 doit mener une derniere sequence d audits et de tests de penetration.

Au-dela de la phase 10, la phase 11 reste encore a executer : gel du code cible mainnet, formalisation du genesis, definition finale des parametres de lancement, demarrage avec validateurs de confiance, ouverture progressive au public et gouvernance plus complete. En consequence, le statut correct de Prims au 31 mars 2026 n est ni "idee", ni "mainnet pret", mais "prototype avance en cours de consolidation avant les dernieres etapes de preparation mainnet".

---

## 6. Architecture technique actuelle

### 6.1 Couche réseau P2P

La couche reseau actuelle de Prims repose sur Rust et libp2p. Le prototype integre la decouverte de pairs, la diffusion de messages par gossip, la gestion de seed nodes, ainsi que des mecanismes de base de robustesse reseau comme les limites de connexion, la reconnexion et le rejet de traffic invalide. Cette couche constitue la base de circulation des transactions, blocs et votes dans le prototype.

Dans l etat actuel, cette couche reseau est fonctionnelle et testee localement, y compris sur cluster local et sur scenarios de resistance simples au spam de connexions. Elle reste cependant une couche de prototype avancee, pas encore une infrastructure reseau mainnet durcie a grande echelle.

### 6.2 Structures blockchain et stockage

Le coeur blockchain de Prims s appuie sur des structures de donnees explicites pour les transactions, blocs, comptes, validateurs, contrats et etats associes. Le stockage persistant repose sur RocksDB, avec un schema de cles dedie pour les blocs, transactions, comptes, stakes, contrats, storage contrat et elements lies a la confidentialite.

L architecture actuelle permet deja la persistance, la relecture apres redemarrage, l indexation de base, ainsi qu un renforcement d integrite sur certaines donnees critiques via checksum. Cette couche est donc deja plus qu un simple squelette : elle forme un socle persistant utilisable par les autres modules du prototype.

### 6.3 Cryptographie et validation

La pile cryptographique actuelle combine ed25519 pour les signatures, sha2 pour le hash et arkworks pour la partie zk. Le prototype sait generer des paires de cles, signer des transactions, verifier les signatures, calculer des hashes deterministes et valider l integrite de blocs et transactions.

La validation actuelle couvre aussi plusieurs garde-fous importants : anti-replay via nonce, verification des soldes, limites de taille, verification de merkle root et verification de signatures de validateurs. Cette couche fournit donc deja une base de securite fonctionnelle pour le prototype, meme si la securite globale continue d etre renforcee par les audits et les tests de non-regression.

### 6.4 Consensus Proof of Stake

Le consensus actuel est un Proof of Stake maison avec votes pondérés, finalisation au-dela des deux tiers du stake actif, selection deterministe du proposant, gestion de forks par poids cumule et logique de slashing en cas de double-vote prouve.

Cette couche consensus est deja prototypee, testee et benchmarkee localement. Elle permet de valider la logique de proposition, de vote, de finalisation, de recompenses et de sanctions. En revanche, elle doit encore etre consideree comme un mecanisme de prototype avance, pas comme un consensus deja eprouve sur un reseau public a grande echelle.

### 6.5 Mempool partitionnée et parallélisme

Prims introduit deja une mempool partitionnee, avec distribution logique des transactions et traitement parallele prepare en fonction du nombre de coeurs disponibles. Le prototype applique aussi des frais fixes et evite la priorisation par frais dans la mempool, ce qui s inscrit dans l objectif de reduction des asymetries d execution.

Cette couche est l un des premiers marqueurs architecturaux forts du projet, car elle relie directement la vision de debit eleve a des choix techniques deja implementes. Les benchmarks locaux ont valide une capacite de traitement elevee sur machine unique, mais ces resultats doivent encore etre distingues d un comportement confirme sur reseau public reel.

### 6.6 Sharding

L architecture actuelle inclut deja une couche de sharding prototypee avec configuration de shards, beacon chain, comites de validateurs, consensus par shard, racines d etat de shards et transactions cross-shard avec logique de preparation, validation, commit et receipts.

Cela signifie que le sharding n est pas traite seulement comme une idee de roadmap lointaine : plusieurs briques essentielles existent deja dans le code. En revanche, la validation actuelle reste une validation de prototype, avec simulations locales et benchmarks limites, et non un deploiement sharde en production.

### 6.7 Confidentialité optionnelle

La confidentialite actuelle repose sur une couche optionnelle distincte combinant notes, arbre de Merkle, preuves zk-SNARKs, transactions anonymes et conversion entre modele public et modele anonyme. Le prototype inclut deja un trusted setup simplifie documente, des benchmarks de preuve et de verification, ainsi que des tests de defense contre certains cas de double depense ou de preuve invalide.

Cette couche est techniquement ambitieuse et deja bien avancee pour un prototype. Elle doit toutefois etre lue comme une base de travail serieuse et testee, pas encore comme un systeme de confidentialite declare pret pour des usages sensibles en production.

### 6.8 API RPC, explorateur et CLI

L architecture actuelle expose une interface JSON-RPC via jsonrpsee, un explorateur web via axum et un CLI en clap. Le prototype sait deja fournir des informations de base, soumettre certaines transactions, afficher des donnees reseau, exposer un site testnet simple et gerer un stockage chiffre de cles cote CLI.

Cette couche outillage est importante car elle rend le prototype manipulable et observable. Elle ne se limite pas a des bibliotheques internes : elle fournit deja une surface d interaction concrete pour les tests, la documentation et les futures etapes de preparation testnet/mainnet.

### 6.9 Machine virtuelle Wasm et smart contracts

La couche smart contracts repose actuellement sur Wasmtime et une machine virtuelle Wasm capable de charger un contrat, d executer un appel, d exposer des host functions, de gerer une limite de fuel, de persister le storage contrat et d appliquer un rollback en cas d erreur ou d epuisement de gaz.

Le prototype supporte deja le deploiement et l appel de contrats, avec validations associees, benchmarks simples et tests de securite sur des comportements malveillants elementaires. Cette couche est donc deja reelle dans l architecture, meme si elle reste encore dans un cadre de prototype avance.

### 6.10 Testnet, automatisation et sécurité

Autour du noyau technique, Prims dispose deja d une couche d automatisation et de securite composee d une CI multi-OS GitHub Actions, d un mini portail testnet, d un seed node public, d un programme de bug bounty documente, de tests de charge auto-heberges, de simulations de pannes via Toxiproxy et d un audit interne consolide.

Cette derniere couche est importante car elle montre que l architecture actuelle ne se limite pas au code coeur. Elle inclut aussi les outils de validation, de reproduction, de documentation et de durcissement necessaires pour approcher une vraie preparation testnet/mainnet.

---

## 7. Fonctionnalités déjà implémentées et validées

### 7.1 Réseau
### 7.2 Stockage
### 7.3 Sécurité transactionnelle
### 7.4 Consensus
### 7.5 Sharding
### 7.6 Confidentialité
### 7.7 Outils développeur et utilisateur
### 7.8 Exécution de smart contracts
### 7.9 Audit et durcissement sécurité

---

## 8. Limites actuelles et points encore en cours

### 8.1 Limites du prototype actuel
### 8.2 Écarts entre vision long terme et état actuel
### 8.3 Hypothèses à confirmer
### 8.4 Points de vigilance sécurité

---

## 9. Tokenomics

### 9.1 Rôle du token PRIMS
### 9.2 Offre totale
### 9.3 Répartition proposée
### 9.4 Usage du token dans le réseau
### 9.5 Récompenses et incitations
### 9.6 Ce qui est déjà décidé vs ce qui reste à formaliser

---

## 10. Gouvernance

### 10.1 Gouvernance visée à long terme
### 10.2 Place de la DAO dans la roadmap
### 10.3 État actuel
### 10.4 Transition progressive vers le mainnet

---

## 11. Roadmap à partir de l état actuel

### 11.1 Étape 10.8 : whitepaper
### 11.2 Étape 10.9 : documentation et tutoriels
### 11.3 Étape 10.10 : derniers audits et tests de pénétration
### 11.4 Phase 11 : préparation du mainnet

---

## 12. Risques, hypothèses et stratégie de livraison

### 12.1 Risques techniques
### 12.2 Risques sécurité
### 12.3 Risques de complexité
### 12.4 Stratégie de progression

---

## 13. Conclusion

---

## 14. Annexes

### 14.1 Choix techniques retenus
### 14.2 Résumé des validations importantes
### 14.3 Terminologie du projet
