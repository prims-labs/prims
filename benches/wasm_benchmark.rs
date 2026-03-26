use criterion::{Criterion, black_box, criterion_group, criterion_main};
use prims::blockchain::types::{Account, AnonymousAccountState, Contract, DEFAULT_SHARD_ID};
use prims::storage::RocksDbStorage;
use prims::vm::{DEFAULT_WASM_FUEL_LIMIT, WasmExecutionContext, WasmVM};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

fn temp_path(label: &str) -> std::path::PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after unix epoch")
        .as_nanos();

    std::env::temp_dir().join(format!("prims-bench-{label}-{unique}"))
}

fn bench_execute_simple_contract_call(c: &mut Criterion) {
    let path = temp_path("wasm-exec");
    let storage = Arc::new(RocksDbStorage::open(&path).expect("open storage"));

    let contract_address = vec![0x88; 32];
    let caller_address = vec![0x11; 32];

    let contract_account = Account {
        balance: 0,
        nonce: 0,
        code_hash: Some(vec![0xAB; 32]),
        anonymous_state: AnonymousAccountState::default(),
    };

    let contract = Contract {
        code_wasm: r#"
            (module
              (import "prims" "set_storage" (func $set_storage (param i32 i32 i32 i32)))
              (memory (export "memory") 1)
              (data (i32.const 0) "counter")
              (data (i32.const 16) "1")
              (func (export "increment")
                i32.const 0
                i32.const 7
                i32.const 16
                i32.const 1
                call $set_storage))
        "#
        .as_bytes()
        .to_vec(),
        storage_root: vec![0; 32],
    };

    storage
        .update_account(&contract_address, &contract_account)
        .expect("seed contract account");
    storage
        .update_contract(&contract_address, &contract)
        .expect("seed contract metadata");

    let context = WasmExecutionContext {
        contract_address: contract_address.clone(),
        caller: caller_address,
        block_height: 1,
        shard_id: DEFAULT_SHARD_ID,
    };

    c.bench_function("wasm_execute_simple_contract_call", |b| {
        b.iter(|| {
            let outcome = WasmVM::execute_contract_call(
                Arc::clone(&storage),
                context.clone(),
                "increment",
                br#"{"delta":1}"#,
                DEFAULT_WASM_FUEL_LIMIT,
            )
            .expect("simple wasm contract call should succeed");

            black_box(outcome);
        });
    });

    drop(storage);
    std::fs::remove_dir_all(&path).ok();
}

criterion_group! {
    name = wasm_benches;
    config = Criterion::default()
        .sample_size(20)
        .warm_up_time(Duration::from_secs(1))
        .measurement_time(Duration::from_secs(5));
    targets = bench_execute_simple_contract_call
}
criterion_main!(wasm_benches);
