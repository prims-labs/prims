# Suivi des constats d audit securite - Prims

## Objectif

Centraliser les retours recus pendant l etape 10.7, suivre leur qualification, leur reproduction, leur correction et leur validation.

## Source de l appel a audit

- Issue GitHub publique : https://github.com/prims-labs/prims/issues/1
- Cadre interne : `docs/security_audit.md`
- Appel public : `docs/security_audit_call.md`
- Bug bounty testnet : `docs/bug_bounty.md`

## Statut actuel

Aucune faille exploitable recue et documentee pour le moment.
Un premier commentaire non exploitable a ete recu sur l issue publique d appel a audit et a fait l objet d une demande de details.


## Constats audit interne

- 30 mars 2026 : audit interne RPC en cours. Constat sensible : `send_transaction` peut executer un `CallContract` via `WasmVM::execute_contract_call(...)` dans le chemin RPC avant ajout en mempool, ce qui en fait une surface de risque elevee cote charge applicative. Verification des protections existantes : les tests RPC couvrent deja l execution valide, le rejet sur trap avec rollback du storage, et le rejet si `gas_limit = 0`. Un test complementaire a ete ajoute et valide : `send_transaction_is_rate_limited_after_quota_is_exhausted`, confirmant que `send_transaction` est bien soumis au rate limiting RPC.
- 30 mars 2026 : faille de validation confirmee cote RPC. `send_transaction` n effectuait pas de verification cryptographique explicite de la signature avant acceptation. Correctif applique dans `src/api/mod.rs` par ajout de `verify_transaction(...)` avant chargement du compte expediteur et avant validation metier. Impact estime : acceptation potentielle de transactions falsifiees via RPC si les autres couches ne filtraient pas ensuite. Validation realisee : correction appliquee, tests RPC adaptes pour utiliser de vraies signatures Ed25519, puis `cargo test --test rpc_api -- --nocapture` valide avec `8 passed; 0 failed`, incluant le test de non-regression `send_transaction_rejects_invalid_signature`, et `cargo test --test cli_integration -- --nocapture` valide avec `9 passed; 0 failed`. Statut actuel : corrige et valide pour la couche RPC et pour l integration CLI -> RPC, analyse RPC globale encore en cours.

- 30 mars 2026 : audit interne VM Wasm avance. Verification ciblee de `execute_contract_call` : la suite de tests valide l execution normale avec mise a jour du storage, le rollback sur trap, le rollback sur epuisement du fuel, le rollback sur acces memoire hors bornes, et un test complementaire ajoute pendant l audit valide aussi le rollback sur offset memoire negatif (`execute_contract_call_rolls_back_storage_on_negative_memory_offset`). Validation realisee : `cargo test execute_contract_call -- --nocapture` OK avec `5 passed; 0 failed`. Statut actuel : aucune faille critique confirmee a ce stade sur cette sous-partie VM, analyse VM globale encore en cours.
- 30 mars 2026 : audit interne reseau avance sur l identite reseau persistante. Verification ciblee de `decode_hex_32` utilise par `PRIMS_NETWORK_SECRET_KEY_HEX` et `PRIMS_NETWORK_SECRET_KEY_FILE` : couverture renforcee avec tests sur cas valide, longueur invalide et entree non hexadecimale (`decode_hex_32_accepts_valid_32_byte_hex`, `decode_hex_32_rejects_invalid_length`, `decode_hex_32_rejects_non_hex_input`). Validation realisee : `cargo test decode_hex_32 -- --nocapture` OK avec `3 passed; 0 failed`. Statut actuel : parseur de cle reseau mieux verrouille, analyse reseau globale encore en cours.
- 30 mars 2026 : audit interne consensus avance sur la verification cryptographique des votes. Couverture renforcee avec deux tests complementaires : `verify_vote_rejects_invalid_voter_key_length` et `verify_vote_rejects_invalid_signature_length`. Validation realisee : `cargo test verify_vote -- --nocapture` OK avec `4 passed; 0 failed`. Statut actuel : verification des signatures de vote mieux verrouillee, analyse consensus globale encore en cours.
- 30 mars 2026 : audit interne sharding avance sur la validation des routes cross-shard. Couverture renforcee avec deux tests complementaires : `prepare_rejects_invalid_source_shard` et `prepare_rejects_invalid_destination_shard`. Validation realisee : `cargo test prepare_rejects_invalid_ -- --nocapture` OK avec `2 passed; 0 failed`. Statut actuel : validation des routes cross-shard mieux verrouillee, analyse sharding globale encore en cours.
- 30 mars 2026 : audit interne confidentialite avance sur `convert_anon_to_public`. Couverture renforcee avec le test complementaire `anon_to_public_rejects_invalid_ownership_proof`, validant le rejet d un mismatch entre la note reconstruite et la feuille Merkle annoncee. Validation realisee : `cargo test anon_to_public -- --nocapture` OK avec `4 passed; 0 failed`. Statut actuel : verification de consommation de note mieux verrouillee, analyse confidentialite globale encore en cours.
- 30 mars 2026 : validation globale de non-regression effectuee apres les durcissements 10.7. Execution complete de `cargo test -- --nocapture` terminee avec succes : `182 passed; 0 failed` sur la lib, `9 passed; 0 failed` sur `cli_integration`, `8 passed; 0 failed` sur `rpc_api`, aucun test en echec. Statut actuel : socle technique consolide apres audit interne pousse, mais etape 10.7 encore ouverte tant que la revue finale, la consolidation documentaire complete et la decision de verrouillage Git/GitHub ne sont pas terminees.
## Historique des interactions initiales

- 29 mars 2026 : tentative de mise en place de labels GitHub de triage securite (`security`, severites, `status:needs-triage`) refusee par l API GitHub ; verification `gh repo view` confirmee avec `viewerPermission = READ` sur `prims-labs/prims`, donc impossibilite de gerer les labels du depot depuis ce compte.

- 29 mars 2026 : commentaire GitHub recu sur l issue publique `#1` par `zhaog100` avec le contenu `/attempt` ; message non exploitable en l etat, interprete comme test ou commentaire parasite leger.
- 29 mars 2026 : reponse publiee sur l issue pour demander un rapport structure (composant, severite, preconditions, reproduction, impact, preuve de concept minimale).

## Mode d utilisation

Pour chaque retour de securite :
1. attribuer un identifiant interne
2. noter la date de reception
3. noter la source (issue GitHub, auditeur externe, autre canal prive autorise)
4. qualifier la severite
5. decrire la reproduction
6. evaluer l impact
7. decider de la mitigation ou du correctif
8. lier le commit de correction
9. valider le correctif
10. documenter la cloture

## Tableau de suivi

| ID | Date | Source | Composant | Severite | Statut | Commit correctif | Validation |
|----|------|--------|-----------|----------|--------|------------------|------------|
| A renseigner | A renseigner | A renseigner | A renseigner | A renseigner | Ouvert | A renseigner | A renseigner |

## Statuts recommandes

- Ouvert
- En analyse
- Reproduit
- Corrige
- Valide
- Clos

## Notes

- Ne jamais inclure de secret, cle privee, mot de passe ou token.
- Reduire toute preuve de concept au strict necessaire.
- Conserver la coordination de divulgation tant que le correctif n est pas valide.
