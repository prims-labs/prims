use ark_bls12_381::{Bls12_381, Fr};
use ark_crypto_primitives::crh::{
    constraints::CRHSchemeGadget,
    sha256::{
        Sha256 as ArkSha256,
        constraints::{DigestVar as Sha256DigestVar, Sha256Gadget, UnitVar as Sha256UnitVar},
    },
};
use ark_groth16::data_structures::{PreparedVerifyingKey, Proof, ProvingKey, VerifyingKey};
use ark_groth16::{Groth16, prepare_verifying_key};
use ark_r1cs_std::{prelude::*, uint64::UInt64};
use ark_relations::r1cs::{ConstraintSynthesizer, ConstraintSystemRef, SynthesisError};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Référence unique vers une sortie UTXO existante.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct UtxoId {
    pub tx_hash: [u8; 32],
    pub output_index: u32,
}

/// Entrée d'une transaction UTXO : elle consomme une sortie existante.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UtxoInput {
    pub previous_output: UtxoId,
}

/// Sortie d'une transaction UTXO : elle crée une nouvelle valeur dépensable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UtxoOutput {
    pub amount: u64,
}

/// Modèle UTXO minimal pour l'étape 7.2.
/// Une transaction consomme des entrées existantes et produit de nouvelles sorties.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UtxoTransaction {
    pub inputs: Vec<UtxoInput>,
    pub outputs: Vec<UtxoOutput>,
    pub fee: u64,
}

impl UtxoTransaction {
    pub fn validate_basic(&self) -> Result<(), UtxoModelError> {
        if self.inputs.is_empty() {
            return Err(UtxoModelError::MissingInputs);
        }

        if self.outputs.is_empty() {
            return Err(UtxoModelError::MissingOutputs);
        }

        if self.outputs.iter().any(|output| output.amount == 0) {
            return Err(UtxoModelError::ZeroOutputAmount);
        }

        Ok(())
    }

    pub fn input_count(&self) -> usize {
        self.inputs.len()
    }

    pub fn output_count(&self) -> usize {
        self.outputs.len()
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum UtxoModelError {
    #[error("utxo transaction must consume at least one input")]
    MissingInputs,
    #[error("utxo transaction must create at least one output")]
    MissingOutputs,
    #[error("utxo transaction outputs must all be strictly positive")]
    ZeroOutputAmount,
}

mod conversion;
pub use conversion::*;

/// Clé publique du destinataire d'une note anonyme.
/// Pour cette étape, on garde un format simple de 32 octets.
pub type NoteRecipientPublicKey = [u8; 32];

/// Clé de vision permettant au destinataire de détecter les notes qui lui sont destinées.
pub type NoteViewingKey = [u8; 32];

/// Empreinte dérivée d'une clé de vision, stockable/indexable sans exposer la clé elle-même.
pub type NoteViewingHint = [u8; 32];

/// Engagement cryptographique d'une note.
pub type NoteCommitment = [u8; 32];

/// Note anonyme minimale.
/// Elle ne stocke pas la valeur ni la clé publique en clair :
/// elle ne conserve que leur engagement, calculé avec un facteur d'aveuglement.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Note {
    pub commitment: NoteCommitment,
}

impl Note {
    pub fn new(
        value: u64,
        recipient_public_key: NoteRecipientPublicKey,
        blinding: [u8; 32],
    ) -> Result<Self, NoteError> {
        if value == 0 {
            return Err(NoteError::ZeroValue);
        }

        if recipient_public_key == [0; 32] {
            return Err(NoteError::InvalidRecipientPublicKey);
        }

        if blinding == [0; 32] {
            return Err(NoteError::InvalidBlinding);
        }

        Ok(Self {
            commitment: Self::compute_commitment(value, &recipient_public_key, &blinding),
        })
    }

    pub fn compute_commitment(
        value: u64,
        recipient_public_key: &NoteRecipientPublicKey,
        blinding: &[u8; 32],
    ) -> NoteCommitment {
        let mut hasher = Sha256::new();
        hasher.update(value.to_le_bytes());
        hasher.update(recipient_public_key);
        hasher.update(blinding);

        let digest = hasher.finalize();
        let mut commitment = [0u8; 32];
        commitment.copy_from_slice(&digest);
        commitment
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum NoteError {
    #[error("note value must be strictly positive")]
    ZeroValue,
    #[error("note recipient public key must not be all zeros")]
    InvalidRecipientPublicKey,
    #[error("note blinding factor must not be all zeros")]
    InvalidBlinding,
    #[error("note viewing key must not be all zeros")]
    InvalidViewingKey,
}

/// Note indexable par clé de vision.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewableNote {
    pub note: Note,
    pub viewing_hint: NoteViewingHint,
    pub shard_id: u16,
}

impl ViewableNote {
    pub fn new(
        value: u64,
        recipient_public_key: NoteRecipientPublicKey,
        viewing_key: NoteViewingKey,
        blinding: [u8; 32],
        shard_id: u16,
    ) -> Result<Self, NoteError> {
        let note = Note::new(value, recipient_public_key, blinding)?;
        Self::from_note(note, &viewing_key, shard_id)
    }

    pub fn from_note(
        note: Note,
        viewing_key: &NoteViewingKey,
        shard_id: u16,
    ) -> Result<Self, NoteError> {
        let viewing_hint =
            Self::compute_viewing_hint(viewing_key).ok_or(NoteError::InvalidViewingKey)?;

        Ok(Self {
            note,
            viewing_hint,
            shard_id,
        })
    }

    pub fn compute_viewing_hint(viewing_key: &NoteViewingKey) -> Option<NoteViewingHint> {
        if *viewing_key == [0u8; 32] {
            return None;
        }

        let mut hasher = Sha256::new();
        hasher.update(b"prims-viewing-key");
        hasher.update(viewing_key);

        let digest = hasher.finalize();
        let mut viewing_hint = [0u8; 32];
        viewing_hint.copy_from_slice(&digest);
        Some(viewing_hint)
    }

    pub fn is_visible_to(&self, viewing_key: &NoteViewingKey) -> bool {
        Self::compute_viewing_hint(viewing_key)
            .map(|viewing_hint| viewing_hint == self.viewing_hint)
            .unwrap_or(false)
    }

    pub fn commitment(&self) -> NoteCommitment {
        self.note.commitment
    }
}

/// Hash d'un noeud de l'arbre de Merkle des notes.
pub type NoteMerkleHash = [u8; 32];

/// Preuve d'appartenance d'une note dans l'arbre de Merkle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NoteMerkleProof {
    pub leaf_index: usize,
    pub leaf: NoteCommitment,
    pub siblings: Vec<NoteMerkleHash>,
}

/// Arbre de Merkle minimal des notes anonymes.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct NoteMerkleTree {
    leaves: Vec<NoteCommitment>,
}

impl NoteMerkleTree {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_notes(notes: &[Note]) -> Self {
        Self {
            leaves: notes.iter().map(|note| note.commitment).collect(),
        }
    }

    pub fn from_commitments(leaves: Vec<NoteCommitment>) -> Self {
        Self { leaves }
    }

    pub fn append(&mut self, note: &Note) {
        self.leaves.push(note.commitment);
    }

    pub fn leaf_count(&self) -> usize {
        self.leaves.len()
    }

    pub fn root(&self) -> NoteMerkleHash {
        Self::compute_root_from_leaves(&self.leaves)
    }

    pub fn generate_proof(
        &self,
        leaf_index: usize,
    ) -> Result<NoteMerkleProof, NoteMerkleTreeError> {
        if leaf_index >= self.leaves.len() {
            return Err(NoteMerkleTreeError::LeafIndexOutOfBounds {
                leaf_index,
                leaf_count: self.leaves.len(),
            });
        }

        let mut siblings = Vec::new();
        let mut index = leaf_index;
        let mut level: Vec<NoteMerkleHash> = self.leaves.iter().map(Self::hash_leaf).collect();

        while level.len() > 1 {
            let sibling_index = if index % 2 == 0 { index + 1 } else { index - 1 };
            let sibling = if sibling_index < level.len() {
                level[sibling_index]
            } else {
                level[index]
            };
            siblings.push(sibling);

            level = Self::build_next_level(&level);
            index /= 2;
        }

        Ok(NoteMerkleProof {
            leaf_index,
            leaf: self.leaves[leaf_index],
            siblings,
        })
    }

    pub fn verify_proof(root: &NoteMerkleHash, proof: &NoteMerkleProof) -> bool {
        let mut hash = Self::hash_leaf(&proof.leaf);
        let mut index = proof.leaf_index;

        for sibling in &proof.siblings {
            hash = if index % 2 == 0 {
                Self::hash_internal(&hash, sibling)
            } else {
                Self::hash_internal(sibling, &hash)
            };
            index /= 2;
        }

        &hash == root
    }

    fn compute_root_from_leaves(leaves: &[NoteCommitment]) -> NoteMerkleHash {
        if leaves.is_empty() {
            return Self::hash_empty_tree();
        }

        let mut level: Vec<NoteMerkleHash> = leaves.iter().map(Self::hash_leaf).collect();

        while level.len() > 1 {
            level = Self::build_next_level(&level);
        }

        level[0]
    }

    fn build_next_level(level: &[NoteMerkleHash]) -> Vec<NoteMerkleHash> {
        let mut next_level = Vec::with_capacity((level.len() + 1) / 2);

        for pair in level.chunks(2) {
            let left = pair[0];
            let right = if pair.len() == 2 { pair[1] } else { pair[0] };
            next_level.push(Self::hash_internal(&left, &right));
        }

        next_level
    }

    fn hash_empty_tree() -> NoteMerkleHash {
        let mut hasher = Sha256::new();
        hasher.update(b"prims-note-merkle-empty");
        let digest = hasher.finalize();

        let mut hash = [0u8; 32];
        hash.copy_from_slice(&digest);
        hash
    }

    fn hash_leaf(commitment: &NoteCommitment) -> NoteMerkleHash {
        let mut hasher = Sha256::new();
        hasher.update(b"prims-note-merkle-leaf");
        hasher.update(commitment);
        let digest = hasher.finalize();

        let mut hash = [0u8; 32];
        hash.copy_from_slice(&digest);
        hash
    }

    fn hash_internal(left: &NoteMerkleHash, right: &NoteMerkleHash) -> NoteMerkleHash {
        let mut hasher = Sha256::new();
        hasher.update(b"prims-note-merkle-node");
        hasher.update(left);
        hasher.update(right);
        let digest = hasher.finalize();

        let mut hash = [0u8; 32];
        hash.copy_from_slice(&digest);
        hash
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum NoteMerkleTreeError {
    #[error("note merkle proof leaf index {leaf_index} is out of bounds for {leaf_count} leaves")]
    LeafIndexOutOfBounds {
        leaf_index: usize,
        leaf_count: usize,
    },
}

/// Clé privée simplifiée utilisée pour dépenser une note.
/// Pour cette étape, on dérive la "clé publique destinataire" par SHA-256
/// afin de rester cohérent avec le format actuel de `NoteRecipientPublicKey`.
pub type NoteSpendingPrivateKey = [u8; 32];

pub type ZkTransferProof = Proof<Bls12_381>;
pub type ZkTransferProvingKey = ProvingKey<Bls12_381>;
pub type ZkTransferVerifyingKey = VerifyingKey<Bls12_381>;
pub type ZkTransferPreparedVerifyingKey = PreparedVerifyingKey<Bls12_381>;

/// Transaction anonyme propagée sur le réseau.
/// Elle ne révèle que les éléments publics nécessaires à la vérification :
/// racine de Merkle, frais, engagements des sorties et preuve Groth16 sérialisée.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnonTransaction {
    pub merkle_root: NoteMerkleHash,
    pub fee: u64,
    pub output_commitments: Vec<NoteCommitment>,
    pub proof: Vec<u8>,
}

#[derive(Debug, thiserror::Error)]
pub enum AnonTransactionError {
    #[error("anon transaction proof serialization failed: {message}")]
    ProofSerializationFailed { message: String },
    #[error("anon transaction proof deserialization failed: {message}")]
    ProofDeserializationFailed { message: String },
    #[error("anon transaction verification failed: {message}")]
    VerificationFailed { message: String },
}

impl AnonTransaction {
    pub fn new(
        public_inputs: &ZkTransferPublicInputs,
        proof: &ZkTransferProof,
    ) -> Result<Self, AnonTransactionError> {
        let mut proof_bytes = Vec::new();
        ark_serialize::CanonicalSerialize::serialize_compressed(proof, &mut proof_bytes).map_err(
            |error| AnonTransactionError::ProofSerializationFailed {
                message: error.to_string(),
            },
        )?;

        Ok(Self {
            merkle_root: public_inputs.merkle_root,
            fee: public_inputs.fee,
            output_commitments: public_inputs.output_commitments.clone(),
            proof: proof_bytes,
        })
    }

    pub fn public_inputs(&self) -> ZkTransferPublicInputs {
        ZkTransferPublicInputs {
            merkle_root: self.merkle_root,
            fee: self.fee,
            output_commitments: self.output_commitments.clone(),
        }
    }

    pub fn decode_proof(&self) -> Result<ZkTransferProof, AnonTransactionError> {
        <ZkTransferProof as ark_serialize::CanonicalDeserialize>::deserialize_compressed(
            std::io::Cursor::new(&self.proof),
        )
        .map_err(|error| AnonTransactionError::ProofDeserializationFailed {
            message: error.to_string(),
        })
    }

    pub fn verify(
        &self,
        verifying_key: &ZkTransferVerifyingKey,
    ) -> Result<bool, AnonTransactionError> {
        let proof = self.decode_proof()?;
        let public_inputs = self.public_inputs();

        verify_zk_transfer_proof(verifying_key, &public_inputs, &proof).map_err(|error| {
            AnonTransactionError::VerificationFailed {
                message: error.to_string(),
            }
        })
    }

    pub fn verify_with_prepared_key(
        &self,
        prepared_verifying_key: &ZkTransferPreparedVerifyingKey,
    ) -> Result<bool, AnonTransactionError> {
        let proof = self.decode_proof()?;
        let public_inputs = self.public_inputs();

        verify_zk_transfer_proof_with_prepared_key(prepared_verifying_key, &public_inputs, &proof)
            .map_err(|error| AnonTransactionError::VerificationFailed {
                message: error.to_string(),
            })
    }
}

/// Témoin privé d'une entrée anonyme consommée par une transaction zk.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ZkNoteInputWitness {
    pub value: u64,
    pub spending_private_key: NoteSpendingPrivateKey,
    pub blinding: [u8; 32],
    pub merkle_proof: NoteMerkleProof,
}

/// Témoin privé d'une sortie anonyme créée par une transaction zk.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ZkNoteOutputWitness {
    pub value: u64,
    pub recipient_public_key: NoteRecipientPublicKey,
    pub blinding: [u8; 32],
}

/// Entrées publiques qui seront exposées au vérificateur du circuit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ZkTransferPublicInputs {
    pub merkle_root: NoteMerkleHash,
    pub fee: u64,
    pub output_commitments: Vec<NoteCommitment>,
}

/// Témoin privé complet de la transaction anonyme.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ZkTransferWitness {
    pub inputs: Vec<ZkNoteInputWitness>,
    pub outputs: Vec<ZkNoteOutputWitness>,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ZkTransferError {
    #[error("zk transfer witness must contain at least one input")]
    MissingInputs,
    #[error("zk transfer witness must contain at least one output")]
    MissingOutputs,
    #[error(
        "zk transfer public output commitment count mismatch: expected {expected}, got {actual}"
    )]
    OutputCommitmentCountMismatch { expected: usize, actual: usize },
    #[error("zk transfer input {index} has an invalid all-zero spending private key")]
    InvalidSpendingPrivateKey { index: usize },
    #[error("zk transfer input note {index} is invalid")]
    InvalidInputNote { index: usize },
    #[error("zk transfer input note commitment mismatch at index {index}")]
    InvalidInputCommitment { index: usize },
    #[error("zk transfer merkle proof is invalid for input {index}")]
    InvalidMerkleProof { index: usize },
    #[error("zk transfer output note {index} is invalid")]
    InvalidOutputNote { index: usize },
    #[error("zk transfer output commitment mismatch at index {index}")]
    InvalidOutputCommitment { index: usize },
    #[error("zk transfer amount overflow")]
    AmountOverflow,
    #[error(
        "zk transfer amount imbalance: inputs={inputs_total}, outputs={outputs_total}, fee={fee}"
    )]
    AmountImbalance {
        inputs_total: u64,
        outputs_total: u64,
        fee: u64,
    },
}

#[derive(Debug, thiserror::Error)]
pub enum ZkTransferProofSystemError {
    #[error("zk transfer public inputs could not be encoded as field elements")]
    PublicInputsEncodingFailed,
    #[error("zk transfer witness is invalid: {0}")]
    InvalidWitness(ZkTransferError),
    #[error("zk transfer proving failed: {message}")]
    ProvingFailed { message: String },
    #[error("zk transfer verification failed: {message}")]
    VerificationFailed { message: String },
}

/// Dérive la clé publique simplifiée associée à une clé privée de dépense.
/// Cette dérivation est déterministe et sert de lien entre possession de la clé
/// et `NoteRecipientPublicKey` utilisé par les engagements actuels.
pub fn derive_note_recipient_public_key(
    spending_private_key: &NoteSpendingPrivateKey,
) -> NoteRecipientPublicKey {
    let mut hasher = Sha256::new();
    hasher.update(b"prims-note-owner");
    hasher.update(spending_private_key);
    let digest = hasher.finalize();

    let mut public_key = [0u8; 32];
    public_key.copy_from_slice(&digest);
    public_key
}

/// Recalcule la racine de Merkle attendue à partir d'une preuve d'appartenance.
pub fn compute_note_merkle_root_from_proof(proof: &NoteMerkleProof) -> NoteMerkleHash {
    let mut hash = NoteMerkleTree::hash_leaf(&proof.leaf);
    let mut index = proof.leaf_index;

    for sibling in &proof.siblings {
        hash = if index % 2 == 0 {
            NoteMerkleTree::hash_internal(&hash, sibling)
        } else {
            NoteMerkleTree::hash_internal(sibling, &hash)
        };
        index /= 2;
    }

    hash
}

/// Vérification native de référence pour l'étape 7.5.
/// Le futur circuit zk devra imposer exactement les mêmes règles :
/// - appartenance des entrées à l'arbre de Merkle,
/// - connaissance de la clé privée de dépense,
/// - conservation des montants,
/// - validité des engagements de sortie.
pub fn verify_zk_transfer_public_inputs(
    public_inputs: &ZkTransferPublicInputs,
    witness: &ZkTransferWitness,
) -> Result<(), ZkTransferError> {
    if witness.inputs.is_empty() {
        return Err(ZkTransferError::MissingInputs);
    }

    if witness.outputs.is_empty() {
        return Err(ZkTransferError::MissingOutputs);
    }

    if witness.outputs.len() != public_inputs.output_commitments.len() {
        return Err(ZkTransferError::OutputCommitmentCountMismatch {
            expected: witness.outputs.len(),
            actual: public_inputs.output_commitments.len(),
        });
    }

    let mut total_inputs = 0u64;

    for (index, input) in witness.inputs.iter().enumerate() {
        if input.spending_private_key == [0u8; 32] {
            return Err(ZkTransferError::InvalidSpendingPrivateKey { index });
        }

        let recipient_public_key = derive_note_recipient_public_key(&input.spending_private_key);

        let expected_note = Note::new(input.value, recipient_public_key, input.blinding)
            .map_err(|_| ZkTransferError::InvalidInputNote { index })?;

        if input.merkle_proof.leaf != expected_note.commitment {
            return Err(ZkTransferError::InvalidInputCommitment { index });
        }

        let derived_root = compute_note_merkle_root_from_proof(&input.merkle_proof);
        if derived_root != public_inputs.merkle_root {
            return Err(ZkTransferError::InvalidMerkleProof { index });
        }

        total_inputs = total_inputs
            .checked_add(input.value)
            .ok_or(ZkTransferError::AmountOverflow)?;
    }

    let mut total_outputs = 0u64;

    for (index, output) in witness.outputs.iter().enumerate() {
        let expected_note = Note::new(output.value, output.recipient_public_key, output.blinding)
            .map_err(|_| ZkTransferError::InvalidOutputNote { index })?;

        if public_inputs.output_commitments[index] != expected_note.commitment {
            return Err(ZkTransferError::InvalidOutputCommitment { index });
        }

        total_outputs = total_outputs
            .checked_add(output.value)
            .ok_or(ZkTransferError::AmountOverflow)?;
    }

    let expected_inputs_total = total_outputs
        .checked_add(public_inputs.fee)
        .ok_or(ZkTransferError::AmountOverflow)?;

    if total_inputs != expected_inputs_total {
        return Err(ZkTransferError::AmountImbalance {
            inputs_total: total_inputs,
            outputs_total: total_outputs,
            fee: public_inputs.fee,
        });
    }

    Ok(())
}

pub fn zk_transfer_public_inputs_to_field_elements(
    public_inputs: &ZkTransferPublicInputs,
) -> Result<Vec<Fr>, ZkTransferProofSystemError> {
    let cs = ark_relations::r1cs::ConstraintSystem::<Fr>::new_ref();

    let _merkle_root_var =
        Sha256DigestVar::<Fr>::new_input(cs.clone(), || Ok(public_inputs.merkle_root.to_vec()))
            .map_err(|_| ZkTransferProofSystemError::PublicInputsEncodingFailed)?;

    let _fee_var = UInt64::<Fr>::new_input(cs.clone(), || Ok(public_inputs.fee))
        .map_err(|_| ZkTransferProofSystemError::PublicInputsEncodingFailed)?;

    for commitment in &public_inputs.output_commitments {
        let _commitment_var =
            Sha256DigestVar::<Fr>::new_input(cs.clone(), || Ok(commitment.to_vec()))
                .map_err(|_| ZkTransferProofSystemError::PublicInputsEncodingFailed)?;
    }

    let mut field_elements = Vec::with_capacity(cs.num_instance_variables().saturating_sub(1));

    for index in 1..cs.num_instance_variables() {
        let value = cs
            .assigned_value(ark_relations::r1cs::Variable::Instance(index))
            .ok_or(ZkTransferProofSystemError::PublicInputsEncodingFailed)?;
        field_elements.push(value);
    }

    Ok(field_elements)
}

pub fn prepare_zk_transfer_verifying_key(
    verifying_key: &ZkTransferVerifyingKey,
) -> ZkTransferPreparedVerifyingKey {
    prepare_verifying_key(verifying_key)
}

pub fn generate_zk_transfer_proof<R: RngCore>(
    proving_key: &ZkTransferProvingKey,
    public_inputs: &ZkTransferPublicInputs,
    witness: &ZkTransferWitness,
    rng: &mut R,
) -> Result<ZkTransferProof, ZkTransferProofSystemError> {
    verify_zk_transfer_public_inputs(public_inputs, witness)
        .map_err(ZkTransferProofSystemError::InvalidWitness)?;

    let circuit = ZkTransferCircuit {
        public_inputs: public_inputs.clone(),
        witness: witness.clone(),
    };

    Groth16::<Bls12_381>::create_random_proof_with_reduction(circuit, proving_key, rng).map_err(
        |error| ZkTransferProofSystemError::ProvingFailed {
            message: error.to_string(),
        },
    )
}

pub fn verify_zk_transfer_proof(
    verifying_key: &ZkTransferVerifyingKey,
    public_inputs: &ZkTransferPublicInputs,
    proof: &ZkTransferProof,
) -> Result<bool, ZkTransferProofSystemError> {
    let prepared_verifying_key = prepare_zk_transfer_verifying_key(verifying_key);
    verify_zk_transfer_proof_with_prepared_key(&prepared_verifying_key, public_inputs, proof)
}

pub fn verify_zk_transfer_proof_with_prepared_key(
    prepared_verifying_key: &ZkTransferPreparedVerifyingKey,
    public_inputs: &ZkTransferPublicInputs,
    proof: &ZkTransferProof,
) -> Result<bool, ZkTransferProofSystemError> {
    let field_elements = zk_transfer_public_inputs_to_field_elements(public_inputs)?;

    Groth16::<Bls12_381>::verify_proof(prepared_verifying_key, proof, &field_elements).map_err(
        |error| ZkTransferProofSystemError::VerificationFailed {
            message: error.to_string(),
        },
    )
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ZkTransferCircuit {
    pub public_inputs: ZkTransferPublicInputs,
    pub witness: ZkTransferWitness,
}

impl ZkTransferCircuit {
    fn constant_bytes(bytes: &[u8]) -> Vec<UInt8<Fr>> {
        UInt8::constant_vec(bytes)
    }

    fn sha256_bytes(bytes: &[UInt8<Fr>]) -> Result<Sha256DigestVar<Fr>, SynthesisError> {
        let params = Sha256UnitVar::<Fr>::default();
        <Sha256Gadget<Fr> as CRHSchemeGadget<ArkSha256, Fr>>::evaluate(&params, bytes)
    }

    fn derive_recipient_public_key_var(
        spending_private_key: &[UInt8<Fr>],
    ) -> Result<Sha256DigestVar<Fr>, SynthesisError> {
        let mut data = Self::constant_bytes(b"prims-note-owner");
        data.extend_from_slice(spending_private_key);
        Self::sha256_bytes(&data)
    }

    fn note_commitment_var(
        value: &UInt64<Fr>,
        recipient_public_key: &[UInt8<Fr>],
        blinding: &[UInt8<Fr>],
    ) -> Result<Sha256DigestVar<Fr>, SynthesisError> {
        let mut data = value.to_bytes_le()?;
        data.extend_from_slice(recipient_public_key);
        data.extend_from_slice(blinding);
        Self::sha256_bytes(&data)
    }

    fn merkle_leaf_hash_var(
        commitment: &Sha256DigestVar<Fr>,
    ) -> Result<Sha256DigestVar<Fr>, SynthesisError> {
        let mut data = Self::constant_bytes(b"prims-note-merkle-leaf");
        data.extend(commitment.to_bytes_le()?);
        Self::sha256_bytes(&data)
    }

    fn merkle_internal_hash_var(
        left: &Sha256DigestVar<Fr>,
        right: &Sha256DigestVar<Fr>,
    ) -> Result<Sha256DigestVar<Fr>, SynthesisError> {
        let mut prefix = Self::constant_bytes(b"prims-note-merkle-node");
        let left_bytes = left.to_bytes_le()?;
        let right_bytes = right.to_bytes_le()?;
        prefix.extend(left_bytes);
        prefix.extend(right_bytes);
        Self::sha256_bytes(&prefix)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ZkTransferSetupCircuitMetadata {
    pub input_count: usize,
    pub output_count: usize,
    pub merkle_path_len: usize,
    pub tree_leaf_count: usize,
    pub fee: u64,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ZkTransferSetupError {
    #[error("failed to create reference input note for setup")]
    InvalidReferenceInputNote,
    #[error("failed to create reference filler note for setup")]
    InvalidReferenceFillerNote,
    #[error("failed to generate reference merkle proof for setup")]
    InvalidReferenceMerkleProof,
}

/// Construit un circuit de référence valide pour la génération initiale des paramètres.
/// Important : dans l'état actuel du prototype, les paramètres générés sont liés à cette
/// forme de circuit (1 entrée, 2 sorties, profondeur de preuve de Merkle correspondante).
pub fn build_reference_zk_transfer_setup_circuit()
-> Result<(ZkTransferCircuit, ZkTransferSetupCircuitMetadata), ZkTransferSetupError> {
    let owner_private_key: NoteSpendingPrivateKey = [21u8; 32];
    let owner_public_key = derive_note_recipient_public_key(&owner_private_key);

    let input_note = Note::new(70, owner_public_key, [31u8; 32])
        .map_err(|_| ZkTransferSetupError::InvalidReferenceInputNote)?;
    let filler_note = Note::new(5, [22u8; 32], [32u8; 32])
        .map_err(|_| ZkTransferSetupError::InvalidReferenceFillerNote)?;

    let tree = NoteMerkleTree::from_notes(&vec![input_note.clone(), filler_note]);
    let merkle_proof = tree
        .generate_proof(0)
        .map_err(|_| ZkTransferSetupError::InvalidReferenceMerkleProof)?;

    let outputs = vec![
        ZkNoteOutputWitness {
            value: 40,
            recipient_public_key: [41u8; 32],
            blinding: [51u8; 32],
        },
        ZkNoteOutputWitness {
            value: 25,
            recipient_public_key: [42u8; 32],
            blinding: [52u8; 32],
        },
    ];

    let output_commitments = outputs
        .iter()
        .map(|output| {
            Note::compute_commitment(output.value, &output.recipient_public_key, &output.blinding)
        })
        .collect();

    let metadata = ZkTransferSetupCircuitMetadata {
        input_count: 1,
        output_count: outputs.len(),
        merkle_path_len: merkle_proof.siblings.len(),
        tree_leaf_count: 2,
        fee: 5,
    };

    Ok((
        ZkTransferCircuit {
            public_inputs: ZkTransferPublicInputs {
                merkle_root: tree.root(),
                fee: metadata.fee,
                output_commitments,
            },
            witness: ZkTransferWitness {
                inputs: vec![ZkNoteInputWitness {
                    value: 70,
                    spending_private_key: owner_private_key,
                    blinding: [31u8; 32],
                    merkle_proof,
                }],
                outputs,
            },
        },
        metadata,
    ))
}

impl ConstraintSynthesizer<Fr> for ZkTransferCircuit {
    fn generate_constraints(self, cs: ConstraintSystemRef<Fr>) -> Result<(), SynthesisError> {
        if self.witness.inputs.is_empty()
            || self.witness.outputs.is_empty()
            || self.witness.outputs.len() != self.public_inputs.output_commitments.len()
        {
            return Err(SynthesisError::Unsatisfiable);
        }

        verify_zk_transfer_public_inputs(&self.public_inputs, &self.witness)
            .map_err(|_| SynthesisError::Unsatisfiable)?;

        let merkle_root_var = Sha256DigestVar::<Fr>::new_input(cs.clone(), || {
            Ok(self.public_inputs.merkle_root.to_vec())
        })?;
        let fee_var = UInt64::<Fr>::new_input(cs.clone(), || Ok(self.public_inputs.fee))?;

        let mut total_input_values = Vec::with_capacity(self.witness.inputs.len());

        for input in &self.witness.inputs {
            let value_var = UInt64::<Fr>::new_witness(cs.clone(), || Ok(input.value))?;
            total_input_values.push(value_var.clone());

            let spending_private_key_var =
                UInt8::new_witness_vec(cs.clone(), &input.spending_private_key)?;
            let blinding_var = UInt8::new_witness_vec(cs.clone(), &input.blinding)?;
            let proof_leaf_var = Sha256DigestVar::<Fr>::new_witness(cs.clone(), || {
                Ok(input.merkle_proof.leaf.to_vec())
            })?;

            let derived_recipient_public_key_var =
                Self::derive_recipient_public_key_var(&spending_private_key_var)?;
            let expected_commitment_var = Self::note_commitment_var(
                &value_var,
                &derived_recipient_public_key_var.to_bytes_le()?,
                &blinding_var,
            )?;
            expected_commitment_var.enforce_equal(&proof_leaf_var)?;

            let mut current_hash = Self::merkle_leaf_hash_var(&proof_leaf_var)?;
            let mut current_index = input.merkle_proof.leaf_index;

            for sibling in &input.merkle_proof.siblings {
                let sibling_var =
                    Sha256DigestVar::<Fr>::new_witness(cs.clone(), || Ok(sibling.to_vec()))?;

                current_hash = if current_index % 2 == 0 {
                    Self::merkle_internal_hash_var(&current_hash, &sibling_var)?
                } else {
                    Self::merkle_internal_hash_var(&sibling_var, &current_hash)?
                };

                current_index /= 2;
            }

            current_hash.enforce_equal(&merkle_root_var)?;
        }

        let mut total_output_values = Vec::with_capacity(self.witness.outputs.len());

        for (index, output) in self.witness.outputs.iter().enumerate() {
            let value_var = UInt64::<Fr>::new_witness(cs.clone(), || Ok(output.value))?;
            total_output_values.push(value_var.clone());

            let recipient_public_key_var =
                UInt8::new_witness_vec(cs.clone(), &output.recipient_public_key)?;
            let blinding_var = UInt8::new_witness_vec(cs.clone(), &output.blinding)?;
            let expected_commitment_var =
                Self::note_commitment_var(&value_var, &recipient_public_key_var, &blinding_var)?;

            let public_commitment_var = Sha256DigestVar::<Fr>::new_input(cs.clone(), || {
                Ok(self.public_inputs.output_commitments[index].to_vec())
            })?;

            expected_commitment_var.enforce_equal(&public_commitment_var)?;
        }

        let total_inputs_var = UInt64::<Fr>::wrapping_add_many(&total_input_values)?;
        let total_outputs_var = UInt64::<Fr>::wrapping_add_many(&total_output_values)?;
        let expected_inputs_var = UInt64::<Fr>::wrapping_add_many(&[total_outputs_var, fee_var])?;

        total_inputs_var.enforce_equal(&expected_inputs_var)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::{SeedableRng, rngs::StdRng};

    fn sample_utxo_id(index: u32) -> UtxoId {
        UtxoId {
            tx_hash: [index as u8; 32],
            output_index: index,
        }
    }

    fn sample_recipient_public_key(seed: u8) -> NoteRecipientPublicKey {
        [seed; 32]
    }

    fn sample_blinding(seed: u8) -> [u8; 32] {
        [seed; 32]
    }

    #[test]
    fn utxo_transaction_consumes_inputs_and_creates_outputs() {
        let transaction = UtxoTransaction {
            inputs: vec![UtxoInput {
                previous_output: sample_utxo_id(0),
            }],
            outputs: vec![UtxoOutput { amount: 40 }, UtxoOutput { amount: 55 }],
            fee: 5,
        };

        assert_eq!(transaction.input_count(), 1);
        assert_eq!(transaction.output_count(), 2);
        assert_eq!(transaction.validate_basic(), Ok(()));
    }

    #[test]
    fn utxo_transaction_requires_at_least_one_input() {
        let transaction = UtxoTransaction {
            inputs: vec![],
            outputs: vec![UtxoOutput { amount: 50 }],
            fee: 1,
        };

        assert_eq!(
            transaction.validate_basic(),
            Err(UtxoModelError::MissingInputs)
        );
    }

    #[test]
    fn utxo_transaction_requires_at_least_one_output() {
        let transaction = UtxoTransaction {
            inputs: vec![UtxoInput {
                previous_output: sample_utxo_id(1),
            }],
            outputs: vec![],
            fee: 1,
        };

        assert_eq!(
            transaction.validate_basic(),
            Err(UtxoModelError::MissingOutputs)
        );
    }

    #[test]
    fn utxo_transaction_rejects_zero_value_outputs() {
        let transaction = UtxoTransaction {
            inputs: vec![UtxoInput {
                previous_output: sample_utxo_id(2),
            }],
            outputs: vec![UtxoOutput { amount: 0 }],
            fee: 1,
        };

        assert_eq!(
            transaction.validate_basic(),
            Err(UtxoModelError::ZeroOutputAmount)
        );
    }

    #[test]
    fn note_commitment_is_deterministic_for_same_inputs() {
        let recipient = sample_recipient_public_key(7);
        let blinding = sample_blinding(9);

        let first = Note::compute_commitment(42, &recipient, &blinding);
        let second = Note::compute_commitment(42, &recipient, &blinding);

        assert_eq!(first, second);
    }

    #[test]
    fn note_commitment_changes_when_value_changes() {
        let recipient = sample_recipient_public_key(7);
        let blinding = sample_blinding(9);

        let first = Note::compute_commitment(42, &recipient, &blinding);
        let second = Note::compute_commitment(43, &recipient, &blinding);

        assert_ne!(first, second);
    }

    #[test]
    fn note_creation_builds_commitment_from_value_and_recipient() {
        let recipient = sample_recipient_public_key(5);
        let blinding = sample_blinding(8);

        let note = Note::new(99, recipient, blinding).expect("note should be created");
        let expected = Note::compute_commitment(99, &recipient, &blinding);

        assert_eq!(note.commitment, expected);
    }

    #[test]
    fn note_creation_rejects_zero_value() {
        let recipient = sample_recipient_public_key(5);
        let blinding = sample_blinding(8);

        assert_eq!(Note::new(0, recipient, blinding), Err(NoteError::ZeroValue));
    }

    #[test]
    fn note_creation_rejects_zero_recipient_public_key() {
        let blinding = sample_blinding(8);

        assert_eq!(
            Note::new(10, [0; 32], blinding),
            Err(NoteError::InvalidRecipientPublicKey)
        );
    }

    #[test]
    fn note_creation_rejects_zero_blinding() {
        let recipient = sample_recipient_public_key(5);

        assert_eq!(
            Note::new(10, recipient, [0; 32]),
            Err(NoteError::InvalidBlinding)
        );
    }

    #[test]
    fn note_merkle_tree_empty_root_is_deterministic() {
        let first = NoteMerkleTree::new().root();
        let second = NoteMerkleTree::new().root();

        assert_eq!(first, second);
        assert_ne!(first, [0u8; 32]);
    }

    #[test]
    fn note_merkle_tree_root_is_deterministic_for_same_notes() {
        let notes = vec![
            Note::new(10, sample_recipient_public_key(1), sample_blinding(11))
                .expect("first note should be created"),
            Note::new(20, sample_recipient_public_key(2), sample_blinding(12))
                .expect("second note should be created"),
            Note::new(30, sample_recipient_public_key(3), sample_blinding(13))
                .expect("third note should be created"),
        ];

        let first = NoteMerkleTree::from_notes(&notes);
        let second = NoteMerkleTree::from_notes(&notes);

        assert_eq!(first.root(), second.root());
    }

    #[test]
    fn note_merkle_tree_root_changes_when_notes_change() {
        let first_notes = vec![
            Note::new(10, sample_recipient_public_key(1), sample_blinding(11))
                .expect("first note should be created"),
            Note::new(20, sample_recipient_public_key(2), sample_blinding(12))
                .expect("second note should be created"),
        ];
        let second_notes = vec![
            Note::new(10, sample_recipient_public_key(1), sample_blinding(11))
                .expect("first note should be created"),
            Note::new(21, sample_recipient_public_key(2), sample_blinding(12))
                .expect("second note should be created"),
        ];

        let first_tree = NoteMerkleTree::from_notes(&first_notes);
        let second_tree = NoteMerkleTree::from_notes(&second_notes);

        assert_ne!(first_tree.root(), second_tree.root());
    }

    #[test]
    fn note_merkle_tree_append_matches_from_notes() {
        let notes = vec![
            Note::new(15, sample_recipient_public_key(4), sample_blinding(14))
                .expect("first note should be created"),
            Note::new(25, sample_recipient_public_key(5), sample_blinding(15))
                .expect("second note should be created"),
            Note::new(35, sample_recipient_public_key(6), sample_blinding(16))
                .expect("third note should be created"),
        ];

        let from_notes = NoteMerkleTree::from_notes(&notes);

        let mut appended = NoteMerkleTree::new();
        for note in &notes {
            appended.append(note);
        }

        assert_eq!(appended.leaf_count(), notes.len());
        assert_eq!(appended.root(), from_notes.root());
    }

    #[test]
    fn note_merkle_tree_generates_valid_proof_for_existing_leaf() {
        let notes = vec![
            Note::new(11, sample_recipient_public_key(7), sample_blinding(17))
                .expect("first note should be created"),
            Note::new(22, sample_recipient_public_key(8), sample_blinding(18))
                .expect("second note should be created"),
            Note::new(33, sample_recipient_public_key(9), sample_blinding(19))
                .expect("third note should be created"),
        ];

        let tree = NoteMerkleTree::from_notes(&notes);
        let root = tree.root();
        let proof = tree
            .generate_proof(1)
            .expect("proof should be generated for existing leaf");

        assert_eq!(proof.leaf, notes[1].commitment);
        assert!(NoteMerkleTree::verify_proof(&root, &proof));
    }

    #[test]
    fn note_merkle_tree_rejects_out_of_bounds_leaf_index() {
        let notes = vec![
            Note::new(44, sample_recipient_public_key(10), sample_blinding(20))
                .expect("note should be created"),
        ];
        let tree = NoteMerkleTree::from_notes(&notes);

        assert_eq!(
            tree.generate_proof(1),
            Err(NoteMerkleTreeError::LeafIndexOutOfBounds {
                leaf_index: 1,
                leaf_count: 1,
            })
        );
    }

    fn sample_spending_private_key(seed: u8) -> NoteSpendingPrivateKey {
        [seed; 32]
    }

    fn sample_zk_input(
        value: u64,
        spending_key_seed: u8,
        blinding_seed: u8,
        merkle_proof: NoteMerkleProof,
    ) -> ZkNoteInputWitness {
        ZkNoteInputWitness {
            value,
            spending_private_key: sample_spending_private_key(spending_key_seed),
            blinding: sample_blinding(blinding_seed),
            merkle_proof,
        }
    }

    fn sample_zk_output(value: u64, recipient_seed: u8, blinding_seed: u8) -> ZkNoteOutputWitness {
        ZkNoteOutputWitness {
            value,
            recipient_public_key: sample_recipient_public_key(recipient_seed),
            blinding: sample_blinding(blinding_seed),
        }
    }

    #[test]
    fn zk_transfer_reference_verifier_accepts_balanced_valid_witness() {
        let owner_private_key = sample_spending_private_key(21);
        let owner_public_key = derive_note_recipient_public_key(&owner_private_key);

        let input_note = Note::new(70, owner_public_key, sample_blinding(31))
            .expect("input note should be created");
        let filler_note = Note::new(5, sample_recipient_public_key(22), sample_blinding(32))
            .expect("filler note should be created");

        let tree = NoteMerkleTree::from_notes(&vec![input_note.clone(), filler_note]);
        let merkle_root = tree.root();
        let merkle_proof = tree.generate_proof(0).expect("proof should be generated");

        let outputs = vec![sample_zk_output(40, 41, 51), sample_zk_output(25, 42, 52)];
        let output_commitments = outputs
            .iter()
            .map(|output| {
                Note::compute_commitment(
                    output.value,
                    &output.recipient_public_key,
                    &output.blinding,
                )
            })
            .collect();

        let public_inputs = ZkTransferPublicInputs {
            merkle_root,
            fee: 5,
            output_commitments,
        };

        let witness = ZkTransferWitness {
            inputs: vec![sample_zk_input(70, 21, 31, merkle_proof)],
            outputs,
        };

        assert_eq!(
            verify_zk_transfer_public_inputs(&public_inputs, &witness),
            Ok(())
        );
    }

    #[test]
    fn zk_transfer_reference_verifier_rejects_amount_imbalance() {
        let owner_private_key = sample_spending_private_key(23);
        let owner_public_key = derive_note_recipient_public_key(&owner_private_key);

        let input_note = Note::new(70, owner_public_key, sample_blinding(33))
            .expect("input note should be created");
        let filler_note = Note::new(6, sample_recipient_public_key(24), sample_blinding(34))
            .expect("filler note should be created");

        let tree = NoteMerkleTree::from_notes(&vec![input_note.clone(), filler_note]);
        let merkle_root = tree.root();
        let merkle_proof = tree.generate_proof(0).expect("proof should be generated");

        let outputs = vec![sample_zk_output(40, 43, 53), sample_zk_output(26, 44, 54)];
        let output_commitments = outputs
            .iter()
            .map(|output| {
                Note::compute_commitment(
                    output.value,
                    &output.recipient_public_key,
                    &output.blinding,
                )
            })
            .collect();

        let public_inputs = ZkTransferPublicInputs {
            merkle_root,
            fee: 5,
            output_commitments,
        };

        let witness = ZkTransferWitness {
            inputs: vec![sample_zk_input(70, 23, 33, merkle_proof)],
            outputs,
        };

        assert_eq!(
            verify_zk_transfer_public_inputs(&public_inputs, &witness),
            Err(ZkTransferError::AmountImbalance {
                inputs_total: 70,
                outputs_total: 66,
                fee: 5,
            })
        );
    }

    #[test]
    fn zk_transfer_reference_verifier_rejects_invalid_merkle_proof() {
        let owner_private_key = sample_spending_private_key(25);
        let owner_public_key = derive_note_recipient_public_key(&owner_private_key);

        let input_note = Note::new(70, owner_public_key, sample_blinding(35))
            .expect("input note should be created");
        let filler_note = Note::new(7, sample_recipient_public_key(26), sample_blinding(36))
            .expect("filler note should be created");

        let tree = NoteMerkleTree::from_notes(&vec![input_note.clone(), filler_note]);
        let mut merkle_proof = tree.generate_proof(0).expect("proof should be generated");
        merkle_proof.siblings[0][0] ^= 0x01;

        let outputs = vec![sample_zk_output(40, 45, 55), sample_zk_output(25, 46, 56)];
        let output_commitments = outputs
            .iter()
            .map(|output| {
                Note::compute_commitment(
                    output.value,
                    &output.recipient_public_key,
                    &output.blinding,
                )
            })
            .collect();

        let public_inputs = ZkTransferPublicInputs {
            merkle_root: tree.root(),
            fee: 5,
            output_commitments,
        };

        let witness = ZkTransferWitness {
            inputs: vec![sample_zk_input(70, 25, 35, merkle_proof)],
            outputs,
        };

        assert_eq!(
            verify_zk_transfer_public_inputs(&public_inputs, &witness),
            Err(ZkTransferError::InvalidMerkleProof { index: 0 })
        );
    }

    #[test]
    fn zk_transfer_circuit_accepts_valid_witness() {
        let owner_private_key = sample_spending_private_key(61);
        let owner_public_key = derive_note_recipient_public_key(&owner_private_key);

        let input_note = Note::new(70, owner_public_key, sample_blinding(71))
            .expect("input note should be created");
        let filler_note = Note::new(9, sample_recipient_public_key(62), sample_blinding(72))
            .expect("filler note should be created");

        let tree = NoteMerkleTree::from_notes(&vec![input_note.clone(), filler_note]);
        let merkle_proof = tree.generate_proof(0).expect("proof should be generated");

        let outputs = vec![sample_zk_output(40, 63, 73), sample_zk_output(25, 64, 74)];
        let output_commitments = outputs
            .iter()
            .map(|output| {
                Note::compute_commitment(
                    output.value,
                    &output.recipient_public_key,
                    &output.blinding,
                )
            })
            .collect();

        let circuit = ZkTransferCircuit {
            public_inputs: ZkTransferPublicInputs {
                merkle_root: tree.root(),
                fee: 5,
                output_commitments,
            },
            witness: ZkTransferWitness {
                inputs: vec![sample_zk_input(70, 61, 71, merkle_proof)],
                outputs,
            },
        };

        let cs = ark_relations::r1cs::ConstraintSystem::<Fr>::new_ref();
        circuit
            .generate_constraints(cs.clone())
            .expect("valid circuit should synthesize");

        assert!(
            cs.is_satisfied()
                .expect("constraint system satisfaction should be checked"),
            "valid witness should satisfy the zk transfer circuit"
        );
    }

    #[test]
    fn zk_transfer_circuit_rejects_invalid_public_output_commitment() {
        let owner_private_key = sample_spending_private_key(65);
        let owner_public_key = derive_note_recipient_public_key(&owner_private_key);

        let input_note = Note::new(70, owner_public_key, sample_blinding(75))
            .expect("input note should be created");
        let filler_note = Note::new(10, sample_recipient_public_key(66), sample_blinding(76))
            .expect("filler note should be created");

        let tree = NoteMerkleTree::from_notes(&vec![input_note.clone(), filler_note]);
        let merkle_proof = tree.generate_proof(0).expect("proof should be generated");

        let outputs = vec![sample_zk_output(40, 67, 77), sample_zk_output(25, 68, 78)];
        let mut output_commitments: Vec<NoteCommitment> = outputs
            .iter()
            .map(|output| {
                Note::compute_commitment(
                    output.value,
                    &output.recipient_public_key,
                    &output.blinding,
                )
            })
            .collect();
        output_commitments[0][0] ^= 0x01;

        let circuit = ZkTransferCircuit {
            public_inputs: ZkTransferPublicInputs {
                merkle_root: tree.root(),
                fee: 5,
                output_commitments,
            },
            witness: ZkTransferWitness {
                inputs: vec![sample_zk_input(70, 65, 75, merkle_proof)],
                outputs,
            },
        };

        let cs = ark_relations::r1cs::ConstraintSystem::<Fr>::new_ref();
        let result = circuit.generate_constraints(cs);

        assert!(
            matches!(
                result,
                Err(ark_relations::r1cs::SynthesisError::Unsatisfiable)
            ),
            "invalid public commitment should make the circuit unsatisfiable"
        );
    }

    #[test]
    fn zk_transfer_groth16_proof_verifies_with_matching_public_inputs() {
        let (circuit, _) = build_reference_zk_transfer_setup_circuit()
            .expect("reference setup circuit should be built");

        let mut setup_rng = StdRng::seed_from_u64(7_700);
        let proving_key = Groth16::<Bls12_381>::generate_random_parameters_with_reduction(
            circuit.clone(),
            &mut setup_rng,
        )
        .expect("trusted setup should succeed");
        let verifying_key = proving_key.vk.clone();

        let mut proof_rng = StdRng::seed_from_u64(7_701);
        let proof = generate_zk_transfer_proof(
            &proving_key,
            &circuit.public_inputs,
            &circuit.witness,
            &mut proof_rng,
        )
        .expect("proof generation should succeed");

        assert!(
            verify_zk_transfer_proof(&verifying_key, &circuit.public_inputs, &proof)
                .expect("proof verification should succeed"),
            "matching public inputs should verify"
        );
    }

    #[test]
    fn zk_transfer_groth16_proof_rejects_tampered_fee() {
        let (circuit, _) = build_reference_zk_transfer_setup_circuit()
            .expect("reference setup circuit should be built");

        let mut setup_rng = StdRng::seed_from_u64(7_702);
        let proving_key = Groth16::<Bls12_381>::generate_random_parameters_with_reduction(
            circuit.clone(),
            &mut setup_rng,
        )
        .expect("trusted setup should succeed");
        let prepared_verifying_key = prepare_zk_transfer_verifying_key(&proving_key.vk);

        let mut proof_rng = StdRng::seed_from_u64(7_703);
        let proof = generate_zk_transfer_proof(
            &proving_key,
            &circuit.public_inputs,
            &circuit.witness,
            &mut proof_rng,
        )
        .expect("proof generation should succeed");

        let mut tampered_public_inputs = circuit.public_inputs.clone();
        tampered_public_inputs.fee += 1;

        assert!(
            !verify_zk_transfer_proof_with_prepared_key(
                &prepared_verifying_key,
                &tampered_public_inputs,
                &proof,
            )
            .expect("proof verification should return a boolean"),
            "tampered public inputs must be rejected"
        );
    }

    #[test]
    fn anon_transaction_verifies_with_matching_public_inputs() {
        let (circuit, _) = build_reference_zk_transfer_setup_circuit()
            .expect("reference setup circuit should be built");

        let mut setup_rng = StdRng::seed_from_u64(7_800);
        let proving_key = Groth16::<Bls12_381>::generate_random_parameters_with_reduction(
            circuit.clone(),
            &mut setup_rng,
        )
        .expect("trusted setup should succeed");
        let verifying_key = proving_key.vk.clone();

        let mut proof_rng = StdRng::seed_from_u64(7_801);
        let proof = generate_zk_transfer_proof(
            &proving_key,
            &circuit.public_inputs,
            &circuit.witness,
            &mut proof_rng,
        )
        .expect("proof generation should succeed");

        let anon_transaction = AnonTransaction::new(&circuit.public_inputs, &proof)
            .expect("anon transaction should be created");

        assert_eq!(
            anon_transaction.public_inputs(),
            circuit.public_inputs,
            "anon transaction should reconstruct the original public inputs"
        );

        assert!(
            anon_transaction
                .verify(&verifying_key)
                .expect("anon transaction verification should succeed"),
            "anon transaction should verify with matching public inputs"
        );
    }

    #[test]
    fn anon_transaction_rejects_tampered_fee() {
        let (circuit, _) = build_reference_zk_transfer_setup_circuit()
            .expect("reference setup circuit should be built");

        let mut setup_rng = StdRng::seed_from_u64(7_802);
        let proving_key = Groth16::<Bls12_381>::generate_random_parameters_with_reduction(
            circuit.clone(),
            &mut setup_rng,
        )
        .expect("trusted setup should succeed");
        let prepared_verifying_key = prepare_zk_transfer_verifying_key(&proving_key.vk);

        let mut proof_rng = StdRng::seed_from_u64(7_803);
        let proof = generate_zk_transfer_proof(
            &proving_key,
            &circuit.public_inputs,
            &circuit.witness,
            &mut proof_rng,
        )
        .expect("proof generation should succeed");

        let mut anon_transaction = AnonTransaction::new(&circuit.public_inputs, &proof)
            .expect("anon transaction should be created");
        anon_transaction.fee += 1;

        assert!(
            !anon_transaction
                .verify_with_prepared_key(&prepared_verifying_key)
                .expect("anon transaction verification should return a boolean"),
            "tampered fee must be rejected"
        );
    }

    #[test]
    fn anon_transaction_public_inputs_do_not_reveal_which_same_value_note_was_spent() {
        let first_private_key = sample_spending_private_key(87);
        let second_private_key = sample_spending_private_key(88);

        let first_note = Note::new(
            70,
            derive_note_recipient_public_key(&first_private_key),
            sample_blinding(89),
        )
        .expect("first input note should be created");
        let second_note = Note::new(
            70,
            derive_note_recipient_public_key(&second_private_key),
            sample_blinding(90),
        )
        .expect("second input note should be created");

        let tree = NoteMerkleTree::from_notes(&vec![first_note.clone(), second_note.clone()]);
        let first_merkle_proof = tree
            .generate_proof(0)
            .expect("first proof should be generated");
        let second_merkle_proof = tree
            .generate_proof(1)
            .expect("second proof should be generated");

        let outputs = vec![sample_zk_output(40, 91, 92), sample_zk_output(25, 93, 94)];
        let output_commitments = outputs
            .iter()
            .map(|output| {
                Note::compute_commitment(
                    output.value,
                    &output.recipient_public_key,
                    &output.blinding,
                )
            })
            .collect();

        let public_inputs = ZkTransferPublicInputs {
            merkle_root: tree.root(),
            fee: 5,
            output_commitments,
        };

        let first_witness = ZkTransferWitness {
            inputs: vec![sample_zk_input(70, 87, 89, first_merkle_proof)],
            outputs: outputs.clone(),
        };
        let second_witness = ZkTransferWitness {
            inputs: vec![sample_zk_input(70, 88, 90, second_merkle_proof)],
            outputs,
        };

        assert_eq!(
            verify_zk_transfer_public_inputs(&public_inputs, &first_witness),
            Ok(())
        );
        assert_eq!(
            verify_zk_transfer_public_inputs(&public_inputs, &second_witness),
            Ok(())
        );

        let first_circuit = ZkTransferCircuit {
            public_inputs: public_inputs.clone(),
            witness: first_witness.clone(),
        };
        let second_circuit = ZkTransferCircuit {
            public_inputs: public_inputs.clone(),
            witness: second_witness.clone(),
        };

        let mut first_setup_rng = StdRng::seed_from_u64(7_804);
        let first_proving_key = Groth16::<Bls12_381>::generate_random_parameters_with_reduction(
            first_circuit,
            &mut first_setup_rng,
        )
        .expect("first trusted setup should succeed");
        let first_verifying_key = first_proving_key.vk.clone();

        let mut second_setup_rng = StdRng::seed_from_u64(7_805);
        let second_proving_key = Groth16::<Bls12_381>::generate_random_parameters_with_reduction(
            second_circuit,
            &mut second_setup_rng,
        )
        .expect("second trusted setup should succeed");
        let second_verifying_key = second_proving_key.vk.clone();

        let mut first_proof_rng = StdRng::seed_from_u64(7_806);
        let first_proof = generate_zk_transfer_proof(
            &first_proving_key,
            &public_inputs,
            &first_witness,
            &mut first_proof_rng,
        )
        .expect("first proof generation should succeed");

        let mut second_proof_rng = StdRng::seed_from_u64(7_807);
        let second_proof = generate_zk_transfer_proof(
            &second_proving_key,
            &public_inputs,
            &second_witness,
            &mut second_proof_rng,
        )
        .expect("second proof generation should succeed");

        let first_anon_transaction = AnonTransaction::new(&public_inputs, &first_proof)
            .expect("first anon transaction should be created");
        let second_anon_transaction = AnonTransaction::new(&public_inputs, &second_proof)
            .expect("second anon transaction should be created");

        assert!(
            first_anon_transaction
                .verify(&first_verifying_key)
                .expect("first anon transaction verification should succeed"),
            "first anon transaction should verify"
        );
        assert!(
            second_anon_transaction
                .verify(&second_verifying_key)
                .expect("second anon transaction verification should succeed"),
            "second anon transaction should verify"
        );

        assert_eq!(
            first_anon_transaction.public_inputs(),
            second_anon_transaction.public_inputs(),
            "public inputs should stay identical for both anonymous transactions"
        );
        assert_ne!(
            first_anon_transaction.proof, second_anon_transaction.proof,
            "different private witnesses should produce different proofs"
        );
        assert!(
            first_anon_transaction
                .output_commitments
                .iter()
                .all(|commitment| commitment != &first_note.commitment
                    && commitment != &second_note.commitment),
            "public outputs should not reveal which input note was spent"
        );
    }

    #[test]
    fn viewable_note_matches_its_viewing_key() {
        let viewing_key = [81u8; 32];
        let other_viewing_key = [82u8; 32];

        let note = ViewableNote::new(
            42,
            sample_recipient_public_key(83),
            viewing_key,
            sample_blinding(84),
            1,
        )
        .expect("viewable note should be created");

        assert!(note.is_visible_to(&viewing_key));
        assert!(!note.is_visible_to(&other_viewing_key));
        assert_eq!(note.shard_id, 1);
    }

    #[test]
    fn viewable_note_rejects_zero_viewing_key() {
        let result = ViewableNote::new(
            42,
            sample_recipient_public_key(85),
            [0u8; 32],
            sample_blinding(86),
            0,
        );

        assert!(matches!(result, Err(NoteError::InvalidViewingKey)));
    }

    #[test]
    fn anon_transaction_rejects_truncated_serialized_proof() {
        let (circuit, _) = build_reference_zk_transfer_setup_circuit()
            .expect("reference setup circuit should be built");

        let mut setup_rng = StdRng::seed_from_u64(7_808);
        let proving_key = Groth16::<Bls12_381>::generate_random_parameters_with_reduction(
            circuit.clone(),
            &mut setup_rng,
        )
        .expect("trusted setup should succeed");
        let prepared_verifying_key = prepare_zk_transfer_verifying_key(&proving_key.vk);

        let mut proof_rng = StdRng::seed_from_u64(7_809);
        let proof = generate_zk_transfer_proof(
            &proving_key,
            &circuit.public_inputs,
            &circuit.witness,
            &mut proof_rng,
        )
        .expect("proof generation should succeed");

        let mut anon_transaction = AnonTransaction::new(&circuit.public_inputs, &proof)
            .expect("anon transaction should be created");
        anon_transaction
            .proof
            .pop()
            .expect("serialized proof bytes should not be empty");

        assert!(
            matches!(
                anon_transaction.verify_with_prepared_key(&prepared_verifying_key),
                Err(AnonTransactionError::ProofDeserializationFailed { .. })
            ),
            "truncated serialized proof must be rejected"
        );
    }

    #[test]
    fn zk_transfer_proof_generation_and_verification_succeeds() {
        let (circuit, _) = build_reference_zk_transfer_setup_circuit()
            .expect("reference setup circuit should be built");

        let mut setup_rng = StdRng::seed_from_u64(7_120);
        let proving_key = Groth16::<Bls12_381>::generate_random_parameters_with_reduction(
            circuit.clone(),
            &mut setup_rng,
        )
        .expect("trusted setup should succeed");
        let prepared_verifying_key = prepare_zk_transfer_verifying_key(&proving_key.vk);

        let mut proof_rng = StdRng::seed_from_u64(7_121);
        let proof = generate_zk_transfer_proof(
            &proving_key,
            &circuit.public_inputs,
            &circuit.witness,
            &mut proof_rng,
        )
        .expect("proof generation should succeed");

        let anon_transaction = AnonTransaction::new(&circuit.public_inputs, &proof)
            .expect("anon transaction should be created");

        assert!(
            anon_transaction
                .verify_with_prepared_key(&prepared_verifying_key)
                .expect("anon transaction verification should succeed"),
            "proof should verify successfully"
        );
    }
}
