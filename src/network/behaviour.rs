use libp2p::{
    PeerId, gossipsub,
    identity::Keypair,
    kad::{self, store::MemoryStore},
    mdns,
    swarm::NetworkBehaviour,
};

use crate::network::gossip::build_gossipsub;

#[derive(NetworkBehaviour)]
pub struct PrimsBehaviour {
    pub gossipsub: gossipsub::Behaviour,
    pub mdns: mdns::tokio::Behaviour,
    pub kademlia: kad::Behaviour<MemoryStore>,
}

impl PrimsBehaviour {
    pub fn new(
        local_key: &Keypair,
        local_peer_id: PeerId,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let gossipsub = build_gossipsub(local_key)?;

        let mdns = mdns::tokio::Behaviour::new(mdns::Config::default(), local_peer_id)?;

        let store = MemoryStore::new(local_peer_id);
        let kademlia = kad::Behaviour::new(local_peer_id, store);

        Ok(Self {
            gossipsub,
            mdns,
            kademlia,
        })
    }
}
