use criterion::{BatchSize, Criterion, black_box, criterion_group, criterion_main};
use prims::blockchain::hash::calculate_merkle_root;
use prims::blockchain::types::{Block, BlockHeader, Transaction};
use prims::storage::RocksDbStorage;
use std::time::{SystemTime, UNIX_EPOCH};

fn temp_path(label: &str) -> std::path::PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after unix epoch")
        .as_nanos();

    std::env::temp_dir().join(format!("prims-bench-{label}-{unique}"))
}

fn sample_transaction(index: u64) -> Transaction {
    Transaction {
        from: index.to_le_bytes().to_vec(),
        to: (index + 1).to_le_bytes().to_vec(),
        amount: index + 100,
        fee: 1,
        nonce: index,
        signature: vec![index as u8; 64],
        data: Some(format!("tx-{index}").into_bytes()),
    }
}

fn sample_block(height: u64) -> Block {
    let transactions = vec![sample_transaction(height)];
    let merkle_root = calculate_merkle_root(&transactions);

    Block {
        header: BlockHeader {
            version: 1,
            previous_hash: if height == 0 {
                vec![0; 32]
            } else {
                (height - 1).to_le_bytes().repeat(4)
            },
            merkle_root,
            timestamp: 1_710_000_000 + height,
            height,
            validator: vec![7; 32],
            signature: vec![6; 64],
        },
        transactions,
    }
}

fn bench_write_10k_blocks(c: &mut Criterion) {
    c.bench_function("storage_write_10k_blocks", |b| {
        b.iter_batched(
            || {
                let path = temp_path("write");
                let storage = RocksDbStorage::open(&path).expect("open storage");
                (path, storage)
            },
            |(path, storage)| {
                for height in 0..10_000_u64 {
                    let block = sample_block(height);
                    storage.save_block(&block).expect("save block");
                }

                drop(storage);
                std::fs::remove_dir_all(&path).ok();
            },
            BatchSize::SmallInput,
        );
    });
}

fn bench_read_10k_blocks(c: &mut Criterion) {
    c.bench_function("storage_read_10k_blocks", |b| {
        b.iter_batched(
            || {
                let path = temp_path("read");
                let storage = RocksDbStorage::open(&path).expect("open storage");

                for height in 0..10_000_u64 {
                    let block = sample_block(height);
                    storage.save_block(&block).expect("save block");
                }

                (path, storage)
            },
            |(path, storage)| {
                for height in 0..10_000_u64 {
                    let block = storage.get_block(height).expect("read block");
                    black_box(block.expect("block should exist"));
                }

                drop(storage);
                std::fs::remove_dir_all(&path).ok();
            },
            BatchSize::SmallInput,
        );
    });
}

criterion_group!(
    storage_benches,
    bench_write_10k_blocks,
    bench_read_10k_blocks
);
criterion_main!(storage_benches);
