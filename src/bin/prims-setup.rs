use std::{
    fs::{self, File},
    io::{BufWriter, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, anyhow};
use ark_bls12_381::Bls12_381;
use ark_groth16::Groth16;
use ark_serialize::CanonicalSerialize;
use clap::{Parser, Subcommand};
use prims::privacy::{ZkTransferSetupCircuitMetadata, build_reference_zk_transfer_setup_circuit};
use rand::{
    RngCore, SeedableRng,
    rngs::{OsRng, StdRng},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const DEFAULT_OUT_DIR: &str = "artifacts/zk-setup";
const STATE_FILE_NAME: &str = "ceremony_state.json";
const METADATA_FILE_NAME: &str = "setup_metadata.json";
const PROVING_KEY_FILE_NAME: &str = "groth16_proving_key.bin";
const VERIFYING_KEY_FILE_NAME: &str = "groth16_verifying_key.bin";

#[derive(Parser, Debug)]
#[command(name = "prims-setup")]
#[command(about = "Cérémonie trusted setup simplifiée pour le circuit zk de Prims")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    Init {
        #[arg(long, default_value = DEFAULT_OUT_DIR)]
        out_dir: String,
        #[arg(long, default_value = "phase1-organizer")]
        organizer: String,
        #[arg(long, default_value_t = false)]
        force: bool,
    },
    Contribute {
        #[arg(long, default_value = DEFAULT_OUT_DIR)]
        out_dir: String,
        #[arg(long)]
        contributor: String,
    },
    Finalize {
        #[arg(long, default_value = DEFAULT_OUT_DIR)]
        out_dir: String,
        #[arg(long, default_value_t = false)]
        force: bool,
    },
    Inspect {
        #[arg(long, default_value = DEFAULT_OUT_DIR)]
        out_dir: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CeremonyState {
    version: u32,
    ceremony_kind: String,
    curve: String,
    circuit: String,
    contributions: Vec<CeremonyContribution>,
    last_transcript_digest_hex: String,
    created_at_unix: u64,
    updated_at_unix: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CeremonyContribution {
    index: usize,
    contributor: String,
    contributed_at_unix: u64,
    entropy_digest_hex: String,
    transcript_digest_hex: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SetupMetadata {
    version: u32,
    ceremony_kind: String,
    curve: String,
    proving_system: String,
    transcript_final_digest_hex: String,
    contribution_count: usize,
    contributor_names: Vec<String>,
    proving_key_file: String,
    verifying_key_file: String,
    circuit_metadata: ZkTransferSetupCircuitMetadata,
    generated_at_unix: u64,
    notes: Vec<String>,
}

fn now_unix() -> Result<u64> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("horloge système invalide")?
        .as_secs())
}

fn hex_encode(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(&mut output, "{byte:02x}");
    }
    output
}

fn digest_bytes(parts: &[&[u8]]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update(part);
    }
    let digest = hasher.finalize();
    let mut out = [0u8; 32];
    out.copy_from_slice(&digest);
    out
}

fn state_path(out_dir: &Path) -> PathBuf {
    out_dir.join(STATE_FILE_NAME)
}

fn metadata_path(out_dir: &Path) -> PathBuf {
    out_dir.join(METADATA_FILE_NAME)
}

fn proving_key_path(out_dir: &Path) -> PathBuf {
    out_dir.join(PROVING_KEY_FILE_NAME)
}

fn verifying_key_path(out_dir: &Path) -> PathBuf {
    out_dir.join(VERIFYING_KEY_FILE_NAME)
}

fn empty_transcript_digest() -> [u8; 32] {
    digest_bytes(&[b"prims-zk-setup-empty-transcript"])
}

fn write_json_pretty<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let file =
        File::create(path).with_context(|| format!("création du fichier {}", path.display()))?;
    serde_json::to_writer_pretty(BufWriter::new(file), value)
        .with_context(|| format!("écriture JSON {}", path.display()))?;
    Ok(())
}

fn read_state(out_dir: &Path) -> Result<CeremonyState> {
    let path = state_path(out_dir);
    let content = fs::read_to_string(&path)
        .with_context(|| format!("lecture du fichier {}", path.display()))?;
    serde_json::from_str(&content).with_context(|| format!("décodage JSON {}", path.display()))
}

fn ensure_out_dir(out_dir: &Path) -> Result<()> {
    fs::create_dir_all(out_dir)
        .with_context(|| format!("création du dossier {}", out_dir.display()))
}

fn build_initial_state(organizer: &str) -> Result<CeremonyState> {
    let now = now_unix()?;
    let mut organizer_entropy = [0u8; 32];
    OsRng.fill_bytes(&mut organizer_entropy);

    let entropy_digest = digest_bytes(&[
        b"prims-zk-setup-initial-entropy",
        organizer.as_bytes(),
        &organizer_entropy,
    ]);
    let transcript_digest = digest_bytes(&[
        b"prims-zk-setup-transcript",
        &empty_transcript_digest(),
        &entropy_digest,
        organizer.as_bytes(),
        &0usize.to_le_bytes(),
    ]);

    Ok(CeremonyState {
        version: 1,
        ceremony_kind: "simplified-participatory-trusted-setup".to_string(),
        curve: "BLS12-381".to_string(),
        circuit: "ZkTransferCircuit(1 input, 2 outputs, reference shape)".to_string(),
        contributions: vec![CeremonyContribution {
            index: 0,
            contributor: organizer.to_string(),
            contributed_at_unix: now,
            entropy_digest_hex: hex_encode(&entropy_digest),
            transcript_digest_hex: hex_encode(&transcript_digest),
        }],
        last_transcript_digest_hex: hex_encode(&transcript_digest),
        created_at_unix: now,
        updated_at_unix: now,
    })
}

fn append_contribution(mut state: CeremonyState, contributor: &str) -> Result<CeremonyState> {
    let now = now_unix()?;
    let mut fresh_entropy = [0u8; 32];
    OsRng.fill_bytes(&mut fresh_entropy);

    let index = state.contributions.len();
    let previous_digest_hex = if state.contributions.is_empty() {
        hex_encode(&empty_transcript_digest())
    } else {
        state.last_transcript_digest_hex.clone()
    };

    let entropy_digest = digest_bytes(&[
        b"prims-zk-setup-contribution",
        contributor.as_bytes(),
        &fresh_entropy,
        previous_digest_hex.as_bytes(),
    ]);
    let transcript_digest = digest_bytes(&[
        b"prims-zk-setup-transcript",
        previous_digest_hex.as_bytes(),
        &entropy_digest,
        contributor.as_bytes(),
        &index.to_le_bytes(),
    ]);

    state.contributions.push(CeremonyContribution {
        index,
        contributor: contributor.to_string(),
        contributed_at_unix: now,
        entropy_digest_hex: hex_encode(&entropy_digest),
        transcript_digest_hex: hex_encode(&transcript_digest),
    });
    state.last_transcript_digest_hex = hex_encode(&transcript_digest);
    state.updated_at_unix = now;

    Ok(state)
}

fn derive_final_seed(state: &CeremonyState) -> [u8; 32] {
    digest_bytes(&[
        b"prims-zk-setup-final-seed",
        state.last_transcript_digest_hex.as_bytes(),
        &state.contributions.len().to_le_bytes(),
    ])
}

fn write_key_file<T: CanonicalSerialize>(path: &Path, value: &T) -> Result<()> {
    let file =
        File::create(path).with_context(|| format!("création du fichier {}", path.display()))?;
    let mut writer = BufWriter::new(file);
    value
        .serialize_compressed(&mut writer)
        .with_context(|| format!("sérialisation compressée {}", path.display()))?;
    writer
        .flush()
        .with_context(|| format!("flush du fichier {}", path.display()))?;
    Ok(())
}

fn cmd_init(out_dir: &Path, organizer: &str, force: bool) -> Result<()> {
    ensure_out_dir(out_dir)?;
    let state_file = state_path(out_dir);

    if state_file.exists() && !force {
        return Err(anyhow!(
            "le fichier {} existe déjà ; relance avec --force pour réinitialiser",
            state_file.display()
        ));
    }

    let state = build_initial_state(organizer)?;
    write_json_pretty(&state_file, &state)?;

    println!("Cérémonie initialisée");
    println!("Dossier: {}", out_dir.display());
    println!("Fichier d'état: {}", state_file.display());
    println!("Contributions: {}", state.contributions.len());
    println!("Digest transcript: {}", state.last_transcript_digest_hex);

    Ok(())
}

fn cmd_contribute(out_dir: &Path, contributor: &str) -> Result<()> {
    ensure_out_dir(out_dir)?;
    let state = read_state(out_dir)?;
    let updated_state = append_contribution(state, contributor)?;
    let state_file = state_path(out_dir);
    write_json_pretty(&state_file, &updated_state)?;

    let contribution = updated_state
        .contributions
        .last()
        .context("aucune contribution ajoutée")?;

    println!("Contribution ajoutée");
    println!("Contributeur: {}", contribution.contributor);
    println!("Index: {}", contribution.index);
    println!("Digest transcript: {}", contribution.transcript_digest_hex);
    println!("Fichier d'état: {}", state_file.display());

    Ok(())
}

fn cmd_finalize(out_dir: &Path, force: bool) -> Result<()> {
    ensure_out_dir(out_dir)?;
    let state = read_state(out_dir)?;

    let pk_path = proving_key_path(out_dir);
    let vk_path = verifying_key_path(out_dir);
    let meta_path = metadata_path(out_dir);

    if !force && (pk_path.exists() || vk_path.exists() || meta_path.exists()) {
        return Err(anyhow!(
            "des artefacts existent déjà dans {} ; relance avec --force pour régénérer",
            out_dir.display()
        ));
    }

    let seed = derive_final_seed(&state);
    let mut rng = StdRng::from_seed(seed);

    let (circuit, circuit_metadata) = build_reference_zk_transfer_setup_circuit()
        .map_err(|error| anyhow!("circuit de setup invalide: {error}"))?;

    let proving_key =
        Groth16::<Bls12_381>::generate_random_parameters_with_reduction(circuit, &mut rng)
            .map_err(|error| anyhow!("échec du trusted setup Groth16: {error}"))?;

    let verifying_key = proving_key.vk.clone();

    write_key_file(&pk_path, &proving_key)?;
    write_key_file(&vk_path, &verifying_key)?;

    let metadata = SetupMetadata {
        version: 1,
        ceremony_kind: state.ceremony_kind.clone(),
        curve: state.curve.clone(),
        proving_system: "Groth16".to_string(),
        transcript_final_digest_hex: state.last_transcript_digest_hex.clone(),
        contribution_count: state.contributions.len(),
        contributor_names: state
            .contributions
            .iter()
            .map(|item| item.contributor.clone())
            .collect(),
        proving_key_file: PROVING_KEY_FILE_NAME.to_string(),
        verifying_key_file: VERIFYING_KEY_FILE_NAME.to_string(),
        circuit_metadata,
        generated_at_unix: now_unix()?,
        notes: vec![
            "Setup simplifié de démonstration : à remplacer plus tard par une cérémonie multi-parties plus robuste.".to_string(),
            "Les paramètres produits sont liés à la forme de circuit de référence actuelle (1 entrée, 2 sorties).".to_string(),
            "Aucune entropie brute n'est écrite sur disque ; seuls des digests SHA-256 de transcript sont conservés.".to_string(),
        ],
    };
    write_json_pretty(&meta_path, &metadata)?;

    println!("Trusted setup finalisé");
    println!("Proving key: {}", pk_path.display());
    println!("Verifying key: {}", vk_path.display());
    println!("Metadata: {}", meta_path.display());
    println!("Contributions: {}", metadata.contribution_count);
    println!(
        "Transcript final digest: {}",
        metadata.transcript_final_digest_hex
    );

    Ok(())
}

fn cmd_inspect(out_dir: &Path) -> Result<()> {
    let state = read_state(out_dir)?;

    println!("Cérémonie: {}", state.ceremony_kind);
    println!("Courbe: {}", state.curve);
    println!("Circuit: {}", state.circuit);
    println!("Contributions: {}", state.contributions.len());
    println!(
        "Digest transcript courant: {}",
        state.last_transcript_digest_hex
    );

    for contribution in &state.contributions {
        println!(
            "- #{} {} @ {} => {}",
            contribution.index,
            contribution.contributor,
            contribution.contributed_at_unix,
            contribution.transcript_digest_hex
        );
    }

    Ok(())
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Init {
            out_dir,
            organizer,
            force,
        } => cmd_init(Path::new(&out_dir), &organizer, force),
        Commands::Contribute {
            out_dir,
            contributor,
        } => cmd_contribute(Path::new(&out_dir), &contributor),
        Commands::Finalize { out_dir, force } => cmd_finalize(Path::new(&out_dir), force),
        Commands::Inspect { out_dir } => cmd_inspect(Path::new(&out_dir)),
    }
}
