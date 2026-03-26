# API JSON-RPC de Prims

## Objectif

Documentation de l API JSON-RPC actuellement exposee par le noeud Prims.

## Adresse par defaut

- RPC : `http://127.0.0.1:7002`
- P2P : `127.0.0.1:7001`
- Explorateur web : `127.0.0.1:7003`

## Regles generales

- Toutes les requetes utilisent POST.
- Le header recommande est `Content-Type: application/json`.
- Les parametres doivent etre passes dans `params` sous forme d objet JSON.
- Les valeurs hexadecimales peuvent etre fournies avec ou sans prefixe `0x`.
- Le rate limit RPC par defaut est `20` requetes par seconde.
- Les methodes retournent soit un objet JSON, soit une liste JSON, soit `null` si l objet demande n existe pas.

## `get_info`

Retourne des informations generales sur le noeud.

### Requete

```bash
curl -s http://127.0.0.1:7002 \
  -H "content-type: application/json" \
  -d "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"get_info\",\"params\":{}}"
```

### Reponse

Champs retournes :
- `name`
- `rpc_version`
- `storage_backend`
- `latest_block_height`
- `mempool_size`
- `validator_count`
- `note_commitment_count`

## `get_validators`

Retourne la liste des validateurs connus.

### Requete

```bash
curl -s http://127.0.0.1:7002 \
  -H "content-type: application/json" \
  -d "{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"get_validators\",\"params\":{}}"
```

### Reponse

Chaque entree contient :
- `address`
- `stake`
- `locked_until`

## `get_note_commitments`

Retourne la liste des commitments anonymes enregistres.

### Requete

```bash
curl -s http://127.0.0.1:7002 \
  -H "content-type: application/json" \
  -d "{\"jsonrpc\":\"2.0\",\"id\":3,\"method\":\"get_note_commitments\",\"params\":{}}"
```

### Reponse

Liste JSON de chaines hexadecimales.

## `get_balance`

Retourne le solde public et quelques metadonnees non sensibles d un compte.

### Parametres

- `address` : adresse hexadecimale du compte

### Requete

```bash
curl -s http://127.0.0.1:7002 \
  -H "content-type: application/json" \
  -d "{\"jsonrpc\":\"2.0\",\"id\":4,\"method\":\"get_balance\",\"params\":{\"address\":\"0123456789abcdef\"}}"
```

### Reponse

Champs retournes :
- `address`
- `found`
- `balance`
- `nonce`
- `note_commitment_count`

Si l adresse n existe pas :
- `found = false`
- `balance = 0`
- `nonce = 0`
- `note_commitment_count = 0`

Metadonnees volontairement omises :
- `viewing_hint` n est jamais expose par l API RPC publique.

## `get_block`

Retourne un bloc par hauteur ou par hash.

### Parametres

Utiliser exactement un seul selecteur :
- `height`
- `hash`

### Requete par hauteur

```bash
curl -s http://127.0.0.1:7002 \
  -H "content-type: application/json" \
  -d "{\"jsonrpc\":\"2.0\",\"id\":5,\"method\":\"get_block\",\"params\":{\"height\":0}}"
```

### Requete par hash

```bash
curl -s http://127.0.0.1:7002 \
  -H "content-type: application/json" \
  -d "{\"jsonrpc\":\"2.0\",\"id\":6,\"method\":\"get_block\",\"params\":{\"hash\":\"abcdef0123456789\"}}"
```

### Reponse

Le resultat contient :
- `hash`
- `header`
- `transactions`
- `receipts`

Le champ `header` contient :
- `version`
- `previous_hash`
- `merkle_root`
- `timestamp`
- `height`
- `validator`
- `signature`

Chaque transaction contient :
- `tx_type`
- `stake_duration`
- `from`
- `to`
- `amount`
- `fee`
- `nonce`
- `source_shard`
- `destination_shard`
- `signature`
- `data`

Chaque recu cross-shard contient :
- `tx_hash`
- `source_shard`
- `destination_shard`
- `phase`
- `proof`

### Comportement important

- si le bloc n existe pas, le resultat est `null`
- si `height` et `hash` sont fournis ensemble, la requete est rejetee
- si aucun des deux n est fourni, la requete est rejetee

## `get_transaction`

Retourne une transaction par hash.

### Parametres

- `hash` : hash hexadecimale de la transaction

### Requete

```bash
curl -s http://127.0.0.1:7002 \
  -H "content-type: application/json" \
  -d "{\"jsonrpc\":\"2.0\",\"id\":7,\"method\":\"get_transaction\",\"params\":{\"hash\":\"abcdef0123456789\"}}"
```

### Reponse

Le resultat contient :
- `hash`
- `transaction`

Le champ `transaction` reprend la vue RPC des transactions :
- `tx_type`
- `stake_duration`
- `from`
- `to`
- `amount`
- `fee`
- `nonce`
- `source_shard`
- `destination_shard`
- `signature`
- `data`

### Comportement important

- si la transaction n existe pas, le resultat est `null`

## `send_transaction`

Injecte une transaction dans la mempool.

### Parametres

- `hex_tx` : transaction serialisee en `bincode`, puis encodee en hexadecimale

### Requete

```bash
curl -s http://127.0.0.1:7002 \
  -H "content-type: application/json" \
  -d "{\"jsonrpc\":\"2.0\",\"id\":8,\"method\":\"send_transaction\",\"params\":{\"hex_tx\":\"HEX_BINCODAGE_D_UNE_TRANSACTION_SIGNEE\"}}"
```

### Reponse

Champs retournes :
- `accepted`
- `tx_hash`
- `mempool_size`

### Validations metier actuelles

- format hexadecimale de `hex_tx`
- deserialisation `bincode`
- taille de la transaction
- nonce
- solde pour `Transfer`, `Stake`, `PublicToAnon` et `AnonToPublic`
- pour `Unstake`, frais fixe attendu et solde suffisant pour payer ce frais

### Types de transaction visibles cote RPC

- `Transfer`
- `Stake`
- `Unstake`
- `PublicToAnon`
- `AnonToPublic`

Pour `Stake`, `stake_duration` contient la duree de verrouillage.
Pour les autres types, `stake_duration` vaut `null`.

## Gestion des erreurs

### Erreurs de parametres

- hexadecimale invalide
- objet `params` incomplet
- mauvaise combinaison de selecteurs dans `get_block`

### Rate limit

- code : `-32029`
- message : `rate limit exceeded`

### Erreurs de validation metier

- nonce invalide
- solde insuffisant
- frais invalides
- transaction trop grosse
- exemple observe : `transaction rejected: insufficient balance for fee`

## Notes de securite

- Ne jamais coller de cle privee, mot de passe ou token dans une requete RPC.
- `send_transaction` attend une transaction deja signee et serialisee.
- La documentation ne doit jamais contenir de secret reutilisable.

## Fichiers lies

- `src/api/mod.rs`
- `src/bin/prims-cli.rs`
- `src/bin/prims-explorer.rs`
- `tests/rpc_api.rs`
