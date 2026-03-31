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

La couche reseau deja validee couvre un noeud P2P base sur libp2p, la decouverte de pairs, l utilisation d un seed node, la diffusion de messages par gossip, des messages reseau specialises, des limites de connexion, une logique de reconnexion et de bannissement temporaire, ainsi qu un script de cluster local a 3 noeuds. La propagation d un message de test a ete benchmarkee localement, et une resistance de base au spam de connexions a aussi ete verifiee.

### 7.2 Stockage

Le stockage deja implemente couvre les structures blockchain de base, le hash et le merkle root, l abstraction de stockage RocksDB, les cles metier, la persistance des blocs, transactions et comptes, la reprise apres redemarrage, un benchmark sur 10 000 blocs et un renforcement d integrite par checksum pour certaines donnees critiques. Cette couche est donc deja utilisable comme socle persistant du prototype.

### 7.3 Sécurité transactionnelle

La securite transactionnelle deja validee couvre la generation et la verification de signatures ed25519, l anti-replay par nonce, la verification de solde, les controles de montants et d overflow, les limites de taille, la validation des blocs et plusieurs tests de non-regression sur l integrite des hashes et l ordre des transactions. Cela fournit deja une base solide pour refuser les cas elementaires invalides ou malveillants dans le prototype.

### 7.4 Consensus

Le consensus deja implemente couvre les validateurs, les transactions de stake et unstake, la selection deterministe du proposant, la proposition de blocs, les votes signes, la finalisation au-dela des deux tiers du stake actif, la gestion de forks, le slashing sur double-vote, la distribution de recompenses, une simulation integree et des tests de resilience byzantine. Un benchmark local a egalement valide la finalisation sous l objectif vise dans le prototype.

### 7.5 Sharding

Le sharding deja valide couvre une configuration initiale de shards, une beacon chain, des comites de validateurs, un consensus propre a chaque shard, des transactions cross-shard avec receipts, la verification des preuves et de la finalite globale, la mise a jour des racines d etat, une simulation Docker multi-shards, un benchmark local multi-shards et un test de securite confirmant qu une compromission simulee d un shard n affecte pas les autres.

### 7.6 Confidentialité

La couche de confidentialite deja implementee couvre un modele UTXO minimal, des notes anonymes, un arbre de Merkle, un circuit zk-SNARK, un trusted setup simplifie documente, la generation et la verification de preuves Groth16, les transactions anonymes, l etat anonyme des comptes, la conversion public/anon, un test de non-tracabilite entre notes de meme valeur, un benchmark de performance et des tests de defense contre certaines preuves invalides et certaines doubles depenses.

### 7.7 Outils développeur et utilisateur

Les outils deja disponibles couvrent une API JSON-RPC, un explorateur web minimal, un CLI utilisateur, la documentation RPC, des tests d integration RPC + CLI, un stockage chiffre de cles cote CLI, une CI multi-OS GitHub Actions et un mini portail testnet. Le projet dispose donc deja d une surface d usage concrete pour developper, tester et observer le prototype.

### 7.8 Exécution de smart contracts

La couche smart contracts deja validee couvre le runtime Wasm, les host functions de base, le stockage persistant des contrats, le fuel et la limite de gaz, les transactions natives de deploiement et d appel, l execution reelle d un contrat, le rollback atomique du storage en cas d erreur, un test fonctionnel de contrat simple de type jeton/ERC20 minimal, un benchmark d execution et des tests de defense contre boucle infinie, epuisement de fuel et acces memoire hors limites.

### 7.9 Audit et durcissement sécurité

Au-dela des validations par module, Prims a deja passe une phase explicite de durcissement securite. L API publique a ete revue pour ne plus exposer certains champs sensibles, la VM Wasm a ete renforcee, l audit interne de l etape 10.7 a consolide les points reseau, consensus, sharding, confidentialite, RPC et execution Wasm, et un correctif majeur a impose la verification cryptographique des signatures cote RPC avant acceptation. Cette phase s est accompagnee de tests de non-regression et d un verrouillage Git/GitHub proprement trace.

---

## 8. Limites actuelles et points encore en cours

### 8.1 Limites du prototype actuel

Malgre l avancement technique du projet, Prims reste au 31 mars 2026 un prototype avance en preparation testnet/mainnet, pas un reseau mainnet ouvert et eprouve a grande echelle. Une partie importante des validations a ete obtenue en local, sur machine unique, sur cluster local ou dans des simulations Docker. Ces resultats sont utiles et solides pour un prototype, mais ils ne remplacent pas encore une validation en environnement public reel.

Certaines validations restent aussi volontairement bornees. Par exemple, les benchmarks Wasm ont ete realises sur des contrats simples, la validation fonctionnelle de contrat de type jeton reste minimale, et la couche de confidentialite repose encore sur un setup simplifie et sur des hypotheses de prototype. Le projet a donc deja une base serieuse, mais pas encore le niveau de couverture qu exigerait un lancement mainnet complet.

### 8.2 Écarts entre vision long terme et état actuel

La vision de Prims vise une blockchain tres performante, modulaire, orientee parallellisme, confidentialite optionnelle, smart contracts Wasm, gouvernance progressive et, dans la presentation du projet, des ambitions larges comme l interoperabilite native. L etat actuel rejoint deja une partie importante de cette vision sur le plan architectural, mais pas encore sur le plan d un deploiement public complet ni sur tous les axes fonctionnels de long terme.

En particulier, il existe encore un ecart entre des validations locales prometteuses et une preuve de comportement robuste sur un reseau public ouvert avec validateurs, trafic, incidents et contraintes reelles d exploitation. De meme, certaines ambitions de long terme, comme la gouvernance on-chain mature ou d autres extensions ecosysteme, restent encore des objectifs de roadmap et non des fonctions deja finalisees dans le prototype actuel.

### 8.3 Hypothèses à confirmer

Plusieurs hypotheses restent a confirmer dans les prochaines etapes. Il faut encore verifier le comportement du systeme sur des conditions plus proches d un veritable testnet public, completer la documentation et les tutoriels, confirmer les derniers parametres techniques et operationnels utiles avant le mainnet, puis finaliser la sequence d audits et de tests de penetration prevue par la roadmap.

D autres hypotheses techniques restent aussi ouvertes a un niveau plus fin : comportement sur des charges Wasm plus complexes, evolution du systeme avec davantage de validateurs et de topologies reseau, couverture supplementaire de certains cas limites de confidentialite, et finalisation du cadre de lancement mainnet avec parametres genesis et gouvernance progressive. Ces sujets ne remettent pas en cause le travail deja valide, mais ils montrent clairement que la consolidation n est pas terminee.

### 8.4 Points de vigilance sécurité

Plusieurs points de vigilance securite doivent rester actifs dans la suite du projet. La surface RPC doit continuer a etre revue avec prudence, la VM Wasm doit continuer a etre testee contre des contrats malveillants plus varies, et les mecanismes de sharding, de receipts et de finalite globale doivent rester fortement surveilles a mesure que les scenarios montent en complexite.

La confidentialite optionnelle demande egalement une vigilance particuliere. Le prototype a deja valide des briques importantes, mais certains choix restent ceux d un systeme en cours de maturation, par exemple l absence actuelle d un registre explicite de nullifiers dans l etat present du prototype. Enfin, sur le plan operationnel, la gestion des secrets, cles, seed nodes, fichiers chiffrés et procedures de deploiement doit rester stricte jusqu aux dernieres etapes avant mainnet.

---

## 9. Tokenomics

### 9.1 Rôle du token PRIMS

Dans la vision du projet, le token PRIMS a vocation a jouer plusieurs roles : actif natif du reseau, unite economique pour les transferts, support du staking, base des incitations validateurs et, a terme, element de gouvernance dans une logique de transition progressive vers une DAO.

Dans l etat actuel du prototype, une partie de cette logique economique existe deja au niveau technique, notamment via les transactions de transfert, de stake et d unstake, ainsi que par la presence de frais fixes et d un mecanisme de recompenses pour les validateurs votants. En revanche, le cadre economique complet du lancement mainnet n est pas encore fige.

### 9.2 Offre totale

La presentation du projet fixe une offre totale cible de 1 milliard PRIMS. Cette valeur appartient aujourd hui au cadrage de vision et de tokenomics proposee.

Au 31 mars 2026, cette offre totale ne doit pas etre interpretee comme un parametre genesis deja verrouille dans un lancement mainnet effectif. La roadmap indique encore que les parametres du bloc genesis restent a definir en phase 11. L offre totale doit donc etre lue comme une cible de conception a formaliser definitivement avant le lancement.

### 9.3 Répartition proposée

La presentation du projet propose la repartition suivante :
- 60% staking ;
- 20% tresorerie DAO ;
- 10% premiers contributeurs ;
- 10% fondateur, avec exigence de transparence.

A ce stade, cette repartition releve du cadrage whitepaper/presentation, pas d une distribution deja executee sur un reseau lance. Elle devra etre reprise, precisee et verrouillee au moment de la definition du genesis et du cadre de lancement mainnet.

### 9.4 Usage du token dans le réseau

Sur le plan fonctionnel, plusieurs usages du token sont deja visibles ou prepares dans le prototype :
- transfert de valeur entre comptes ;
- staking et unstaking ;
- paiement de frais de transaction ;
- base de calcul de certaines recompenses validateurs.

En revanche, d autres usages relevent encore d etapes ulterieures, notamment la gouvernance on-chain complete, la formalisation definitive du lancement mainnet et la mise en place operationnelle des mecanismes associes au genesis. Il faut donc distinguer usage technique deja prototype et economie reseau completement finalisee.

### 9.5 Récompenses et incitations

Le prototype implemente deja une logique de recompenses pour les validateurs ayant effectivement vote. Cette logique combine les frais de transaction et une inflation annuelle parametree a 2% via `ANNUAL_INFLATION_BPS = 200`. Le projet a aussi valide des frais fixes via `FIXED_TRANSACTION_FEE`, sans priorite par frais.

Cela signifie que Prims dispose deja d une base economique minimale executable dans le prototype. En revanche, cela ne suffit pas encore a decrire une tokenomics mainnet complete, car il reste a formaliser la distribution initiale, les parametres de genesis, les conditions de lancement et les mecanismes de gouvernance associes.

### 9.6 Ce qui est déjà décidé vs ce qui reste à formaliser

Ce qui est deja fixe au niveau de la vision ou du prototype :
- une cible d offre totale a 1 milliard PRIMS dans la presentation ;
- une repartition proposee entre staking, tresorerie DAO, premiers contributeurs et fondateur ;
- l existence technique de transferts, stake, unstake, frais fixes et recompenses avec inflation annuelle de 2% dans le prototype.

Ce qui reste encore a formaliser avant le mainnet :
- les parametres exacts du bloc genesis ;
- la traduction definitive de la repartition proposee en allocations executable ;
- les regles finales de lancement et de distribution initiale ;
- l articulation precise entre tokenomics et gouvernance on-chain ;
- les decisions finales qui relevent encore des etapes 10.9, 10.10 et de la phase 11.

---

## 10. Gouvernance

### 10.1 Gouvernance visée à long terme

Dans la vision du projet, Prims vise une gouvernance de type DAO. Cette orientation apparait deja dans la presentation du projet, avec l idee d une tresorerie DAO et d une gouvernance progressive du reseau. L objectif de long terme est donc que les grandes evolutions du protocole, de la tresorerie et des regles collectives ne reposent pas durablement sur un controle centralise unique.

Cette cible doit toutefois etre lue comme une direction strategique de long terme, pas comme une realite deja deployee dans le prototype actuel.

### 10.2 Place de la DAO dans la roadmap

La roadmap situe explicitement la gouvernance on-chain en phase 11. Elle prevoit d abord la definition des parametres du bloc genesis, le lancement du reseau avec un petit groupe de validateurs de confiance, puis l ouverture plus large au public. L implementation de la gouvernance on-chain DAO et le transfert progressif du controle a cette DAO interviennent ensuite, pas avant.

Cela signifie que la DAO n est pas un composant a considerer comme deja actif dans l etat courant du projet. Elle appartient encore au chemin de maturation qui suit la finalisation de la phase 10 et la preparation du mainnet.

### 10.3 État actuel

Au 31 mars 2026, l etat actuel du projet correspond encore a l etape 10.8 en cours. Le prototype dispose deja de briques techniques importantes, d un cadre documentaire en cours de consolidation et d une preparation testnet/mainnet serieuse, mais pas encore d une gouvernance on-chain deployee.

En pratique, cela signifie qu il existe deja une intention claire de decentralisation progressive et de transfert futur de controle, mais que les mecanismes concrets de vote on-chain, de tresorerie DAO et de gouvernance protocolaire restent a implementer et a formaliser dans les etapes ulterieures.

### 10.4 Transition progressive vers le mainnet

La transition de gouvernance envisagee par Prims est volontairement progressive. Le projet ne cherche pas a declarer une gouvernance totalement decentralisee avant d avoir termine le whitepaper, la documentation, les derniers audits, la definition du genesis et les conditions initiales de lancement du reseau.

Cette prudence est coherente avec le reste du projet : d abord consolider le prototype, ensuite finaliser la preparation testnet/mainnet, puis seulement deplacer progressivement le centre de gravite du controle vers des mecanismes plus ouverts et plus communautaires. La gouvernance doit donc etre comprise comme une trajectoire de livraison et de maturation, pas comme un statut deja atteint.

---

## 11. Roadmap à partir de l état actuel

### 11.1 Étape 10.8 : whitepaper

Au moment de rediger ce document, l etape 10.8 est celle qui est effectivement en cours dans le journal local. Son objectif est de formaliser proprement la vision, l architecture technique, l etat reel du prototype, la tokenomics proposee, la gouvernance visee et les limites actuelles du projet.

Cette etape est importante car elle sert de point de clarification entre ce qui est deja valide dans le code et ce qui reste encore a accomplir avant le mainnet. Le whitepaper ne remplace pas les validations techniques, mais il structure la lecture du projet et prepare les etapes suivantes.

### 11.2 Étape 10.9 : documentation et tutoriels

Apres le whitepaper, la roadmap prevoit une phase de documentation plus directement exploitable par les developpeurs et les utilisateurs. Cela inclut la documentation du code, ainsi que des tutoriels sous forme texte ou video.

Cette etape a pour role de transformer un prototype techniquement riche en un systeme plus accessible, plus transmissible et plus facile a reproduire. Elle constitue une condition importante pour sortir d une logique purement interne de validation et approcher un usage testnet plus large.

### 11.3 Étape 10.10 : derniers audits et tests de pénétration

Une fois le whitepaper et la documentation consolides, la roadmap prevoit une derniere sequence de securite avant la phase mainnet. Cette sequence doit completer le travail deja effectue en 10.7 par des audits supplementaires et des tests de penetration.

L objectif n est pas seulement de verifier une fois de plus le code, mais de reduire au maximum l ecart entre un prototype durci et un systeme suffisamment prepare pour une exposition plus large. Cette etape reste donc critique avant toute bascule vers la phase 11.

### 11.4 Phase 11 : préparation du mainnet

La phase 11 marque le passage d un prototype consolide vers un reseau vivant. D apres la roadmap, elle comprend notamment le gel du code sur une branche mainnet, la definition des parametres du bloc genesis, le lancement initial avec un petit groupe de validateurs de confiance, l ouverture progressive au public, puis la mise en place de la gouvernance on-chain et le transfert progressif du controle vers la DAO.

Cette phase ne doit pas etre lue comme deja engagee dans l etat actuel du projet. Elle represente la suite logique apres la finalisation des etapes 10.8, 10.9 et 10.10. Autrement dit, au 31 mars 2026, Prims se situe encore avant le mainnet, dans une phase de consolidation finale.

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
