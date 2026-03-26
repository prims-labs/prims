use prims::{
    api::{RpcState, build_rpc_module},
    blockchain::types::{
        Account, AnonymousAccountState, Contract, ContractCallPayload, FIXED_TRANSACTION_FEE,
        Transaction, TransactionType,
    },
    consensus::Mempool,
    storage::RocksDbStorage,
};
use serde_json::json;
use std::{
    num::NonZeroU32,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

fn temp_db_path(label: &str) -> std::path::PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after unix epoch")
        .as_nanos();

    std::env::temp_dir().join(format!("prims-rpc-{label}-{unique}"))
}

#[test]
fn send_transaction_accepts_valid_bincode_hex_transaction() {
    let path = temp_db_path("send-transaction-success");
    let storage = Arc::new(RocksDbStorage::open(&path).expect("open rocksdb"));
    let mempool = Arc::new(Mempool::new());

    let sender = vec![0x11; 32];
    let recipient = vec![0x22; 32];

    let account = Account {
        balance: 1_000,
        nonce: 0,
        code_hash: None,
        anonymous_state: AnonymousAccountState::default(),
    };

    storage
        .update_account(&sender, &account)
        .expect("store sender account");

    let tx = Transaction {
        tx_type: TransactionType::Transfer,
        from: sender,
        to: recipient,
        amount: 100,
        fee: FIXED_TRANSACTION_FEE,
        nonce: 1,
        source_shard: 0,
        destination_shard: 0,
        signature: vec![0x33; 64],
        data: Some(b"rpc-success".to_vec()),
    };

    let tx_hex = hex::encode(bincode::serialize(&tx).expect("serialize tx"));

    let module = build_rpc_module(RpcState::new(Arc::clone(&storage), Arc::clone(&mempool)))
        .expect("build rpc module");

    let request =
        r#"{"jsonrpc":"2.0","id":1,"method":"send_transaction","params":{"hex_tx":"__HEX_TX__"}}"#
            .replace("__HEX_TX__", &tx_hex);

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("build test runtime");

    let payload: serde_json::Value = runtime.block_on(async {
        let (response, _receiver) = module
            .raw_json_request(&request, 1)
            .await
            .expect("rpc response");

        let payload: serde_json::Value =
            serde_json::from_str(response.get()).expect("decode rpc json payload");

        assert_eq!(payload["jsonrpc"], json!("2.0"));
        assert_eq!(payload["id"], json!(1));
        assert_eq!(payload["result"]["accepted"], json!(true));
        assert_eq!(payload["result"]["mempool_size"], json!(1));

        let returned_hash = payload["result"]["tx_hash"]
            .as_str()
            .expect("tx_hash string");
        assert_eq!(returned_hash.len(), 64);

        assert_eq!(mempool.len_async().await, 1);
        payload
    });

    assert_eq!(payload["result"]["accepted"], json!(true));
}

#[test]
fn send_transaction_accepts_valid_call_contract_transaction() {
    let path = temp_db_path("send-call-contract-success");
    let storage = Arc::new(RocksDbStorage::open(&path).expect("open rocksdb"));
    let mempool = Arc::new(Mempool::new());

    let sender = vec![0x77; 32];
    let contract_address = vec![0x88; 32];

    let account = Account {
        balance: 1_000,
        nonce: 0,
        code_hash: None,
        anonymous_state: AnonymousAccountState::default(),
    };

    let contract_account = Account {
        balance: 0,
        nonce: 0,
        code_hash: Some(vec![0xAB; 32]),
        anonymous_state: AnonymousAccountState::default(),
    };

    let wat = r#"
        (module
          (import "prims" "set_storage" (func $set_storage (param i32 i32 i32 i32)))
          (import "prims" "emit_event" (func $emit_event (param i32 i32 i32 i32)))
          (memory (export "memory") 1)
          (data (i32.const 0) "counter")
          (data (i32.const 16) "1")
          (data (i32.const 32) "Incremented")
          (func (export "increment")
            i32.const 0
            i32.const 7
            i32.const 16
            i32.const 1
            call $set_storage
            i32.const 32
            i32.const 11
            i32.const 16
            i32.const 1
            call $emit_event))
    "#;

    let contract = Contract {
        code_wasm: wat.as_bytes().to_vec(),
        storage_root: vec![0; 32],
    };

    storage
        .update_account(&sender, &account)
        .expect("store sender account");
    storage
        .update_account(&contract_address, &contract_account)
        .expect("store contract account");
    storage
        .update_contract(&contract_address, &contract)
        .expect("store contract metadata");

    let call_payload = ContractCallPayload {
        method: "increment".to_string(),
        params: br#"{"delta":1}"#.to_vec(),
        gas_limit: 50_000,
    };

    let tx = Transaction {
        tx_type: TransactionType::CallContract,
        from: sender,
        to: contract_address.clone(),
        amount: 0,
        fee: FIXED_TRANSACTION_FEE,
        nonce: 1,
        source_shard: 0,
        destination_shard: 0,
        signature: vec![0x44; 64],
        data: Some(bincode::serialize(&call_payload).expect("serialize call contract payload")),
    };

    let tx_hex = hex::encode(bincode::serialize(&tx).expect("serialize tx"));

    let module = build_rpc_module(RpcState::new(Arc::clone(&storage), Arc::clone(&mempool)))
        .expect("build rpc module");

    let request =
        r#"{"jsonrpc":"2.0","id":1,"method":"send_transaction","params":{"hex_tx":"__HEX_TX__"}}"#
            .replace("__HEX_TX__", &tx_hex);

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("build test runtime");

    let response_payload: serde_json::Value = runtime.block_on(async {
        let (response, _receiver) = module
            .raw_json_request(&request, 1)
            .await
            .expect("rpc response");

        let payload: serde_json::Value =
            serde_json::from_str(response.get()).expect("decode rpc json payload");

        assert_eq!(payload["jsonrpc"], json!("2.0"));
        assert_eq!(payload["id"], json!(1));
        assert_eq!(payload["result"]["accepted"], json!(true));
        assert_eq!(payload["result"]["mempool_size"], json!(1));

        let returned_hash = payload["result"]["tx_hash"]
            .as_str()
            .expect("tx_hash string");
        assert_eq!(returned_hash.len(), 64);

        assert_eq!(mempool.len_async().await, 1);
        payload
    });

    assert_eq!(response_payload["result"]["accepted"], json!(true));
    assert_eq!(
        storage
            .get_contract_storage(&contract_address, b"counter")
            .expect("read contract storage after rpc"),
        Some(b"1".to_vec())
    );
}

#[test]
fn send_transaction_rejects_call_contract_when_execution_traps_and_rolls_back_storage() {
    let path = temp_db_path("send-call-contract-trap");
    let storage = Arc::new(RocksDbStorage::open(&path).expect("open rocksdb"));
    let mempool = Arc::new(Mempool::new());

    let sender = vec![0x66; 32];
    let contract_address = vec![0x67; 32];

    let account = Account {
        balance: 1_000,
        nonce: 0,
        code_hash: None,
        anonymous_state: AnonymousAccountState::default(),
    };

    let contract_account = Account {
        balance: 0,
        nonce: 0,
        code_hash: Some(vec![0xCD; 32]),
        anonymous_state: AnonymousAccountState::default(),
    };

    let wat = r#"
        (module
          (import "prims" "set_storage" (func $set_storage (param i32 i32 i32 i32)))
          (memory (export "memory") 1)
          (data (i32.const 0) "counter")
          (data (i32.const 16) "1")
          (func (export "increment_then_trap")
            i32.const 0
            i32.const 7
            i32.const 16
            i32.const 1
            call $set_storage
            unreachable))
    "#;

    let contract = Contract {
        code_wasm: wat.as_bytes().to_vec(),
        storage_root: vec![0; 32],
    };

    storage
        .update_account(&sender, &account)
        .expect("store sender account");
    storage
        .update_account(&contract_address, &contract_account)
        .expect("store contract account");
    storage
        .update_contract(&contract_address, &contract)
        .expect("store contract metadata");

    let call_payload = ContractCallPayload {
        method: "increment_then_trap".to_string(),
        params: br#"{"delta":1}"#.to_vec(),
        gas_limit: 50_000,
    };

    let tx = Transaction {
        tx_type: TransactionType::CallContract,
        from: sender,
        to: contract_address.clone(),
        amount: 0,
        fee: FIXED_TRANSACTION_FEE,
        nonce: 1,
        source_shard: 0,
        destination_shard: 0,
        signature: vec![0x68; 64],
        data: Some(bincode::serialize(&call_payload).expect("serialize call contract payload")),
    };

    let tx_hex = hex::encode(bincode::serialize(&tx).expect("serialize tx"));

    let module = build_rpc_module(RpcState::new(Arc::clone(&storage), Arc::clone(&mempool)))
        .expect("build rpc module");

    let request =
        r#"{"jsonrpc":"2.0","id":1,"method":"send_transaction","params":{"hex_tx":"__HEX_TX__"}}"#
            .replace("__HEX_TX__", &tx_hex);

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("build test runtime");

    runtime.block_on(async {
        let (response, _receiver) = module
            .raw_json_request(&request, 1)
            .await
            .expect("rpc response");

        let payload: serde_json::Value =
            serde_json::from_str(response.get()).expect("decode rpc json payload");

        assert_eq!(payload["jsonrpc"], json!("2.0"));
        assert_eq!(payload["id"], json!(1));
        assert_eq!(payload["error"]["code"], json!(-32010));

        let message = payload["error"]["message"]
            .as_str()
            .expect("rpc error message");
        assert!(
            message.contains("transaction rejected: call contract execution failed:"),
            "unexpected rpc error message: {message}"
        );

        assert_eq!(mempool.len_async().await, 0);
    });

    assert_eq!(
        storage
            .get_contract_storage(&contract_address, b"counter")
            .expect("read contract storage after failed rpc"),
        None
    );
}

#[test]
fn send_transaction_rejects_call_contract_with_zero_gas_limit() {
    let path = temp_db_path("send-call-contract-zero-gas");
    let storage = Arc::new(RocksDbStorage::open(&path).expect("open rocksdb"));
    let mempool = Arc::new(Mempool::new());

    let sender = vec![0x99; 32];
    let contract_address = vec![0xAA; 32];

    let account = Account {
        balance: 1_000,
        nonce: 0,
        code_hash: None,
        anonymous_state: AnonymousAccountState::default(),
    };

    storage
        .update_account(&sender, &account)
        .expect("store sender account");

    let call_payload = ContractCallPayload {
        method: "increment".to_string(),
        params: br#"{"delta":1}"#.to_vec(),
        gas_limit: 0,
    };

    let tx = Transaction {
        tx_type: TransactionType::CallContract,
        from: sender,
        to: contract_address,
        amount: 0,
        fee: FIXED_TRANSACTION_FEE,
        nonce: 1,
        source_shard: 0,
        destination_shard: 0,
        signature: vec![0x55; 64],
        data: Some(bincode::serialize(&call_payload).expect("serialize call contract payload")),
    };

    let tx_hex = hex::encode(bincode::serialize(&tx).expect("serialize tx"));

    let module = build_rpc_module(RpcState::new(Arc::clone(&storage), Arc::clone(&mempool)))
        .expect("build rpc module");

    let request =
        r#"{"jsonrpc":"2.0","id":1,"method":"send_transaction","params":{"hex_tx":"__HEX_TX__"}}"#
            .replace("__HEX_TX__", &tx_hex);

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("build test runtime");

    runtime.block_on(async {
        let (response, _receiver) = module
            .raw_json_request(&request, 1)
            .await
            .expect("rpc response");

        let payload: serde_json::Value =
            serde_json::from_str(response.get()).expect("decode rpc json payload");

        assert_eq!(payload["jsonrpc"], json!("2.0"));
        assert_eq!(payload["id"], json!(1));
        assert_eq!(payload["error"]["code"], json!(-32010));
        assert_eq!(
            payload["error"]["message"],
            json!("transaction rejected: call contract gas_limit must be strictly positive")
        );
        assert_eq!(mempool.len_async().await, 0);
    });
}

#[test]
fn get_balance_omits_sensitive_viewing_hint_metadata() {
    let path = temp_db_path("get-balance-no-viewing-hint");
    let storage = Arc::new(RocksDbStorage::open(&path).expect("open rocksdb"));
    let mempool = Arc::new(Mempool::new());

    let address = vec![0x44; 32];
    let mut anonymous_state = AnonymousAccountState::default();
    anonymous_state.viewing_hint = Some(vec![0x55; 32]);

    let account = Account {
        balance: 777,
        nonce: 9,
        code_hash: None,
        anonymous_state,
    };

    storage
        .update_account(&address, &account)
        .expect("store account with viewing hint");

    let module = build_rpc_module(RpcState::new(Arc::clone(&storage), Arc::clone(&mempool)))
        .expect("build rpc module");

    let request = format!(
        r#"{{"jsonrpc":"2.0","id":1,"method":"get_balance","params":{{"address":"{}"}}}}"#,
        hex::encode(&address)
    );

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("build test runtime");

    let payload: serde_json::Value = runtime.block_on(async {
        let (response, _receiver) = module
            .raw_json_request(&request, request.len())
            .await
            .expect("rpc response");

        let payload: serde_json::Value =
            serde_json::from_str(response.get()).expect("decode rpc json payload");

        assert_eq!(payload["jsonrpc"], json!("2.0"));
        assert_eq!(payload["id"], json!(1));
        assert_eq!(payload["result"]["found"], json!(true));
        assert_eq!(payload["result"]["balance"], json!(777));
        assert_eq!(payload["result"]["nonce"], json!(9));
        assert_eq!(payload["result"]["note_commitment_count"], json!(0));
        assert!(
            payload["result"].get("viewing_hint").is_none(),
            "get_balance must not expose viewing_hint"
        );

        payload
    });

    assert!(payload["result"].get("viewing_hint").is_none());
}

#[test]
fn get_validators_is_rate_limited_after_quota_is_exhausted() {
    let path = temp_db_path("rate-limit");
    let storage = Arc::new(RocksDbStorage::open(&path).expect("open rocksdb"));
    let mempool = Arc::new(Mempool::new());

    let module = build_rpc_module(RpcState::with_rate_limit(
        Arc::clone(&storage),
        Arc::clone(&mempool),
        NonZeroU32::new(1).expect("non-zero rate limit"),
    ))
    .expect("build rpc module");

    let first_request = r#"{"jsonrpc":"2.0","id":1,"method":"get_validators","params":{}}"#;
    let second_request = r#"{"jsonrpc":"2.0","id":2,"method":"get_validators","params":{}}"#;

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("build test runtime");

    runtime.block_on(async {
        let (first_response, _receiver) = module
            .raw_json_request(first_request, first_request.len())
            .await
            .expect("first rpc response");

        let first_payload: serde_json::Value =
            serde_json::from_str(first_response.get()).expect("decode first rpc json payload");

        assert_eq!(first_payload["jsonrpc"], json!("2.0"));
        assert_eq!(first_payload["id"], json!(1));
        assert!(
            first_payload.get("error").is_none(),
            "first request should succeed"
        );
        assert!(
            first_payload.get("result").is_some(),
            "first request should return a result"
        );

        let (second_response, _receiver) = module
            .raw_json_request(second_request, second_request.len())
            .await
            .expect("second rpc response");

        let second_payload: serde_json::Value =
            serde_json::from_str(second_response.get()).expect("decode second rpc json payload");

        assert_eq!(second_payload["jsonrpc"], json!("2.0"));
        assert_eq!(second_payload["id"], json!(2));
        assert_eq!(second_payload["error"]["code"], json!(-32029));
        assert_eq!(
            second_payload["error"]["message"],
            json!("rate limit exceeded")
        );
    });
}
