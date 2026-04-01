//! Logique de consensus et mempool.
//!
//! Ce module regroupe la mempool partitionnée, la sélection du proposant,
//! les votes signés, la finalisation, la gestion des forks,
//! le slashing et la distribution des récompenses.

use crate::blockchain::{
    hash::{calculate_merkle_root, sha256},
    types::{Block, BlockHeader, Transaction, Validator},
};
use anyhow::{Result, anyhow};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use std::{collections::HashMap, sync::Arc};
use thiserror::Error;
use tokio::{
    runtime::{Builder, Runtime},
    sync::{mpsc, oneshot},
};

const MAX_TRANSACTIONS_PER_ADDRESS_PER_BLOCK: usize = 100;

#[derive(Debug)]
enum MempoolCommand {
    AddTransaction(Transaction),
    TakeForBlock {
        max_transactions: usize,
        response: oneshot::Sender<Vec<Transaction>>,
    },
    Len {
        response: oneshot::Sender<usize>,
    },
    PendingTransactions {
        response: oneshot::Sender<Vec<Transaction>>,
    },
}

#[derive(Debug, Clone)]
struct MempoolPartition {
    sender: mpsc::Sender<MempoolCommand>,
}

#[derive(Clone)]
pub struct Mempool {
    partition_count: usize,
    partitions: Vec<MempoolPartition>,
    runtime: Arc<Runtime>,
}

impl Default for Mempool {
    fn default() -> Self {
        Self::with_partition_count(
            std::thread::available_parallelism()
                .map(|parallelism| parallelism.get())
                .unwrap_or(1),
        )
    }
}

impl Mempool {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_partition_count(partition_count: usize) -> Self {
        let partition_count = partition_count.max(1);
        let runtime = Arc::new(
            Builder::new_multi_thread()
                .worker_threads(partition_count)
                .enable_all()
                .build()
                .expect("failed to build mempool runtime"),
        );
        let partitions = (0..partition_count)
            .map(|_| Self::spawn_partition_task(&runtime))
            .collect();

        Self {
            partition_count,
            partitions,
            runtime,
        }
    }

    fn spawn_partition_task(runtime: &Arc<Runtime>) -> MempoolPartition {
        let (sender, mut receiver) = mpsc::channel(1024);

        runtime.spawn(async move {
            let mut transactions = Vec::new();

            while let Some(command) = receiver.recv().await {
                match command {
                    MempoolCommand::AddTransaction(transaction) => {
                        transactions.push(transaction);
                    }
                    MempoolCommand::TakeForBlock {
                        max_transactions,
                        response,
                    } => {
                        let mut selected =
                            Vec::with_capacity(max_transactions.min(transactions.len()));
                        let mut remaining_transactions = Vec::with_capacity(transactions.len());
                        let mut selected_per_sender: HashMap<Vec<u8>, usize> = HashMap::new();

                        for transaction in std::mem::take(&mut transactions) {
                            if selected.len() == max_transactions {
                                remaining_transactions.push(transaction);
                                continue;
                            }

                            let sender_count = selected_per_sender
                                .entry(transaction.from.clone())
                                .or_insert(0);

                            if *sender_count < MAX_TRANSACTIONS_PER_ADDRESS_PER_BLOCK {
                                *sender_count += 1;
                                selected.push(transaction);
                            } else {
                                remaining_transactions.push(transaction);
                            }
                        }

                        transactions = remaining_transactions;
                        let _ = response.send(selected);
                    }
                    MempoolCommand::Len { response } => {
                        let _ = response.send(transactions.len());
                    }
                    MempoolCommand::PendingTransactions { response } => {
                        let _ = response.send(transactions.clone());
                    }
                }
            }
        });

        MempoolPartition { sender }
    }

    pub fn partition_count(&self) -> usize {
        self.partition_count
    }

    fn partition_for_address(&self, address: &[u8]) -> usize {
        if address.is_empty() {
            return 0;
        }

        let hash = address.iter().fold(0u64, |acc, byte| {
            acc.wrapping_mul(31).wrapping_add(u64::from(*byte))
        });

        (hash as usize) % self.partition_count
    }

    pub async fn add_transaction_async(&self, transaction: Transaction) {
        let partition = self.partition_for_address(&transaction.from);
        let sender = self.partitions[partition].sender.clone();

        sender
            .send(MempoolCommand::AddTransaction(transaction))
            .await
            .expect("failed to send transaction to mempool partition");
    }

    pub fn add_transaction(&mut self, transaction: Transaction) {
        let partition = self.partition_for_address(&transaction.from);
        let sender = self.partitions[partition].sender.clone();

        self.runtime
            .block_on(async move {
                sender
                    .send(MempoolCommand::AddTransaction(transaction))
                    .await
            })
            .expect("failed to send transaction to mempool partition");
    }

    pub fn len(&self) -> usize {
        self.runtime.block_on(self.len_async())
    }

    pub async fn len_async(&self) -> usize {
        let mut total = 0usize;

        for partition in &self.partitions {
            let sender = partition.sender.clone();
            let (response_tx, response_rx) = oneshot::channel();

            sender
                .send(MempoolCommand::Len {
                    response: response_tx,
                })
                .await
                .expect("failed to request mempool partition length");

            total += response_rx
                .await
                .expect("failed to receive mempool partition length");
        }

        total
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn pending_transactions(&self) -> Vec<Transaction> {
        let mut pending = Vec::with_capacity(self.len());

        for partition in &self.partitions {
            let sender = partition.sender.clone();
            let (response_tx, response_rx) = oneshot::channel();

            self.runtime
                .block_on(async move {
                    sender
                        .send(MempoolCommand::PendingTransactions {
                            response: response_tx,
                        })
                        .await
                })
                .expect("failed to request pending mempool transactions");

            pending.extend(
                self.runtime
                    .block_on(response_rx)
                    .expect("failed to receive pending mempool transactions"),
            );
        }

        pending
    }

    pub fn take_for_block(&mut self, max_transactions: usize) -> Vec<Transaction> {
        let mut selected = Vec::with_capacity(max_transactions.min(self.len()));

        if max_transactions == 0 || self.is_empty() {
            return selected;
        }

        for partition in &self.partitions {
            if selected.len() == max_transactions {
                break;
            }

            let remaining = max_transactions - selected.len();
            let sender = partition.sender.clone();
            let (response_tx, response_rx) = oneshot::channel();

            self.runtime
                .block_on(async move {
                    sender
                        .send(MempoolCommand::TakeForBlock {
                            max_transactions: remaining,
                            response: response_tx,
                        })
                        .await
                })
                .expect("failed to request transactions from mempool partition");

            selected.extend(
                self.runtime
                    .block_on(response_rx)
                    .expect("failed to receive transactions from mempool partition"),
            );
        }

        selected
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ConsensusVote {
    pub height: u64,
    pub block_hash: Vec<u8>,
    pub voter: Vec<u8>,
    pub approve: bool,
    pub signature: Vec<u8>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct VoteTally {
    pub approve_stake: u64,
    pub reject_stake: u64,
    pub total_votes: usize,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct VoteCollector {
    votes_by_voter: HashMap<Vec<u8>, ConsensusVote>,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum VoteError {
    #[error("invalid voter public key length: expected 32 bytes")]
    InvalidVoterKeyLength,
    #[error("invalid vote signature length: expected 64 bytes")]
    InvalidSignatureLength,
    #[error("invalid vote signature")]
    InvalidSignature,
    #[error("unknown validator")]
    UnknownValidator,
    #[error("duplicate vote from validator")]
    DuplicateVote,
    #[error("vote serialization failed: {0}")]
    Serialization(String),
}

impl VoteCollector {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.votes_by_voter.len()
    }

    pub fn is_empty(&self) -> bool {
        self.votes_by_voter.is_empty()
    }

    pub fn collect_vote(
        &mut self,
        vote: ConsensusVote,
        validators: &[Validator],
    ) -> std::result::Result<(), VoteError> {
        if validator_stake(validators, &vote.voter).is_none() {
            return Err(VoteError::UnknownValidator);
        }

        if !verify_vote(&vote)? {
            return Err(VoteError::InvalidSignature);
        }

        if self.votes_by_voter.contains_key(&vote.voter) {
            return Err(VoteError::DuplicateVote);
        }

        self.votes_by_voter.insert(vote.voter.clone(), vote);
        Ok(())
    }

    pub fn tally(&self, validators: &[Validator]) -> std::result::Result<VoteTally, VoteError> {
        let mut tally = VoteTally::default();

        for vote in self.votes_by_voter.values() {
            let stake =
                validator_stake(validators, &vote.voter).ok_or(VoteError::UnknownValidator)?;

            if vote.approve {
                tally.approve_stake = tally
                    .approve_stake
                    .checked_add(stake)
                    .expect("approve stake should not overflow u64 in tests");
            } else {
                tally.reject_stake = tally
                    .reject_stake
                    .checked_add(stake)
                    .expect("reject stake should not overflow u64 in tests");
            }

            tally.total_votes += 1;
        }

        Ok(tally)
    }
}

pub fn total_active_stake(validators: &[Validator]) -> u64 {
    validators
        .iter()
        .filter(|validator| validator.stake > 0)
        .map(|validator| validator.stake)
        .sum()
}

pub fn is_block_finalized(tally: &VoteTally, validators: &[Validator]) -> bool {
    let total_stake = total_active_stake(validators) as u128;
    if total_stake == 0 {
        return false;
    }

    (tally.approve_stake as u128) * 3 > total_stake * 2
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChainFork {
    pub tip_hash: Vec<u8>,
    pub height: u64,
    pub cumulative_stake_weight: u128,
}

pub fn cumulative_chain_weight(tallies: &[VoteTally]) -> u128 {
    tallies
        .iter()
        .map(|tally| tally.approve_stake as u128)
        .sum()
}

pub fn choose_heaviest_chain(forks: &[ChainFork]) -> Option<ChainFork> {
    forks.iter().cloned().max_by(|left, right| {
        left.cumulative_stake_weight
            .cmp(&right.cumulative_stake_weight)
            .then(left.height.cmp(&right.height))
            .then(left.tip_hash.cmp(&right.tip_hash))
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DoubleVoteProof {
    pub first_vote: ConsensusVote,
    pub second_vote: ConsensusVote,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum SlashingError {
    #[error("double-vote proof requires the same voter on both votes")]
    DifferentVoters,
    #[error("double-vote proof requires the same height on both votes")]
    DifferentHeights,
    #[error("double-vote proof requires two distinct block hashes")]
    SameBlockHash,
    #[error("first vote is invalid")]
    InvalidFirstVote,
    #[error("second vote is invalid")]
    InvalidSecondVote,
    #[error("invalid slashing penalty bps: must be between 1 and 10_000")]
    InvalidPenaltyBps,
    #[error("validator does not match double-vote proof")]
    ValidatorDoesNotMatchProof,
}

pub fn detect_double_vote(
    first_vote: &ConsensusVote,
    second_vote: &ConsensusVote,
) -> std::result::Result<DoubleVoteProof, SlashingError> {
    if first_vote.voter != second_vote.voter {
        return Err(SlashingError::DifferentVoters);
    }

    if first_vote.height != second_vote.height {
        return Err(SlashingError::DifferentHeights);
    }

    if first_vote.block_hash == second_vote.block_hash {
        return Err(SlashingError::SameBlockHash);
    }

    match verify_vote(first_vote) {
        Ok(true) => {}
        _ => return Err(SlashingError::InvalidFirstVote),
    }

    match verify_vote(second_vote) {
        Ok(true) => {}
        _ => return Err(SlashingError::InvalidSecondVote),
    }

    Ok(DoubleVoteProof {
        first_vote: first_vote.clone(),
        second_vote: second_vote.clone(),
    })
}

pub fn slash_validator_for_double_vote(
    validator: &Validator,
    proof: &DoubleVoteProof,
    penalty_bps: u16,
) -> std::result::Result<Validator, SlashingError> {
    if penalty_bps == 0 || penalty_bps > 10_000 {
        return Err(SlashingError::InvalidPenaltyBps);
    }

    if validator.address != proof.first_vote.voter {
        return Err(SlashingError::ValidatorDoesNotMatchProof);
    }

    let mut updated = validator.clone();

    if updated.stake == 0 {
        return Ok(updated);
    }

    let computed_penalty = ((updated.stake as u128) * (penalty_bps as u128)) / 10_000u128;
    let penalty = computed_penalty.max(1).min(updated.stake as u128) as u64;

    updated.stake -= penalty;
    Ok(updated)
}

pub const ANNUAL_INFLATION_BPS: u64 = 200;
pub const SECONDS_PER_YEAR: u64 = 31_536_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatorReward {
    pub validator: Vec<u8>,
    pub amount: u64,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum RewardError {
    #[error("transaction fees overflowed u64")]
    FeeOverflow,
    #[error("reward pool overflowed u64")]
    RewardOverflow,
    #[error("voting stake must be greater than zero")]
    NoVotingStake,
}

pub fn total_transaction_fees(
    transactions: &[Transaction],
) -> std::result::Result<u64, RewardError> {
    transactions.iter().try_fold(0u64, |acc, tx| {
        acc.checked_add(tx.fee).ok_or(RewardError::FeeOverflow)
    })
}

pub fn annualized_inflation_reward(total_stake: u64, elapsed_secs: u64) -> u64 {
    if total_stake == 0 || elapsed_secs == 0 {
        return 0;
    }

    (((total_stake as u128) * (ANNUAL_INFLATION_BPS as u128) * (elapsed_secs as u128))
        / 10_000u128
        / (SECONDS_PER_YEAR as u128)) as u64
}

pub fn distribute_voting_rewards(
    validators: &[Validator],
    collector: &VoteCollector,
    transactions: &[Transaction],
    elapsed_secs: u64,
) -> std::result::Result<Vec<ValidatorReward>, RewardError> {
    let fees = total_transaction_fees(transactions)?;
    let inflation = annualized_inflation_reward(total_active_stake(validators), elapsed_secs);
    let reward_pool = fees
        .checked_add(inflation)
        .ok_or(RewardError::RewardOverflow)?;

    let mut participants: Vec<Validator> = validators
        .iter()
        .filter(|validator| {
            validator.stake > 0 && collector.votes_by_voter.contains_key(&validator.address)
        })
        .cloned()
        .collect();

    participants.sort_by(|left, right| left.address.cmp(&right.address));

    let voting_stake: u128 = participants
        .iter()
        .map(|validator| validator.stake as u128)
        .sum();
    if voting_stake == 0 {
        return Err(RewardError::NoVotingStake);
    }

    let mut rewards = Vec::with_capacity(participants.len());
    let mut distributed = 0u64;

    for validator in &participants {
        let amount = (((reward_pool as u128) * (validator.stake as u128)) / voting_stake) as u64;
        distributed = distributed
            .checked_add(amount)
            .ok_or(RewardError::RewardOverflow)?;
        rewards.push(ValidatorReward {
            validator: validator.address.clone(),
            amount,
        });
    }

    let remainder = reward_pool - distributed;
    for reward in rewards.iter_mut().take(remainder as usize) {
        reward.amount += 1;
    }

    Ok(rewards)
}

fn validator_stake(validators: &[Validator], address: &[u8]) -> Option<u64> {
    validators
        .iter()
        .find(|validator| validator.address == address && validator.stake > 0)
        .map(|validator| validator.stake)
}

fn proposer_seed(previous_block_hash: &[u8], height: u64) -> [u8; 32] {
    let mut payload = Vec::with_capacity(previous_block_hash.len() + 8);
    payload.extend_from_slice(previous_block_hash);
    payload.extend_from_slice(&height.to_be_bytes());

    let hash = sha256(&payload);
    hash.try_into()
        .expect("sha256 must always return exactly 32 bytes")
}

fn select_proposer_by_seed(validators: &[Validator], seed: &[u8; 32]) -> Result<Validator> {
    let mut eligible: Vec<Validator> = validators
        .iter()
        .filter(|validator| validator.stake > 0)
        .cloned()
        .collect();

    if eligible.is_empty() {
        return Err(anyhow!("no eligible validators with positive stake"));
    }

    eligible.sort_by(|left, right| left.address.cmp(&right.address));

    let total_stake: u128 = eligible
        .iter()
        .map(|validator| validator.stake as u128)
        .sum();
    if total_stake == 0 {
        return Err(anyhow!("total validator stake must be greater than zero"));
    }

    let draw = u128::from_be_bytes(
        seed[..16]
            .try_into()
            .expect("seed slice should contain 16 bytes"),
    ) % total_stake;

    let mut cumulative = 0u128;
    for validator in eligible {
        cumulative += validator.stake as u128;
        if draw < cumulative {
            return Ok(validator);
        }
    }

    Err(anyhow!("failed to select proposer from validator set"))
}

fn unsigned_block_header_bytes(header: &BlockHeader) -> Result<Vec<u8>> {
    let mut unsigned = header.clone();
    unsigned.signature.clear();
    Ok(bincode::serialize(&unsigned)?)
}

fn sign_block_header(header: &BlockHeader, secret_key: &[u8; 32]) -> Result<Vec<u8>> {
    let signing_key = SigningKey::from_bytes(secret_key);
    let message = unsigned_block_header_bytes(header)?;
    Ok(signing_key.sign(&message).to_bytes().to_vec())
}

fn unsigned_vote_bytes(vote: &ConsensusVote) -> std::result::Result<Vec<u8>, VoteError> {
    let mut unsigned = vote.clone();
    unsigned.signature.clear();
    bincode::serialize(&unsigned).map_err(|err| VoteError::Serialization(err.to_string()))
}

pub fn create_signed_vote(
    height: u64,
    block_hash: &[u8],
    approve: bool,
    secret_key: &[u8; 32],
) -> std::result::Result<ConsensusVote, VoteError> {
    let signing_key = SigningKey::from_bytes(secret_key);
    let voter = signing_key.verifying_key().to_bytes().to_vec();

    let mut vote = ConsensusVote {
        height,
        block_hash: block_hash.to_vec(),
        voter,
        approve,
        signature: Vec::new(),
    };

    let message = unsigned_vote_bytes(&vote)?;
    vote.signature = signing_key.sign(&message).to_bytes().to_vec();

    Ok(vote)
}

pub fn verify_vote(vote: &ConsensusVote) -> std::result::Result<bool, VoteError> {
    let voter_bytes: [u8; 32] = vote
        .voter
        .clone()
        .try_into()
        .map_err(|_| VoteError::InvalidVoterKeyLength)?;
    let verifying_key =
        VerifyingKey::from_bytes(&voter_bytes).map_err(|_| VoteError::InvalidVoterKeyLength)?;

    let signature_bytes: [u8; 64] = vote
        .signature
        .clone()
        .try_into()
        .map_err(|_| VoteError::InvalidSignatureLength)?;
    let signature = Signature::from_bytes(&signature_bytes);

    let message = unsigned_vote_bytes(vote)?;
    Ok(verifying_key.verify(&message, &signature).is_ok())
}

pub fn select_proposer(
    validators: &[Validator],
    previous_block_hash: &[u8],
    height: u64,
) -> Result<Validator> {
    let seed = proposer_seed(previous_block_hash, height);
    select_proposer_by_seed(validators, &seed)
}

pub fn propose_block(
    proposer: &Validator,
    proposer_secret_key: &[u8; 32],
    previous_block_hash: &[u8],
    height: u64,
    timestamp: u64,
    mempool: &mut Mempool,
    max_transactions: usize,
) -> Result<Block> {
    if max_transactions == 0 {
        return Err(anyhow!("max_transactions must be greater than zero"));
    }

    let transactions = mempool.take_for_block(max_transactions);
    let merkle_root = calculate_merkle_root(&transactions);

    let mut header = BlockHeader {
        version: 1,
        previous_hash: previous_block_hash.to_vec(),
        merkle_root,
        timestamp,
        height,
        validator: proposer.address.clone(),
        signature: Vec::new(),
    };

    header.signature = sign_block_header(&header, proposer_secret_key)?;

    Ok(Block {
        header,
        transactions,
        receipts: vec![],
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blockchain::{
        hash::hash_block_header,
        types::{FIXED_TRANSACTION_FEE, TransactionType},
        validation::validate_block,
    };
    use crate::crypto::{KeyPair, generate_keypair};

    fn sample_validators() -> Vec<Validator> {
        vec![
            Validator {
                address: b"alice".to_vec(),
                stake: 10,
                locked_until: 100,
            },
            Validator {
                address: b"bob".to_vec(),
                stake: 20,
                locked_until: 200,
            },
            Validator {
                address: b"carol".to_vec(),
                stake: 30,
                locked_until: 300,
            },
        ]
    }

    fn sample_transaction(nonce: u64) -> Transaction {
        Transaction {
            tx_type: TransactionType::Transfer,
            from: vec![1; 32],
            to: vec![2; 32],
            amount: 42,
            fee: FIXED_TRANSACTION_FEE,
            nonce,
            source_shard: 0,
            destination_shard: 0,
            signature: vec![9; 64],
            data: Some(b"pending".to_vec()),
        }
    }

    fn seed_from_u128(value: u128) -> [u8; 32] {
        let mut seed = [0u8; 32];
        seed[..16].copy_from_slice(&value.to_be_bytes());
        seed
    }

    fn validator_from_pair(pair: &KeyPair, stake: u64) -> Validator {
        Validator {
            address: pair.public_key.to_vec(),
            stake,
            locked_until: 1_800_000_000,
        }
    }

    #[test]
    fn select_proposer_rejects_empty_validator_set() {
        assert!(select_proposer(&[], &[7; 32], 1).is_err());
    }

    #[test]
    fn select_proposer_ignores_zero_stake_validators() {
        let validators = vec![
            Validator {
                address: b"alice".to_vec(),
                stake: 0,
                locked_until: 100,
            },
            Validator {
                address: b"bob".to_vec(),
                stake: 50,
                locked_until: 200,
            },
        ];

        let selected = select_proposer(&validators, &[9; 32], 1).expect("select proposer");
        assert_eq!(selected.address, b"bob".to_vec());
    }

    #[test]
    fn select_proposer_by_seed_respects_weighted_ranges() {
        let validators = sample_validators();

        assert_eq!(
            select_proposer_by_seed(&validators, &seed_from_u128(0))
                .expect("select proposer at draw 0")
                .address,
            b"alice".to_vec()
        );
        assert_eq!(
            select_proposer_by_seed(&validators, &seed_from_u128(10))
                .expect("select proposer at draw 10")
                .address,
            b"bob".to_vec()
        );
        assert_eq!(
            select_proposer_by_seed(&validators, &seed_from_u128(29))
                .expect("select proposer at draw 29")
                .address,
            b"bob".to_vec()
        );
        assert_eq!(
            select_proposer_by_seed(&validators, &seed_from_u128(30))
                .expect("select proposer at draw 30")
                .address,
            b"carol".to_vec()
        );
        assert_eq!(
            select_proposer_by_seed(&validators, &seed_from_u128(59))
                .expect("select proposer at draw 59")
                .address,
            b"carol".to_vec()
        );
    }

    #[test]
    fn select_proposer_is_deterministic_for_same_inputs() {
        let validators = sample_validators();
        let previous_block_hash = [5u8; 32];

        let first = select_proposer(&validators, &previous_block_hash, 42)
            .expect("first proposer selection");
        let second = select_proposer(&validators, &previous_block_hash, 42)
            .expect("second proposer selection");

        assert_eq!(first, second);
    }

    #[test]
    fn select_proposer_is_independent_of_input_order() {
        let validators = sample_validators();
        let mut reversed = validators.clone();
        reversed.reverse();
        let previous_block_hash = [11u8; 32];

        let selected_from_original = select_proposer(&validators, &previous_block_hash, 7)
            .expect("selection from original order");
        let selected_from_reversed = select_proposer(&reversed, &previous_block_hash, 7)
            .expect("selection from reversed order");

        assert_eq!(selected_from_original, selected_from_reversed);
    }

    #[test]
    fn propose_block_builds_signed_block_from_mempool() {
        let pair = generate_keypair();
        let proposer = validator_from_pair(&pair, 100_000);
        let previous_block_hash = vec![7; 32];

        let mut mempool = Mempool::new();
        mempool.add_transaction(sample_transaction(1));
        mempool.add_transaction(sample_transaction(2));

        let block = propose_block(
            &proposer,
            &pair.secret_key,
            &previous_block_hash,
            2,
            1_710_000_123,
            &mut mempool,
            10,
        )
        .expect("propose block");

        assert_eq!(block.header.height, 2);
        assert_eq!(block.header.previous_hash, previous_block_hash);
        assert_eq!(block.header.validator, proposer.address);
        assert_eq!(block.transactions.len(), 2);
        assert!(mempool.is_empty());
        assert!(validate_block(&block, &[7; 32]).is_ok());
    }

    #[test]
    fn propose_block_respects_transaction_limit_and_keeps_rest_in_mempool() {
        let pair = generate_keypair();
        let proposer = validator_from_pair(&pair, 100_000);

        let mut mempool = Mempool::new();
        mempool.add_transaction(sample_transaction(1));
        mempool.add_transaction(sample_transaction(2));
        mempool.add_transaction(sample_transaction(3));

        let block = propose_block(
            &proposer,
            &pair.secret_key,
            &[8; 32],
            3,
            1_710_000_456,
            &mut mempool,
            2,
        )
        .expect("propose limited block");

        assert_eq!(block.transactions.len(), 2);
        assert_eq!(mempool.len(), 1);
        assert_eq!(mempool.pending_transactions()[0].nonce, 3);
    }

    #[test]
    fn propose_block_can_pull_transactions_from_all_partitions() {
        let pair = generate_keypair();
        let proposer = validator_from_pair(&pair, 100_000);
        let mut mempool = Mempool::with_partition_count(4);

        let mut senders_by_partition = vec![None; mempool.partition_count()];
        let mut candidate = 0u64;

        while senders_by_partition.iter().any(|entry| entry.is_none()) {
            let sender = format!("sender-{candidate}").into_bytes();
            let partition = mempool.partition_for_address(&sender);

            if senders_by_partition[partition].is_none() {
                senders_by_partition[partition] = Some(sender);
            }

            candidate += 1;
            assert!(
                candidate < 10_000,
                "failed to find one sender per partition"
            );
        }

        for (nonce, sender) in senders_by_partition.into_iter().enumerate() {
            let mut tx = sample_transaction((nonce + 1) as u64);
            tx.from = sender.expect("sender for partition");
            mempool.add_transaction(tx);
        }

        let block = propose_block(
            &proposer,
            &pair.secret_key,
            &[9; 32],
            5,
            1_710_000_999,
            &mut mempool,
            4,
        )
        .expect("propose block across partitions");

        let selected_nonces: Vec<u64> = block.transactions.iter().map(|tx| tx.nonce).collect();

        assert_eq!(block.transactions.len(), 4);
        assert!(selected_nonces.contains(&1));
        assert!(selected_nonces.contains(&2));
        assert!(selected_nonces.contains(&3));
        assert!(selected_nonces.contains(&4));
        assert!(mempool.is_empty());
    }

    #[test]
    fn propose_block_does_not_preserve_global_submission_order_preventing_sandwich_attacks() {
        let pair = generate_keypair();
        let proposer = validator_from_pair(&pair, 100_000);
        let mut mempool = Mempool::with_partition_count(4);

        let mut senders_by_partition = vec![None; mempool.partition_count()];
        let mut candidate = 0u64;

        while senders_by_partition[0].is_none()
            || senders_by_partition[1].is_none()
            || senders_by_partition[3].is_none()
        {
            let sender = format!("sandwich-sender-{candidate}").into_bytes();
            let partition = mempool.partition_for_address(&sender);

            if senders_by_partition[partition].is_none() {
                senders_by_partition[partition] = Some(sender);
            }

            candidate += 1;
            assert!(
                candidate < 10_000,
                "failed to find required sandwich partitions"
            );
        }

        let mut attacker_before = sample_transaction(1);
        attacker_before.from = senders_by_partition[0]
            .clone()
            .expect("attacker-before sender");
        attacker_before.to = b"dex-pool".to_vec();

        let mut victim = sample_transaction(2);
        victim.from = senders_by_partition[3].clone().expect("victim sender");
        victim.to = b"dex-pool".to_vec();

        let mut attacker_after = sample_transaction(3);
        attacker_after.from = senders_by_partition[1]
            .clone()
            .expect("attacker-after sender");
        attacker_after.to = b"dex-pool".to_vec();

        mempool.add_transaction(attacker_before);
        mempool.add_transaction(victim);
        mempool.add_transaction(attacker_after);

        let block = propose_block(
            &proposer,
            &pair.secret_key,
            &[10; 32],
            6,
            1_710_001_111,
            &mut mempool,
            3,
        )
        .expect("propose block without visible global order");

        let selected_nonces: Vec<u64> = block.transactions.iter().map(|tx| tx.nonce).collect();

        let attacker_before_index = selected_nonces
            .iter()
            .position(|nonce| *nonce == 1)
            .expect("attacker-before tx");
        let victim_index = selected_nonces
            .iter()
            .position(|nonce| *nonce == 2)
            .expect("victim tx");
        let attacker_after_index = selected_nonces
            .iter()
            .position(|nonce| *nonce == 3)
            .expect("attacker-after tx");

        assert_eq!(selected_nonces, vec![1, 3, 2]);
        assert_ne!(selected_nonces, vec![1, 2, 3]);
        assert!(
            !(attacker_before_index < victim_index && victim_index < attacker_after_index),
            "victim transaction remained sandwichable in block order"
        );
    }

    #[test]
    fn mempool_uses_explicit_partition_count() {
        let mempool = Mempool::with_partition_count(4);

        assert_eq!(mempool.partition_count(), 4);
        assert!(mempool.is_empty());
    }

    #[test]
    fn mempool_keeps_same_sender_fifo_within_a_partition() {
        let mut mempool = Mempool::with_partition_count(4);

        let mut tx1 = sample_transaction(1);
        tx1.from = b"alice".to_vec();

        let mut tx2 = sample_transaction(2);
        tx2.from = b"alice".to_vec();

        let mut tx3 = sample_transaction(3);
        tx3.from = b"bob".to_vec();

        mempool.add_transaction(tx1);
        mempool.add_transaction(tx2);
        mempool.add_transaction(tx3);

        let same_sender_nonces: Vec<u64> = mempool
            .take_for_block(3)
            .into_iter()
            .filter(|tx| tx.from == b"alice".to_vec())
            .map(|tx| tx.nonce)
            .collect();

        assert_eq!(same_sender_nonces, vec![1, 2]);
    }

    #[test]
    fn mempool_pending_and_take_for_block_work_across_partition_tasks() {
        let mut mempool = Mempool::with_partition_count(4);

        let mut tx1 = sample_transaction(1);
        tx1.from = b"alice".to_vec();

        let mut tx2 = sample_transaction(2);
        tx2.from = b"bob".to_vec();

        let mut tx3 = sample_transaction(3);
        tx3.from = b"carol".to_vec();

        mempool.add_transaction(tx1);
        mempool.add_transaction(tx2);
        mempool.add_transaction(tx3);

        let pending_nonces: Vec<u64> = mempool
            .pending_transactions()
            .into_iter()
            .map(|tx| tx.nonce)
            .collect();

        assert_eq!(pending_nonces.len(), 3);
        assert!(pending_nonces.contains(&1));
        assert!(pending_nonces.contains(&2));
        assert!(pending_nonces.contains(&3));

        let selected = mempool.take_for_block(2);
        assert_eq!(selected.len(), 2);
        assert_eq!(mempool.len(), 1);
    }

    #[test]
    fn mempool_limits_sender_to_one_hundred_transactions_per_block() {
        let mut mempool = Mempool::with_partition_count(4);

        for nonce in 1..=101 {
            let mut tx = sample_transaction(nonce);
            tx.from = b"alice".to_vec();
            mempool.add_transaction(tx);
        }

        let selected = mempool.take_for_block(101);

        let alice_selected_nonces: Vec<u64> = selected
            .iter()
            .filter(|tx| tx.from == b"alice".to_vec())
            .map(|tx| tx.nonce)
            .collect();

        assert_eq!(selected.len(), 100);
        assert_eq!(alice_selected_nonces.len(), 100);
        assert_eq!(mempool.len(), 1);
        assert_eq!(mempool.pending_transactions()[0].nonce, 101);
    }

    #[test]
    fn create_and_verify_vote_roundtrip() {
        let pair = generate_keypair();
        let block = propose_block(
            &validator_from_pair(&pair, 100_000),
            &pair.secret_key,
            &[1; 32],
            4,
            1_710_000_789,
            &mut Mempool::new(),
            1,
        )
        .expect("propose block for vote");
        let block_hash = hash_block_header(&block.header);

        let vote = create_signed_vote(block.header.height, &block_hash, true, &pair.secret_key)
            .expect("create signed vote");

        assert!(verify_vote(&vote).expect("verify vote"));
    }

    #[test]
    fn verify_vote_fails_when_payload_changes() {
        let pair = generate_keypair();
        let vote =
            create_signed_vote(5, &[2; 32], true, &pair.secret_key).expect("create signed vote");
        let mut tampered = vote.clone();
        tampered.approve = false;

        assert!(
            !verify_vote(&tampered).expect("verify tampered vote"),
            "tampered vote must not verify"
        );
    }

    #[test]
    fn vote_collector_aggregates_weighted_votes() {
        let pair_a = generate_keypair();
        let pair_b = generate_keypair();
        let pair_c = generate_keypair();

        let validators = vec![
            validator_from_pair(&pair_a, 10),
            validator_from_pair(&pair_b, 20),
            validator_from_pair(&pair_c, 30),
        ];

        let mut collector = VoteCollector::new();
        collector
            .collect_vote(
                create_signed_vote(6, &[3; 32], true, &pair_a.secret_key).expect("vote a"),
                &validators,
            )
            .expect("collect vote a");
        collector
            .collect_vote(
                create_signed_vote(6, &[3; 32], false, &pair_b.secret_key).expect("vote b"),
                &validators,
            )
            .expect("collect vote b");
        collector
            .collect_vote(
                create_signed_vote(6, &[3; 32], true, &pair_c.secret_key).expect("vote c"),
                &validators,
            )
            .expect("collect vote c");

        let tally = collector.tally(&validators).expect("tally votes");
        assert_eq!(tally.approve_stake, 40);
        assert_eq!(tally.reject_stake, 20);
        assert_eq!(tally.total_votes, 3);
    }

    #[test]
    fn vote_collector_rejects_duplicate_vote_from_same_validator() {
        let pair = generate_keypair();
        let validators = vec![validator_from_pair(&pair, 25)];
        let vote =
            create_signed_vote(7, &[4; 32], true, &pair.secret_key).expect("create signed vote");

        let mut collector = VoteCollector::new();
        collector
            .collect_vote(vote.clone(), &validators)
            .expect("collect first vote");

        assert_eq!(
            collector.collect_vote(vote, &validators),
            Err(VoteError::DuplicateVote)
        );
    }

    #[test]
    fn vote_collector_rejects_unknown_validator() {
        let pair = generate_keypair();
        let other = generate_keypair();
        let validators = vec![validator_from_pair(&pair, 25)];
        let vote = create_signed_vote(8, &[5; 32], true, &other.secret_key)
            .expect("create unknown validator vote");

        let mut collector = VoteCollector::new();
        assert_eq!(
            collector.collect_vote(vote, &validators),
            Err(VoteError::UnknownValidator)
        );
    }

    #[test]
    fn verify_vote_rejects_invalid_voter_key_length() {
        let pair = generate_keypair();
        let mut vote =
            create_signed_vote(9, &[6; 32], true, &pair.secret_key).expect("create signed vote");
        vote.voter.pop();

        assert_eq!(verify_vote(&vote), Err(VoteError::InvalidVoterKeyLength));
    }

    #[test]
    fn verify_vote_rejects_invalid_signature_length() {
        let pair = generate_keypair();
        let mut vote =
            create_signed_vote(9, &[6; 32], true, &pair.secret_key).expect("create signed vote");
        vote.signature.pop();

        assert_eq!(verify_vote(&vote), Err(VoteError::InvalidSignatureLength));
    }

    #[test]
    fn vote_collector_rejects_invalid_signature() {
        let pair = generate_keypair();
        let validators = vec![validator_from_pair(&pair, 25)];
        let mut vote =
            create_signed_vote(9, &[6; 32], true, &pair.secret_key).expect("create signed vote");
        vote.signature[0] ^= 0xFF;

        let mut collector = VoteCollector::new();
        assert_eq!(
            collector.collect_vote(vote, &validators),
            Err(VoteError::InvalidSignature)
        );
    }

    #[test]
    fn block_is_not_finalized_at_exact_two_thirds() {
        let pair_a = generate_keypair();
        let pair_b = generate_keypair();
        let pair_c = generate_keypair();

        let validators = vec![
            validator_from_pair(&pair_a, 20),
            validator_from_pair(&pair_b, 20),
            validator_from_pair(&pair_c, 20),
        ];

        let mut collector = VoteCollector::new();
        collector
            .collect_vote(
                create_signed_vote(10, &[7; 32], true, &pair_a.secret_key).expect("vote a"),
                &validators,
            )
            .expect("collect vote a");
        collector
            .collect_vote(
                create_signed_vote(10, &[7; 32], true, &pair_b.secret_key).expect("vote b"),
                &validators,
            )
            .expect("collect vote b");
        collector
            .collect_vote(
                create_signed_vote(10, &[7; 32], false, &pair_c.secret_key).expect("vote c"),
                &validators,
            )
            .expect("collect vote c");

        let tally = collector.tally(&validators).expect("tally votes");
        assert_eq!(tally.approve_stake, 40);
        assert!(!is_block_finalized(&tally, &validators));
    }

    #[test]
    fn block_is_finalized_above_two_thirds_of_weighted_votes() {
        let pair_a = generate_keypair();
        let pair_b = generate_keypair();
        let pair_c = generate_keypair();

        let validators = vec![
            validator_from_pair(&pair_a, 10),
            validator_from_pair(&pair_b, 20),
            validator_from_pair(&pair_c, 31),
        ];

        let mut collector = VoteCollector::new();
        collector
            .collect_vote(
                create_signed_vote(11, &[8; 32], true, &pair_a.secret_key).expect("vote a"),
                &validators,
            )
            .expect("collect vote a");
        collector
            .collect_vote(
                create_signed_vote(11, &[8; 32], true, &pair_b.secret_key).expect("vote b"),
                &validators,
            )
            .expect("collect vote b");
        collector
            .collect_vote(
                create_signed_vote(11, &[8; 32], true, &pair_c.secret_key).expect("vote c"),
                &validators,
            )
            .expect("collect vote c");

        let tally = collector.tally(&validators).expect("tally votes");
        assert_eq!(tally.approve_stake, 61);
        assert!(is_block_finalized(&tally, &validators));
    }

    #[test]
    fn block_is_not_finalized_when_no_active_stake_exists() {
        let tally = VoteTally {
            approve_stake: 10,
            reject_stake: 0,
            total_votes: 1,
        };
        let validators = vec![Validator {
            address: b"nobody".to_vec(),
            stake: 0,
            locked_until: 0,
        }];

        assert!(!is_block_finalized(&tally, &validators));
    }

    #[test]
    fn cumulative_chain_weight_sums_approve_stake() {
        let tallies = vec![
            VoteTally {
                approve_stake: 40,
                reject_stake: 20,
                total_votes: 3,
            },
            VoteTally {
                approve_stake: 61,
                reject_stake: 0,
                total_votes: 3,
            },
        ];

        assert_eq!(cumulative_chain_weight(&tallies), 101);
    }

    #[test]
    fn choose_heaviest_chain_prefers_higher_cumulative_stake() {
        let forks = vec![
            ChainFork {
                tip_hash: vec![1; 32],
                height: 10,
                cumulative_stake_weight: 100,
            },
            ChainFork {
                tip_hash: vec![2; 32],
                height: 9,
                cumulative_stake_weight: 120,
            },
            ChainFork {
                tip_hash: vec![3; 32],
                height: 11,
                cumulative_stake_weight: 90,
            },
        ];

        let selected = choose_heaviest_chain(&forks).expect("select heaviest chain");
        assert_eq!(selected.tip_hash, vec![2; 32]);
        assert_eq!(selected.cumulative_stake_weight, 120);
    }

    #[test]
    fn choose_heaviest_chain_breaks_tie_with_height_then_hash() {
        let forks = vec![
            ChainFork {
                tip_hash: vec![1; 32],
                height: 10,
                cumulative_stake_weight: 150,
            },
            ChainFork {
                tip_hash: vec![2; 32],
                height: 12,
                cumulative_stake_weight: 150,
            },
            ChainFork {
                tip_hash: vec![3; 32],
                height: 12,
                cumulative_stake_weight: 150,
            },
        ];

        let selected = choose_heaviest_chain(&forks).expect("select heaviest chain");
        assert_eq!(selected.tip_hash, vec![3; 32]);
        assert_eq!(selected.height, 12);
    }

    #[test]
    fn choose_heaviest_chain_returns_none_for_empty_input() {
        assert!(choose_heaviest_chain(&[]).is_none());
    }

    #[test]
    fn detect_double_vote_accepts_valid_conflicting_votes() {
        let pair = generate_keypair();

        let first_vote =
            create_signed_vote(12, &[9; 32], true, &pair.secret_key).expect("create first vote");
        let second_vote =
            create_signed_vote(12, &[8; 32], true, &pair.secret_key).expect("create second vote");

        let proof = detect_double_vote(&first_vote, &second_vote).expect("detect double vote");

        assert_eq!(proof.first_vote, first_vote);
        assert_eq!(proof.second_vote, second_vote);
    }

    #[test]
    fn detect_double_vote_rejects_same_block_hash() {
        let pair = generate_keypair();

        let first_vote =
            create_signed_vote(13, &[7; 32], true, &pair.secret_key).expect("create first vote");
        let second_vote =
            create_signed_vote(13, &[7; 32], false, &pair.secret_key).expect("create second vote");

        assert_eq!(
            detect_double_vote(&first_vote, &second_vote),
            Err(SlashingError::SameBlockHash)
        );
    }

    #[test]
    fn detect_double_vote_rejects_different_heights() {
        let pair = generate_keypair();

        let first_vote =
            create_signed_vote(14, &[6; 32], true, &pair.secret_key).expect("create first vote");
        let second_vote =
            create_signed_vote(15, &[5; 32], true, &pair.secret_key).expect("create second vote");

        assert_eq!(
            detect_double_vote(&first_vote, &second_vote),
            Err(SlashingError::DifferentHeights)
        );
    }

    #[test]
    fn detect_double_vote_rejects_invalid_signature() {
        let pair = generate_keypair();

        let first_vote =
            create_signed_vote(16, &[4; 32], true, &pair.secret_key).expect("create first vote");
        let mut second_vote =
            create_signed_vote(16, &[3; 32], true, &pair.secret_key).expect("create second vote");
        second_vote.signature[0] ^= 0xFF;

        assert_eq!(
            detect_double_vote(&first_vote, &second_vote),
            Err(SlashingError::InvalidSecondVote)
        );
    }

    #[test]
    fn slash_validator_for_double_vote_reduces_stake() {
        let pair = generate_keypair();
        let validator = validator_from_pair(&pair, 1_000);

        let first_vote =
            create_signed_vote(17, &[2; 32], true, &pair.secret_key).expect("create first vote");
        let second_vote =
            create_signed_vote(17, &[1; 32], true, &pair.secret_key).expect("create second vote");
        let proof = detect_double_vote(&first_vote, &second_vote).expect("detect double vote");

        let slashed =
            slash_validator_for_double_vote(&validator, &proof, 1_500).expect("slash validator");

        assert_eq!(slashed.address, validator.address);
        assert_eq!(slashed.locked_until, validator.locked_until);
        assert_eq!(slashed.stake, 850);
    }

    #[test]
    fn slash_validator_for_double_vote_rejects_invalid_penalty_bps() {
        let pair = generate_keypair();
        let validator = validator_from_pair(&pair, 1_000);

        let first_vote =
            create_signed_vote(18, &[11; 32], true, &pair.secret_key).expect("create first vote");
        let second_vote =
            create_signed_vote(18, &[12; 32], true, &pair.secret_key).expect("create second vote");
        let proof = detect_double_vote(&first_vote, &second_vote).expect("detect double vote");

        assert_eq!(
            slash_validator_for_double_vote(&validator, &proof, 0),
            Err(SlashingError::InvalidPenaltyBps)
        );
    }

    #[test]
    fn slash_validator_for_double_vote_rejects_mismatched_validator() {
        let pair_a = generate_keypair();
        let pair_b = generate_keypair();

        let validator = validator_from_pair(&pair_a, 1_000);
        let first_vote =
            create_signed_vote(19, &[13; 32], true, &pair_b.secret_key).expect("create first vote");
        let second_vote = create_signed_vote(19, &[14; 32], true, &pair_b.secret_key)
            .expect("create second vote");
        let proof = detect_double_vote(&first_vote, &second_vote).expect("detect double vote");

        assert_eq!(
            slash_validator_for_double_vote(&validator, &proof, 500),
            Err(SlashingError::ValidatorDoesNotMatchProof)
        );
    }

    #[test]
    fn total_transaction_fees_sums_fees_across_transactions() {
        let mut tx1 = sample_transaction(1);
        tx1.fee = FIXED_TRANSACTION_FEE;
        let mut tx2 = sample_transaction(2);
        tx2.fee = FIXED_TRANSACTION_FEE;
        let mut tx3 = sample_transaction(3);
        tx3.fee = FIXED_TRANSACTION_FEE;

        assert_eq!(
            total_transaction_fees(&[tx1, tx2, tx3]),
            Ok(FIXED_TRANSACTION_FEE * 3)
        );
    }

    #[test]
    fn annualized_inflation_reward_matches_two_percent_for_one_year() {
        assert_eq!(annualized_inflation_reward(10_000, SECONDS_PER_YEAR), 200);
    }

    #[test]
    fn distribute_voting_rewards_splits_pool_across_voting_validators_only() {
        let pair_a = generate_keypair();
        let pair_b = generate_keypair();
        let pair_c = generate_keypair();

        let validators = vec![
            validator_from_pair(&pair_a, 25),
            validator_from_pair(&pair_b, 75),
            validator_from_pair(&pair_c, 100),
        ];

        let mut collector = VoteCollector::new();
        collector
            .collect_vote(
                create_signed_vote(20, &[21; 32], true, &pair_a.secret_key).expect("vote a"),
                &validators,
            )
            .expect("collect vote a");
        collector
            .collect_vote(
                create_signed_vote(20, &[21; 32], true, &pair_b.secret_key).expect("vote b"),
                &validators,
            )
            .expect("collect vote b");

        let mut tx1 = sample_transaction(1);
        tx1.fee = FIXED_TRANSACTION_FEE;
        let mut tx2 = sample_transaction(2);
        tx2.fee = FIXED_TRANSACTION_FEE;

        let rewards =
            distribute_voting_rewards(&validators, &collector, &[tx1, tx2], SECONDS_PER_YEAR)
                .expect("distribute rewards");

        assert_eq!(rewards.len(), 2);

        let amount_a = rewards
            .iter()
            .find(|reward| reward.validator == pair_a.public_key.to_vec())
            .map(|reward| reward.amount);
        let amount_b = rewards
            .iter()
            .find(|reward| reward.validator == pair_b.public_key.to_vec())
            .map(|reward| reward.amount);

        assert_eq!(
            rewards
                .iter()
                .find(|reward| reward.validator == pair_c.public_key.to_vec())
                .map(|reward| reward.amount),
            None
        );

        let amount_a = amount_a.expect("reward for pair_a");
        let amount_b = amount_b.expect("reward for pair_b");

        assert_eq!(amount_a + amount_b, 6);
        assert!(
            (amount_a == 2 && amount_b == 4) || (amount_a == 1 && amount_b == 5),
            "unexpected reward split: pair_a={amount_a}, pair_b={amount_b}"
        );
    }

    #[test]
    fn distribute_voting_rewards_rejects_empty_vote_set() {
        let pair = generate_keypair();
        let validators = vec![validator_from_pair(&pair, 50)];
        let tx = sample_transaction(1);
        let collector = VoteCollector::new();

        assert_eq!(
            distribute_voting_rewards(&validators, &collector, &[tx], SECONDS_PER_YEAR),
            Err(RewardError::NoVotingStake)
        );
    }

    #[test]
    fn four_validator_simulation_covers_selection_fork_and_slashing() {
        let pair_a = generate_keypair();
        let pair_b = generate_keypair();
        let pair_c = generate_keypair();
        let pair_d = generate_keypair();

        let validators = vec![
            validator_from_pair(&pair_a, 40),
            validator_from_pair(&pair_b, 30),
            validator_from_pair(&pair_c, 20),
            validator_from_pair(&pair_d, 10),
        ];

        let first_selected =
            select_proposer(&validators, &[42; 32], 21).expect("first proposer selection");
        let second_selected =
            select_proposer(&validators, &[42; 32], 21).expect("second proposer selection");

        assert_eq!(first_selected, second_selected);
        assert!(
            validators
                .iter()
                .any(|validator| validator.address == first_selected.address),
            "selected proposer must belong to validator set"
        );

        let mut winning_collector = VoteCollector::new();
        winning_collector
            .collect_vote(
                create_signed_vote(21, &[1; 32], true, &pair_a.secret_key).expect("vote a"),
                &validators,
            )
            .expect("collect vote a");
        winning_collector
            .collect_vote(
                create_signed_vote(21, &[1; 32], true, &pair_b.secret_key).expect("vote b"),
                &validators,
            )
            .expect("collect vote b");
        winning_collector
            .collect_vote(
                create_signed_vote(21, &[1; 32], true, &pair_c.secret_key).expect("vote c"),
                &validators,
            )
            .expect("collect vote c");

        let winning_tally = winning_collector
            .tally(&validators)
            .expect("tally winning fork votes");
        assert_eq!(winning_tally.approve_stake, 90);
        assert!(is_block_finalized(&winning_tally, &validators));

        let mut losing_collector = VoteCollector::new();
        losing_collector
            .collect_vote(
                create_signed_vote(21, &[2; 32], true, &pair_d.secret_key).expect("vote d"),
                &validators,
            )
            .expect("collect vote d");

        let losing_tally = losing_collector
            .tally(&validators)
            .expect("tally losing fork votes");
        assert_eq!(losing_tally.approve_stake, 10);
        assert!(!is_block_finalized(&losing_tally, &validators));

        let forks = vec![
            ChainFork {
                tip_hash: vec![1; 32],
                height: 21,
                cumulative_stake_weight: cumulative_chain_weight(&[winning_tally.clone()]),
            },
            ChainFork {
                tip_hash: vec![2; 32],
                height: 21,
                cumulative_stake_weight: cumulative_chain_weight(&[losing_tally.clone()]),
            },
        ];

        let selected_fork = choose_heaviest_chain(&forks).expect("select winning fork");
        assert_eq!(selected_fork.tip_hash, vec![1; 32]);
        assert_eq!(selected_fork.cumulative_stake_weight, 90);

        let first_vote =
            create_signed_vote(21, &[1; 32], true, &pair_d.secret_key).expect("first fork vote");
        let second_vote =
            create_signed_vote(21, &[2; 32], true, &pair_d.secret_key).expect("second fork vote");
        let proof =
            detect_double_vote(&first_vote, &second_vote).expect("detect double-vote proof");

        let slashed = slash_validator_for_double_vote(&validators[3], &proof, 2_500)
            .expect("slash double-voting validator");
        assert_eq!(slashed.stake, 8);
    }

    #[test]
    fn byzantine_thirty_three_percent_attack_does_not_break_finalization() {
        let pair_a = generate_keypair();
        let pair_b = generate_keypair();
        let pair_c = generate_keypair();

        let validators = vec![
            validator_from_pair(&pair_a, 34),
            validator_from_pair(&pair_b, 33),
            validator_from_pair(&pair_c, 33),
        ];

        let honest_block_hash = [7; 32];
        let byzantine_block_hash = [8; 32];

        let mut honest_collector = VoteCollector::new();
        honest_collector
            .collect_vote(
                create_signed_vote(23, &honest_block_hash, true, &pair_a.secret_key)
                    .expect("vote a"),
                &validators,
            )
            .expect("collect vote a");
        honest_collector
            .collect_vote(
                create_signed_vote(23, &honest_block_hash, true, &pair_b.secret_key)
                    .expect("vote b"),
                &validators,
            )
            .expect("collect vote b");

        let honest_tally = honest_collector
            .tally(&validators)
            .expect("tally honest votes");
        assert_eq!(honest_tally.approve_stake, 67);
        assert!(is_block_finalized(&honest_tally, &validators));

        let mut byzantine_collector = VoteCollector::new();
        byzantine_collector
            .collect_vote(
                create_signed_vote(23, &byzantine_block_hash, true, &pair_c.secret_key)
                    .expect("byzantine vote"),
                &validators,
            )
            .expect("collect byzantine vote");

        let byzantine_tally = byzantine_collector
            .tally(&validators)
            .expect("tally byzantine votes");
        assert_eq!(byzantine_tally.approve_stake, 33);
        assert!(!is_block_finalized(&byzantine_tally, &validators));

        let forks = vec![
            ChainFork {
                tip_hash: honest_block_hash.to_vec(),
                height: 23,
                cumulative_stake_weight: cumulative_chain_weight(&[honest_tally.clone()]),
            },
            ChainFork {
                tip_hash: byzantine_block_hash.to_vec(),
                height: 23,
                cumulative_stake_weight: cumulative_chain_weight(&[byzantine_tally.clone()]),
            },
        ];

        let selected_fork = choose_heaviest_chain(&forks).expect("select winning fork");
        assert_eq!(selected_fork.tip_hash, honest_block_hash.to_vec());
        assert_eq!(selected_fork.cumulative_stake_weight, 67);
    }

    #[test]
    fn finalization_benchmark_completes_under_two_seconds() {
        let started_at = std::time::Instant::now();

        let pair_a = generate_keypair();
        let pair_b = generate_keypair();
        let pair_c = generate_keypair();
        let pair_d = generate_keypair();

        let validators = vec![
            validator_from_pair(&pair_a, 40),
            validator_from_pair(&pair_b, 30),
            validator_from_pair(&pair_c, 20),
            validator_from_pair(&pair_d, 10),
        ];

        let proposer = select_proposer(&validators, &[55; 32], 22).expect("select proposer");

        let proposer_secret_key = if proposer.address == pair_a.public_key.to_vec() {
            &pair_a.secret_key
        } else if proposer.address == pair_b.public_key.to_vec() {
            &pair_b.secret_key
        } else if proposer.address == pair_c.public_key.to_vec() {
            &pair_c.secret_key
        } else {
            &pair_d.secret_key
        };

        let mut mempool = Mempool::new();
        for nonce in 1..=100 {
            mempool.add_transaction(sample_transaction(nonce));
        }

        let block = propose_block(
            &proposer,
            proposer_secret_key,
            &[9; 32],
            22,
            1_710_001_111,
            &mut mempool,
            100,
        )
        .expect("propose block");

        let block_hash = hash_block_header(&block.header);

        let mut collector = VoteCollector::new();
        collector
            .collect_vote(
                create_signed_vote(22, &block_hash, true, &pair_a.secret_key).expect("vote a"),
                &validators,
            )
            .expect("collect vote a");
        collector
            .collect_vote(
                create_signed_vote(22, &block_hash, true, &pair_b.secret_key).expect("vote b"),
                &validators,
            )
            .expect("collect vote b");
        collector
            .collect_vote(
                create_signed_vote(22, &block_hash, true, &pair_c.secret_key).expect("vote c"),
                &validators,
            )
            .expect("collect vote c");
        collector
            .collect_vote(
                create_signed_vote(22, &block_hash, true, &pair_d.secret_key).expect("vote d"),
                &validators,
            )
            .expect("collect vote d");

        let tally = collector.tally(&validators).expect("tally votes");
        assert!(is_block_finalized(&tally, &validators));

        let elapsed = started_at.elapsed();
        assert!(
            elapsed < std::time::Duration::from_secs(2),
            "finalization benchmark exceeded 2 seconds: {:?}",
            elapsed
        );
    }
}
