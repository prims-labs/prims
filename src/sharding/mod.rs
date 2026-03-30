use crate::{
    blockchain::types::{
        Block, CrossShardPhase, CrossShardReceipt, RECEIPT_PROOF_LENGTH, Transaction, Validator,
    },
    consensus::{ConsensusVote, VoteCollector, VoteError, VoteTally, is_block_finalized},
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const DEFAULT_INITIAL_SHARD_COUNT: u16 = 64;
pub const MIN_SHARD_COUNT: u16 = 1;
pub const SHARD_STATE_ROOT_LENGTH: usize = 32;
pub const EMPTY_SHARD_STATE_ROOT: [u8; SHARD_STATE_ROOT_LENGTH] = [0; SHARD_STATE_ROOT_LENGTH];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ShardingConfig {
    pub shard_count: u16,
}

impl Default for ShardingConfig {
    fn default() -> Self {
        Self {
            shard_count: DEFAULT_INITIAL_SHARD_COUNT,
        }
    }
}

impl ShardingConfig {
    pub fn new(shard_count: u16) -> Result<Self, ShardingConfigError> {
        validate_shard_count(shard_count)?;
        Ok(Self { shard_count })
    }

    pub fn apply_governance_update(
        &mut self,
        new_shard_count: u16,
    ) -> Result<(), ShardingConfigError> {
        validate_shard_count(new_shard_count)?;
        self.shard_count = new_shard_count;
        Ok(())
    }
}

fn validate_shard_count(shard_count: u16) -> Result<(), ShardingConfigError> {
    if shard_count < MIN_SHARD_COUNT {
        return Err(ShardingConfigError::ShardCountTooLow {
            minimum: MIN_SHARD_COUNT,
        });
    }

    if !shard_count.is_power_of_two() {
        return Err(ShardingConfigError::ShardCountMustBePowerOfTwo {
            provided: shard_count,
        });
    }

    Ok(())
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ShardingConfigError {
    #[error("shard count must be at least {minimum}")]
    ShardCountTooLow { minimum: u16 },

    #[error("shard count must be a power of two, got {provided}")]
    ShardCountMustBePowerOfTwo { provided: u16 },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ShardCommitteeAssignment {
    pub epoch: u64,
    pub shard_id: u16,
    pub validator_addresses: Vec<Vec<u8>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ShardStateRoot {
    pub epoch: u64,
    pub shard_id: u16,
    pub state_root: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CrossShardExecution {
    pub transaction: Transaction,
    pub tx_hash: Vec<u8>,
    pub source_prepared: bool,
    pub destination_validated: bool,
    pub committed: bool,
    pub receipts: Vec<CrossShardReceipt>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShardConsensus {
    pub epoch: u64,
    pub shard_id: u16,
    pub committee: Vec<Validator>,
    collector: VoteCollector,
}

impl ShardConsensus {
    pub fn new(epoch: u64, shard_id: u16, committee: Vec<Validator>) -> Self {
        Self {
            epoch,
            shard_id,
            committee: sort_validators(committee),
            collector: VoteCollector::new(),
        }
    }

    pub fn committee(&self) -> &[Validator] {
        &self.committee
    }

    pub fn vote_count(&self) -> usize {
        self.collector.len()
    }

    pub fn collect_vote(&mut self, vote: ConsensusVote) -> Result<(), ShardConsensusError> {
        self.collector.collect_vote(vote, &self.committee)?;
        Ok(())
    }

    pub fn tally(&self) -> Result<VoteTally, ShardConsensusError> {
        Ok(self.collector.tally(&self.committee)?)
    }

    pub fn is_finalized(&self) -> Result<bool, ShardConsensusError> {
        Ok(is_block_finalized(&self.tally()?, &self.committee))
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ShardConsensusError {
    #[error(transparent)]
    BeaconChain(#[from] BeaconChainError),

    #[error(transparent)]
    Vote(#[from] VoteError),

    #[error("missing committee assignment for shard {shard_id} at epoch {epoch}")]
    MissingCommitteeAssignment { epoch: u64, shard_id: u16 },
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum CrossShardTransactionError {
    #[error(transparent)]
    BeaconChain(#[from] BeaconChainError),

    #[error("transaction must target another shard for cross-shard flow")]
    NotCrossShard,

    #[error("source shard must be prepared before destination validation")]
    SourceShardNotPrepared,

    #[error("destination shard must be validated before commit")]
    DestinationShardNotValidated,

    #[error("destination shard has already been validated")]
    DestinationAlreadyValidated,

    #[error("cross-shard transaction has already been committed")]
    AlreadyCommitted,

    #[error("cross-shard transaction serialization failed: {0}")]
    Serialization(String),

    #[error("cross-shard receipt count mismatch: expected {expected}, got {provided}")]
    InvalidReceiptCount { expected: usize, provided: usize },

    #[error("missing cross-shard receipt for phase {phase:?}")]
    MissingReceiptPhase { phase: CrossShardPhase },

    #[error("cross-shard receipt transaction hash does not match execution hash")]
    ReceiptTxHashMismatch,

    #[error(
        "cross-shard receipt route mismatch: expected {expected_source_shard}->{expected_destination_shard}, got {actual_source_shard}->{actual_destination_shard}"
    )]
    ReceiptRouteMismatch {
        expected_source_shard: u16,
        expected_destination_shard: u16,
        actual_source_shard: u16,
        actual_destination_shard: u16,
    },

    #[error("invalid cross-shard receipt proof for phase {phase:?}")]
    InvalidReceiptProof { phase: CrossShardPhase },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BeaconChain {
    pub config: ShardingConfig,
    pub validators: Vec<Validator>,
    pub current_epoch: u64,
    pub shard_committees: Vec<ShardCommitteeAssignment>,
    pub shard_state_roots: Vec<ShardStateRoot>,
}

impl BeaconChain {
    pub fn new(config: ShardingConfig, validators: Vec<Validator>) -> Self {
        let mut beacon_chain = Self {
            config,
            validators: sort_validators(validators),
            current_epoch: 0,
            shard_committees: Vec::new(),
            shard_state_roots: Vec::new(),
        };

        beacon_chain.rebuild_epoch(0);
        beacon_chain
    }

    pub fn active_validators(&self) -> Vec<Validator> {
        self.validators
            .iter()
            .filter(|validator| validator.stake > 0)
            .cloned()
            .collect()
    }

    pub fn register_validator(&mut self, validator: Validator) {
        self.validators.push(validator);
        self.validators = sort_validators(std::mem::take(&mut self.validators));
        self.rebuild_epoch(self.current_epoch);
    }

    pub fn replace_validators(&mut self, validators: Vec<Validator>) {
        self.validators = sort_validators(validators);
        self.rebuild_epoch(self.current_epoch);
    }

    pub fn advance_epoch(&mut self) {
        let next_epoch = self.current_epoch + 1;
        self.rebuild_epoch(next_epoch);
    }

    pub fn committees_for_epoch(&self, epoch: u64) -> Vec<&ShardCommitteeAssignment> {
        self.shard_committees
            .iter()
            .filter(|assignment| assignment.epoch == epoch)
            .collect()
    }

    pub fn committee_for_shard(
        &self,
        epoch: u64,
        shard_id: u16,
    ) -> Option<&ShardCommitteeAssignment> {
        self.shard_committees
            .iter()
            .find(|assignment| assignment.epoch == epoch && assignment.shard_id == shard_id)
    }

    pub fn validators_for_shard(
        &self,
        epoch: u64,
        shard_id: u16,
    ) -> Result<Vec<Validator>, ShardConsensusError> {
        validate_shard_id(shard_id, self.config.shard_count)?;

        let assignment = self
            .committee_for_shard(epoch, shard_id)
            .ok_or(ShardConsensusError::MissingCommitteeAssignment { epoch, shard_id })?;

        let mut committee = self
            .active_validators()
            .into_iter()
            .filter(|validator| {
                assignment
                    .validator_addresses
                    .iter()
                    .any(|address| address == &validator.address)
            })
            .collect::<Vec<_>>();

        committee = sort_validators(committee);
        Ok(committee)
    }

    pub fn consensus_for_shard(
        &self,
        epoch: u64,
        shard_id: u16,
    ) -> Result<ShardConsensus, ShardConsensusError> {
        let committee = self.validators_for_shard(epoch, shard_id)?;
        Ok(ShardConsensus::new(epoch, shard_id, committee))
    }

    pub fn prepare_cross_shard_transaction(
        &self,
        transaction: &Transaction,
    ) -> Result<CrossShardExecution, CrossShardTransactionError> {
        validate_cross_shard_route(transaction, self.config.shard_count)?;
        let tx_hash = build_cross_shard_transaction_hash(transaction)?;
        let prepare_receipt =
            build_cross_shard_receipt(transaction, &tx_hash, CrossShardPhase::Prepare);

        Ok(CrossShardExecution {
            transaction: transaction.clone(),
            tx_hash,
            source_prepared: true,
            destination_validated: false,
            committed: false,
            receipts: vec![prepare_receipt],
        })
    }

    pub fn validate_cross_shard_transaction(
        &self,
        execution: &mut CrossShardExecution,
    ) -> Result<(), CrossShardTransactionError> {
        validate_cross_shard_route(&execution.transaction, self.config.shard_count)?;

        if !execution.source_prepared {
            return Err(CrossShardTransactionError::SourceShardNotPrepared);
        }

        if execution.destination_validated {
            return Err(CrossShardTransactionError::DestinationAlreadyValidated);
        }

        if execution.committed {
            return Err(CrossShardTransactionError::AlreadyCommitted);
        }

        execution.receipts.push(build_cross_shard_receipt(
            &execution.transaction,
            &execution.tx_hash,
            CrossShardPhase::Validate,
        ));
        execution.destination_validated = true;

        Ok(())
    }

    pub fn commit_cross_shard_transaction(
        &self,
        execution: &mut CrossShardExecution,
    ) -> Result<(), CrossShardTransactionError> {
        validate_cross_shard_route(&execution.transaction, self.config.shard_count)?;

        if execution.committed {
            return Err(CrossShardTransactionError::AlreadyCommitted);
        }

        if !execution.source_prepared {
            return Err(CrossShardTransactionError::SourceShardNotPrepared);
        }

        if !execution.destination_validated {
            return Err(CrossShardTransactionError::DestinationShardNotValidated);
        }

        execution.receipts.push(build_cross_shard_receipt(
            &execution.transaction,
            &execution.tx_hash,
            CrossShardPhase::Commit,
        ));
        execution.committed = true;

        Ok(())
    }

    pub fn include_receipts_in_block(&self, block: &mut Block, execution: &CrossShardExecution) {
        block.receipts.extend(execution.receipts.iter().cloned());
    }

    pub fn validate_cross_shard_receipt(
        &self,
        transaction: &Transaction,
        tx_hash: &[u8],
        receipt: &CrossShardReceipt,
    ) -> Result<(), CrossShardTransactionError> {
        validate_cross_shard_route(transaction, self.config.shard_count)?;

        if receipt.tx_hash != tx_hash {
            return Err(CrossShardTransactionError::ReceiptTxHashMismatch);
        }

        if receipt.source_shard != transaction.source_shard
            || receipt.destination_shard != transaction.destination_shard
        {
            return Err(CrossShardTransactionError::ReceiptRouteMismatch {
                expected_source_shard: transaction.source_shard,
                expected_destination_shard: transaction.destination_shard,
                actual_source_shard: receipt.source_shard,
                actual_destination_shard: receipt.destination_shard,
            });
        }

        let expected_proof = build_receipt_proof(
            tx_hash,
            &receipt.phase,
            receipt.source_shard,
            receipt.destination_shard,
        );

        if receipt.proof != expected_proof {
            return Err(CrossShardTransactionError::InvalidReceiptProof {
                phase: receipt.phase.clone(),
            });
        }

        Ok(())
    }

    pub fn validate_cross_shard_execution_proofs(
        &self,
        execution: &CrossShardExecution,
    ) -> Result<(), CrossShardTransactionError> {
        validate_cross_shard_route(&execution.transaction, self.config.shard_count)?;

        if execution.committed && !execution.destination_validated {
            return Err(CrossShardTransactionError::DestinationShardNotValidated);
        }

        if (execution.destination_validated || execution.committed) && !execution.source_prepared {
            return Err(CrossShardTransactionError::SourceShardNotPrepared);
        }

        let expected_phases = expected_receipt_phases(execution);
        if execution.receipts.len() != expected_phases.len() {
            return Err(CrossShardTransactionError::InvalidReceiptCount {
                expected: expected_phases.len(),
                provided: execution.receipts.len(),
            });
        }

        for (receipt, expected_phase) in execution.receipts.iter().zip(expected_phases.iter()) {
            if receipt.phase != *expected_phase {
                return Err(CrossShardTransactionError::MissingReceiptPhase {
                    phase: expected_phase.clone(),
                });
            }

            self.validate_cross_shard_receipt(&execution.transaction, &execution.tx_hash, receipt)?;
        }

        Ok(())
    }

    pub fn is_cross_shard_execution_globally_finalized(
        &self,
        execution: &CrossShardExecution,
    ) -> Result<bool, CrossShardTransactionError> {
        self.validate_cross_shard_execution_proofs(execution)?;

        Ok(execution.source_prepared
            && execution.destination_validated
            && execution.committed
            && execution.receipts.len() == 3)
    }

    pub fn store_shard_state_root(
        &mut self,
        epoch: u64,
        shard_id: u16,
        state_root: Vec<u8>,
    ) -> Result<(), BeaconChainError> {
        validate_shard_id(shard_id, self.config.shard_count)?;
        validate_state_root_length(&state_root)?;

        if let Some(existing_root) = self
            .shard_state_roots
            .iter_mut()
            .find(|root| root.epoch == epoch && root.shard_id == shard_id)
        {
            existing_root.state_root = state_root;
            return Ok(());
        }

        self.shard_state_roots.push(ShardStateRoot {
            epoch,
            shard_id,
            state_root,
        });

        self.shard_state_roots.sort_by(|left, right| {
            left.epoch
                .cmp(&right.epoch)
                .then(left.shard_id.cmp(&right.shard_id))
        });

        Ok(())
    }

    pub fn state_root_for_shard(&self, epoch: u64, shard_id: u16) -> Option<&[u8]> {
        self.shard_state_roots
            .iter()
            .find(|root| root.epoch == epoch && root.shard_id == shard_id)
            .map(|root| root.state_root.as_slice())
    }

    pub fn update_shard_state_root_from_block(
        &mut self,
        epoch: u64,
        shard_id: u16,
        block: &Block,
    ) -> Result<(), BeaconChainError> {
        self.store_shard_state_root(epoch, shard_id, block.header.merkle_root.clone())
    }

    fn rebuild_epoch(&mut self, epoch: u64) {
        self.current_epoch = epoch;
        self.replace_committees_for_epoch(epoch);
        self.initialize_state_roots_for_epoch(epoch);
    }

    fn replace_committees_for_epoch(&mut self, epoch: u64) {
        let assignments =
            build_committee_assignments(epoch, self.config.shard_count, &self.active_validators());

        self.shard_committees
            .retain(|assignment| assignment.epoch != epoch);
        self.shard_committees.extend(assignments);
        self.shard_committees.sort_by(|left, right| {
            left.epoch
                .cmp(&right.epoch)
                .then(left.shard_id.cmp(&right.shard_id))
        });
    }

    fn initialize_state_roots_for_epoch(&mut self, epoch: u64) {
        for shard_id in 0..self.config.shard_count {
            if self
                .shard_state_roots
                .iter()
                .any(|root| root.epoch == epoch && root.shard_id == shard_id)
            {
                continue;
            }

            self.shard_state_roots.push(ShardStateRoot {
                epoch,
                shard_id,
                state_root: EMPTY_SHARD_STATE_ROOT.to_vec(),
            });
        }

        self.shard_state_roots.sort_by(|left, right| {
            left.epoch
                .cmp(&right.epoch)
                .then(left.shard_id.cmp(&right.shard_id))
        });
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum BeaconChainError {
    #[error("invalid shard id {shard_id}, shard count is {shard_count}")]
    InvalidShardId { shard_id: u16, shard_count: u16 },

    #[error("state root must be {expected} bytes, got {provided}")]
    InvalidStateRootLength { expected: usize, provided: usize },
}

fn build_committee_assignments(
    epoch: u64,
    shard_count: u16,
    validators: &[Validator],
) -> Vec<ShardCommitteeAssignment> {
    let mut assignments = (0..shard_count)
        .map(|shard_id| ShardCommitteeAssignment {
            epoch,
            shard_id,
            validator_addresses: Vec::new(),
        })
        .collect::<Vec<_>>();

    let mut shuffled_validators = validators
        .iter()
        .filter(|validator| validator.stake > 0)
        .cloned()
        .collect::<Vec<_>>();

    shuffled_validators.sort_by(|left, right| {
        assignment_score(epoch, &left.address)
            .cmp(&assignment_score(epoch, &right.address))
            .then(left.address.cmp(&right.address))
    });

    for (index, validator) in shuffled_validators.into_iter().enumerate() {
        let shard_id = (index % shard_count as usize) as u16;
        assignments[shard_id as usize]
            .validator_addresses
            .push(validator.address);
    }

    assignments
}

fn validate_shard_id(shard_id: u16, shard_count: u16) -> Result<(), BeaconChainError> {
    if shard_id >= shard_count {
        return Err(BeaconChainError::InvalidShardId {
            shard_id,
            shard_count,
        });
    }

    Ok(())
}

fn validate_state_root_length(state_root: &[u8]) -> Result<(), BeaconChainError> {
    if state_root.len() != SHARD_STATE_ROOT_LENGTH {
        return Err(BeaconChainError::InvalidStateRootLength {
            expected: SHARD_STATE_ROOT_LENGTH,
            provided: state_root.len(),
        });
    }

    Ok(())
}

fn sort_validators(mut validators: Vec<Validator>) -> Vec<Validator> {
    validators.sort_by(|left, right| {
        left.address
            .cmp(&right.address)
            .then(left.stake.cmp(&right.stake))
            .then(left.locked_until.cmp(&right.locked_until))
    });

    validators
}

fn validate_cross_shard_route(
    transaction: &Transaction,
    shard_count: u16,
) -> Result<(), CrossShardTransactionError> {
    validate_shard_id(transaction.source_shard, shard_count)?;
    validate_shard_id(transaction.destination_shard, shard_count)?;

    if !transaction.is_cross_shard() {
        return Err(CrossShardTransactionError::NotCrossShard);
    }

    Ok(())
}

fn build_cross_shard_transaction_hash(
    transaction: &Transaction,
) -> Result<Vec<u8>, CrossShardTransactionError> {
    let encoded = bincode::serialize(transaction)
        .map_err(|err| CrossShardTransactionError::Serialization(err.to_string()))?;

    Ok(deterministic_digest(&encoded, 0xA5A5_A5A5_A5A5_A5A5))
}

fn build_cross_shard_receipt(
    transaction: &Transaction,
    tx_hash: &[u8],
    phase: CrossShardPhase,
) -> CrossShardReceipt {
    CrossShardReceipt {
        tx_hash: tx_hash.to_vec(),
        source_shard: transaction.source_shard,
        destination_shard: transaction.destination_shard,
        proof: build_receipt_proof(
            tx_hash,
            &phase,
            transaction.source_shard,
            transaction.destination_shard,
        ),
        phase,
    }
}

fn expected_receipt_phases(execution: &CrossShardExecution) -> Vec<CrossShardPhase> {
    let mut phases = Vec::new();

    if execution.source_prepared || execution.destination_validated || execution.committed {
        phases.push(CrossShardPhase::Prepare);
    }

    if execution.destination_validated || execution.committed {
        phases.push(CrossShardPhase::Validate);
    }

    if execution.committed {
        phases.push(CrossShardPhase::Commit);
    }

    phases
}

fn build_receipt_proof(
    tx_hash: &[u8],
    phase: &CrossShardPhase,
    source_shard: u16,
    destination_shard: u16,
) -> Vec<u8> {
    let mut payload = Vec::new();
    payload.extend_from_slice(tx_hash);
    payload.extend_from_slice(&source_shard.to_be_bytes());
    payload.extend_from_slice(&destination_shard.to_be_bytes());
    payload.push(match phase {
        CrossShardPhase::Prepare => 1,
        CrossShardPhase::Validate => 2,
        CrossShardPhase::Commit => 3,
    });

    deterministic_digest(&payload, 0xB4B4_B4B4_B4B4_B4B4)
}

fn deterministic_digest(payload: &[u8], seed: u64) -> Vec<u8> {
    let mut state = seed ^ ((payload.len() as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15));

    for (index, byte) in payload.iter().enumerate() {
        state ^=
            (*byte as u64).wrapping_add(((index as u64) + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15));
        state = splitmix64(state);
    }

    let mut digest = Vec::with_capacity(RECEIPT_PROOF_LENGTH);
    for lane in 0..(RECEIPT_PROOF_LENGTH / 8) {
        digest.extend_from_slice(&splitmix64(state ^ lane as u64).to_be_bytes());
    }

    digest
}

fn assignment_score(epoch: u64, address: &[u8]) -> u64 {
    let mut state = epoch ^ 0x9E37_79B9_7F4A_7C15;

    for (index, byte) in address.iter().enumerate() {
        state ^=
            (*byte as u64).wrapping_add(((index as u64) + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15));
        state = splitmix64(state);
    }

    splitmix64(state)
}

fn splitmix64(mut value: u64) -> u64 {
    value = value.wrapping_add(0x9E37_79B9_7F4A_7C15);
    value = (value ^ (value >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    value ^ (value >> 31)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        blockchain::types::{BlockHeader, FIXED_TRANSACTION_FEE, TransactionType},
        consensus::{VoteError, create_signed_vote},
        crypto::{KeyPair, generate_keypair},
    };

    fn validator_with_id(id: u8, stake: u64) -> Validator {
        Validator {
            address: vec![id; 32],
            stake,
            locked_until: 1_710_000_000,
        }
    }

    fn validator_from_pair(pair: &KeyPair, stake: u64) -> Validator {
        Validator {
            address: pair.public_key.to_vec(),
            stake,
            locked_until: 1_710_000_000,
        }
    }

    fn sample_cross_shard_transaction(
        source_shard: u16,
        destination_shard: u16,
        nonce: u64,
    ) -> Transaction {
        Transaction {
            tx_type: TransactionType::Transfer,
            from: vec![1; 32],
            to: vec![2; 32],
            amount: 42,
            fee: FIXED_TRANSACTION_FEE,
            nonce,
            source_shard,
            destination_shard,
            signature: vec![9; 64],
            data: Some(b"cross-shard".to_vec()),
        }
    }

    fn sample_block() -> Block {
        Block {
            header: BlockHeader {
                version: 1,
                previous_hash: vec![0; 32],
                merkle_root: vec![1; 32],
                timestamp: 1_710_000_000,
                height: 1,
                validator: vec![7; 32],
                signature: vec![6; 64],
            },
            transactions: vec![],
            receipts: vec![],
        }
    }

    #[test]
    fn default_sharding_config_uses_64_shards() {
        let config = ShardingConfig::default();
        assert_eq!(config.shard_count, DEFAULT_INITIAL_SHARD_COUNT);
    }

    #[test]
    fn custom_shard_count_must_be_a_power_of_two() {
        let error = ShardingConfig::new(63).expect_err("63 must be rejected");
        assert_eq!(
            error,
            ShardingConfigError::ShardCountMustBePowerOfTwo { provided: 63 }
        );
    }

    #[test]
    fn governance_can_update_shard_count_to_another_power_of_two() {
        let mut config = ShardingConfig::default();

        config
            .apply_governance_update(128)
            .expect("128 must be accepted");

        assert_eq!(config.shard_count, 128);
    }

    #[test]
    fn beacon_chain_assigns_all_active_validators_to_committees() {
        let config = ShardingConfig::new(4).expect("4 shards must be valid");
        let validators = vec![
            validator_with_id(1, 10),
            validator_with_id(2, 20),
            validator_with_id(3, 30),
            validator_with_id(4, 0),
        ];

        let beacon_chain = BeaconChain::new(config, validators);
        let committees = beacon_chain.committees_for_epoch(0);

        assert_eq!(committees.len(), 4);

        let mut assigned_addresses = committees
            .iter()
            .flat_map(|committee| committee.validator_addresses.iter().cloned())
            .collect::<Vec<_>>();
        assigned_addresses.sort();

        assert_eq!(assigned_addresses.len(), 3);
        assert!(assigned_addresses.contains(&vec![1; 32]));
        assert!(assigned_addresses.contains(&vec![2; 32]));
        assert!(assigned_addresses.contains(&vec![3; 32]));
        assert!(!assigned_addresses.contains(&vec![4; 32]));
    }

    #[test]
    fn committee_assignment_is_deterministic_for_same_epoch() {
        let config = ShardingConfig::new(2).expect("2 shards must be valid");
        let validators = vec![
            validator_with_id(1, 10),
            validator_with_id(2, 20),
            validator_with_id(3, 30),
            validator_with_id(4, 40),
        ];

        let left = BeaconChain::new(config.clone(), validators.clone());
        let right = BeaconChain::new(config, validators);

        assert_eq!(left.committees_for_epoch(0), right.committees_for_epoch(0));
    }

    #[test]
    fn epoch_changes_assignment_score() {
        let address = vec![7; 32];

        assert_ne!(assignment_score(0, &address), assignment_score(1, &address));
    }

    #[test]
    fn shard_state_roots_are_initialized_and_updatable() {
        let config = ShardingConfig::new(2).expect("2 shards must be valid");
        let validators = vec![validator_with_id(1, 10), validator_with_id(2, 20)];
        let mut beacon_chain = BeaconChain::new(config, validators);

        assert_eq!(
            beacon_chain.state_root_for_shard(0, 0),
            Some(EMPTY_SHARD_STATE_ROOT.as_slice())
        );

        beacon_chain
            .store_shard_state_root(0, 0, vec![9; SHARD_STATE_ROOT_LENGTH])
            .expect("state root update must succeed");

        assert_eq!(
            beacon_chain.state_root_for_shard(0, 0),
            Some(vec![9; SHARD_STATE_ROOT_LENGTH].as_slice())
        );
    }

    #[test]
    fn beacon_chain_updates_shard_state_root_from_block() {
        let config = ShardingConfig::new(2).expect("2 shards must be valid");
        let validators = vec![validator_with_id(1, 10), validator_with_id(2, 20)];
        let mut beacon_chain = BeaconChain::new(config, validators);

        let mut finalized_block = sample_block();
        finalized_block.header.merkle_root = vec![7; SHARD_STATE_ROOT_LENGTH];

        beacon_chain
            .update_shard_state_root_from_block(0, 1, &finalized_block)
            .expect("block-based shard state update must succeed");

        assert_eq!(
            beacon_chain.state_root_for_shard(0, 1),
            Some(vec![7; SHARD_STATE_ROOT_LENGTH].as_slice())
        );
    }

    #[test]
    fn beacon_chain_tracks_shard_state_updates_across_epochs() {
        let config = ShardingConfig::new(2).expect("2 shards must be valid");
        let validators = vec![validator_with_id(1, 10), validator_with_id(2, 20)];
        let mut beacon_chain = BeaconChain::new(config, validators);

        let mut epoch_zero_block = sample_block();
        epoch_zero_block.header.merkle_root = vec![3; SHARD_STATE_ROOT_LENGTH];
        beacon_chain
            .update_shard_state_root_from_block(0, 0, &epoch_zero_block)
            .expect("epoch 0 shard state update must succeed");

        beacon_chain.advance_epoch();

        let mut epoch_one_block = sample_block();
        epoch_one_block.header.merkle_root = vec![4; SHARD_STATE_ROOT_LENGTH];
        beacon_chain
            .update_shard_state_root_from_block(1, 0, &epoch_one_block)
            .expect("epoch 1 shard state update must succeed");

        assert_eq!(
            beacon_chain.state_root_for_shard(0, 0),
            Some(vec![3; SHARD_STATE_ROOT_LENGTH].as_slice())
        );
        assert_eq!(
            beacon_chain.state_root_for_shard(1, 0),
            Some(vec![4; SHARD_STATE_ROOT_LENGTH].as_slice())
        );
        assert_eq!(
            beacon_chain.state_root_for_shard(1, 1),
            Some(EMPTY_SHARD_STATE_ROOT.as_slice())
        );
    }

    #[test]
    fn storing_state_root_rejects_invalid_shard_id() {
        let config = ShardingConfig::new(2).expect("2 shards must be valid");
        let validators = vec![validator_with_id(1, 10)];
        let mut beacon_chain = BeaconChain::new(config, validators);

        let error = beacon_chain
            .store_shard_state_root(0, 2, vec![1; SHARD_STATE_ROOT_LENGTH])
            .expect_err("invalid shard id must be rejected");

        assert_eq!(
            error,
            BeaconChainError::InvalidShardId {
                shard_id: 2,
                shard_count: 2,
            }
        );
    }

    #[test]
    fn beacon_chain_builds_shard_consensus_from_committee() {
        let config = ShardingConfig::new(2).expect("2 shards must be valid");
        let pairs = vec![
            generate_keypair(),
            generate_keypair(),
            generate_keypair(),
            generate_keypair(),
        ];
        let validators = vec![
            validator_from_pair(&pairs[0], 10),
            validator_from_pair(&pairs[1], 20),
            validator_from_pair(&pairs[2], 30),
            validator_from_pair(&pairs[3], 40),
        ];

        let beacon_chain = BeaconChain::new(config, validators);
        let shard_consensus = beacon_chain
            .consensus_for_shard(0, 0)
            .expect("shard consensus must be built");

        let mut consensus_addresses = shard_consensus
            .committee()
            .iter()
            .map(|validator| validator.address.clone())
            .collect::<Vec<_>>();
        consensus_addresses.sort();

        let mut committee_addresses = beacon_chain
            .committee_for_shard(0, 0)
            .expect("committee must exist")
            .validator_addresses
            .clone();
        committee_addresses.sort();

        assert_eq!(consensus_addresses, committee_addresses);
        assert_eq!(shard_consensus.vote_count(), 0);
    }

    #[test]
    fn shard_consensus_finalizes_block_with_its_committee_votes() {
        let config = ShardingConfig::new(2).expect("2 shards must be valid");
        let pairs = vec![
            generate_keypair(),
            generate_keypair(),
            generate_keypair(),
            generate_keypair(),
        ];
        let validators = vec![
            validator_from_pair(&pairs[0], 10),
            validator_from_pair(&pairs[1], 20),
            validator_from_pair(&pairs[2], 30),
            validator_from_pair(&pairs[3], 40),
        ];

        let beacon_chain = BeaconChain::new(config, validators);
        let mut shard_consensus = beacon_chain
            .consensus_for_shard(0, 0)
            .expect("shard consensus must be built");

        let committee_addresses = shard_consensus
            .committee()
            .iter()
            .map(|validator| validator.address.clone())
            .collect::<Vec<_>>();
        let block_hash = vec![7; 32];

        for address in committee_addresses {
            let pair = pairs
                .iter()
                .find(|pair| pair.public_key.to_vec() == address)
                .expect("committee validator keypair must exist");
            let vote = create_signed_vote(12, &block_hash, true, &pair.secret_key)
                .expect("committee vote must be created");

            shard_consensus
                .collect_vote(vote)
                .expect("committee vote must be accepted");
        }

        let tally = shard_consensus.tally().expect("tally must succeed");
        assert_eq!(tally.total_votes, shard_consensus.committee().len());
        assert!(
            shard_consensus
                .is_finalized()
                .expect("finalization check must succeed")
        );
    }

    #[test]
    fn shard_consensus_rejects_vote_from_validator_outside_committee() {
        let config = ShardingConfig::new(2).expect("2 shards must be valid");
        let pairs = vec![
            generate_keypair(),
            generate_keypair(),
            generate_keypair(),
            generate_keypair(),
        ];
        let validators = vec![
            validator_from_pair(&pairs[0], 10),
            validator_from_pair(&pairs[1], 20),
            validator_from_pair(&pairs[2], 30),
            validator_from_pair(&pairs[3], 40),
        ];

        let beacon_chain = BeaconChain::new(config, validators);
        let mut shard_consensus = beacon_chain
            .consensus_for_shard(0, 0)
            .expect("shard consensus must be built");

        let committee_addresses = shard_consensus
            .committee()
            .iter()
            .map(|validator| validator.address.clone())
            .collect::<Vec<_>>();

        let outsider_pair = pairs
            .iter()
            .find(|pair| !committee_addresses.contains(&pair.public_key.to_vec()))
            .expect("an outsider validator must exist");

        let vote = create_signed_vote(13, &[8; 32], true, &outsider_pair.secret_key)
            .expect("outsider vote must be created");

        let error = shard_consensus
            .collect_vote(vote)
            .expect_err("outsider vote must be rejected");

        assert_eq!(
            error,
            ShardConsensusError::Vote(VoteError::UnknownValidator)
        );
    }

    #[test]
    fn prepare_rejects_same_shard_transaction() {
        let config = ShardingConfig::new(4).expect("4 shards must be valid");
        let validators = vec![validator_with_id(1, 10), validator_with_id(2, 20)];
        let beacon_chain = BeaconChain::new(config, validators);
        let transaction = sample_cross_shard_transaction(1, 1, 7);

        let error = beacon_chain
            .prepare_cross_shard_transaction(&transaction)
            .expect_err("same-shard transaction must be rejected");

        assert_eq!(error, CrossShardTransactionError::NotCrossShard);
    }

    #[test]
    fn prepare_rejects_invalid_source_shard() {
        let config = ShardingConfig::new(4).expect("4 shards must be valid");
        let validators = vec![validator_with_id(1, 10), validator_with_id(2, 20)];
        let beacon_chain = BeaconChain::new(config, validators);
        let transaction = sample_cross_shard_transaction(4, 1, 7);

        let error = beacon_chain
            .prepare_cross_shard_transaction(&transaction)
            .expect_err("invalid source shard must be rejected");

        assert_eq!(
            error,
            CrossShardTransactionError::BeaconChain(BeaconChainError::InvalidShardId {
                shard_id: 4,
                shard_count: 4,
            })
        );
    }

    #[test]
    fn prepare_rejects_invalid_destination_shard() {
        let config = ShardingConfig::new(4).expect("4 shards must be valid");
        let validators = vec![validator_with_id(1, 10), validator_with_id(2, 20)];
        let beacon_chain = BeaconChain::new(config, validators);
        let transaction = sample_cross_shard_transaction(1, 4, 7);

        let error = beacon_chain
            .prepare_cross_shard_transaction(&transaction)
            .expect_err("invalid destination shard must be rejected");

        assert_eq!(
            error,
            CrossShardTransactionError::BeaconChain(BeaconChainError::InvalidShardId {
                shard_id: 4,
                shard_count: 4,
            })
        );
    }

    #[test]
    fn cross_shard_transaction_prepare_validate_commit_generates_receipts() {
        let config = ShardingConfig::new(4).expect("4 shards must be valid");
        let validators = vec![
            validator_with_id(1, 10),
            validator_with_id(2, 20),
            validator_with_id(3, 30),
            validator_with_id(4, 40),
        ];
        let beacon_chain = BeaconChain::new(config, validators);
        let transaction = sample_cross_shard_transaction(0, 3, 9);

        let mut execution = beacon_chain
            .prepare_cross_shard_transaction(&transaction)
            .expect("prepare must succeed");

        assert!(execution.source_prepared);
        assert!(!execution.destination_validated);
        assert!(!execution.committed);
        assert_eq!(execution.receipts.len(), 1);
        assert_eq!(execution.receipts[0].phase, CrossShardPhase::Prepare);
        assert_eq!(execution.receipts[0].proof.len(), RECEIPT_PROOF_LENGTH);

        beacon_chain
            .validate_cross_shard_transaction(&mut execution)
            .expect("destination validation must succeed");

        assert!(execution.destination_validated);
        assert_eq!(execution.receipts.len(), 2);
        assert_eq!(execution.receipts[1].phase, CrossShardPhase::Validate);
        assert_eq!(execution.receipts[1].proof.len(), RECEIPT_PROOF_LENGTH);

        beacon_chain
            .commit_cross_shard_transaction(&mut execution)
            .expect("commit must succeed");

        assert!(execution.committed);
        assert_eq!(execution.receipts.len(), 3);
        assert_eq!(
            execution
                .receipts
                .iter()
                .map(|receipt| receipt.phase.clone())
                .collect::<Vec<_>>(),
            vec![
                CrossShardPhase::Prepare,
                CrossShardPhase::Validate,
                CrossShardPhase::Commit,
            ]
        );
        assert!(
            execution
                .receipts
                .iter()
                .all(|receipt| receipt.tx_hash == execution.tx_hash)
        );
    }

    #[test]
    fn cross_shard_commit_requires_destination_validation() {
        let config = ShardingConfig::new(4).expect("4 shards must be valid");
        let validators = vec![validator_with_id(1, 10), validator_with_id(2, 20)];
        let beacon_chain = BeaconChain::new(config, validators);
        let transaction = sample_cross_shard_transaction(0, 1, 11);

        let mut execution = beacon_chain
            .prepare_cross_shard_transaction(&transaction)
            .expect("prepare must succeed");

        let error = beacon_chain
            .commit_cross_shard_transaction(&mut execution)
            .expect_err("commit before destination validation must fail");

        assert_eq!(
            error,
            CrossShardTransactionError::DestinationShardNotValidated
        );
    }

    #[test]
    fn beacon_chain_validates_cross_shard_proofs_and_global_finality() {
        let config = ShardingConfig::new(4).expect("4 shards must be valid");
        let validators = vec![
            validator_with_id(1, 10),
            validator_with_id(2, 20),
            validator_with_id(3, 30),
            validator_with_id(4, 40),
        ];
        let beacon_chain = BeaconChain::new(config, validators);
        let transaction = sample_cross_shard_transaction(0, 3, 21);

        let mut execution = beacon_chain
            .prepare_cross_shard_transaction(&transaction)
            .expect("prepare must succeed");
        beacon_chain
            .validate_cross_shard_transaction(&mut execution)
            .expect("validation must succeed");
        beacon_chain
            .commit_cross_shard_transaction(&mut execution)
            .expect("commit must succeed");

        beacon_chain
            .validate_cross_shard_execution_proofs(&execution)
            .expect("receipt proofs must be valid");
        assert!(
            beacon_chain
                .is_cross_shard_execution_globally_finalized(&execution)
                .expect("global finality check must succeed")
        );
    }

    #[test]
    fn cross_shard_global_finality_is_false_until_commit_phase() {
        let config = ShardingConfig::new(4).expect("4 shards must be valid");
        let validators = vec![validator_with_id(1, 10), validator_with_id(2, 20)];
        let beacon_chain = BeaconChain::new(config, validators);
        let transaction = sample_cross_shard_transaction(1, 2, 22);

        let mut execution = beacon_chain
            .prepare_cross_shard_transaction(&transaction)
            .expect("prepare must succeed");
        beacon_chain
            .validate_cross_shard_transaction(&mut execution)
            .expect("validation must succeed");

        beacon_chain
            .validate_cross_shard_execution_proofs(&execution)
            .expect("partial receipt proofs must still be valid");
        assert!(
            !beacon_chain
                .is_cross_shard_execution_globally_finalized(&execution)
                .expect("global finality check must succeed")
        );
    }

    #[test]
    fn beacon_chain_rejects_tampered_cross_shard_receipt_proof() {
        let config = ShardingConfig::new(4).expect("4 shards must be valid");
        let validators = vec![
            validator_with_id(1, 10),
            validator_with_id(2, 20),
            validator_with_id(3, 30),
            validator_with_id(4, 40),
        ];
        let beacon_chain = BeaconChain::new(config, validators);
        let transaction = sample_cross_shard_transaction(0, 3, 23);

        let mut execution = beacon_chain
            .prepare_cross_shard_transaction(&transaction)
            .expect("prepare must succeed");
        beacon_chain
            .validate_cross_shard_transaction(&mut execution)
            .expect("validation must succeed");
        beacon_chain
            .commit_cross_shard_transaction(&mut execution)
            .expect("commit must succeed");

        execution.receipts[1].proof[0] ^= 0xFF;

        let error = beacon_chain
            .validate_cross_shard_execution_proofs(&execution)
            .expect_err("tampered proof must be rejected");

        assert_eq!(
            error,
            CrossShardTransactionError::InvalidReceiptProof {
                phase: CrossShardPhase::Validate,
            }
        );
    }

    #[test]
    fn receipts_can_be_included_in_block() {
        let config = ShardingConfig::new(4).expect("4 shards must be valid");
        let validators = vec![validator_with_id(1, 10), validator_with_id(2, 20)];
        let beacon_chain = BeaconChain::new(config, validators);
        let transaction = sample_cross_shard_transaction(1, 2, 13);

        let mut execution = beacon_chain
            .prepare_cross_shard_transaction(&transaction)
            .expect("prepare must succeed");
        beacon_chain
            .validate_cross_shard_transaction(&mut execution)
            .expect("validation must succeed");
        beacon_chain
            .commit_cross_shard_transaction(&mut execution)
            .expect("commit must succeed");

        let mut block = sample_block();
        beacon_chain.include_receipts_in_block(&mut block, &execution);

        assert_eq!(block.receipts.len(), 3);
        assert_eq!(block.receipts[0].phase, CrossShardPhase::Prepare);
        assert_eq!(block.receipts[1].phase, CrossShardPhase::Validate);
        assert_eq!(block.receipts[2].phase, CrossShardPhase::Commit);
        assert!(
            block
                .receipts
                .iter()
                .all(|receipt| receipt.proof.len() == RECEIPT_PROOF_LENGTH)
        );
    }

    #[test]
    fn compromised_shard_state_root_does_not_affect_other_shards() {
        let config = ShardingConfig::new(4).expect("4 shards must be valid");
        let validators = vec![
            validator_with_id(1, 10),
            validator_with_id(2, 20),
            validator_with_id(3, 30),
            validator_with_id(4, 40),
        ];
        let mut beacon_chain = BeaconChain::new(config, validators);

        let mut shard_zero_block = sample_block();
        shard_zero_block.header.merkle_root = vec![10; 32];
        beacon_chain
            .update_shard_state_root_from_block(0, 0, &shard_zero_block)
            .expect("shard 0 state root must update");

        let mut shard_one_block = sample_block();
        shard_one_block.header.merkle_root = vec![11; 32];
        beacon_chain
            .update_shard_state_root_from_block(0, 1, &shard_one_block)
            .expect("shard 1 state root must update");

        let mut shard_two_block = sample_block();
        shard_two_block.header.merkle_root = vec![12; 32];
        beacon_chain
            .update_shard_state_root_from_block(0, 2, &shard_two_block)
            .expect("shard 2 state root must update");

        let mut shard_three_block = sample_block();
        shard_three_block.header.merkle_root = vec![13; 32];
        beacon_chain
            .update_shard_state_root_from_block(0, 3, &shard_three_block)
            .expect("shard 3 state root must update");

        let mut compromised_block = sample_block();
        compromised_block.header.merkle_root = vec![99; 32];
        beacon_chain
            .update_shard_state_root_from_block(0, 1, &compromised_block)
            .expect("compromised shard state root must update");

        assert_eq!(
            beacon_chain.state_root_for_shard(0, 0),
            Some(shard_zero_block.header.merkle_root.as_slice())
        );
        assert_eq!(
            beacon_chain.state_root_for_shard(0, 1),
            Some(compromised_block.header.merkle_root.as_slice())
        );
        assert_eq!(
            beacon_chain.state_root_for_shard(0, 2),
            Some(shard_two_block.header.merkle_root.as_slice())
        );
        assert_eq!(
            beacon_chain.state_root_for_shard(0, 3),
            Some(shard_three_block.header.merkle_root.as_slice())
        );
    }
}
