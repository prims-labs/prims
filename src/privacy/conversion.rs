use crate::blockchain::types::Account;
use serde::{Deserialize, Serialize};

use super::{
    Note, NoteCommitment, NoteMerkleHash, NoteMerkleProof, NoteMerkleTree, NoteRecipientPublicKey,
    NoteSpendingPrivateKey, NoteViewingKey, ViewableNote, derive_note_recipient_public_key,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicToAnonRequest {
    pub amount: u64,
    pub recipient_public_key: NoteRecipientPublicKey,
    pub viewing_key: NoteViewingKey,
    pub blinding: [u8; 32],
    pub shard_id: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnonToPublicRequest {
    pub amount: u64,
    pub spending_private_key: NoteSpendingPrivateKey,
    pub blinding: [u8; 32],
    pub merkle_proof: NoteMerkleProof,
    pub shard_id: Option<u16>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublicToAnonConversion {
    pub account: Account,
    pub viewable_note: ViewableNote,
    pub merkle_root: NoteMerkleHash,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnonToPublicConversion {
    pub account: Account,
    pub consumed_commitment: NoteCommitment,
    pub merkle_root: NoteMerkleHash,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ConversionError {
    #[error("conversion amount must be strictly positive")]
    ZeroAmount,
    #[error(
        "public balance is insufficient for conversion: balance={balance}, required={required}"
    )]
    InsufficientPublicBalance { balance: u64, required: u64 },
    #[error("anonymous note viewing hint does not match the account state")]
    ViewingHintMismatch,
    #[error("created anonymous note is invalid: {message}")]
    InvalidCreatedNote { message: String },
    #[error("consumed anonymous note is invalid: {message}")]
    InvalidConsumedNote { message: String },
    #[error("note ownership proof does not match the consumed note")]
    InvalidOwnershipProof,
    #[error("note merkle proof does not match the current note tree root")]
    InvalidMerkleProof,
    #[error("consumed note commitment is not tracked in the account anonymous state")]
    MissingTrackedCommitment,
    #[error("consumed note commitment is missing from the current note tree")]
    MissingTreeCommitment,
    #[error("public balance overflow during anonymous to public conversion")]
    BalanceOverflow,
}

pub fn convert_public_to_anon(
    account: &Account,
    request: &PublicToAnonRequest,
    existing_commitments: &[NoteCommitment],
) -> Result<PublicToAnonConversion, ConversionError> {
    if request.amount == 0 {
        return Err(ConversionError::ZeroAmount);
    }

    if account.balance < request.amount {
        return Err(ConversionError::InsufficientPublicBalance {
            balance: account.balance,
            required: request.amount,
        });
    }

    let viewable_note = ViewableNote::new(
        request.amount,
        request.recipient_public_key,
        request.viewing_key,
        request.blinding,
        request.shard_id,
    )
    .map_err(|error| ConversionError::InvalidCreatedNote {
        message: error.to_string(),
    })?;

    let expected_hint = viewable_note.viewing_hint.to_vec();
    if let Some(current_hint) = &account.anonymous_state.viewing_hint {
        if current_hint != &expected_hint {
            return Err(ConversionError::ViewingHintMismatch);
        }
    }

    let mut updated_account = account.clone();
    updated_account.balance -= request.amount;
    updated_account.anonymous_state.viewing_hint = Some(expected_hint);
    updated_account
        .anonymous_state
        .note_commitments
        .push(viewable_note.commitment().to_vec());

    let mut commitments = existing_commitments.to_vec();
    commitments.push(viewable_note.commitment());

    Ok(PublicToAnonConversion {
        account: updated_account,
        viewable_note,
        merkle_root: NoteMerkleTree::from_commitments(commitments).root(),
    })
}

pub fn convert_anon_to_public(
    account: &Account,
    request: &AnonToPublicRequest,
    current_commitments: &[NoteCommitment],
) -> Result<AnonToPublicConversion, ConversionError> {
    if request.amount == 0 {
        return Err(ConversionError::ZeroAmount);
    }

    let recipient_public_key = derive_note_recipient_public_key(&request.spending_private_key);
    let note =
        Note::new(request.amount, recipient_public_key, request.blinding).map_err(|error| {
            ConversionError::InvalidConsumedNote {
                message: error.to_string(),
            }
        })?;
    let commitment = note.commitment;
    let commitment_bytes = commitment.to_vec();

    if request.merkle_proof.leaf != commitment {
        return Err(ConversionError::InvalidOwnershipProof);
    }

    let current_root = NoteMerkleTree::from_commitments(current_commitments.to_vec()).root();
    if !NoteMerkleTree::verify_proof(&current_root, &request.merkle_proof) {
        return Err(ConversionError::InvalidMerkleProof);
    }

    let tracked_index = account
        .anonymous_state
        .note_commitments
        .iter()
        .position(|stored| stored == &commitment_bytes)
        .ok_or(ConversionError::MissingTrackedCommitment)?;

    let tree_index = current_commitments
        .iter()
        .position(|stored| *stored == commitment)
        .ok_or(ConversionError::MissingTreeCommitment)?;

    let mut updated_account = account.clone();
    updated_account.balance = updated_account
        .balance
        .checked_add(request.amount)
        .ok_or(ConversionError::BalanceOverflow)?;
    updated_account
        .anonymous_state
        .note_commitments
        .remove(tracked_index);

    if updated_account.anonymous_state.note_commitments.is_empty() {
        updated_account.anonymous_state.viewing_hint = None;
    }

    let mut remaining_commitments = current_commitments.to_vec();
    remaining_commitments.remove(tree_index);

    Ok(AnonToPublicConversion {
        account: updated_account,
        consumed_commitment: commitment,
        merkle_root: NoteMerkleTree::from_commitments(remaining_commitments).root(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blockchain::types::{
        Account, AnonymousAccountState, DEFAULT_SHARD_ID, FIXED_TRANSACTION_FEE, Transaction,
        TransactionType,
    };

    fn sample_account(balance: u64) -> Account {
        Account {
            balance,
            nonce: 7,
            code_hash: None,
            anonymous_state: AnonymousAccountState::default(),
        }
    }

    fn sample_viewing_key(seed: u8) -> NoteViewingKey {
        [seed; 32]
    }

    fn sample_blinding(seed: u8) -> [u8; 32] {
        [seed; 32]
    }

    fn sample_spending_private_key(seed: u8) -> NoteSpendingPrivateKey {
        [seed; 32]
    }

    #[test]
    fn public_to_anon_burns_public_balance_and_creates_note_commitment() {
        let account = sample_account(100);
        let request = PublicToAnonRequest {
            amount: 40,
            recipient_public_key: [11u8; 32],
            viewing_key: sample_viewing_key(12),
            blinding: sample_blinding(13),
            shard_id: DEFAULT_SHARD_ID,
        };

        let result =
            convert_public_to_anon(&account, &request, &[]).expect("public to anon must succeed");

        assert_eq!(result.account.balance, 60);
        assert_eq!(
            result.account.anonymous_state.viewing_hint,
            Some(result.viewable_note.viewing_hint.to_vec())
        );
        assert_eq!(
            result.account.anonymous_state.note_commitments,
            vec![result.viewable_note.commitment().to_vec()]
        );
        assert_eq!(
            result.merkle_root,
            NoteMerkleTree::from_commitments(vec![result.viewable_note.commitment()]).root()
        );
    }

    #[test]
    fn public_to_anon_rejects_insufficient_public_balance() {
        let account = sample_account(10);
        let request = PublicToAnonRequest {
            amount: 40,
            recipient_public_key: [21u8; 32],
            viewing_key: sample_viewing_key(22),
            blinding: sample_blinding(23),
            shard_id: DEFAULT_SHARD_ID,
        };

        assert_eq!(
            convert_public_to_anon(&account, &request, &[]),
            Err(ConversionError::InsufficientPublicBalance {
                balance: 10,
                required: 40,
            })
        );
    }

    #[test]
    fn anon_to_public_consumes_note_and_credits_public_balance() {
        let spending_private_key = sample_spending_private_key(31);
        let recipient_public_key = derive_note_recipient_public_key(&spending_private_key);
        let note =
            Note::new(55, recipient_public_key, sample_blinding(32)).expect("note must be valid");
        let filler =
            Note::new(7, [41u8; 32], sample_blinding(42)).expect("filler note must be valid");
        let tree = NoteMerkleTree::from_notes(&vec![note.clone(), filler.clone()]);
        let merkle_proof = tree.generate_proof(0).expect("proof must exist");
        let viewable_note =
            ViewableNote::from_note(note.clone(), &sample_viewing_key(33), DEFAULT_SHARD_ID)
                .expect("viewable note must be valid");
        let account = Account {
            balance: 9,
            nonce: 8,
            code_hash: None,
            anonymous_state: AnonymousAccountState {
                viewing_hint: Some(viewable_note.viewing_hint.to_vec()),
                note_commitments: vec![note.commitment.to_vec()],
            },
        };

        let result = convert_anon_to_public(
            &account,
            &AnonToPublicRequest {
                amount: 55,
                spending_private_key,
                blinding: sample_blinding(32),
                merkle_proof,
                shard_id: None,
            },
            &[note.commitment, filler.commitment],
        )
        .expect("anon to public must succeed");

        assert_eq!(result.account.balance, 64);
        assert!(result.account.anonymous_state.note_commitments.is_empty());
        assert_eq!(result.account.anonymous_state.viewing_hint, None);
        assert_eq!(result.consumed_commitment, note.commitment);
        assert_eq!(
            result.merkle_root,
            NoteMerkleTree::from_commitments(vec![filler.commitment]).root()
        );
    }

    #[test]
    fn anon_to_public_rejects_spending_same_note_twice_after_state_update() {
        let spending_private_key = sample_spending_private_key(41);
        let recipient_public_key = derive_note_recipient_public_key(&spending_private_key);
        let note =
            Note::new(55, recipient_public_key, sample_blinding(42)).expect("note must be valid");
        let filler =
            Note::new(7, [43u8; 32], sample_blinding(44)).expect("filler note must be valid");
        let tree = NoteMerkleTree::from_notes(&vec![note.clone(), filler.clone()]);
        let merkle_proof = tree.generate_proof(0).expect("proof must exist");
        let viewable_note =
            ViewableNote::from_note(note.clone(), &sample_viewing_key(45), DEFAULT_SHARD_ID)
                .expect("viewable note must be valid");
        let account = Account {
            balance: 9,
            nonce: 8,
            code_hash: None,
            anonymous_state: AnonymousAccountState {
                viewing_hint: Some(viewable_note.viewing_hint.to_vec()),
                note_commitments: vec![note.commitment.to_vec()],
            },
        };

        let first_result = convert_anon_to_public(
            &account,
            &AnonToPublicRequest {
                amount: 55,
                spending_private_key,
                blinding: sample_blinding(42),
                merkle_proof: merkle_proof.clone(),
                shard_id: None,
            },
            &[note.commitment, filler.commitment],
        )
        .expect("first anon to public conversion must succeed");

        assert_eq!(first_result.account.balance, 64);
        assert!(
            first_result
                .account
                .anonymous_state
                .note_commitments
                .is_empty()
        );

        assert_eq!(
            convert_anon_to_public(
                &first_result.account,
                &AnonToPublicRequest {
                    amount: 55,
                    spending_private_key,
                    blinding: sample_blinding(42),
                    merkle_proof,
                    shard_id: None,
                },
                &[note.commitment, filler.commitment],
            ),
            Err(ConversionError::MissingTrackedCommitment)
        );
    }

    #[test]
    fn anon_to_public_rejects_invalid_merkle_proof() {
        let spending_private_key = sample_spending_private_key(51);
        let recipient_public_key = derive_note_recipient_public_key(&spending_private_key);
        let note =
            Note::new(20, recipient_public_key, sample_blinding(52)).expect("note must be valid");
        let filler =
            Note::new(8, [61u8; 32], sample_blinding(62)).expect("filler note must be valid");
        let tree = NoteMerkleTree::from_notes(&vec![note.clone(), filler.clone()]);
        let mut merkle_proof = tree.generate_proof(0).expect("proof must exist");
        merkle_proof.siblings[0][0] ^= 0x01;

        let account = Account {
            balance: 3,
            nonce: 2,
            code_hash: None,
            anonymous_state: AnonymousAccountState {
                viewing_hint: Some(
                    ViewableNote::compute_viewing_hint(&sample_viewing_key(53))
                        .expect("hint must exist")
                        .to_vec(),
                ),
                note_commitments: vec![note.commitment.to_vec()],
            },
        };

        assert_eq!(
            convert_anon_to_public(
                &account,
                &AnonToPublicRequest {
                    amount: 20,
                    spending_private_key,
                    blinding: sample_blinding(52),
                    merkle_proof,
                    shard_id: None,
                },
                &[note.commitment, filler.commitment],
            ),
            Err(ConversionError::InvalidMerkleProof)
        );
    }

    #[test]
    fn conversion_transaction_types_roundtrip_with_bincode() {
        let public_to_anon_tx = Transaction {
            tx_type: TransactionType::PublicToAnon,
            from: vec![1; 32],
            to: vec![2; 32],
            amount: 42,
            fee: FIXED_TRANSACTION_FEE,
            nonce: 9,
            source_shard: DEFAULT_SHARD_ID,
            destination_shard: DEFAULT_SHARD_ID,
            signature: vec![3; 64],
            data: Some(b"public-to-anon".to_vec()),
        };
        let anon_to_public_tx = Transaction {
            tx_type: TransactionType::AnonToPublic,
            from: vec![4; 32],
            to: vec![5; 32],
            amount: 24,
            fee: FIXED_TRANSACTION_FEE,
            nonce: 10,
            source_shard: DEFAULT_SHARD_ID,
            destination_shard: DEFAULT_SHARD_ID,
            signature: vec![6; 64],
            data: Some(b"anon-to-public".to_vec()),
        };

        let encoded_public_to_anon =
            bincode::serialize(&public_to_anon_tx).expect("serialize public_to_anon");
        let decoded_public_to_anon: Transaction =
            bincode::deserialize(&encoded_public_to_anon).expect("deserialize public_to_anon");
        assert_eq!(decoded_public_to_anon, public_to_anon_tx);

        let encoded_anon_to_public =
            bincode::serialize(&anon_to_public_tx).expect("serialize anon_to_public");
        let decoded_anon_to_public: Transaction =
            bincode::deserialize(&encoded_anon_to_public).expect("deserialize anon_to_public");
        assert_eq!(decoded_anon_to_public, anon_to_public_tx);
    }
}
