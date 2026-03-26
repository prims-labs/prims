use anyhow::{Result, anyhow};
use libp2p::{
    gossipsub::{self, IdentTopic, MessageAuthenticity},
    identity::Keypair,
};

pub const BLOCKS_TOPIC_NAME: &str = "prims.blocks";
pub const TRANSACTIONS_TOPIC_NAME: &str = "prims.transactions";
pub const VOTES_TOPIC_NAME: &str = "prims.votes";

pub fn blocks_topic() -> IdentTopic {
    IdentTopic::new(BLOCKS_TOPIC_NAME)
}

pub fn transactions_topic() -> IdentTopic {
    IdentTopic::new(TRANSACTIONS_TOPIC_NAME)
}

pub fn votes_topic() -> IdentTopic {
    IdentTopic::new(VOTES_TOPIC_NAME)
}

pub fn build_gossipsub(local_key: &Keypair) -> Result<gossipsub::Behaviour> {
    let config = gossipsub::Config::default();
    let mut behaviour =
        gossipsub::Behaviour::new(MessageAuthenticity::Signed(local_key.clone()), config)
            .map_err(|e| anyhow!(e))?;

    behaviour
        .subscribe(&blocks_topic())
        .map_err(|e| anyhow!(e))?;
    behaviour
        .subscribe(&transactions_topic())
        .map_err(|e| anyhow!(e))?;
    behaviour
        .subscribe(&votes_topic())
        .map_err(|e| anyhow!(e))?;

    Ok(behaviour)
}
