use serde::{Deserialize, Serialize};

pub const FIXED_TRANSACTION_FEE: u64 = 1;
pub const DEFAULT_SHARD_ID: u16 = 0;
pub const RECEIPT_PROOF_LENGTH: usize = 32;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum TransactionType {
    Transfer,
    Stake { duration: u64 },
    Unstake,
    PublicToAnon,
    AnonToPublic,
    DeployContract,
    CallContract,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum CrossShardPhase {
    Prepare,
    Validate,
    Commit,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CrossShardReceipt {
    pub tx_hash: Vec<u8>,
    pub source_shard: u16,
    pub destination_shard: u16,
    pub phase: CrossShardPhase,
    pub proof: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Transaction {
    pub tx_type: TransactionType,
    pub from: Vec<u8>,
    pub to: Vec<u8>,
    pub amount: u64,
    pub fee: u64,
    pub nonce: u64,
    pub source_shard: u16,
    pub destination_shard: u16,
    pub signature: Vec<u8>,
    pub data: Option<Vec<u8>>,
}

impl Transaction {
    pub fn is_cross_shard(&self) -> bool {
        self.source_shard != self.destination_shard
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BlockHeader {
    pub version: u32,
    pub previous_hash: Vec<u8>,
    pub merkle_root: Vec<u8>,
    pub timestamp: u64,
    pub height: u64,
    pub validator: Vec<u8>,
    pub signature: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Block {
    pub header: BlockHeader,
    pub transactions: Vec<Transaction>,
    pub receipts: Vec<CrossShardReceipt>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct AnonymousAccountState {
    pub viewing_hint: Option<Vec<u8>>,
    pub note_commitments: Vec<Vec<u8>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Account {
    pub balance: u64,
    pub nonce: u64,
    pub code_hash: Option<Vec<u8>>,
    pub anonymous_state: AnonymousAccountState,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Contract {
    pub code_wasm: Vec<u8>,
    pub storage_root: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ContractCallPayload {
    pub method: String,
    pub params: Vec<u8>,
    pub gas_limit: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Validator {
    pub address: Vec<u8>,
    pub stake: u64,
    pub locked_until: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn block_structures_roundtrip_with_bincode() {
        let tx = Transaction {
            tx_type: TransactionType::Transfer,
            from: vec![1, 2, 3],
            to: vec![4, 5, 6],
            amount: 42,
            fee: FIXED_TRANSACTION_FEE,
            nonce: 7,
            source_shard: 1,
            destination_shard: 2,
            signature: vec![9, 9, 9],
            data: Some(vec![8, 8]),
        };

        let receipt = CrossShardReceipt {
            tx_hash: vec![5; 32],
            source_shard: 1,
            destination_shard: 2,
            phase: CrossShardPhase::Prepare,
            proof: vec![4; RECEIPT_PROOF_LENGTH],
        };

        let header = BlockHeader {
            version: 1,
            previous_hash: vec![0; 32],
            merkle_root: vec![1; 32],
            timestamp: 1_710_000_000,
            height: 1,
            validator: vec![7; 32],
            signature: vec![6; 64],
        };

        let block = Block {
            header,
            transactions: vec![tx],
            receipts: vec![receipt],
        };

        let account = Account {
            balance: 1_000,
            nonce: 2,
            code_hash: Some(vec![3; 32]),
            anonymous_state: AnonymousAccountState {
                viewing_hint: Some(vec![4; 32]),
                note_commitments: vec![vec![5; 32], vec![6; 32]],
            },
        };

        let encoded_block = bincode::serialize(&block).expect("serialize block");
        let decoded_block: Block = bincode::deserialize(&encoded_block).expect("deserialize block");
        assert_eq!(decoded_block, block);

        let encoded_account = bincode::serialize(&account).expect("serialize account");
        let decoded_account: Account =
            bincode::deserialize(&encoded_account).expect("deserialize account");
        assert_eq!(decoded_account, account);
    }

    #[test]
    fn validator_structure_roundtrip_with_bincode() {
        let validator = Validator {
            address: vec![9; 32],
            stake: 100_000,
            locked_until: 1_710_086_400,
        };

        let encoded_validator = bincode::serialize(&validator).expect("serialize validator");
        let decoded_validator: Validator =
            bincode::deserialize(&encoded_validator).expect("deserialize validator");

        assert_eq!(decoded_validator, validator);
    }

    #[test]
    fn contract_structure_roundtrip_with_bincode() {
        let contract = Contract {
            code_wasm: b"\0asm\x01\0\0\0".to_vec(),
            storage_root: vec![8; 32],
        };

        let encoded_contract = bincode::serialize(&contract).expect("serialize contract");
        let decoded_contract: Contract =
            bincode::deserialize(&encoded_contract).expect("deserialize contract");

        assert_eq!(decoded_contract, contract);
    }

    #[test]
    fn call_contract_transaction_roundtrip_with_bincode() {
        let payload = ContractCallPayload {
            method: "increment".to_string(),
            params: br#"{"delta":1}"#.to_vec(),
            gas_limit: 50_000,
        };

        let tx = Transaction {
            tx_type: TransactionType::CallContract,
            from: vec![1; 32],
            to: vec![2; 32],
            amount: 0,
            fee: FIXED_TRANSACTION_FEE,
            nonce: 3,
            source_shard: DEFAULT_SHARD_ID,
            destination_shard: DEFAULT_SHARD_ID,
            signature: vec![7; 64],
            data: Some(
                bincode::serialize(&payload).expect("serialize contract call payload into tx data"),
            ),
        };

        let encoded_tx = bincode::serialize(&tx).expect("serialize call contract transaction");
        let decoded_tx: Transaction =
            bincode::deserialize(&encoded_tx).expect("deserialize call contract transaction");

        assert_eq!(decoded_tx, tx);

        let decoded_payload: ContractCallPayload = bincode::deserialize(
            decoded_tx
                .data
                .as_deref()
                .expect("call contract transaction should contain payload"),
        )
        .expect("deserialize contract call payload from tx data");

        assert_eq!(decoded_payload, payload);
    }

    #[test]
    fn special_transactions_roundtrip_with_bincode() {
        let stake_tx = Transaction {
            tx_type: TransactionType::Stake { duration: 86_400 },
            from: vec![1; 32],
            to: vec![0; 32],
            amount: 50_000,
            fee: FIXED_TRANSACTION_FEE,
            nonce: 1,
            source_shard: DEFAULT_SHARD_ID,
            destination_shard: DEFAULT_SHARD_ID,
            signature: vec![7; 64],
            data: None,
        };

        let unstake_tx = Transaction {
            tx_type: TransactionType::Unstake,
            from: vec![1; 32],
            to: vec![0; 32],
            amount: 0,
            fee: FIXED_TRANSACTION_FEE,
            nonce: 2,
            source_shard: DEFAULT_SHARD_ID,
            destination_shard: DEFAULT_SHARD_ID,
            signature: vec![8; 64],
            data: None,
        };

        let encoded_stake_tx = bincode::serialize(&stake_tx).expect("serialize stake transaction");
        let decoded_stake_tx: Transaction =
            bincode::deserialize(&encoded_stake_tx).expect("deserialize stake transaction");
        assert_eq!(decoded_stake_tx, stake_tx);

        let encoded_unstake_tx =
            bincode::serialize(&unstake_tx).expect("serialize unstake transaction");
        let decoded_unstake_tx: Transaction =
            bincode::deserialize(&encoded_unstake_tx).expect("deserialize unstake transaction");
        assert_eq!(decoded_unstake_tx, unstake_tx);
    }

    #[test]
    fn cross_shard_detection_matches_shard_ids() {
        let local_tx = Transaction {
            tx_type: TransactionType::Transfer,
            from: vec![1; 32],
            to: vec![2; 32],
            amount: 10,
            fee: FIXED_TRANSACTION_FEE,
            nonce: 1,
            source_shard: 3,
            destination_shard: 3,
            signature: vec![9; 64],
            data: None,
        };

        let cross_tx = Transaction {
            destination_shard: 4,
            ..local_tx.clone()
        };

        assert!(!local_tx.is_cross_shard());
        assert!(cross_tx.is_cross_shard());
    }
}
