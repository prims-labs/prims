use crate::blockchain::types::{BlockHeader, Transaction};
use sha2::{Digest, Sha256};

pub fn sha256(data: &[u8]) -> Vec<u8> {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hasher.finalize().to_vec()
}

pub fn contract_code_hash(code_wasm: &[u8]) -> Vec<u8> {
    sha256(code_wasm)
}

pub fn derive_contract_address(sender: &[u8], code_wasm: &[u8]) -> Vec<u8> {
    let code_hash = contract_code_hash(code_wasm);
    let mut payload = Vec::with_capacity(b"prims-contract".len() + sender.len() + code_hash.len());
    payload.extend_from_slice(b"prims-contract");
    payload.extend_from_slice(sender);
    payload.extend_from_slice(&code_hash);
    sha256(&payload)
}

pub fn hash_transaction(tx: &Transaction) -> Vec<u8> {
    let encoded = bincode::serialize(tx).expect("transaction serialization should succeed");
    sha256(&encoded)
}

pub fn hash_block_header(header: &BlockHeader) -> Vec<u8> {
    let encoded = bincode::serialize(header).expect("block header serialization should succeed");
    sha256(&encoded)
}

pub fn calculate_merkle_root(transactions: &[Transaction]) -> Vec<u8> {
    if transactions.is_empty() {
        return sha256(&[]);
    }

    let mut level: Vec<Vec<u8>> = transactions.iter().map(hash_transaction).collect();

    while level.len() > 1 {
        if level.len() % 2 == 1 {
            let last = level
                .last()
                .cloned()
                .expect("merkle level should not be empty");
            level.push(last);
        }

        let mut next_level = Vec::with_capacity(level.len() / 2);

        for pair in level.chunks(2) {
            let mut combined = Vec::with_capacity(pair[0].len() + pair[1].len());
            combined.extend_from_slice(&pair[0]);
            combined.extend_from_slice(&pair[1]);
            next_level.push(sha256(&combined));
        }

        level = next_level;
    }

    level.pop().expect("merkle root should exist")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blockchain::types::FIXED_TRANSACTION_FEE;

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

    #[test]
    fn sha256_is_deterministic_for_same_input() {
        let data = b"prims-security-check";
        assert_eq!(sha256(data), sha256(data));
    }

    #[test]
    fn sha256_differs_for_distinct_inputs() {
        assert_ne!(sha256(b"alpha"), sha256(b"beta"));
    }

    #[test]
    fn merkle_root_empty_transactions_matches_sha256_empty() {
        assert_eq!(calculate_merkle_root(&[]), sha256(&[]));
    }

    #[test]
    fn merkle_root_single_transaction_matches_transaction_hash() {
        let tx = sample_transaction();
        assert_eq!(calculate_merkle_root(&[tx.clone()]), hash_transaction(&tx));
    }

    #[test]
    fn hash_transaction_changes_when_payload_changes() {
        let tx = sample_transaction();
        let mut tampered = tx.clone();
        tampered.amount += 1;

        assert_ne!(hash_transaction(&tx), hash_transaction(&tampered));
    }

    #[test]
    fn derive_contract_address_is_deterministic_for_same_sender_and_code() {
        let sender = vec![7; 32];
        let code = b"\0asm\x01\0\0\0".to_vec();

        assert_eq!(
            derive_contract_address(&sender, &code),
            derive_contract_address(&sender, &code)
        );
        assert_eq!(contract_code_hash(&code), sha256(&code));
    }

    #[test]
    fn derive_contract_address_changes_when_sender_or_code_changes() {
        let code_a = b"\0asm\x01\0\0\0".to_vec();
        let code_b = b"\0asm\x01\0\0\x01".to_vec();

        let sender_a = vec![1; 32];
        let sender_b = vec![2; 32];

        assert_ne!(
            derive_contract_address(&sender_a, &code_a),
            derive_contract_address(&sender_b, &code_a)
        );
        assert_ne!(
            derive_contract_address(&sender_a, &code_a),
            derive_contract_address(&sender_a, &code_b)
        );
    }

    #[test]
    fn merkle_root_changes_when_transaction_order_changes() {
        let tx1 = sample_transaction();
        let mut tx2 = sample_transaction();
        tx2.nonce += 1;

        let ordered = calculate_merkle_root(&[tx1.clone(), tx2.clone()]);
        let reversed = calculate_merkle_root(&[tx2, tx1]);

        assert_ne!(ordered, reversed);
    }

    #[test]
    fn block_header_hash_is_deterministic() {
        let header = BlockHeader {
            version: 1,
            previous_hash: vec![0; 32],
            merkle_root: vec![1; 32],
            timestamp: 1_710_000_000,
            height: 1,
            validator: vec![7; 32],
            signature: vec![6; 64],
        };

        assert_eq!(hash_block_header(&header), hash_block_header(&header));
    }
}
