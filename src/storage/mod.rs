pub mod keys;

use crate::blockchain::hash::{
    contract_code_hash, derive_contract_address, hash_block_header, hash_transaction, sha256,
};
use crate::blockchain::types::{
    Account, AnonymousAccountState, Block, Contract, Transaction, Validator,
};
use crate::privacy::{NoteMerkleHash, NoteViewingKey, ViewableNote};
use anyhow::{Result, anyhow};
use rocksdb::{DB, Direction, IteratorMode, Options, WriteBatch};
use std::path::Path;

pub trait StorageBackend {
    fn kind(&self) -> &'static str;
    fn put(&self, key: &[u8], value: &[u8]) -> Result<()>;
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>>;
    fn delete(&self, key: &[u8]) -> Result<()>;
    fn iter(&self, prefix: &[u8]) -> Result<Vec<(Vec<u8>, Vec<u8>)>>;
}

pub struct RocksDbStorage {
    db: DB,
}

impl RocksDbStorage {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let mut options = Options::default();
        options.create_if_missing(true);

        let db = DB::open(&options, path)?;
        Ok(Self { db })
    }

    pub fn db(&self) -> &DB {
        &self.db
    }

    pub fn save_block(&self, block: &Block) -> Result<Vec<u8>> {
        let block_hash = hash_block_header(&block.header);
        let block_key = keys::block_key(block.header.height);
        let height_index_key = keys::height_index_key(&block_hash);

        self.put(&block_key, &bincode::serialize(block)?)?;
        self.put(
            &height_index_key,
            &bincode::serialize(&block.header.height)?,
        )?;

        Ok(block_hash)
    }

    pub fn get_block(&self, height: u64) -> Result<Option<Block>> {
        match self.get(&keys::block_key(height))? {
            Some(bytes) => Ok(Some(bincode::deserialize(&bytes)?)),
            None => Ok(None),
        }
    }

    pub fn save_transaction(&self, transaction: &Transaction) -> Result<Vec<u8>> {
        let tx_hash = hash_transaction(transaction);
        let tx_key = keys::transaction_key(&tx_hash);

        self.put(&tx_key, &bincode::serialize(transaction)?)?;
        Ok(tx_hash)
    }

    pub fn get_transaction(&self, hash: &[u8]) -> Result<Option<Transaction>> {
        match self.get(&keys::transaction_key(hash))? {
            Some(bytes) => Ok(Some(bincode::deserialize(&bytes)?)),
            None => Ok(None),
        }
    }

    pub fn update_account(&self, address: &[u8], account: &Account) -> Result<()> {
        self.put(&keys::account_key(address), &bincode::serialize(account)?)
    }

    pub fn get_account(&self, address: &[u8]) -> Result<Option<Account>> {
        match self.get(&keys::account_key(address))? {
            Some(bytes) => Ok(Some(bincode::deserialize(&bytes)?)),
            None => Ok(None),
        }
    }

    pub fn deploy_contract(&self, sender: &[u8], code_wasm: &[u8]) -> Result<Vec<u8>> {
        if sender.is_empty() {
            return Err(anyhow!("sender address must not be empty"));
        }

        if code_wasm.is_empty() {
            return Err(anyhow!("contract bytecode must not be empty"));
        }

        let contract_address = derive_contract_address(sender, code_wasm);

        if self.get_account(&contract_address)?.is_some()
            || self.get_contract(&contract_address)?.is_some()
        {
            return Err(anyhow!("contract address already exists"));
        }

        let contract = Contract {
            code_wasm: code_wasm.to_vec(),
            storage_root: sha256(&[]),
        };
        let account = Account {
            balance: 0,
            nonce: 0,
            code_hash: Some(contract_code_hash(code_wasm)),
            anonymous_state: AnonymousAccountState::default(),
        };

        self.update_contract(&contract_address, &contract)?;
        self.update_account(&contract_address, &account)?;

        Ok(contract_address)
    }

    pub fn update_contract(&self, address: &[u8], contract: &Contract) -> Result<()> {
        self.put(&keys::contract_key(address), &bincode::serialize(contract)?)
    }

    pub fn get_contract(&self, address: &[u8]) -> Result<Option<Contract>> {
        match self.get(&keys::contract_key(address))? {
            Some(bytes) => Ok(Some(bincode::deserialize(&bytes)?)),
            None => Ok(None),
        }
    }

    pub fn set_contract_storage(&self, address: &[u8], key: &[u8], value: &[u8]) -> Result<()> {
        let mut contract = self
            .get_contract(address)?
            .ok_or_else(|| anyhow!("contract does not exist"))?;

        self.put(&keys::contract_storage_key(address, key), value)?;
        contract.storage_root = self.compute_contract_storage_root(address)?;
        self.update_contract(address, &contract)
    }

    pub fn get_contract_storage(&self, address: &[u8], key: &[u8]) -> Result<Option<Vec<u8>>> {
        self.get_contract(address)?
            .ok_or_else(|| anyhow!("contract does not exist"))?;
        self.get(&keys::contract_storage_key(address, key))
    }

    pub fn commit_contract_storage_batch(
        &self,
        address: &[u8],
        writes: &std::collections::BTreeMap<Vec<u8>, Vec<u8>>,
    ) -> Result<()> {
        if writes.is_empty() {
            return Ok(());
        }

        let mut contract = self
            .get_contract(address)?
            .ok_or_else(|| anyhow!("contract does not exist"))?;

        let prefix = keys::contract_storage_prefix(address);
        let mut slot_entries = std::collections::BTreeMap::new();

        for (storage_key, value) in self.iter(&prefix)? {
            let slot_key = storage_key[prefix.len()..].to_vec();
            slot_entries.insert(slot_key, value);
        }

        for (key, value) in writes {
            slot_entries.insert(key.clone(), value.clone());
        }

        let mut payload = Vec::new();
        for (slot_key, value) in slot_entries {
            let slot_key_hash = sha256(&slot_key);
            let value_hash = sha256(&value);
            payload.extend_from_slice(&slot_key_hash);
            payload.extend_from_slice(&value_hash);
        }

        contract.storage_root = if payload.is_empty() {
            sha256(&[])
        } else {
            sha256(&payload)
        };

        let mut batch = WriteBatch::default();
        for (key, value) in writes {
            batch.put(keys::contract_storage_key(address, key), value);
        }
        batch.put(
            &keys::contract_key(address),
            &bincode::serialize(&contract)?,
        );

        self.db.write(batch)?;
        Ok(())
    }

    fn compute_contract_storage_root(&self, address: &[u8]) -> Result<Vec<u8>> {
        let prefix = keys::contract_storage_prefix(address);
        let mut entries = self.iter(&prefix)?;
        entries.sort_by(|left, right| left.0.cmp(&right.0));

        if entries.is_empty() {
            return Ok(sha256(&[]));
        }

        let mut payload = Vec::new();

        for (storage_key, value) in entries {
            let slot_key = &storage_key[prefix.len()..];
            let slot_key_hash = sha256(slot_key);
            let value_hash = sha256(&value);

            payload.extend_from_slice(&slot_key_hash);
            payload.extend_from_slice(&value_hash);
        }

        Ok(sha256(&payload))
    }

    pub fn save_viewable_note(&self, note: &ViewableNote) -> Result<()> {
        let commitment = note.commitment();

        self.put(
            &keys::anonymous_note_key(&commitment),
            &bincode::serialize(note)?,
        )?;
        self.put(
            &keys::viewing_hint_note_key(&note.viewing_hint, &commitment),
            b"1",
        )?;

        Ok(())
    }

    pub fn get_viewable_note(&self, commitment: &[u8]) -> Result<Option<ViewableNote>> {
        match self.get(&keys::anonymous_note_key(commitment))? {
            Some(bytes) => Ok(Some(bincode::deserialize(&bytes)?)),
            None => Ok(None),
        }
    }

    pub fn find_notes_for_viewing_key(
        &self,
        viewing_key: &NoteViewingKey,
    ) -> Result<Vec<ViewableNote>> {
        let Some(viewing_hint) = ViewableNote::compute_viewing_hint(viewing_key) else {
            return Ok(Vec::new());
        };

        let prefix = keys::viewing_hint_prefix(&viewing_hint);
        let mut notes = Vec::new();

        for (key, _) in self.iter(&prefix)? {
            let commitment = &key[prefix.len()..];
            if let Some(note) = self.get_viewable_note(commitment)? {
                notes.push(note);
            }
        }

        Ok(notes)
    }

    pub fn save_note_merkle_root(
        &self,
        shard_id: Option<u16>,
        root: &NoteMerkleHash,
    ) -> Result<()> {
        self.put(
            &keys::note_merkle_root_key(shard_id),
            &bincode::serialize(root)?,
        )
    }

    pub fn get_note_merkle_root(&self, shard_id: Option<u16>) -> Result<Option<NoteMerkleHash>> {
        match self.get(&keys::note_merkle_root_key(shard_id))? {
            Some(bytes) => Ok(Some(bincode::deserialize(&bytes)?)),
            None => Ok(None),
        }
    }

    pub fn save_validator(&self, validator: &Validator) -> Result<()> {
        self.put(
            &keys::stake_key(&validator.address),
            &bincode::serialize(validator)?,
        )
    }

    pub fn get_validator(&self, address: &[u8]) -> Result<Option<Validator>> {
        match self.get(&keys::stake_key(address))? {
            Some(bytes) => Ok(Some(bincode::deserialize(&bytes)?)),
            None => Ok(None),
        }
    }
}

impl StorageBackend for RocksDbStorage {
    fn kind(&self) -> &'static str {
        "rocksdb"
    }

    fn put(&self, key: &[u8], value: &[u8]) -> Result<()> {
        self.db.put(key, value)?;
        Ok(())
    }

    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        Ok(self.db.get(key)?.map(|value| value.to_vec()))
    }

    fn delete(&self, key: &[u8]) -> Result<()> {
        self.db.delete(key)?;
        Ok(())
    }

    fn iter(&self, prefix: &[u8]) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
        let mut entries = Vec::new();

        for item in self
            .db
            .iterator(IteratorMode::From(prefix, Direction::Forward))
        {
            let (key, value) = item?;
            if !key.starts_with(prefix) {
                break;
            }

            entries.push((key.to_vec(), value.to_vec()));
        }

        Ok(entries)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blockchain::types::{AnonymousAccountState, Contract, FIXED_TRANSACTION_FEE};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_path(label: &str) -> std::path::PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after unix epoch")
            .as_nanos();

        std::env::temp_dir().join(format!("prims-storage-{label}-{unique}"))
    }

    fn sample_transaction() -> Transaction {
        Transaction {
            tx_type: crate::blockchain::types::TransactionType::Transfer,
            from: vec![1, 2, 3],
            to: vec![4, 5, 6],
            amount: 42,
            fee: FIXED_TRANSACTION_FEE,
            nonce: 7,
            source_shard: 0,
            destination_shard: 0,
            signature: vec![9, 9, 9],
            data: Some(vec![8, 8]),
        }
    }

    fn sample_block() -> Block {
        Block {
            header: crate::blockchain::types::BlockHeader {
                version: 1,
                previous_hash: vec![0; 32],
                merkle_root: vec![1; 32],
                timestamp: 1_710_000_000,
                height: 1,
                validator: vec![7; 32],
                signature: vec![6; 64],
            },
            transactions: vec![sample_transaction()],
            receipts: vec![],
        }
    }

    fn sample_validator() -> Validator {
        Validator {
            address: vec![3; 32],
            stake: 50_000,
            locked_until: 1_710_086_400,
        }
    }

    fn sample_contract() -> Contract {
        Contract {
            code_wasm: b"\0asm\x01\0\0\0".to_vec(),
            storage_root: crate::blockchain::hash::sha256(&[]),
        }
    }

    #[test]
    fn deploy_contract_persists_code_and_contract_account() {
        let path = temp_path("deploy-contract");
        let storage = RocksDbStorage::open(&path).expect("rocksdb should open successfully");
        let sender = vec![9; 32];
        let code_wasm = b"\0asm\x01\0\0\0".to_vec();

        let address = storage
            .deploy_contract(&sender, &code_wasm)
            .expect("deploy contract");

        let contract = storage
            .get_contract(&address)
            .expect("get deployed contract")
            .expect("contract should exist");
        let account = storage
            .get_account(&address)
            .expect("get deployed account")
            .expect("account should exist");

        assert_eq!(contract.code_wasm, code_wasm);
        assert_eq!(contract.storage_root, crate::blockchain::hash::sha256(&[]));
        assert_eq!(
            account.code_hash,
            Some(crate::blockchain::hash::contract_code_hash(
                &contract.code_wasm
            ))
        );
        assert_eq!(account.balance, 0);
        assert_eq!(account.nonce, 0);

        drop(storage);
        std::fs::remove_dir_all(&path).ok();
    }

    #[test]
    fn deploy_contract_rejects_empty_bytecode() {
        let path = temp_path("deploy-contract-empty");
        let storage = RocksDbStorage::open(&path).expect("rocksdb should open successfully");

        let error = storage
            .deploy_contract(&[7; 32], &[])
            .expect_err("empty bytecode must be rejected");

        assert!(
            error
                .to_string()
                .contains("contract bytecode must not be empty")
        );

        drop(storage);
        std::fs::remove_dir_all(&path).ok();
    }

    #[test]
    fn deploy_contract_rejects_same_sender_and_code_twice() {
        let path = temp_path("deploy-contract-duplicate");
        let storage = RocksDbStorage::open(&path).expect("rocksdb should open successfully");
        let sender = vec![5; 32];
        let code_wasm = b"\0asm\x01\0\0\0".to_vec();

        let first_address = storage
            .deploy_contract(&sender, &code_wasm)
            .expect("first deploy should succeed");

        let error = storage
            .deploy_contract(&sender, &code_wasm)
            .expect_err("duplicate derived address must be rejected");

        assert_eq!(
            first_address,
            crate::blockchain::hash::derive_contract_address(&sender, &code_wasm)
        );
        assert!(
            error
                .to_string()
                .contains("contract address already exists")
        );

        drop(storage);
        std::fs::remove_dir_all(&path).ok();
    }

    #[test]
    fn saves_and_reads_validator() {
        let path = temp_path("validator-crud");
        let storage = RocksDbStorage::open(&path).expect("rocksdb should open successfully");
        let validator = sample_validator();

        storage.save_validator(&validator).expect("save validator");

        assert_eq!(
            storage
                .get_validator(&validator.address)
                .expect("get validator"),
            Some(validator.clone())
        );

        drop(storage);
        std::fs::remove_dir_all(&path).ok();
    }

    #[test]
    fn persists_validator_after_restart() {
        let path = temp_path("validator-restart");
        let validator = sample_validator();

        {
            let storage = RocksDbStorage::open(&path).expect("open storage before restart");
            storage
                .save_validator(&validator)
                .expect("save validator before restart");
        }

        let reopened = RocksDbStorage::open(&path).expect("open storage after restart");
        assert_eq!(
            reopened
                .get_validator(&validator.address)
                .expect("get validator after restart"),
            Some(validator)
        );

        drop(reopened);
        std::fs::remove_dir_all(&path).ok();
    }

    #[test]
    fn opens_rocksdb_storage() {
        let path = temp_path("open");

        let storage = RocksDbStorage::open(&path).expect("rocksdb should open successfully");
        assert_eq!(storage.kind(), "rocksdb");

        drop(storage);
        std::fs::remove_dir_all(&path).ok();
    }

    #[test]
    fn storage_put_get_delete_and_iter() {
        let path = temp_path("crud");
        let storage = RocksDbStorage::open(&path).expect("rocksdb should open successfully");

        let alice = keys::account_key(b"alice");
        let bob = keys::account_key(b"bob");
        let block = keys::block_key(1);

        storage.put(&alice, b"100").expect("put alice");
        storage.put(&bob, b"250").expect("put bob");
        storage.put(&block, b"block-1").expect("put block");

        assert_eq!(
            storage.get(&alice).expect("get alice"),
            Some(b"100".to_vec())
        );
        assert_eq!(storage.get(&bob).expect("get bob"), Some(b"250".to_vec()));
        assert_eq!(storage.get(b"missing").expect("get missing"), None);

        let accounts = storage
            .iter(keys::ACCOUNT_PREFIX.as_bytes())
            .expect("iterate accounts");

        assert_eq!(accounts.len(), 2);
        assert!(
            accounts
                .iter()
                .any(|(key, value)| key == &alice && value == b"100")
        );
        assert!(
            accounts
                .iter()
                .any(|(key, value)| key == &bob && value == b"250")
        );

        storage.delete(&alice).expect("delete alice");
        assert_eq!(storage.get(&alice).expect("get alice after delete"), None);

        drop(storage);
        std::fs::remove_dir_all(&path).ok();
    }

    #[test]
    fn storage_high_level_block_transaction_account_roundtrip() {
        let path = temp_path("high-level");
        let storage = RocksDbStorage::open(&path).expect("rocksdb should open successfully");

        let block = sample_block();
        let tx = block.transactions[0].clone();
        let account = Account {
            balance: 1_000,
            nonce: 2,
            code_hash: Some(vec![3; 32]),
            anonymous_state: AnonymousAccountState::default(),
        };

        let block_hash = storage.save_block(&block).expect("save block");
        let tx_hash = storage.save_transaction(&tx).expect("save transaction");
        storage
            .update_account(b"alice", &account)
            .expect("update account");

        assert_eq!(
            storage.get_block(block.header.height).expect("get block"),
            Some(block.clone())
        );
        assert_eq!(
            storage.get_transaction(&tx_hash).expect("get transaction"),
            Some(tx.clone())
        );
        assert_eq!(
            storage.get_account(b"alice").expect("get account"),
            Some(account.clone())
        );

        let stored_height = storage
            .get(&keys::height_index_key(&block_hash))
            .expect("get height index")
            .expect("height index should exist");
        let decoded_height: u64 = bincode::deserialize(&stored_height).expect("deserialize height");
        assert_eq!(decoded_height, block.header.height);

        drop(storage);
        std::fs::remove_dir_all(&path).ok();
    }

    #[test]
    fn contract_storage_roundtrip_updates_storage_root() {
        let path = temp_path("contract-storage");
        let storage = RocksDbStorage::open(&path).expect("rocksdb should open successfully");
        let initial_contract = sample_contract();

        storage
            .update_contract(b"contract-1", &initial_contract)
            .expect("save contract");

        storage
            .set_contract_storage(b"contract-1", b"counter", b"1")
            .expect("set counter");
        storage
            .set_contract_storage(b"contract-1", b"owner", b"alice")
            .expect("set owner");

        assert_eq!(
            storage
                .get_contract_storage(b"contract-1", b"counter")
                .expect("get contract storage"),
            Some(b"1".to_vec())
        );

        let stored_contract = storage
            .get_contract(b"contract-1")
            .expect("get contract")
            .expect("contract should exist");

        assert_eq!(stored_contract.code_wasm, initial_contract.code_wasm);
        assert_ne!(stored_contract.storage_root, initial_contract.storage_root);
        assert_eq!(stored_contract.storage_root.len(), 32);

        drop(storage);
        std::fs::remove_dir_all(&path).ok();
    }

    #[test]
    fn storage_persists_after_restart() {
        let path = temp_path("restart");

        let block = sample_block();
        let tx = block.transactions[0].clone();
        let account = Account {
            balance: 5_000,
            nonce: 9,
            code_hash: Some(vec![4; 32]),
            anonymous_state: AnonymousAccountState::default(),
        };

        let tx_hash = {
            let storage = RocksDbStorage::open(&path).expect("open storage before restart");
            let tx_hash = storage.save_transaction(&tx).expect("save transaction");
            storage.save_block(&block).expect("save block");
            storage
                .update_account(b"alice", &account)
                .expect("update account");
            tx_hash
        };

        let reopened = RocksDbStorage::open(&path).expect("reopen storage after restart");

        assert_eq!(
            reopened
                .get_block(block.header.height)
                .expect("get block after restart"),
            Some(block.clone())
        );
        assert_eq!(
            reopened
                .get_transaction(&tx_hash)
                .expect("get transaction after restart"),
            Some(tx.clone())
        );
        assert_eq!(
            reopened
                .get_account(b"alice")
                .expect("get account after restart"),
            Some(account.clone())
        );

        drop(reopened);
        std::fs::remove_dir_all(&path).ok();
    }

    #[test]
    fn storage_saves_and_scans_viewable_notes_by_viewing_key() {
        let path = temp_path("viewable-notes");
        let storage = RocksDbStorage::open(&path).expect("rocksdb should open successfully");

        let alice_viewing_key = [21u8; 32];
        let bob_viewing_key = [22u8; 32];

        let alice_note = ViewableNote::new(40, [31u8; 32], alice_viewing_key, [41u8; 32], 0)
            .expect("alice note should be created");
        let bob_note = ViewableNote::new(50, [32u8; 32], bob_viewing_key, [42u8; 32], 1)
            .expect("bob note should be created");

        storage
            .save_viewable_note(&alice_note)
            .expect("save alice note");
        storage
            .save_viewable_note(&bob_note)
            .expect("save bob note");

        let scanned = storage
            .find_notes_for_viewing_key(&alice_viewing_key)
            .expect("scan notes for alice");

        assert_eq!(
            storage
                .get_viewable_note(&alice_note.commitment())
                .expect("get alice note"),
            Some(alice_note.clone())
        );
        assert_eq!(scanned.len(), 1);
        assert_eq!(scanned[0], alice_note);

        drop(storage);
        std::fs::remove_dir_all(&path).ok();
    }

    #[test]
    fn storage_persists_note_merkle_root_globally_and_by_shard() {
        let path = temp_path("anon-root");
        let global_root = [7u8; 32];
        let shard_root = [8u8; 32];

        {
            let storage = RocksDbStorage::open(&path).expect("open storage before restart");
            storage
                .save_note_merkle_root(None, &global_root)
                .expect("save global root");
            storage
                .save_note_merkle_root(Some(2), &shard_root)
                .expect("save shard root");
        }

        let reopened = RocksDbStorage::open(&path).expect("reopen storage after restart");

        assert_eq!(
            reopened
                .get_note_merkle_root(None)
                .expect("get global root"),
            Some(global_root)
        );
        assert_eq!(
            reopened
                .get_note_merkle_root(Some(2))
                .expect("get shard root"),
            Some(shard_root)
        );

        drop(reopened);
        std::fs::remove_dir_all(&path).ok();
    }
}

pub mod checksum;
