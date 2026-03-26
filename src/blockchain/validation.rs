use crate::blockchain::hash::calculate_merkle_root;
use crate::blockchain::types::{Account, Block, BlockHeader, FIXED_TRANSACTION_FEE, Transaction};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use thiserror::Error;

pub const MAX_TRANSACTION_SIZE_BYTES: usize = 1_048_576;
pub const MAX_BLOCK_SIZE_BYTES: usize = 10_485_760;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum BlockValidationError {
    #[error("block size must be smaller than {limit} bytes (got {size})")]
    BlockTooLarge { size: usize, limit: usize },
    #[error("invalid previous hash")]
    InvalidPreviousHash,
    #[error("invalid merkle root")]
    InvalidMerkleRoot,
    #[error("invalid validator public key length: expected 32 bytes")]
    InvalidValidatorKeyLength,
    #[error("invalid validator signature length: expected 64 bytes")]
    InvalidValidatorSignatureLength,
    #[error("invalid validator signature")]
    InvalidValidatorSignature,
    #[error("block serialization failed: {0}")]
    Serialization(String),
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum TransactionValidationError {
    #[error(
        "transaction nonce must be strictly greater than the last stored nonce (last: {last_nonce}, received: {received_nonce})"
    )]
    NonceNotStrictlyGreater {
        last_nonce: u64,
        received_nonce: u64,
    },
    #[error("transaction amount must be strictly positive")]
    AmountMustBePositive,
    #[error("transaction fee must be fixed to {expected} (received: {received})")]
    InvalidFee { expected: u64, received: u64 },
    #[error("transaction amount + fee overflowed u64")]
    AmountOverflow,
    #[error("insufficient balance (balance: {balance}, required: {required})")]
    InsufficientBalance { balance: u64, required: u64 },
    #[error("transaction size must be smaller than {limit} bytes (got {size})")]
    TransactionTooLarge { size: usize, limit: usize },
    #[error("transaction serialization failed: {0}")]
    Serialization(String),
}

fn unsigned_block_header_bytes(header: &BlockHeader) -> Result<Vec<u8>, BlockValidationError> {
    let mut unsigned = header.clone();
    unsigned.signature.clear();
    bincode::serialize(&unsigned)
        .map_err(|err| BlockValidationError::Serialization(err.to_string()))
}

pub fn validate_block_size(block: &Block) -> Result<(), BlockValidationError> {
    let encoded = bincode::serialize(block)
        .map_err(|err| BlockValidationError::Serialization(err.to_string()))?;

    if encoded.len() >= MAX_BLOCK_SIZE_BYTES {
        return Err(BlockValidationError::BlockTooLarge {
            size: encoded.len(),
            limit: MAX_BLOCK_SIZE_BYTES,
        });
    }

    Ok(())
}

pub fn validate_block(
    block: &Block,
    expected_previous_hash: &[u8],
) -> Result<(), BlockValidationError> {
    validate_block_size(block)?;

    if block.header.previous_hash != expected_previous_hash {
        return Err(BlockValidationError::InvalidPreviousHash);
    }

    let expected_merkle_root = calculate_merkle_root(&block.transactions);
    if block.header.merkle_root != expected_merkle_root {
        return Err(BlockValidationError::InvalidMerkleRoot);
    }

    let validator_bytes: [u8; 32] = block
        .header
        .validator
        .clone()
        .try_into()
        .map_err(|_| BlockValidationError::InvalidValidatorKeyLength)?;
    let verifying_key = VerifyingKey::from_bytes(&validator_bytes)
        .map_err(|_| BlockValidationError::InvalidValidatorKeyLength)?;

    let signature_bytes: [u8; 64] = block
        .header
        .signature
        .clone()
        .try_into()
        .map_err(|_| BlockValidationError::InvalidValidatorSignatureLength)?;
    let signature = Signature::from_bytes(&signature_bytes);

    let message = unsigned_block_header_bytes(&block.header)?;

    verifying_key
        .verify(&message, &signature)
        .map_err(|_| BlockValidationError::InvalidValidatorSignature)
}

pub fn validate_transaction_nonce(
    transaction: &Transaction,
    sender_account: Option<&Account>,
) -> Result<(), TransactionValidationError> {
    if let Some(account) = sender_account {
        if transaction.nonce <= account.nonce {
            return Err(TransactionValidationError::NonceNotStrictlyGreater {
                last_nonce: account.nonce,
                received_nonce: transaction.nonce,
            });
        }
    }

    Ok(())
}

pub fn validate_transaction_balance(
    transaction: &Transaction,
    sender_account: Option<&Account>,
) -> Result<(), TransactionValidationError> {
    if transaction.amount == 0 {
        return Err(TransactionValidationError::AmountMustBePositive);
    }

    if transaction.fee != FIXED_TRANSACTION_FEE {
        return Err(TransactionValidationError::InvalidFee {
            expected: FIXED_TRANSACTION_FEE,
            received: transaction.fee,
        });
    }

    let required = transaction
        .amount
        .checked_add(transaction.fee)
        .ok_or(TransactionValidationError::AmountOverflow)?;

    let balance = sender_account.map(|account| account.balance).unwrap_or(0);

    if balance < required {
        return Err(TransactionValidationError::InsufficientBalance { balance, required });
    }

    Ok(())
}

pub fn validate_transaction_size(
    transaction: &Transaction,
) -> Result<(), TransactionValidationError> {
    let encoded = bincode::serialize(transaction)
        .map_err(|err| TransactionValidationError::Serialization(err.to_string()))?;

    if encoded.len() >= MAX_TRANSACTION_SIZE_BYTES {
        return Err(TransactionValidationError::TransactionTooLarge {
            size: encoded.len(),
            limit: MAX_TRANSACTION_SIZE_BYTES,
        });
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::{KeyPair, generate_keypair};
    use ed25519_dalek::{Signer, SigningKey};

    fn sample_transaction() -> Transaction {
        Transaction {
            tx_type: crate::blockchain::types::TransactionType::Transfer,
            from: vec![1; 32],
            to: vec![2; 32],
            amount: 42,
            fee: FIXED_TRANSACTION_FEE,
            nonce: 7,
            source_shard: 0,
            destination_shard: 0,
            signature: vec![9; 64],
            data: Some(b"hello-prims".to_vec()),
        }
    }

    fn sample_transaction_with_data_len(data_len: usize) -> Transaction {
        Transaction {
            tx_type: crate::blockchain::types::TransactionType::Transfer,
            from: vec![1; 32],
            to: vec![2; 32],
            amount: 42,
            fee: FIXED_TRANSACTION_FEE,
            nonce: 7,
            source_shard: 0,
            destination_shard: 0,
            signature: vec![9; 64],
            data: Some(vec![0; data_len]),
        }
    }

    fn sample_account(balance: u64, nonce: u64) -> Account {
        Account {
            balance,
            nonce,
            code_hash: None,
            anonymous_state: crate::blockchain::types::AnonymousAccountState::default(),
        }
    }

    fn sign_block_header(header: &BlockHeader, secret_key: &[u8; 32]) -> Vec<u8> {
        let signing_key = SigningKey::from_bytes(secret_key);
        let message = unsigned_block_header_bytes(header).expect("serialize unsigned block header");
        signing_key.sign(&message).to_bytes().to_vec()
    }

    fn build_valid_block() -> (Block, Vec<u8>, KeyPair) {
        let keypair = generate_keypair();
        let previous_hash = vec![7; 32];
        let transactions = vec![sample_transaction()];
        let merkle_root = calculate_merkle_root(&transactions);

        let mut header = BlockHeader {
            version: 1,
            previous_hash: previous_hash.clone(),
            merkle_root,
            timestamp: 1_710_000_000,
            height: 1,
            validator: keypair.public_key.to_vec(),
            signature: Vec::new(),
        };

        header.signature = sign_block_header(&header, &keypair.secret_key);

        let block = Block {
            header,
            transactions,
            receipts: vec![],
        };

        (block, previous_hash, keypair)
    }

    fn oversized_block() -> Block {
        let transactions = vec![sample_transaction_with_data_len(900_000); 12];
        let block = Block {
            header: BlockHeader {
                version: 1,
                previous_hash: vec![0; 32],
                merkle_root: vec![0; 32],
                timestamp: 1_710_000_000,
                height: 1,
                validator: vec![7; 32],
                signature: vec![6; 64],
            },
            transactions,
            receipts: vec![],
        };

        let encoded = bincode::serialize(&block).expect("serialize oversized block");
        assert!(encoded.len() >= MAX_BLOCK_SIZE_BYTES);

        block
    }

    #[test]
    fn validate_block_accepts_valid_block() {
        let (block, previous_hash, _) = build_valid_block();

        assert_eq!(validate_block(&block, &previous_hash), Ok(()));
    }

    #[test]
    fn validate_block_rejects_invalid_previous_hash() {
        let (block, _, _) = build_valid_block();

        assert_eq!(
            validate_block(&block, &[0; 32]),
            Err(BlockValidationError::InvalidPreviousHash)
        );
    }

    #[test]
    fn validate_block_rejects_invalid_merkle_root() {
        let (mut block, previous_hash, _) = build_valid_block();
        block.header.merkle_root[0] ^= 0xFF;

        assert_eq!(
            validate_block(&block, &previous_hash),
            Err(BlockValidationError::InvalidMerkleRoot)
        );
    }

    #[test]
    fn validate_block_rejects_invalid_validator_signature() {
        let (mut block, previous_hash, _) = build_valid_block();
        block.header.signature[0] ^= 0xFF;

        assert_eq!(
            validate_block(&block, &previous_hash),
            Err(BlockValidationError::InvalidValidatorSignature)
        );
    }

    #[test]
    fn validate_block_size_accepts_small_block() {
        let (block, _, _) = build_valid_block();

        assert_eq!(validate_block_size(&block), Ok(()));
    }

    #[test]
    fn validate_block_size_rejects_oversized_block() {
        let block = oversized_block();
        let encoded = bincode::serialize(&block).expect("serialize oversized block");

        assert_eq!(
            validate_block_size(&block),
            Err(BlockValidationError::BlockTooLarge {
                size: encoded.len(),
                limit: MAX_BLOCK_SIZE_BYTES,
            })
        );
    }

    #[test]
    fn validate_transaction_nonce_accepts_first_transaction_for_new_account() {
        let transaction = sample_transaction();

        assert_eq!(validate_transaction_nonce(&transaction, None), Ok(()));
    }

    #[test]
    fn validate_transaction_nonce_accepts_strictly_greater_nonce() {
        let transaction = sample_transaction();
        let account = sample_account(1_000, 6);

        assert_eq!(
            validate_transaction_nonce(&transaction, Some(&account)),
            Ok(())
        );
    }

    #[test]
    fn validate_transaction_nonce_rejects_same_nonce() {
        let transaction = sample_transaction();
        let account = sample_account(1_000, 7);

        assert_eq!(
            validate_transaction_nonce(&transaction, Some(&account)),
            Err(TransactionValidationError::NonceNotStrictlyGreater {
                last_nonce: 7,
                received_nonce: 7,
            })
        );
    }

    #[test]
    fn validate_transaction_nonce_rejects_lower_nonce() {
        let transaction = sample_transaction();
        let account = sample_account(1_000, 8);

        assert_eq!(
            validate_transaction_nonce(&transaction, Some(&account)),
            Err(TransactionValidationError::NonceNotStrictlyGreater {
                last_nonce: 8,
                received_nonce: 7,
            })
        );
    }

    #[test]
    fn validate_transaction_balance_accepts_sufficient_balance() {
        let transaction = sample_transaction();
        let account = sample_account(43, 0);

        assert_eq!(
            validate_transaction_balance(&transaction, Some(&account)),
            Ok(())
        );
    }

    #[test]
    fn validate_transaction_balance_accepts_exact_balance() {
        let transaction = sample_transaction();
        let account = sample_account(43, 0);

        assert_eq!(
            validate_transaction_balance(&transaction, Some(&account)),
            Ok(())
        );
    }

    #[test]
    fn validate_transaction_balance_rejects_zero_amount() {
        let mut transaction = sample_transaction();
        transaction.amount = 0;
        let account = sample_account(1_000, 0);

        assert_eq!(
            validate_transaction_balance(&transaction, Some(&account)),
            Err(TransactionValidationError::AmountMustBePositive)
        );
    }

    #[test]
    fn validate_transaction_balance_rejects_insufficient_balance() {
        let transaction = sample_transaction();
        let account = sample_account(42, 0);

        assert_eq!(
            validate_transaction_balance(&transaction, Some(&account)),
            Err(TransactionValidationError::InsufficientBalance {
                balance: 42,
                required: 43,
            })
        );
    }

    #[test]
    fn validate_transaction_balance_rejects_missing_account_with_zero_balance() {
        let transaction = sample_transaction();

        assert_eq!(
            validate_transaction_balance(&transaction, None),
            Err(TransactionValidationError::InsufficientBalance {
                balance: 0,
                required: 43,
            })
        );
    }

    #[test]
    fn validate_transaction_balance_rejects_overflow() {
        let mut transaction = sample_transaction();
        transaction.amount = u64::MAX;
        transaction.fee = FIXED_TRANSACTION_FEE;
        let account = sample_account(u64::MAX, 0);

        assert_eq!(
            validate_transaction_balance(&transaction, Some(&account)),
            Err(TransactionValidationError::AmountOverflow)
        );
    }

    #[test]
    fn validate_transaction_size_accepts_small_transaction() {
        let transaction = sample_transaction();

        assert_eq!(validate_transaction_size(&transaction), Ok(()));
    }

    #[test]
    fn validate_transaction_size_rejects_oversized_transaction() {
        let transaction = sample_transaction_with_data_len(MAX_TRANSACTION_SIZE_BYTES);
        let encoded = bincode::serialize(&transaction).expect("serialize oversized transaction");
        assert!(encoded.len() >= MAX_TRANSACTION_SIZE_BYTES);

        assert_eq!(
            validate_transaction_size(&transaction),
            Err(TransactionValidationError::TransactionTooLarge {
                size: encoded.len(),
                limit: MAX_TRANSACTION_SIZE_BYTES,
            })
        );
    }
}
