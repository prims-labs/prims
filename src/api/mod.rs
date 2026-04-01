//! API JSON-RPC de Prims.
//!
//! Ce module expose les méthodes RPC publiques, applique les validations
//! d'entrée, le rate limiting, l'accès au stockage local et le lien
//! avec la mempool et l'exécution Wasm.

use crate::{
    blockchain::{
        hash::{hash_block_header, hash_transaction},
        types::{
            Account, Block, ContractCallPayload, CrossShardReceipt, FIXED_TRANSACTION_FEE,
            Transaction, TransactionType, Validator,
        },
        validation::{
            validate_transaction_balance, validate_transaction_nonce, validate_transaction_size,
        },
    },
    consensus::Mempool,
    crypto::verify_transaction,
    storage::{RocksDbStorage, StorageBackend, keys},
    vm::{WasmExecutionContext, WasmVM},
};
use anyhow::Result;
use governor::{DefaultDirectRateLimiter, Quota, RateLimiter};
use jsonrpsee::{RpcModule, types::ErrorObjectOwned};
use serde::{Deserialize, Serialize};
use std::{num::NonZeroU32, str, sync::Arc};

type RpcResult<T> = std::result::Result<T, ErrorObjectOwned>;

const INVALID_PARAMS_CODE: i32 = -32602;
const INTERNAL_ERROR_CODE: i32 = -32603;
const NOT_FOUND_CODE: i32 = -32004;
const VALIDATION_ERROR_CODE: i32 = -32010;
const RATE_LIMIT_EXCEEDED_CODE: i32 = -32029;
const DEFAULT_RPC_RATE_LIMIT_PER_SECOND: u32 = 20;

#[derive(Clone)]
pub struct RpcState {
    storage: Arc<RocksDbStorage>,
    mempool: Arc<Mempool>,
    rate_limiter: Arc<DefaultDirectRateLimiter>,
}

impl RpcState {
    pub fn new(storage: Arc<RocksDbStorage>, mempool: Arc<Mempool>) -> Self {
        let requests_per_second = NonZeroU32::new(DEFAULT_RPC_RATE_LIMIT_PER_SECOND)
            .expect("DEFAULT_RPC_RATE_LIMIT_PER_SECOND must be non-zero");

        Self::with_rate_limit(storage, mempool, requests_per_second)
    }

    pub fn with_rate_limit(
        storage: Arc<RocksDbStorage>,
        mempool: Arc<Mempool>,
        requests_per_second: NonZeroU32,
    ) -> Self {
        let rate_limiter = Arc::new(RateLimiter::direct(Quota::per_second(requests_per_second)));

        Self {
            storage,
            mempool,
            rate_limiter,
        }
    }

    pub fn storage(&self) -> &RocksDbStorage {
        self.storage.as_ref()
    }

    pub fn mempool(&self) -> &Mempool {
        self.mempool.as_ref()
    }

    pub fn rate_limiter(&self) -> &DefaultDirectRateLimiter {
        self.rate_limiter.as_ref()
    }

    pub fn check_rate_limit(&self) -> RpcResult<()> {
        self.rate_limiter()
            .check()
            .map_err(|_| rate_limit_exceeded())
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct GetBlockParams {
    height: Option<u64>,
    hash: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct HashParam {
    hash: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AddressParam {
    address: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SendTransactionParams {
    hex_tx: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct RpcBlockResponse {
    pub hash: String,
    pub header: RpcBlockHeaderView,
    pub transactions: Vec<RpcTransactionView>,
    pub receipts: Vec<RpcCrossShardReceiptView>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RpcBlockHeaderView {
    pub version: u32,
    pub previous_hash: String,
    pub merkle_root: String,
    pub timestamp: u64,
    pub height: u64,
    pub validator: String,
    pub signature: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct RpcTransactionResponse {
    pub hash: String,
    pub transaction: RpcTransactionView,
}

#[derive(Debug, Clone, Serialize)]
pub struct RpcTransactionView {
    pub tx_type: String,
    pub stake_duration: Option<u64>,
    pub from: String,
    pub to: String,
    pub amount: u64,
    pub fee: u64,
    pub nonce: u64,
    pub source_shard: u16,
    pub destination_shard: u16,
    pub signature: String,
    pub data: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RpcCrossShardReceiptView {
    pub tx_hash: String,
    pub source_shard: u16,
    pub destination_shard: u16,
    pub phase: String,
    pub proof: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct RpcBalanceResponse {
    pub address: String,
    pub found: bool,
    pub balance: u64,
    pub nonce: u64,
    pub note_commitment_count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct RpcInfoResponse {
    pub name: String,
    pub rpc_version: String,
    pub storage_backend: String,
    pub latest_block_height: Option<u64>,
    pub mempool_size: usize,
    pub validator_count: usize,
    pub note_commitment_count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct RpcValidatorView {
    pub address: String,
    pub stake: u64,
    pub locked_until: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct RpcSendTransactionResponse {
    pub accepted: bool,
    pub tx_hash: String,
    pub mempool_size: usize,
}

pub fn build_rpc_module(state: RpcState) -> Result<RpcModule<RpcState>> {
    let mut module = RpcModule::new(state);

    module.register_method("get_block", |params, ctx, _| {
        ctx.check_rate_limit()?;

        let query: GetBlockParams = params
            .parse()
            .map_err(|err| invalid_params(format!("invalid get_block params: {err}")))?;

        get_block(ctx, query)
    })?;

    module.register_method("get_transaction", |params, ctx, _| {
        ctx.check_rate_limit()?;

        let query: HashParam = params
            .parse()
            .map_err(|err| invalid_params(format!("invalid get_transaction params: {err}")))?;

        get_transaction(ctx, &query.hash)
    })?;

    module.register_async_method("send_transaction", |params, ctx, _| async move {
        ctx.check_rate_limit()?;

        let query: SendTransactionParams = params
            .parse()
            .map_err(|err| invalid_params(format!("invalid send_transaction params: {err}")))?;

        send_transaction(&ctx, &query.hex_tx).await
    })?;

    module.register_method("get_balance", |params, ctx, _| {
        ctx.check_rate_limit()?;

        let query: AddressParam = params
            .parse()
            .map_err(|err| invalid_params(format!("invalid get_balance params: {err}")))?;

        get_balance(ctx, &query.address)
    })?;

    module.register_async_method("get_info", |_params, ctx, _| async move {
        ctx.check_rate_limit()?;
        get_info(&ctx).await
    })?;

    module.register_method("get_validators", |_params, ctx, _| {
        ctx.check_rate_limit()?;
        get_validators(ctx)
    })?;

    module.register_method("get_note_commitments", |_params, ctx, _| {
        ctx.check_rate_limit()?;
        get_note_commitments(ctx)
    })?;

    Ok(module)
}

fn get_block(ctx: &RpcState, query: GetBlockParams) -> RpcResult<Option<RpcBlockResponse>> {
    match (query.height, query.hash) {
        (Some(height), None) => {
            let block = ctx.storage().get_block(height).map_err(|err| {
                internal_error(format!("failed to load block at height {height}: {err}"))
            })?;

            Ok(block.map(block_to_rpc))
        }
        (None, Some(hash_hex)) => {
            let hash_bytes = decode_hex(&hash_hex, "hash")?;
            let height_index_key = keys::height_index_key(&hash_bytes);
            let height_bytes = ctx
                .storage()
                .get(&height_index_key)
                .map_err(|err| internal_error(format!("failed to load height index: {err}")))?;

            let Some(height_bytes) = height_bytes else {
                return Ok(None);
            };

            let height: u64 = bincode::deserialize(&height_bytes).map_err(|err| {
                internal_error(format!("failed to decode block height index: {err}"))
            })?;

            let block = ctx.storage().get_block(height).map_err(|err| {
                internal_error(format!(
                    "failed to load indexed block at height {height}: {err}"
                ))
            })?;

            Ok(block.map(block_to_rpc))
        }
        (Some(_), Some(_)) => Err(invalid_params(
            "get_block expects exactly one selector: height or hash",
        )),
        (None, None) => Err(invalid_params(
            "get_block requires one selector: height or hash",
        )),
    }
}

fn get_transaction(ctx: &RpcState, hash_hex: &str) -> RpcResult<Option<RpcTransactionResponse>> {
    let hash_bytes = decode_hex(hash_hex, "hash")?;
    let tx = ctx
        .storage()
        .get_transaction(&hash_bytes)
        .map_err(|err| internal_error(format!("failed to load transaction: {err}")))?;

    Ok(tx.map(|transaction| RpcTransactionResponse {
        hash: encode_hex(&hash_transaction(&transaction)),
        transaction: transaction_to_rpc(&transaction),
    }))
}

async fn send_transaction(ctx: &RpcState, hex_tx: &str) -> RpcResult<RpcSendTransactionResponse> {
    let tx_bytes = decode_hex(hex_tx, "hex_tx")?;
    let transaction: Transaction = bincode::deserialize(&tx_bytes).map_err(|err| {
        invalid_params(format!("hex_tx is not a valid bincode transaction: {err}"))
    })?;

    validate_transaction_size(&transaction)
        .map_err(|err| validation_error(format!("transaction rejected: {err}")))?;

    let sender_public_key: [u8; 32] = transaction.from.as_slice().try_into().map_err(|_| {
        validation_error("transaction rejected: sender public key must be 32 bytes")
    })?;

    let signature_is_valid =
        verify_transaction(&transaction, &sender_public_key).map_err(|err| {
            validation_error(format!("transaction rejected: invalid signature: {err}"))
        })?;

    if !signature_is_valid {
        return Err(validation_error("transaction rejected: invalid signature"));
    }

    let sender_account = ctx
        .storage()
        .get_account(&transaction.from)
        .map_err(|err| internal_error(format!("failed to load sender account: {err}")))?;

    validate_transaction_nonce(&transaction, sender_account.as_ref())
        .map_err(|err| validation_error(format!("transaction rejected: {err}")))?;

    match transaction.tx_type {
        TransactionType::Transfer
        | TransactionType::Stake { .. }
        | TransactionType::PublicToAnon
        | TransactionType::AnonToPublic => {
            validate_transaction_balance(&transaction, sender_account.as_ref())
                .map_err(|err| validation_error(format!("transaction rejected: {err}")))?;
        }
        TransactionType::DeployContract => {
            if transaction.fee != FIXED_TRANSACTION_FEE {
                return Err(validation_error(format!(
                    "transaction rejected: invalid fee (expected {}, received {})",
                    FIXED_TRANSACTION_FEE, transaction.fee
                )));
            }

            let balance = sender_account
                .as_ref()
                .map(|account| account.balance)
                .unwrap_or(0);
            if balance < transaction.fee {
                return Err(validation_error(format!(
                    "transaction rejected: insufficient balance for fee (balance: {balance}, required: {})",
                    transaction.fee
                )));
            }

            let code_wasm = transaction.data.as_ref().ok_or_else(|| {
                validation_error(
                    "transaction rejected: deploy contract requires wasm bytecode in data",
                )
            })?;
            if code_wasm.is_empty() {
                return Err(validation_error(
                    "transaction rejected: deploy contract bytecode must not be empty",
                ));
            }

            if transaction.amount != 0 {
                return Err(validation_error(format!(
                    "transaction rejected: deploy contract amount must be 0 (received {})",
                    transaction.amount
                )));
            }

            if transaction.to.is_empty() {
                return Err(validation_error(
                    "transaction rejected: deploy contract target address must not be empty",
                ));
            }
        }
        TransactionType::CallContract => {
            if transaction.fee != FIXED_TRANSACTION_FEE {
                return Err(validation_error(format!(
                    "transaction rejected: invalid fee (expected {}, received {})",
                    FIXED_TRANSACTION_FEE, transaction.fee
                )));
            }

            let balance = sender_account
                .as_ref()
                .map(|account| account.balance)
                .unwrap_or(0);
            if balance < transaction.fee {
                return Err(validation_error(format!(
                    "transaction rejected: insufficient balance for fee (balance: {balance}, required: {})",
                    transaction.fee
                )));
            }

            if transaction.amount != 0 {
                return Err(validation_error(format!(
                    "transaction rejected: call contract amount must be 0 (received {})",
                    transaction.amount
                )));
            }

            if transaction.to.is_empty() {
                return Err(validation_error(
                    "transaction rejected: call contract target address must not be empty",
                ));
            }

            let payload_bytes = transaction.data.as_ref().ok_or_else(|| {
                validation_error(
                    "transaction rejected: call contract requires serialized payload in data",
                )
            })?;

            let payload: ContractCallPayload =
                bincode::deserialize(payload_bytes).map_err(|err| {
                    validation_error(format!(
                        "transaction rejected: invalid call contract payload: {err}"
                    ))
                })?;

            if payload.method.trim().is_empty() {
                return Err(validation_error(
                    "transaction rejected: call contract method must not be empty",
                ));
            }

            if payload.gas_limit == 0 {
                return Err(validation_error(
                    "transaction rejected: call contract gas_limit must be strictly positive",
                ));
            }

            let context = WasmExecutionContext {
                contract_address: transaction.to.clone(),
                caller: transaction.from.clone(),
                block_height: 0,
                shard_id: transaction.destination_shard,
            };

            WasmVM::execute_contract_call(
                Arc::clone(&ctx.storage),
                context,
                &payload.method,
                &payload.params,
                payload.gas_limit,
            )
            .map_err(|err| {
                validation_error(format!(
                    "transaction rejected: call contract execution failed: {err}"
                ))
            })?;
        }
        TransactionType::Unstake => {
            if transaction.fee != FIXED_TRANSACTION_FEE {
                return Err(validation_error(format!(
                    "transaction rejected: invalid fee (expected {}, received {})",
                    FIXED_TRANSACTION_FEE, transaction.fee
                )));
            }

            let balance = sender_account
                .as_ref()
                .map(|account| account.balance)
                .unwrap_or(0);
            if balance < transaction.fee {
                return Err(validation_error(format!(
                    "transaction rejected: insufficient balance for fee (balance: {balance}, required: {})",
                    transaction.fee
                )));
            }
        }
    }

    let tx_hash = hash_transaction(&transaction);
    ctx.mempool().add_transaction_async(transaction).await;
    let mempool_size = ctx.mempool().len_async().await;

    Ok(RpcSendTransactionResponse {
        accepted: true,
        tx_hash: encode_hex(&tx_hash),
        mempool_size,
    })
}

fn get_balance(ctx: &RpcState, address_hex: &str) -> RpcResult<RpcBalanceResponse> {
    let address = decode_hex(address_hex, "address")?;
    let account = ctx
        .storage()
        .get_account(&address)
        .map_err(|err| internal_error(format!("failed to load account: {err}")))?;

    match account {
        Some(account) => Ok(account_to_rpc(&address, &account)),
        None => Ok(RpcBalanceResponse {
            address: encode_hex(&address),
            found: false,
            balance: 0,
            nonce: 0,
            note_commitment_count: 0,
        }),
    }
}

async fn get_info(ctx: &RpcState) -> RpcResult<RpcInfoResponse> {
    let latest_block_height = latest_block_height(ctx.storage())?;
    let validators = load_validators(ctx.storage())?;
    let note_commitments = load_note_commitments(ctx.storage())?;

    Ok(RpcInfoResponse {
        name: "Prims".to_string(),
        rpc_version: "0.1".to_string(),
        storage_backend: ctx.storage().kind().to_string(),
        latest_block_height,
        mempool_size: ctx.mempool().len_async().await,
        validator_count: validators.len(),
        note_commitment_count: note_commitments.len(),
    })
}

fn get_validators(ctx: &RpcState) -> RpcResult<Vec<RpcValidatorView>> {
    let mut validators = load_validators(ctx.storage())?;
    validators.sort_by(|left, right| left.address.cmp(&right.address));

    Ok(validators.iter().map(validator_to_rpc).collect())
}

fn get_note_commitments(ctx: &RpcState) -> RpcResult<Vec<String>> {
    load_note_commitments(ctx.storage())
}

fn block_to_rpc(block: Block) -> RpcBlockResponse {
    RpcBlockResponse {
        hash: encode_hex(&hash_block_header(&block.header)),
        header: RpcBlockHeaderView {
            version: block.header.version,
            previous_hash: encode_hex(&block.header.previous_hash),
            merkle_root: encode_hex(&block.header.merkle_root),
            timestamp: block.header.timestamp,
            height: block.header.height,
            validator: encode_hex(&block.header.validator),
            signature: encode_hex(&block.header.signature),
        },
        transactions: block.transactions.iter().map(transaction_to_rpc).collect(),
        receipts: block.receipts.iter().map(receipt_to_rpc).collect(),
    }
}

fn transaction_to_rpc(transaction: &Transaction) -> RpcTransactionView {
    let (tx_type, stake_duration) = transaction_type_parts(&transaction.tx_type);

    RpcTransactionView {
        tx_type,
        stake_duration,
        from: encode_hex(&transaction.from),
        to: encode_hex(&transaction.to),
        amount: transaction.amount,
        fee: transaction.fee,
        nonce: transaction.nonce,
        source_shard: transaction.source_shard,
        destination_shard: transaction.destination_shard,
        signature: encode_hex(&transaction.signature),
        data: transaction.data.as_ref().map(|data| encode_hex(data)),
    }
}

fn receipt_to_rpc(receipt: &CrossShardReceipt) -> RpcCrossShardReceiptView {
    RpcCrossShardReceiptView {
        tx_hash: encode_hex(&receipt.tx_hash),
        source_shard: receipt.source_shard,
        destination_shard: receipt.destination_shard,
        phase: format!("{:?}", receipt.phase),
        proof: encode_hex(&receipt.proof),
    }
}

fn account_to_rpc(address: &[u8], account: &Account) -> RpcBalanceResponse {
    RpcBalanceResponse {
        address: encode_hex(address),
        found: true,
        balance: account.balance,
        nonce: account.nonce,
        note_commitment_count: account.anonymous_state.note_commitments.len(),
    }
}

fn validator_to_rpc(validator: &Validator) -> RpcValidatorView {
    RpcValidatorView {
        address: encode_hex(&validator.address),
        stake: validator.stake,
        locked_until: validator.locked_until,
    }
}

fn load_validators(storage: &RocksDbStorage) -> RpcResult<Vec<Validator>> {
    let entries = storage
        .iter(keys::STAKE_PREFIX.as_bytes())
        .map_err(|err| internal_error(format!("failed to iterate validators: {err}")))?;

    let mut validators = Vec::with_capacity(entries.len());
    for (_, value) in entries {
        let validator: Validator = bincode::deserialize(&value)
            .map_err(|err| internal_error(format!("failed to decode validator entry: {err}")))?;
        validators.push(validator);
    }

    Ok(validators)
}

fn load_note_commitments(storage: &RocksDbStorage) -> RpcResult<Vec<String>> {
    let entries = storage
        .iter(keys::ANON_NOTE_PREFIX.as_bytes())
        .map_err(|err| internal_error(format!("failed to iterate note commitments: {err}")))?;

    let mut commitments = Vec::with_capacity(entries.len());
    for (key, _) in entries {
        let Some(commitment) = key.strip_prefix(keys::ANON_NOTE_PREFIX.as_bytes()) else {
            continue;
        };
        commitments.push(encode_hex(commitment));
    }

    commitments.sort();
    commitments.dedup();
    Ok(commitments)
}

fn latest_block_height(storage: &RocksDbStorage) -> RpcResult<Option<u64>> {
    let entries = storage
        .iter(keys::BLOCK_PREFIX.as_bytes())
        .map_err(|err| internal_error(format!("failed to iterate blocks: {err}")))?;

    let mut latest: Option<u64> = None;

    for (key, _) in entries {
        if let Some(height) = parse_block_height_key(&key) {
            latest = Some(match latest {
                Some(current) => current.max(height),
                None => height,
            });
        }
    }

    Ok(latest)
}

fn parse_block_height_key(key: &[u8]) -> Option<u64> {
    let text = str::from_utf8(key).ok()?;
    let raw_height = text.strip_prefix(keys::BLOCK_PREFIX)?;
    raw_height.parse::<u64>().ok()
}

fn transaction_type_parts(tx_type: &TransactionType) -> (String, Option<u64>) {
    match tx_type {
        TransactionType::Transfer => ("Transfer".to_string(), None),
        TransactionType::Stake { duration } => ("Stake".to_string(), Some(*duration)),
        TransactionType::Unstake => ("Unstake".to_string(), None),
        TransactionType::PublicToAnon => ("PublicToAnon".to_string(), None),
        TransactionType::AnonToPublic => ("AnonToPublic".to_string(), None),
        TransactionType::DeployContract => ("DeployContract".to_string(), None),
        TransactionType::CallContract => ("CallContract".to_string(), None),
    }
}

fn decode_hex(value: &str, field_name: &str) -> RpcResult<Vec<u8>> {
    let trimmed = value.trim();
    let normalized = trimmed.strip_prefix("0x").unwrap_or(trimmed);

    hex::decode(normalized)
        .map_err(|err| invalid_params(format!("invalid {field_name} hex: {err}")))
}

fn encode_hex(bytes: &[u8]) -> String {
    hex::encode(bytes)
}

fn rate_limit_exceeded() -> ErrorObjectOwned {
    ErrorObjectOwned::owned(RATE_LIMIT_EXCEEDED_CODE, "rate limit exceeded", None::<()>)
}

fn invalid_params(message: impl Into<String>) -> ErrorObjectOwned {
    ErrorObjectOwned::owned(INVALID_PARAMS_CODE, message.into(), Option::<()>::None)
}

fn internal_error(message: impl Into<String>) -> ErrorObjectOwned {
    ErrorObjectOwned::owned(INTERNAL_ERROR_CODE, message.into(), Option::<()>::None)
}

fn validation_error(message: impl Into<String>) -> ErrorObjectOwned {
    ErrorObjectOwned::owned(VALIDATION_ERROR_CODE, message.into(), Option::<()>::None)
}

#[allow(dead_code)]
fn not_found(message: impl Into<String>) -> ErrorObjectOwned {
    ErrorObjectOwned::owned(NOT_FOUND_CODE, message.into(), Option::<()>::None)
}
