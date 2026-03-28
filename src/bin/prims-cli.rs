use age::secrecy::SecretString;
use age::{decrypt as age_decrypt, encrypt_and_armor, scrypt};
use anyhow::{Context, Result, anyhow};
use clap::{Parser, Subcommand};
use ed25519_dalek::SigningKey;
use futures::StreamExt;
use libp2p::swarm::SwarmEvent;
use prims::{
    blockchain::{
        hash::derive_contract_address,
        types::{
            ContractCallPayload, DEFAULT_SHARD_ID, FIXED_TRANSACTION_FEE, Transaction,
            TransactionType,
        },
    },
    crypto::{generate_keypair, sign_transaction},
    network::{
        config::NetworkConfig,
        gossip::{blocks_topic, transactions_topic, votes_topic},
        node::PrimsNode,
    },
};
use reqwest::Client;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;
use std::{
    env,
    fs::{self, OpenOptions},
    io::Write,
};
use tokio::time::{Duration, Instant, timeout};
use zeroize::Zeroize;

const DEFAULT_RPC_URL: &str = "http://127.0.0.1:7002";
const DEFAULT_STAKE_DURATION_SECS: u64 = 86_400;
const DEFAULT_CALL_CONTRACT_GAS_LIMIT: u64 = 100_000;

#[derive(Parser, Debug)]
#[command(name = "prims-cli")]
#[command(about = "CLI utilisateur pour interagir avec le RPC Prims")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    GenerateKey {
        #[arg(long)]
        encrypted_file: Option<String>,
    },
    Balance {
        address: String,
    },
    Send {
        to: String,
        amount: u64,
        #[arg(long)]
        anon: bool,
        #[arg(long, default_value_t = DEFAULT_SHARD_ID)]
        source_shard: u16,
        #[arg(long, default_value_t = DEFAULT_SHARD_ID)]
        destination_shard: u16,
    },
    Stake {
        amount: u64,
        #[arg(long, default_value_t = DEFAULT_STAKE_DURATION_SECS)]
        duration: u64,
    },
    Unstake,
    ListValidators,
    CreateContract {
        wasm_file: String,
    },
    CallContract {
        address: String,
        method: String,
        params: String,
        #[arg(long, default_value_t = DEFAULT_CALL_CONTRACT_GAS_LIMIT)]
        gas_limit: u64,
    },
    Flood {
        #[arg(long, default_value_t = 1000)]
        count: u64,
        #[arg(long, default_value_t = 1)]
        start_nonce: u64,
        #[arg(long, default_value_t = 42)]
        amount: u64,
        #[arg(long, default_value = "/ip4/127.0.0.1/tcp/0")]
        listen_address: String,
        #[arg(long, value_delimiter = ',')]
        seed_nodes: Vec<String>,
    },
}

#[derive(Debug, Serialize)]
struct GeneratedKeyOutput {
    public_key: String,
    secret_key: String,
    warning: &'static str,
}

#[derive(Debug, Serialize)]
struct GeneratedEncryptedKeyOutput {
    public_key: String,
    encrypted_file: String,
    warning: &'static str,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct RpcBalanceResponse {
    address: String,
    found: bool,
    balance: u64,
    nonce: u64,
    note_commitment_count: usize,
    viewing_hint: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct RpcValidatorView {
    address: String,
    stake: u64,
    locked_until: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct RpcSendTransactionResponse {
    accepted: bool,
    tx_hash: String,
    mempool_size: usize,
}

#[derive(Debug, Serialize)]
struct TransactionSubmissionOutput {
    rpc_url: String,
    from: String,
    to: String,
    tx_type: String,
    stake_duration: Option<u64>,
    amount: u64,
    fee: u64,
    nonce: u64,
    source_shard: u16,
    destination_shard: u16,
    accepted: bool,
    tx_hash: String,
    mempool_size: usize,
}

struct SenderContext {
    secret_key: [u8; 32],
    public_key: [u8; 32],
    address_hex: String,
    next_nonce: u64,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let rpc_url = env::var("PRIMS_RPC_URL").unwrap_or_else(|_| DEFAULT_RPC_URL.to_string());
    let http = Client::builder().build()?;

    match cli.command {
        Commands::GenerateKey { encrypted_file } => generate_key_command(encrypted_file.as_deref()),
        Commands::Balance { address } => balance_command(&http, &rpc_url, &address).await,
        Commands::Send {
            to,
            amount,
            anon,
            source_shard,
            destination_shard,
        } => {
            send_command(
                &http,
                &rpc_url,
                &to,
                amount,
                anon,
                source_shard,
                destination_shard,
            )
            .await
        }
        Commands::Stake { amount, duration } => {
            stake_command(&http, &rpc_url, amount, duration).await
        }
        Commands::Unstake => unstake_command(&http, &rpc_url).await,
        Commands::ListValidators => list_validators_command(&http, &rpc_url).await,
        Commands::CreateContract { wasm_file } => {
            create_contract_command(&http, &rpc_url, &wasm_file).await
        }
        Commands::CallContract {
            address,
            method,
            params,
            gas_limit,
        } => call_contract_command(&http, &rpc_url, &address, &method, &params, gas_limit).await,
        Commands::Flood {
            count,
            start_nonce,
            amount,
            listen_address,
            seed_nodes,
        } => flood(count, start_nonce, amount, listen_address, seed_nodes).await,
    }
}

async fn flood(
    count: u64,
    start_nonce: u64,
    amount: u64,
    listen_address: String,
    seed_nodes: Vec<String>,
) -> Result<()> {
    if count == 0 {
        return Err(anyhow!("count doit être supérieur à 0"));
    }

    let mut config = NetworkConfig::default();
    config.listen_address = listen_address;

    if !seed_nodes.is_empty() {
        config.seed_nodes = seed_nodes;
    }

    if config.seed_nodes.is_empty() {
        return Err(anyhow!(
            "au moins un seed node est requis (--seed-nodes ou PRIMS_SEED_NODES)"
        ));
    }

    let mut node = PrimsNode::new(config)?;
    let gossipsub = &mut node.swarm.behaviour_mut().gossipsub;
    let _ = gossipsub.unsubscribe(&blocks_topic());
    let _ = gossipsub.unsubscribe(&transactions_topic());
    let _ = gossipsub.unsubscribe(&votes_topic());

    println!("PRIMS CLI flood démarré");
    println!("Local PeerId: {}", node.local_peer_id);
    println!("Listen address: {}", node.config.listen_address);
    println!("Seed nodes: {:?}", node.config.seed_nodes);

    let deadline = Instant::now() + Duration::from_secs(10);
    let mut connected = false;

    while Instant::now() < deadline {
        match timeout(Duration::from_millis(500), node.swarm.select_next_some()).await {
            Ok(SwarmEvent::NewListenAddr { address, .. }) => {
                println!("CLI listening on {}", address);
            }
            Ok(SwarmEvent::ConnectionEstablished { peer_id, .. }) => {
                println!("Connection established with {}", peer_id);
                connected = true;
                break;
            }
            Ok(_) => {}
            Err(_) => {}
        }
    }

    if !connected {
        return Err(anyhow!(
            "aucune connexion établie avec les seed nodes sous 10 secondes"
        ));
    }

    let subscription_deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < subscription_deadline {
        match timeout(Duration::from_millis(250), node.swarm.select_next_some()).await {
            Ok(SwarmEvent::Behaviour(_)) => {}
            Ok(SwarmEvent::NewListenAddr { address, .. }) => {
                println!("CLI listening on {}", address);
            }
            Ok(SwarmEvent::ConnectionEstablished { peer_id, .. }) => {
                println!("Connection established with {}", peer_id);
            }
            Ok(_) => {}
            Err(_) => {}
        }
    }

    let sender = generate_keypair();

    for offset in 0..count {
        let nonce = start_nonce + offset;
        let recipient = generate_keypair();

        let mut tx = Transaction {
            tx_type: TransactionType::Transfer,
            from: sender.public_key.to_vec(),
            to: recipient.public_key.to_vec(),
            amount,
            fee: FIXED_TRANSACTION_FEE,
            nonce,
            source_shard: 0,
            destination_shard: 0,
            signature: Vec::new(),
            data: Some(format!("load-test-{nonce}").into_bytes()),
        };

        tx.signature = sign_transaction(&tx, &sender.secret_key)?;

        let mut attempts = 0u32;
        loop {
            match node.publish_transaction(&tx) {
                Ok(_) => break,
                Err(error)
                    if error.to_string().contains("NoPeersSubscribedToTopic") && attempts < 20 =>
                {
                    attempts += 1;
                    println!(
                        "Transaction nonce {} en attente d'abonnés sur le topic, nouvelle tentative {}/20...",
                        nonce, attempts
                    );
                    match timeout(Duration::from_millis(250), node.swarm.select_next_some()).await {
                        Ok(_) => {}
                        Err(_) => {}
                    }
                }
                Err(error) if error.to_string().contains("AllQueuesFull") && attempts < 200 => {
                    attempts += 1;
                    match timeout(Duration::from_millis(5), node.swarm.select_next_some()).await {
                        Ok(_) => {}
                        Err(_) => {}
                    }
                }
                Err(error) => return Err(error),
            }
        }

        println!("Published transaction nonce {}", nonce);
    }

    println!("Total published transactions: {}", count);

    let flush_deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < flush_deadline {
        match timeout(Duration::from_millis(100), node.swarm.select_next_some()).await {
            Ok(_) => {}
            Err(_) => {}
        }
    }

    Ok(())
}

fn generate_key_command(encrypted_file: Option<&str>) -> Result<()> {
    let pair = generate_keypair();

    if let Some(path) = encrypted_file {
        write_encrypted_secret_key_file(path, &pair.secret_key)?;

        return print_json(&GeneratedEncryptedKeyOutput {
            public_key: hex::encode(pair.public_key),
            encrypted_file: path.to_string(),
            warning: "Conserve le mot de passe séparément du fichier chiffré et ne le partage jamais.",
        });
    }

    print_json(&GeneratedKeyOutput {
        public_key: hex::encode(pair.public_key),
        secret_key: hex::encode(pair.secret_key),
        warning: "Sauvegarde la clé privée hors du terminal et ne la partage jamais.",
    })
}

fn write_encrypted_secret_key_file(path: &str, secret_key: &[u8; 32]) -> Result<()> {
    let passphrase = prompt_new_passphrase()?;
    let recipient = scrypt::Recipient::new(passphrase.clone());

    let mut plaintext = hex::encode(secret_key).into_bytes();
    let mut ciphertext = encrypt_and_armor(&recipient, &plaintext)
        .context("échec du chiffrement de la clé privée")?;
    plaintext.zeroize();

    let mut file = new_secret_file(path)?;
    file.write_all(ciphertext.as_bytes())
        .with_context(|| format!("impossible d'écrire le fichier chiffré: {path}"))?;

    if !ciphertext.ends_with('\n') {
        file.write_all(b"\n")
            .with_context(|| format!("impossible de finaliser le fichier chiffré: {path}"))?;
    }

    ciphertext.zeroize();
    Ok(())
}

fn prompt_new_passphrase() -> Result<SecretString> {
    let first = rpassword::prompt_password("Mot de passe du fichier chiffré: ")
        .context("impossible de lire le mot de passe")?;
    let second = rpassword::prompt_password("Confirme le mot de passe: ")
        .context("impossible de lire la confirmation du mot de passe")?;

    let matches = first == second;
    let is_empty = first.trim().is_empty();

    let mut second = second;
    second.zeroize();

    if is_empty {
        let mut first = first;
        first.zeroize();
        return Err(anyhow!(
            "le mot de passe du fichier chiffré ne peut pas être vide"
        ));
    }

    if !matches {
        let mut first = first;
        first.zeroize();
        return Err(anyhow!("les deux mots de passe ne correspondent pas"));
    }

    Ok(SecretString::from(first))
}

fn prompt_existing_passphrase(path: &str) -> Result<SecretString> {
    let password = rpassword::prompt_password(format!("Mot de passe pour {path}: "))
        .context("impossible de lire le mot de passe du fichier chiffré")?;

    if password.trim().is_empty() {
        let mut password = password;
        password.zeroize();
        return Err(anyhow!(
            "le mot de passe du fichier chiffré ne peut pas être vide"
        ));
    }

    Ok(SecretString::from(password))
}

fn new_secret_file(path: &str) -> Result<fs::File> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);

    #[cfg(unix)]
    {
        options.mode(0o600);
    }

    options
        .open(path)
        .with_context(|| format!("impossible de créer le fichier chiffré: {path}"))
}

async fn balance_command(http: &Client, rpc_url: &str, address: &str) -> Result<()> {
    let address_bytes = decode_hex_vec("address", address)?;
    let normalized_address = hex::encode(address_bytes);

    let response: RpcBalanceResponse = rpc_call(
        http,
        rpc_url,
        "get_balance",
        json!({ "address": normalized_address }),
    )
    .await?;

    print_json(&response)
}

async fn list_validators_command(http: &Client, rpc_url: &str) -> Result<()> {
    let validators: Vec<RpcValidatorView> =
        rpc_call(http, rpc_url, "get_validators", json!({})).await?;

    print_json(&validators)
}

async fn send_command(
    http: &Client,
    rpc_url: &str,
    to: &str,
    amount: u64,
    anon: bool,
    source_shard: u16,
    destination_shard: u16,
) -> Result<()> {
    let sender = load_sender_context(http, rpc_url).await?;
    let recipient = decode_hex_vec("to", to)?;
    let tx_type = if anon {
        TransactionType::PublicToAnon
    } else {
        TransactionType::Transfer
    };

    let output = submit_transaction(
        http,
        rpc_url,
        sender,
        tx_type,
        recipient,
        amount,
        source_shard,
        destination_shard,
        Some(if anon {
            b"prims-cli-send-anon".to_vec()
        } else {
            b"prims-cli-send".to_vec()
        }),
    )
    .await?;

    print_json(&output)
}

async fn stake_command(http: &Client, rpc_url: &str, amount: u64, duration: u64) -> Result<()> {
    let sender = load_sender_context(http, rpc_url).await?;
    let validator_address = sender.public_key.to_vec();

    let output = submit_transaction(
        http,
        rpc_url,
        sender,
        TransactionType::Stake { duration },
        validator_address,
        amount,
        DEFAULT_SHARD_ID,
        DEFAULT_SHARD_ID,
        Some(format!("prims-cli-stake:{duration}").into_bytes()),
    )
    .await?;

    print_json(&output)
}

async fn unstake_command(http: &Client, rpc_url: &str) -> Result<()> {
    let sender = load_sender_context(http, rpc_url).await?;
    let validator_address = sender.public_key.to_vec();

    let output = submit_transaction(
        http,
        rpc_url,
        sender,
        TransactionType::Unstake,
        validator_address,
        0,
        DEFAULT_SHARD_ID,
        DEFAULT_SHARD_ID,
        Some(b"prims-cli-unstake".to_vec()),
    )
    .await?;

    print_json(&output)
}

async fn create_contract_command(http: &Client, rpc_url: &str, wasm_file: &str) -> Result<()> {
    let sender = load_sender_context(http, rpc_url).await?;
    let code_wasm = fs::read(wasm_file)
        .with_context(|| format!("impossible de lire le fichier WASM: {wasm_file}"))?;

    if code_wasm.is_empty() {
        return Err(anyhow!("le fichier WASM du contrat ne peut pas être vide"));
    }

    let contract_address = derive_contract_address(&sender.public_key, &code_wasm);
    let output = submit_transaction(
        http,
        rpc_url,
        sender,
        TransactionType::DeployContract,
        contract_address.clone(),
        0,
        DEFAULT_SHARD_ID,
        DEFAULT_SHARD_ID,
        Some(code_wasm.clone()),
    )
    .await?;

    print_json(&json!({
        "rpc_url": output.rpc_url,
        "from": output.from,
        "contract_address": hex::encode(contract_address),
        "tx_type": output.tx_type,
        "amount": output.amount,
        "fee": output.fee,
        "nonce": output.nonce,
        "source_shard": output.source_shard,
        "destination_shard": output.destination_shard,
        "accepted": output.accepted,
        "tx_hash": output.tx_hash,
        "mempool_size": output.mempool_size,
        "wasm_file": wasm_file,
        "wasm_size": code_wasm.len()
    }))
}

async fn call_contract_command(
    http: &Client,
    rpc_url: &str,
    address: &str,
    method: &str,
    params: &str,
    gas_limit: u64,
) -> Result<()> {
    let sender = load_sender_context(http, rpc_url).await?;
    let address_bytes = decode_hex_vec("address", address)?;
    let method = method.trim();

    if method.is_empty() {
        return Err(anyhow!("method ne peut pas être vide"));
    }

    if gas_limit == 0 {
        return Err(anyhow!("gas_limit doit être strictement positif"));
    }

    let params_json: Value =
        serde_json::from_str(params).context("params doit être une chaîne JSON valide")?;
    let payload = ContractCallPayload {
        method: method.to_string(),
        params: serde_json::to_vec(&params_json)
            .context("échec de sérialisation JSON des paramètres du contrat")?,
        gas_limit,
    };
    let payload_bytes = bincode::serialize(&payload)
        .context("échec de sérialisation du payload d'appel de contrat")?;

    let output = submit_transaction(
        http,
        rpc_url,
        sender,
        TransactionType::CallContract,
        address_bytes.clone(),
        0,
        DEFAULT_SHARD_ID,
        DEFAULT_SHARD_ID,
        Some(payload_bytes),
    )
    .await?;

    print_json(&json!({
        "rpc_url": output.rpc_url,
        "from": output.from,
        "contract_address": hex::encode(address_bytes),
        "tx_type": output.tx_type,
        "method": method,
        "params": params_json,
        "gas_limit": gas_limit,
        "amount": output.amount,
        "fee": output.fee,
        "nonce": output.nonce,
        "source_shard": output.source_shard,
        "destination_shard": output.destination_shard,
        "accepted": output.accepted,
        "tx_hash": output.tx_hash,
        "mempool_size": output.mempool_size
    }))
}

async fn load_sender_context(http: &Client, rpc_url: &str) -> Result<SenderContext> {
    let secret_key = load_secret_key()?;
    let signing_key = SigningKey::from_bytes(&secret_key);
    let public_key = signing_key.verifying_key().to_bytes();
    let address_hex = hex::encode(public_key);

    let balance: RpcBalanceResponse = rpc_call(
        http,
        rpc_url,
        "get_balance",
        json!({ "address": address_hex.clone() }),
    )
    .await?;

    Ok(SenderContext {
        secret_key,
        public_key,
        address_hex,
        next_nonce: balance.nonce.saturating_add(1),
    })
}

async fn submit_transaction(
    http: &Client,
    rpc_url: &str,
    sender: SenderContext,
    tx_type: TransactionType,
    to: Vec<u8>,
    amount: u64,
    source_shard: u16,
    destination_shard: u16,
    data: Option<Vec<u8>>,
) -> Result<TransactionSubmissionOutput> {
    let (tx_type_name, stake_duration) = tx_type_view(&tx_type);

    let mut transaction = Transaction {
        tx_type,
        from: sender.public_key.to_vec(),
        to: to.clone(),
        amount,
        fee: FIXED_TRANSACTION_FEE,
        nonce: sender.next_nonce,
        source_shard,
        destination_shard,
        signature: Vec::new(),
        data,
    };

    transaction.signature = sign_transaction(&transaction, &sender.secret_key)?;
    let tx_bytes = bincode::serialize(&transaction).context("échec de sérialisation bincode")?;
    let hex_tx = hex::encode(tx_bytes);

    let response: RpcSendTransactionResponse = rpc_call(
        http,
        rpc_url,
        "send_transaction",
        json!({ "hex_tx": hex_tx }),
    )
    .await?;

    Ok(TransactionSubmissionOutput {
        rpc_url: rpc_url.to_string(),
        from: sender.address_hex,
        to: hex::encode(to),
        tx_type: tx_type_name.to_string(),
        stake_duration,
        amount,
        fee: FIXED_TRANSACTION_FEE,
        nonce: transaction.nonce,
        source_shard,
        destination_shard,
        accepted: response.accepted,
        tx_hash: response.tx_hash,
        mempool_size: response.mempool_size,
    })
}

async fn rpc_call<T: DeserializeOwned>(
    http: &Client,
    rpc_url: &str,
    method: &str,
    params: Value,
) -> Result<T> {
    let response = http
        .post(rpc_url)
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": method,
            "params": params
        }))
        .send()
        .await
        .with_context(|| format!("échec HTTP vers le RPC {rpc_url} pour la méthode {method}"))?;

    let status = response.status();
    let payload: Value = response
        .json()
        .await
        .context("échec de décodage de la réponse JSON-RPC")?;

    let result = if let Some(result) = payload.get("result") {
        result.clone()
    } else if let Some(error) = payload.get("error") {
        let code = error
            .get("code")
            .and_then(Value::as_i64)
            .map(|v| v.to_string())
            .unwrap_or_else(|| "?".to_string());
        let message = error
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("unknown RPC error");

        return Err(anyhow!("RPC error {code}: {message}"));
    } else {
        return Err(anyhow!(
            "payload JSON-RPC inattendu (HTTP {status}): {}",
            serde_json::to_string_pretty(&payload)
                .unwrap_or_else(|_| "<payload non affichable>".to_string())
        ));
    };

    serde_json::from_value(result)
        .with_context(|| format!("échec de désérialisation de la réponse RPC pour {method}"))
}

fn load_secret_key() -> Result<[u8; 32]> {
    if let Ok(path) = env::var("PRIMS_SECRET_KEY_FILE") {
        let raw = fs::read(&path)
            .with_context(|| format!("impossible de lire PRIMS_SECRET_KEY_FILE={path}"))?;
        return parse_secret_key_file_contents(&path, &raw);
    }

    if let Ok(value) = env::var("PRIMS_SECRET_KEY_HEX") {
        return parse_secret_key_material(&value);
    }

    Err(anyhow!(
        "variable d'environnement requise absente : définis PRIMS_SECRET_KEY_HEX ou PRIMS_SECRET_KEY_FILE pour signer une transaction (fichier brut JSON/hex ou fichier chiffré avec mot de passe)"
    ))
}

fn parse_secret_key_file_contents(path: &str, raw: &[u8]) -> Result<[u8; 32]> {
    if let Ok(text) = std::str::from_utf8(raw) {
        let trimmed = text.trim();
        if trimmed.starts_with('{') || looks_like_plaintext_hex_secret(trimmed) {
            return parse_secret_key_material(text);
        }
    }

    decrypt_secret_key_file(path, raw)
}

fn decrypt_secret_key_file(path: &str, ciphertext: &[u8]) -> Result<[u8; 32]> {
    let passphrase = prompt_existing_passphrase(path)?;
    let identity = scrypt::Identity::new(passphrase);

    let mut plaintext = age_decrypt(&identity, ciphertext)
        .with_context(|| format!("impossible de déchiffrer PRIMS_SECRET_KEY_FILE={path}"))?;

    let parsed = match std::str::from_utf8(&plaintext) {
        Ok(text) => parse_secret_key_material(text),
        Err(error) => Err(anyhow!("contenu déchiffré invalide pour {path}: {error}")),
    };

    plaintext.zeroize();
    parsed
}

fn looks_like_plaintext_hex_secret(value: &str) -> bool {
    let normalized = value
        .trim()
        .strip_prefix("0x")
        .or_else(|| value.trim().strip_prefix("0X"))
        .unwrap_or(value.trim());

    !normalized.is_empty() && normalized.chars().all(|c| c.is_ascii_hexdigit())
}

fn parse_secret_key_material(input: &str) -> Result<[u8; 32]> {
    let trimmed = input.trim();

    if trimmed.starts_with('{') {
        let json_value: Value =
            serde_json::from_str(trimmed).context("contenu JSON de clé privée invalide")?;

        if let Some(secret_key) = json_value.get("secret_key").and_then(Value::as_str) {
            return decode_hex_32("secret_key", secret_key);
        }

        if let Some(secret_key) = json_value.get("secret_key_hex").and_then(Value::as_str) {
            return decode_hex_32("secret_key_hex", secret_key);
        }

        return Err(anyhow!(
            "le JSON fourni ne contient ni champ secret_key ni champ secret_key_hex"
        ));
    }

    decode_hex_32("secret_key", trimmed)
}

fn decode_hex_vec(label: &str, value: &str) -> Result<Vec<u8>> {
    let normalized = value
        .trim()
        .strip_prefix("0x")
        .or_else(|| value.trim().strip_prefix("0X"))
        .unwrap_or(value.trim());

    hex::decode(normalized)
        .with_context(|| format!("{label} doit être une chaîne hexadécimale valide"))
}

fn decode_hex_32(label: &str, value: &str) -> Result<[u8; 32]> {
    let bytes = decode_hex_vec(label, value)?;
    bytes
        .try_into()
        .map_err(|_| anyhow!("{label} doit contenir exactement 32 octets (64 caractères hex)"))
}
fn tx_type_view(tx_type: &TransactionType) -> (&'static str, Option<u64>) {
    match tx_type {
        TransactionType::Transfer => ("Transfer", None),
        TransactionType::Stake { duration } => ("Stake", Some(*duration)),
        TransactionType::Unstake => ("Unstake", None),
        TransactionType::PublicToAnon => ("PublicToAnon", None),
        TransactionType::AnonToPublic => ("AnonToPublic", None),
        TransactionType::DeployContract => ("DeployContract", None),
        TransactionType::CallContract => ("CallContract", None),
    }
}

fn print_json<T: Serialize>(value: &T) -> Result<()> {
    println!(
        "{}",
        serde_json::to_string_pretty(value).context("échec de sérialisation JSON")?
    );
    Ok(())
}
