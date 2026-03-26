use futures::StreamExt;
use jsonrpsee::server::ServerBuilder;
use libp2p::{
    gossipsub::{self, IdentTopic},
    swarm::SwarmEvent,
};
use prims::{
    api::{RpcState, build_rpc_module},
    consensus::Mempool,
    network::{
        behaviour::PrimsBehaviourEvent, config::NetworkConfig, message::Message, node::PrimsNode,
    },
    storage::RocksDbStorage,
};
use std::{
    env, fs,
    path::Path,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = NetworkConfig::default();
    let mut node = PrimsNode::new(config)?;

    let rpc_bind_address =
        env::var("PRIMS_RPC_ADDRESS").unwrap_or_else(|_| "127.0.0.1:7002".to_string());
    let storage_path =
        env::var("PRIMS_DB_PATH").unwrap_or_else(|_| "./data/prims-rocksdb".to_string());

    if let Some(parent) = Path::new(&storage_path).parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }

    let storage = Arc::new(RocksDbStorage::open(&storage_path)?);
    let mempool = Arc::new(Mempool::new());

    let rpc_module = build_rpc_module(RpcState::new(Arc::clone(&storage), Arc::clone(&mempool)))?;
    let rpc_server = ServerBuilder::default().build(&rpc_bind_address).await?;
    let rpc_local_addr = rpc_server.local_addr()?;
    let _rpc_handle = rpc_server.start(rpc_module);

    let topic = IdentTopic::new("prims-testnet");
    node.swarm.behaviour_mut().gossipsub.subscribe(&topic)?;

    let publish_message = env::var("PRIMS_PUBLISH_MESSAGE").ok();
    let mut message_sent = false;

    println!("PRIMS node started");
    println!("Local PeerId: {}", node.local_peer_id);
    println!("Listen address: {}", node.config.listen_address);
    println!("Seed nodes: {:?}", node.config.seed_nodes);
    println!("Subscribed topic: prims-testnet");
    println!("Storage path: {}", storage_path);
    println!("RPC server listening on http://{}", rpc_local_addr);
    println!(
        "RPC methods: get_block, get_transaction, send_transaction, get_balance, get_info, get_validators, get_note_commitments"
    );

    loop {
        match node.swarm.select_next_some().await {
            SwarmEvent::NewListenAddr { address, .. } => {
                println!("Listening on {}", address);
            }
            SwarmEvent::ConnectionEstablished { peer_id, .. } => {
                println!("Connection established with {}", peer_id);
            }
            SwarmEvent::Behaviour(PrimsBehaviourEvent::Gossipsub(
                gossipsub::Event::Subscribed {
                    peer_id,
                    topic: subscribed_topic,
                },
            )) => {
                println!("Peer subscribed: {} -> {}", peer_id, subscribed_topic);

                if !message_sent && subscribed_topic == topic.hash() {
                    if let Some(message) = publish_message.as_deref() {
                        let published_at_ms =
                            SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis();
                        let payload = format!("{}|{}", published_at_ms, message);

                        match node
                            .swarm
                            .behaviour_mut()
                            .gossipsub
                            .publish(topic.clone(), payload.as_bytes())
                        {
                            Ok(_) => {
                                println!("Published message: {}", message);
                                println!("Published at ms: {}", published_at_ms);
                                message_sent = true;
                            }
                            Err(e) => {
                                println!("Publish error: {}", e);
                            }
                        }
                    }
                }
            }
            SwarmEvent::Behaviour(PrimsBehaviourEvent::Gossipsub(gossipsub::Event::Message {
                propagation_source,
                message_id: _,
                message,
            })) => match Message::from_bytes(&message.data) {
                Ok(Message::NewTransaction(network_transaction)) => {
                    match bincode::deserialize(&network_transaction.payload) {
                        Ok(transaction) => {
                            mempool.add_transaction_async(transaction).await;
                            println!("Dispatched transaction");
                        }
                        Err(error) => {
                            println!(
                                "Failed to deserialize network transaction {} from {}: {}",
                                network_transaction.id, propagation_source, error
                            );
                        }
                    }
                }
                Ok(decoded_message) => {
                    println!(
                        "Received protocol message from {}: {:?}",
                        propagation_source, decoded_message
                    );
                }
                Err(_) => {
                    let raw = String::from_utf8_lossy(&message.data);

                    if let Some((published_at_raw, body)) = raw.split_once('|') {
                        match published_at_raw.parse::<u128>() {
                            Ok(published_at_ms) => {
                                let received_at_ms =
                                    SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis();
                                let latency_ms = received_at_ms.saturating_sub(published_at_ms);

                                println!(
                                    "Received gossipsub message from {}: {}",
                                    propagation_source, body
                                );
                                println!("Published at ms: {}", published_at_ms);
                                println!("Received at ms: {}", received_at_ms);
                                println!("Propagation latency ms: {}", latency_ms);
                            }
                            Err(_) => {
                                println!(
                                    "Received gossipsub message from {}: {}",
                                    propagation_source, raw
                                );
                            }
                        }
                    } else {
                        println!(
                            "Received gossipsub message from {}: {}",
                            propagation_source, raw
                        );
                    }
                }
            },
            SwarmEvent::Behaviour(event) => {
                println!("Behaviour event: {:?}", event);
            }
            event => {
                println!("Swarm event: {:?}", event);
            }
        }
    }
}
