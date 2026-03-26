use ed25519_dalek::SigningKey;
use jsonrpsee::server::{ServerBuilder, ServerHandle};
use prims::{
    api::{RpcState, build_rpc_module},
    blockchain::{
        hash::derive_contract_address,
        types::{Account, AnonymousAccountState, Contract, Validator},
    },
    consensus::Mempool,
    storage::RocksDbStorage,
};
use serde_json::{Value, json};
use std::{
    process::{Command, Output},
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};
use tokio::runtime::Runtime;

struct TestRpcServer {
    _runtime: Runtime,
    _server_handle: ServerHandle,
    url: String,
    storage: Arc<RocksDbStorage>,
    mempool: Arc<Mempool>,
    _db_path: std::path::PathBuf,
}

fn temp_db_path(label: &str) -> std::path::PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after unix epoch")
        .as_nanos();

    std::env::temp_dir().join(format!("prims-cli-integration-{label}-{unique}"))
}

fn start_test_rpc_server(label: &str) -> TestRpcServer {
    let db_path = temp_db_path(label);
    let storage = Arc::new(RocksDbStorage::open(&db_path).expect("open rocksdb"));
    let mempool = Arc::new(Mempool::new());
    let runtime = Runtime::new().expect("build multi-thread runtime");

    let (url, server_handle) = runtime.block_on(async {
        let module = build_rpc_module(RpcState::new(Arc::clone(&storage), Arc::clone(&mempool)))
            .expect("build rpc module");

        let server = ServerBuilder::default()
            .build("127.0.0.1:0")
            .await
            .expect("build rpc server");

        let local_addr = server.local_addr().expect("get rpc local addr");
        let handle = server.start(module);

        (format!("http://{}", local_addr), handle)
    });

    TestRpcServer {
        _runtime: runtime,
        _server_handle: server_handle,
        url,
        storage,
        mempool,
        _db_path: db_path,
    }
}

fn cli_bin() -> &'static str {
    env!("CARGO_BIN_EXE_prims-cli")
}

fn run_cli(args: &[&str], envs: &[(&str, String)]) -> Output {
    let mut command = Command::new(cli_bin());
    command.current_dir(env!("CARGO_MANIFEST_DIR"));
    command.args(args);

    for (key, value) in envs {
        command.env(key, value);
    }

    command.output().expect("run prims-cli binary")
}

fn assert_success(output: &Output) {
    assert!(
        output.status.success(),
        "prims-cli failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}

fn parse_stdout_json(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout).expect("decode prims-cli stdout as JSON")
}

#[test]
fn balance_command_returns_seeded_account() {
    let server = start_test_rpc_server("balance");
    let address = vec![0xAA; 32];

    let account = Account {
        balance: 1_234,
        nonce: 7,
        code_hash: None,
        anonymous_state: AnonymousAccountState::default(),
    };

    server
        .storage
        .update_account(&address, &account)
        .expect("seed account");

    let address_hex = hex::encode(&address);
    let envs = [("PRIMS_RPC_URL", server.url.clone())];
    let args = ["balance", address_hex.as_str()];

    let output = run_cli(&args, &envs);
    assert_success(&output);

    let payload = parse_stdout_json(&output);
    assert_eq!(payload["address"], json!(address_hex));
    assert_eq!(payload["found"], json!(true));
    assert_eq!(payload["balance"], json!(1_234));
    assert_eq!(payload["nonce"], json!(7));
    assert_eq!(payload["note_commitment_count"], json!(0));
}

#[test]
fn list_validators_command_returns_seeded_validator() {
    let server = start_test_rpc_server("list-validators");

    let validator = Validator {
        address: vec![0x44; 32],
        stake: 50_000,
        locked_until: 1_710_086_400,
    };

    server
        .storage
        .save_validator(&validator)
        .expect("seed validator");

    let envs = [("PRIMS_RPC_URL", server.url.clone())];
    let output = run_cli(&["list-validators"], &envs);
    assert_success(&output);

    let payload = parse_stdout_json(&output);
    let validators = payload.as_array().expect("validators array");

    assert_eq!(validators.len(), 1);
    assert_eq!(
        validators[0]["address"],
        json!(hex::encode(&validator.address))
    );
    assert_eq!(validators[0]["stake"], json!(50_000));
    assert_eq!(validators[0]["locked_until"], json!(1_710_086_400u64));
}

#[test]
fn send_command_submits_transfer_transaction() {
    let server = start_test_rpc_server("send");
    let secret_key = [0x11; 32];
    let signing_key = SigningKey::from_bytes(&secret_key);
    let public_key = signing_key.verifying_key().to_bytes();

    let account = Account {
        balance: 10_000,
        nonce: 0,
        code_hash: None,
        anonymous_state: AnonymousAccountState::default(),
    };

    server
        .storage
        .update_account(&public_key, &account)
        .expect("seed sender account");

    let recipient_hex = hex::encode([0x22; 32]);
    let secret_key_hex = hex::encode(secret_key);

    let envs = [
        ("PRIMS_RPC_URL", server.url.clone()),
        ("PRIMS_SECRET_KEY_HEX", secret_key_hex),
    ];
    let args = [
        "send",
        recipient_hex.as_str(),
        "100",
        "--source-shard",
        "0",
        "--destination-shard",
        "0",
    ];

    let output = run_cli(&args, &envs);
    assert_success(&output);

    let payload = parse_stdout_json(&output);
    assert_eq!(payload["rpc_url"], json!(server.url));
    assert_eq!(payload["from"], json!(hex::encode(public_key)));
    assert_eq!(payload["to"], json!(recipient_hex));
    assert_eq!(payload["tx_type"], json!("Transfer"));
    assert_eq!(payload["amount"], json!(100));
    assert_eq!(payload["nonce"], json!(1));
    assert_eq!(payload["accepted"], json!(true));
    assert_eq!(payload["mempool_size"], json!(1));

    let mempool = Arc::clone(&server.mempool);
    let mempool_len = server
        ._runtime
        .block_on(async move { mempool.len_async().await });
    assert_eq!(mempool_len, 1);
}

#[test]
fn stake_command_submits_stake_transaction() {
    let server = start_test_rpc_server("stake");
    let secret_key = [0x33; 32];
    let signing_key = SigningKey::from_bytes(&secret_key);
    let public_key = signing_key.verifying_key().to_bytes();

    let account = Account {
        balance: 20_000,
        nonce: 0,
        code_hash: None,
        anonymous_state: AnonymousAccountState::default(),
    };

    server
        .storage
        .update_account(&public_key, &account)
        .expect("seed staker account");

    let secret_key_hex = hex::encode(secret_key);
    let envs = [
        ("PRIMS_RPC_URL", server.url.clone()),
        ("PRIMS_SECRET_KEY_HEX", secret_key_hex),
    ];
    let args = ["stake", "500", "--duration", "3600"];

    let output = run_cli(&args, &envs);
    assert_success(&output);

    let payload = parse_stdout_json(&output);
    assert_eq!(payload["rpc_url"], json!(server.url));
    assert_eq!(payload["from"], json!(hex::encode(public_key)));
    assert_eq!(payload["to"], json!(hex::encode(public_key)));
    assert_eq!(payload["tx_type"], json!("Stake"));
    assert_eq!(payload["stake_duration"], json!(3600));
    assert_eq!(payload["amount"], json!(500));
    assert_eq!(payload["nonce"], json!(1));
    assert_eq!(payload["accepted"], json!(true));
    assert_eq!(payload["mempool_size"], json!(1));
}

#[test]
fn create_contract_command_submits_deploy_contract_transaction() {
    let server = start_test_rpc_server("create-contract");
    let secret_key = [0x66; 32];
    let signing_key = SigningKey::from_bytes(&secret_key);
    let public_key = signing_key.verifying_key().to_bytes();

    let account = Account {
        balance: 20_000,
        nonce: 0,
        code_hash: None,
        anonymous_state: AnonymousAccountState::default(),
    };

    server
        .storage
        .update_account(&public_key, &account)
        .expect("seed deployer account");

    let wasm_bytes = b"\0asm\x01\0\0\0".to_vec();
    let wasm_path = std::env::temp_dir().join(format!(
        "prims-cli-contract-{}.wasm",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after unix epoch")
            .as_nanos()
    ));
    std::fs::write(&wasm_path, &wasm_bytes).expect("write temp wasm file");

    let secret_key_hex = hex::encode(secret_key);
    let envs = [
        ("PRIMS_RPC_URL", server.url.clone()),
        ("PRIMS_SECRET_KEY_HEX", secret_key_hex),
    ];
    let wasm_path_string = wasm_path.to_string_lossy().to_string();

    let output = run_cli(&["create-contract", wasm_path_string.as_str()], &envs);
    assert_success(&output);

    let payload = parse_stdout_json(&output);
    let expected_address = hex::encode(derive_contract_address(&public_key, &wasm_bytes));

    assert_eq!(payload["rpc_url"], json!(server.url));
    assert_eq!(payload["from"], json!(hex::encode(public_key)));
    assert_eq!(payload["contract_address"], json!(expected_address));
    assert_eq!(payload["tx_type"], json!("DeployContract"));
    assert_eq!(payload["amount"], json!(0));
    assert_eq!(payload["nonce"], json!(1));
    assert_eq!(payload["accepted"], json!(true));
    assert_eq!(payload["mempool_size"], json!(1));
    assert_eq!(payload["wasm_file"], json!(wasm_path_string));
    assert_eq!(payload["wasm_size"], json!(wasm_bytes.len()));

    let mempool = Arc::clone(&server.mempool);
    let mempool_len = server
        ._runtime
        .block_on(async move { mempool.len_async().await });
    assert_eq!(mempool_len, 1);

    std::fs::remove_file(&wasm_path).ok();
}

#[test]
fn call_contract_command_submits_call_contract_transaction() {
    let server = start_test_rpc_server("call-contract");
    let secret_key = [0x77; 32];
    let signing_key = SigningKey::from_bytes(&secret_key);
    let public_key = signing_key.verifying_key().to_bytes();

    let account = Account {
        balance: 20_000,
        nonce: 0,
        code_hash: None,
        anonymous_state: AnonymousAccountState::default(),
    };

    server
        .storage
        .update_account(&public_key, &account)
        .expect("seed caller account");

    let contract_address = vec![0x88; 32];
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

    server
        .storage
        .update_account(&contract_address, &contract_account)
        .expect("seed contract account");
    server
        .storage
        .update_contract(&contract_address, &contract)
        .expect("seed contract metadata");

    let contract_address_hex = hex::encode(&contract_address);
    let secret_key_hex = hex::encode(secret_key);

    let envs = [
        ("PRIMS_RPC_URL", server.url.clone()),
        ("PRIMS_SECRET_KEY_HEX", secret_key_hex),
    ];
    let args = [
        "call-contract",
        contract_address_hex.as_str(),
        "increment",
        r#"{"delta":1}"#,
        "--gas-limit",
        "50000",
    ];

    let output = run_cli(&args, &envs);
    assert_success(&output);

    let payload = parse_stdout_json(&output);
    assert_eq!(payload["rpc_url"], json!(server.url));
    assert_eq!(payload["from"], json!(hex::encode(public_key)));
    assert_eq!(payload["contract_address"], json!(contract_address_hex));
    assert_eq!(payload["tx_type"], json!("CallContract"));
    assert_eq!(payload["method"], json!("increment"));
    assert_eq!(payload["params"], json!({"delta": 1}));
    assert_eq!(payload["gas_limit"], json!(50_000));
    assert_eq!(payload["amount"], json!(0));
    assert_eq!(payload["nonce"], json!(1));
    assert_eq!(payload["accepted"], json!(true));
    assert_eq!(payload["mempool_size"], json!(1));

    let mempool = Arc::clone(&server.mempool);
    let mempool_len = server
        ._runtime
        .block_on(async move { mempool.len_async().await });
    assert_eq!(mempool_len, 1);
    assert_eq!(
        server
            .storage
            .get_contract_storage(&contract_address, b"counter")
            .expect("read contract storage after cli call"),
        Some(b"1".to_vec())
    );
}

#[test]
fn call_contract_command_reports_execution_trap_and_rolls_back_storage() {
    let server = start_test_rpc_server("call-contract-trap");
    let secret_key = [0x66; 32];
    let signing_key = SigningKey::from_bytes(&secret_key);
    let public_key = signing_key.verifying_key().to_bytes();

    let account = Account {
        balance: 20_000,
        nonce: 0,
        code_hash: None,
        anonymous_state: AnonymousAccountState::default(),
    };

    server
        .storage
        .update_account(&public_key, &account)
        .expect("seed caller account");

    let contract_address = vec![0x89; 32];
    let contract_account = Account {
        balance: 0,
        nonce: 0,
        code_hash: Some(vec![0xBC; 32]),
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

    server
        .storage
        .update_account(&contract_address, &contract_account)
        .expect("seed contract account");
    server
        .storage
        .update_contract(&contract_address, &contract)
        .expect("seed contract metadata");

    let contract_address_hex = hex::encode(&contract_address);
    let secret_key_hex = hex::encode(secret_key);

    let envs = [
        ("PRIMS_RPC_URL", server.url.clone()),
        ("PRIMS_SECRET_KEY_HEX", secret_key_hex),
    ];
    let args = [
        "call-contract",
        contract_address_hex.as_str(),
        "increment_then_trap",
        r#"{"delta":1}"#,
        "--gas-limit",
        "50000",
    ];

    let output = run_cli(&args, &envs);
    assert!(
        !output.status.success(),
        "prims-cli aurait dû échouer\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("RPC error -32010: transaction rejected: call contract execution failed:"),
        "stderr inattendu:\n{stderr}"
    );

    let mempool = Arc::clone(&server.mempool);
    let mempool_len = server
        ._runtime
        .block_on(async move { mempool.len_async().await });
    assert_eq!(mempool_len, 0);

    assert_eq!(
        server
            .storage
            .get_contract_storage(&contract_address, b"counter")
            .expect("read contract storage after failed cli call"),
        None
    );
}

#[test]
fn create_contract_then_call_token_contract_updates_caller_balance_storage() {
    let server = start_test_rpc_server("create-call-token-balance");
    let secret_key = [0x7A; 32];
    let signing_key = SigningKey::from_bytes(&secret_key);
    let public_key = signing_key.verifying_key().to_bytes();

    let account = Account {
        balance: 20_000,
        nonce: 0,
        code_hash: None,
        anonymous_state: AnonymousAccountState::default(),
    };

    server
        .storage
        .update_account(&public_key, &account)
        .expect("seed token deployer account");

    let wat = r#"
        (module
          (import "prims" "get_caller_len" (func $get_caller_len (result i32)))
          (import "prims" "copy_caller" (func $copy_caller (param i32) (result i32)))
          (import "prims" "get_params_len" (func $get_params_len (result i32)))
          (import "prims" "copy_params" (func $copy_params (param i32) (result i32)))
          (import "prims" "set_storage" (func $set_storage (param i32 i32 i32 i32)))
          (memory (export "memory") 1)
          (func (export "mint")
            (local $caller_len i32)
            (local $params_len i32)
            call $get_caller_len
            local.set $caller_len
            call $get_params_len
            local.set $params_len
            i32.const 0
            call $copy_caller
            drop
            i32.const 128
            call $copy_params
            drop
            i32.const 0
            local.get $caller_len
            i32.const 128
            local.get $params_len
            call $set_storage))
    "#;

    let wasm_bytes = wat.as_bytes().to_vec();
    let wasm_path = std::env::temp_dir().join(format!(
        "prims-cli-token-contract-{}.wasm",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after unix epoch")
            .as_nanos()
    ));
    std::fs::write(&wasm_path, &wasm_bytes).expect("write temp token contract file");

    let secret_key_hex = hex::encode(secret_key);
    let envs = [
        ("PRIMS_RPC_URL", server.url.clone()),
        ("PRIMS_SECRET_KEY_HEX", secret_key_hex),
    ];
    let wasm_path_string = wasm_path.to_string_lossy().to_string();

    let create_output = run_cli(&["create-contract", wasm_path_string.as_str()], &envs);
    assert_success(&create_output);

    let create_payload = parse_stdout_json(&create_output);
    let contract_address = derive_contract_address(&public_key, &wasm_bytes);
    let contract_address_hex = hex::encode(&contract_address);

    assert_eq!(create_payload["rpc_url"], json!(server.url));
    assert_eq!(create_payload["from"], json!(hex::encode(public_key)));
    assert_eq!(
        create_payload["contract_address"],
        json!(contract_address_hex)
    );
    assert_eq!(create_payload["tx_type"], json!("DeployContract"));
    assert_eq!(create_payload["nonce"], json!(1));
    assert_eq!(create_payload["accepted"], json!(true));
    assert_eq!(create_payload["mempool_size"], json!(1));

    let deployed_address = server
        .storage
        .deploy_contract(&public_key, &wasm_bytes)
        .expect("materialize deploy contract transaction in storage");
    assert_eq!(deployed_address, contract_address);

    let mut sender_after_deploy = server
        .storage
        .get_account(&public_key)
        .expect("read deployer account after deploy")
        .expect("deployer account should exist");
    sender_after_deploy.nonce = 1;
    server
        .storage
        .update_account(&public_key, &sender_after_deploy)
        .expect("persist deployer nonce after deploy");

    let call_output = run_cli(
        &[
            "call-contract",
            contract_address_hex.as_str(),
            "mint",
            "1000",
            "--gas-limit",
            "50000",
        ],
        &envs,
    );
    assert_success(&call_output);

    let call_payload = parse_stdout_json(&call_output);
    assert_eq!(call_payload["rpc_url"], json!(server.url));
    assert_eq!(call_payload["from"], json!(hex::encode(public_key)));
    assert_eq!(
        call_payload["contract_address"],
        json!(contract_address_hex)
    );
    assert_eq!(call_payload["tx_type"], json!("CallContract"));
    assert_eq!(call_payload["method"], json!("mint"));
    assert_eq!(call_payload["params"], json!(1000));
    assert_eq!(call_payload["gas_limit"], json!(50_000));
    assert_eq!(call_payload["nonce"], json!(2));
    assert_eq!(call_payload["accepted"], json!(true));
    assert_eq!(call_payload["mempool_size"], json!(2));

    let mempool = Arc::clone(&server.mempool);
    let mempool_len = server
        ._runtime
        .block_on(async move { mempool.len_async().await });
    assert_eq!(mempool_len, 2);

    assert_eq!(
        server
            .storage
            .get_contract_storage(&contract_address, &public_key)
            .expect("read token balance storage after contract call"),
        Some(b"1000".to_vec())
    );

    std::fs::remove_file(&wasm_path).ok();
}

#[test]
fn unstake_command_submits_unstake_transaction() {
    let server = start_test_rpc_server("unstake");
    let secret_key = [0x55; 32];
    let signing_key = SigningKey::from_bytes(&secret_key);
    let public_key = signing_key.verifying_key().to_bytes();

    let account = Account {
        balance: 20_000,
        nonce: 0,
        code_hash: None,
        anonymous_state: AnonymousAccountState::default(),
    };

    server
        .storage
        .update_account(&public_key, &account)
        .expect("seed unstake account");

    let secret_key_hex = hex::encode(secret_key);
    let envs = [
        ("PRIMS_RPC_URL", server.url.clone()),
        ("PRIMS_SECRET_KEY_HEX", secret_key_hex),
    ];

    let output = run_cli(&["unstake"], &envs);
    assert_success(&output);

    let payload = parse_stdout_json(&output);
    assert_eq!(payload["rpc_url"], json!(server.url));
    assert_eq!(payload["from"], json!(hex::encode(public_key)));
    assert_eq!(payload["to"], json!(hex::encode(public_key)));
    assert_eq!(payload["tx_type"], json!("Unstake"));
    assert_eq!(payload["stake_duration"], json!(null));
    assert_eq!(payload["amount"], json!(0));
    assert_eq!(payload["nonce"], json!(1));
    assert_eq!(payload["accepted"], json!(true));
    assert_eq!(payload["mempool_size"], json!(1));
}
