use crate::blockchain::types::Transaction;
use anyhow::{Result, anyhow};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use rand::rngs::OsRng;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyPair {
    pub secret_key: [u8; 32],
    pub public_key: [u8; 32],
}

pub fn generate_keypair() -> KeyPair {
    let signing_key = SigningKey::generate(&mut OsRng);
    let verifying_key: VerifyingKey = signing_key.verifying_key();

    KeyPair {
        secret_key: signing_key.to_bytes(),
        public_key: verifying_key.to_bytes(),
    }
}

fn unsigned_transaction_bytes(transaction: &Transaction) -> Result<Vec<u8>> {
    let mut unsigned = transaction.clone();
    unsigned.signature.clear();
    Ok(bincode::serialize(&unsigned)?)
}

pub fn sign_transaction(transaction: &Transaction, secret_key: &[u8; 32]) -> Result<Vec<u8>> {
    let signing_key = SigningKey::from_bytes(secret_key);
    let message = unsigned_transaction_bytes(transaction)?;
    Ok(signing_key.sign(&message).to_bytes().to_vec())
}

pub fn verify_transaction(transaction: &Transaction, public_key: &[u8; 32]) -> Result<bool> {
    let verifying_key = VerifyingKey::from_bytes(public_key)?;
    let signature_bytes: [u8; 64] = transaction
        .signature
        .clone()
        .try_into()
        .map_err(|_| anyhow!("invalid transaction signature length: expected 64 bytes"))?;
    let signature = Signature::from_bytes(&signature_bytes);
    let message = unsigned_transaction_bytes(transaction)?;

    Ok(verifying_key.verify(&message, &signature).is_ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blockchain::types::FIXED_TRANSACTION_FEE;

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
            signature: Vec::new(),
            data: Some(b"hello-prims".to_vec()),
        }
    }

    #[test]
    fn generate_ed25519_keypair_returns_32_byte_keys() {
        let pair = generate_keypair();

        assert_eq!(pair.secret_key.len(), 32);
        assert_eq!(pair.public_key.len(), 32);
        assert_ne!(pair.secret_key, [0u8; 32]);
        assert_ne!(pair.public_key, [0u8; 32]);
    }

    #[test]
    fn sign_and_verify_transaction_roundtrip() {
        let pair = generate_keypair();
        let mut transaction = sample_transaction();

        transaction.signature =
            sign_transaction(&transaction, &pair.secret_key).expect("sign transaction");

        assert!(
            verify_transaction(&transaction, &pair.public_key).expect("verify transaction"),
            "signed transaction should verify"
        );
    }

    #[test]
    fn verify_transaction_fails_when_payload_changes() {
        let pair = generate_keypair();
        let mut transaction = sample_transaction();

        transaction.signature =
            sign_transaction(&transaction, &pair.secret_key).expect("sign transaction");

        let mut tampered = transaction.clone();
        tampered.amount += 1;

        assert!(
            !verify_transaction(&tampered, &pair.public_key).expect("verify tampered transaction"),
            "tampered transaction must not verify"
        );
    }
}
