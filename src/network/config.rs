use serde::{Deserialize, Serialize};
use std::env;

use crate::network::connection::ConnectionLimits;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConfig {
    pub listen_address: String,
    pub seed_nodes: Vec<String>,
    pub external_address: Option<String>,
    pub connection_limits: ConnectionLimits,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        let listen_address = env::var("PRIMS_LISTEN_ADDRESS")
            .unwrap_or_else(|_| "/ip4/0.0.0.0/tcp/7001".to_string());

        let seed_nodes = env::var("PRIMS_SEED_NODES")
            .ok()
            .map(|value| {
                value
                    .split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(String::from)
                    .collect()
            })
            .unwrap_or_else(Vec::new);

        let external_address = env::var("PRIMS_EXTERNAL_ADDRESS")
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());

        let connection_limits = ConnectionLimits {
            max_established_incoming: parse_env_usize("PRIMS_MAX_ESTABLISHED_INCOMING", 32),
            max_established_outgoing: parse_env_usize("PRIMS_MAX_ESTABLISHED_OUTGOING", 32),
            reconnect_delay_secs: parse_env_u64("PRIMS_RECONNECT_DELAY_SECS", 5),
            temporary_ban_secs: parse_env_u64("PRIMS_TEMPORARY_BAN_SECS", 60),
            max_invalid_messages: parse_env_u32("PRIMS_MAX_INVALID_MESSAGES", 3),
        };

        Self {
            listen_address,
            seed_nodes,
            external_address,
            connection_limits,
        }
    }
}

fn parse_env_usize(name: &str, default: usize) -> usize {
    env::var(name)
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(default)
}

fn parse_env_u64(name: &str, default: u64) -> u64 {
    env::var(name)
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(default)
}

fn parse_env_u32(name: &str, default: u32) -> u32 {
    env::var(name)
        .ok()
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(default)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_contains_connection_limits() {
        let config = NetworkConfig::default();

        assert_eq!(config.connection_limits.max_established_incoming, 32);
        assert_eq!(config.connection_limits.max_established_outgoing, 32);
        assert_eq!(config.connection_limits.reconnect_delay_secs, 5);
        assert_eq!(config.connection_limits.temporary_ban_secs, 60);
        assert_eq!(config.connection_limits.max_invalid_messages, 3);
    }
}
