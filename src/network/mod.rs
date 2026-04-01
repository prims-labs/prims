//! Couche réseau P2P de Prims.
//!
//! Ce module regroupe la configuration réseau, le comportement `libp2p`,
//! la gestion des connexions, le gossip applicatif, les messages sérialisés
//! et le nœud réseau principal.

pub mod connection;
pub mod message;

pub mod behaviour;
pub mod config;
pub mod gossip;
pub mod node;
