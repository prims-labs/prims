use anyhow::{Context, Result, anyhow};
use libp2p::{
    Multiaddr, PeerId, Swarm, SwarmBuilder,
    gossipsub::{IdentTopic, MessageId},
    identity::Keypair,
    noise, tcp, yamux,
};
use std::{collections::HashMap, env, fs};

use crate::{
    blockchain::{
        hash::{hash_block_header, hash_transaction},
        types::{Block as ChainBlock, Transaction as ChainTransaction},
    },
    consensus::ConsensusVote,
    network::{
        behaviour::PrimsBehaviour,
        config::NetworkConfig,
        connection::{ConnectionLimits, PeerPenalty},
        gossip::{BLOCKS_TOPIC_NAME, TRANSACTIONS_TOPIC_NAME, VOTES_TOPIC_NAME},
        message::{
            Block as NetworkBlock, Message, Transaction as NetworkTransaction, Vote as NetworkVote,
        },
    },
};

pub struct PrimsNode {
    pub local_peer_id: PeerId,
    pub swarm: Swarm<PrimsBehaviour>,
    pub config: NetworkConfig,
    pub connection_limits: ConnectionLimits,
    pub peer_penalties: HashMap<PeerId, PeerPenalty>,
}

fn bytes_to_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);

    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }

    output
}

fn blockchain_transaction_to_network_transaction(
    transaction: &ChainTransaction,
) -> NetworkTransaction {
    NetworkTransaction {
        id: bytes_to_hex(&hash_transaction(transaction)),
        payload: bincode::serialize(transaction)
            .expect("blockchain transaction serialization should succeed"),
    }
}

fn blockchain_block_to_network_block(block: &ChainBlock) -> NetworkBlock {
    NetworkBlock {
        height: block.header.height,
        hash: bytes_to_hex(&hash_block_header(&block.header)),
        transactions: block
            .transactions
            .iter()
            .map(blockchain_transaction_to_network_transaction)
            .collect(),
    }
}

fn consensus_vote_to_network_vote(vote: &ConsensusVote) -> NetworkVote {
    NetworkVote {
        height: vote.height,
        block_hash: vote.block_hash.clone(),
        voter: vote.voter.clone(),
        approve: vote.approve,
        signature: vote.signature.clone(),
    }
}

fn load_network_identity_keypair() -> Result<Keypair> {
    if let Ok(path) = env::var("PRIMS_NETWORK_SECRET_KEY_FILE") {
        let raw = fs::read(&path)
            .with_context(|| format!("impossible de lire PRIMS_NETWORK_SECRET_KEY_FILE={path}"))?;
        let secret = parse_network_secret_key_file_contents(&path, &raw)?;
        return Keypair::ed25519_from_bytes(secret)
            .map_err(|e| anyhow!("clé réseau libp2p invalide dans {path}: {e}"));
    }

    if let Ok(value) = env::var("PRIMS_NETWORK_SECRET_KEY_HEX") {
        let secret = decode_hex_32("PRIMS_NETWORK_SECRET_KEY_HEX", &value)?;
        return Keypair::ed25519_from_bytes(secret)
            .map_err(|e| anyhow!("PRIMS_NETWORK_SECRET_KEY_HEX invalide pour libp2p: {e}"));
    }

    Ok(Keypair::generate_ed25519())
}

fn parse_network_secret_key_file_contents(path: &str, raw: &[u8]) -> Result<[u8; 32]> {
    let text = std::str::from_utf8(raw)
        .with_context(|| format!("contenu non UTF-8 pour PRIMS_NETWORK_SECRET_KEY_FILE={path}"))?;
    let trimmed = text.trim();

    if trimmed.starts_with('{') {
        let json_value: serde_json::Value =
            serde_json::from_str(trimmed).context("contenu JSON de clé réseau invalide")?;

        if let Some(secret_key) = json_value
            .get("secret_key")
            .and_then(serde_json::Value::as_str)
        {
            return decode_hex_32("secret_key", secret_key);
        }

        if let Some(secret_key) = json_value
            .get("secret_key_hex")
            .and_then(serde_json::Value::as_str)
        {
            return decode_hex_32("secret_key_hex", secret_key);
        }

        return Err(anyhow!(
            "le JSON fourni ne contient ni champ secret_key ni champ secret_key_hex"
        ));
    }

    decode_hex_32("PRIMS_NETWORK_SECRET_KEY_FILE", trimmed)
}

fn decode_hex_32(label: &str, value: &str) -> Result<[u8; 32]> {
    let normalized = value
        .trim()
        .strip_prefix("0x")
        .or_else(|| value.trim().strip_prefix("0X"))
        .unwrap_or(value.trim());

    let decoded = hex::decode(normalized)
        .with_context(|| format!("{label} doit être une chaîne hexadécimale valide"))?;

    if decoded.len() != 32 {
        return Err(anyhow!(
            "{label} doit contenir exactement 32 octets après décodage hexadécimal"
        ));
    }

    decoded
        .try_into()
        .map_err(|_| anyhow!("{label} doit contenir exactement 32 octets"))
}

impl PrimsNode {
    pub fn new(config: NetworkConfig) -> Result<Self> {
        let local_key = load_network_identity_keypair()?;
        let mut swarm = SwarmBuilder::with_existing_identity(local_key.clone())
            .with_tokio()
            .with_tcp(
                tcp::Config::default(),
                noise::Config::new,
                yamux::Config::default,
            )?
            .with_behaviour(|key| {
                let local_peer_id = key.public().to_peer_id();
                PrimsBehaviour::new(key, local_peer_id)
            })?
            .build();

        let listen_address = config.listen_address.parse()?;
        swarm.listen_on(listen_address)?;

        if let Some(external_address) = &config.external_address {
            let addr: Multiaddr = external_address.parse()?;
            swarm.add_external_address(addr.clone());
        }

        for seed in &config.seed_nodes {
            let addr: Multiaddr = seed.parse()?;
            swarm.dial(addr)?;
        }

        let local_peer_id = swarm.local_peer_id().to_owned();
        let connection_limits = config.connection_limits.clone();

        Ok(Self {
            local_peer_id,
            swarm,
            config,
            connection_limits,
            peer_penalties: HashMap::new(),
        })
    }

    pub fn can_accept_peer(&mut self, peer_id: &PeerId, now_unix: u64) -> bool {
        let penalty = self.peer_penalties.entry(*peer_id).or_default();
        penalty.clear_expired_ban(now_unix);
        !penalty.is_banned(now_unix) && penalty.can_reconnect(now_unix, &self.connection_limits)
    }

    pub fn register_peer_disconnect(&mut self, peer_id: &PeerId, now_unix: u64) {
        let penalty = self.peer_penalties.entry(*peer_id).or_default();
        penalty.register_disconnect(now_unix);
    }

    pub fn register_invalid_message(&mut self, peer_id: &PeerId, now_unix: u64) -> bool {
        let penalty = self.peer_penalties.entry(*peer_id).or_default();
        penalty.register_invalid_message(now_unix, &self.connection_limits)
    }

    pub fn publish_message(&mut self, topic: &str, message: &Message) -> Result<MessageId> {
        let topic = IdentTopic::new(topic);
        let bytes = message.to_bytes()?;
        Ok(self.swarm.behaviour_mut().gossipsub.publish(topic, bytes)?)
    }

    pub fn publish_transaction(&mut self, transaction: &ChainTransaction) -> Result<MessageId> {
        let network_transaction = blockchain_transaction_to_network_transaction(transaction);
        self.publish_message(
            TRANSACTIONS_TOPIC_NAME,
            &Message::NewTransaction(network_transaction),
        )
    }

    pub fn publish_proposed_block(&mut self, block: &ChainBlock) -> Result<MessageId> {
        let network_block = blockchain_block_to_network_block(block);
        self.publish_message(BLOCKS_TOPIC_NAME, &Message::NewBlock(network_block))
    }

    pub fn publish_vote(&mut self, vote: &ConsensusVote) -> Result<MessageId> {
        let network_vote = consensus_vote_to_network_vote(vote);
        self.publish_message(VOTES_TOPIC_NAME, &Message::Vote(network_vote))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blockchain::types::{BlockHeader, FIXED_TRANSACTION_FEE, TransactionType};
    use crate::{consensus::create_signed_vote, crypto::generate_keypair};

    fn test_config() -> NetworkConfig {
        let mut config = NetworkConfig::default();
        config.listen_address = "/ip4/127.0.0.1/tcp/0".to_string();
        config.seed_nodes = vec![];
        config
    }

    fn sample_chain_transaction() -> ChainTransaction {
        ChainTransaction {
            tx_type: TransactionType::Transfer,
            from: vec![1; 32],
            to: vec![2; 32],
            amount: 42,
            fee: FIXED_TRANSACTION_FEE,
            nonce: 7,
            source_shard: 0,
            destination_shard: 0,
            signature: vec![9; 64],
            data: Some(b"network-transaction".to_vec()),
        }
    }

    fn sample_chain_block() -> ChainBlock {
        ChainBlock {
            header: BlockHeader {
                version: 1,
                previous_hash: vec![0; 32],
                merkle_root: vec![1; 32],
                timestamp: 1_710_000_000,
                height: 12,
                validator: vec![7; 32],
                signature: vec![6; 64],
            },
            transactions: vec![sample_chain_transaction()],
            receipts: vec![],
        }
    }

    #[tokio::test]
    async fn clean_peer_can_connect() {
        let mut node = PrimsNode::new(test_config()).unwrap();
        let peer = PeerId::random();

        assert!(node.can_accept_peer(&peer, 1_000));
    }

    #[tokio::test]
    async fn peer_is_temporarily_banned_after_invalid_messages() {
        let mut node = PrimsNode::new(test_config()).unwrap();
        let peer = PeerId::random();
        let now = 2_000;

        for _ in 1..node.connection_limits.max_invalid_messages {
            assert!(!node.register_invalid_message(&peer, now));
        }

        assert!(node.register_invalid_message(&peer, now));
        assert!(!node.can_accept_peer(&peer, now + 1));
        assert!(node.can_accept_peer(&peer, now + node.connection_limits.temporary_ban_secs));
    }

    #[tokio::test]
    async fn reconnect_delay_is_checked_by_node() {
        let mut node = PrimsNode::new(test_config()).unwrap();
        let peer = PeerId::random();
        let now = 3_000;

        node.register_peer_disconnect(&peer, now);

        assert!(!node.can_accept_peer(&peer, now + 1));
        assert!(node.can_accept_peer(&peer, now + node.connection_limits.reconnect_delay_secs));
    }

    #[test]
    fn blockchain_block_to_network_block_preserves_height_and_transactions() {
        let block = sample_chain_block();
        let network_block = blockchain_block_to_network_block(&block);

        assert_eq!(network_block.height, block.header.height);
        assert_eq!(network_block.transactions.len(), block.transactions.len());
        assert_eq!(
            network_block.transactions[0].payload,
            bincode::serialize(&block.transactions[0])
                .expect("serialize transaction for comparison")
        );
        assert!(!network_block.hash.is_empty());
        assert!(!network_block.transactions[0].id.is_empty());
    }

    #[test]
    fn consensus_vote_to_network_vote_preserves_signature_and_payload() {
        let pair = generate_keypair();
        let vote =
            create_signed_vote(15, &[4; 32], true, &pair.secret_key).expect("create signed vote");
        let network_vote = consensus_vote_to_network_vote(&vote);

        assert_eq!(network_vote.height, vote.height);
        assert_eq!(network_vote.block_hash, vote.block_hash);
        assert_eq!(network_vote.voter, vote.voter);
        assert_eq!(network_vote.approve, vote.approve);
        assert_eq!(network_vote.signature, vote.signature);
    }

    #[test]
    fn decode_hex_32_accepts_valid_32_byte_hex() {
        let value = "11".repeat(32);
        let decoded = decode_hex_32("TEST_KEY", &value).expect("valid 32-byte hex should decode");

        assert_eq!(decoded, [0x11; 32]);
    }

    #[test]
    fn decode_hex_32_rejects_invalid_length() {
        let value = "11".repeat(31);
        let error = decode_hex_32("TEST_KEY", &value).expect_err("31-byte hex should be rejected");

        assert!(
            error
                .to_string()
                .contains("TEST_KEY doit contenir exactement 32 octets"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn decode_hex_32_rejects_non_hex_input() {
        let error = decode_hex_32("TEST_KEY", "zz").expect_err("non-hex input should be rejected");

        assert!(
            error
                .to_string()
                .contains("TEST_KEY doit être une chaîne hexadécimale valide"),
            "unexpected error: {error}"
        );
    }
}
