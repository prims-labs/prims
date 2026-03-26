use ark_bls12_381::Bls12_381;
use ark_groth16::Groth16;
use criterion::{BatchSize, Criterion, black_box, criterion_group, criterion_main};
use prims::privacy::{
    AnonTransaction, build_reference_zk_transfer_setup_circuit, generate_zk_transfer_proof,
    prepare_zk_transfer_verifying_key,
};
use rand::{SeedableRng, rngs::StdRng};
use std::time::Duration;

fn bench_zk_transfer_proof_generation(c: &mut Criterion) {
    let (circuit, _) = build_reference_zk_transfer_setup_circuit()
        .expect("reference setup circuit should be built");

    let public_inputs = circuit.public_inputs.clone();
    let witness = circuit.witness.clone();

    let mut setup_rng = StdRng::seed_from_u64(7_120);
    let proving_key =
        Groth16::<Bls12_381>::generate_random_parameters_with_reduction(circuit, &mut setup_rng)
            .expect("trusted setup should succeed");

    c.bench_function("privacy_zk_transfer_proof_generation", |b| {
        b.iter_batched(
            || StdRng::seed_from_u64(7_121),
            |mut proof_rng| {
                let proof = generate_zk_transfer_proof(
                    &proving_key,
                    &public_inputs,
                    &witness,
                    &mut proof_rng,
                )
                .expect("proof generation should succeed");
                black_box(proof);
            },
            BatchSize::SmallInput,
        );
    });
}

fn bench_zk_transfer_proof_verification(c: &mut Criterion) {
    let (circuit, _) = build_reference_zk_transfer_setup_circuit()
        .expect("reference setup circuit should be built");

    let mut setup_rng = StdRng::seed_from_u64(7_122);
    let proving_key = Groth16::<Bls12_381>::generate_random_parameters_with_reduction(
        circuit.clone(),
        &mut setup_rng,
    )
    .expect("trusted setup should succeed");
    let prepared_verifying_key = prepare_zk_transfer_verifying_key(&proving_key.vk);

    let mut proof_rng = StdRng::seed_from_u64(7_123);
    let proof = generate_zk_transfer_proof(
        &proving_key,
        &circuit.public_inputs,
        &circuit.witness,
        &mut proof_rng,
    )
    .expect("proof generation should succeed");
    let anon_transaction = AnonTransaction::new(&circuit.public_inputs, &proof)
        .expect("anon transaction should be created");

    c.bench_function("privacy_zk_transfer_proof_verification", |b| {
        b.iter(|| {
            let is_valid = anon_transaction
                .verify_with_prepared_key(&prepared_verifying_key)
                .expect("anon transaction verification should succeed");
            assert!(is_valid, "proof should verify successfully");
            black_box(is_valid);
        });
    });
}

criterion_group! {
    name = privacy_benches;
    config = Criterion::default()
        .sample_size(10)
        .warm_up_time(Duration::from_secs(1))
        .measurement_time(Duration::from_secs(10));
    targets = bench_zk_transfer_proof_generation, bench_zk_transfer_proof_verification
}
criterion_main!(privacy_benches);
