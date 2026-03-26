use anyhow::Result;
use libp2p::{
    Multiaddr, PeerId, Swarm, SwarmBuilder,
    gossipsub::{IdentTopic, MessageId},
    noise, tcp, yamux,
};
use std::collections::HashMap;

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

impl PrimsNode {
    pub fn new(config: NetworkConfig) -> Result<Self> {
        let mut swarm = SwarmBuilder::with_new_identity()
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
}
