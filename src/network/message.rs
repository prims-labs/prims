use anyhow::Result;
use serde::{Deserialize, Serialize};

/// Transaction temporaire pour la couche réseau.
/// Elle sera remplacée plus tard par la vraie structure métier.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Transaction {
    pub id: String,
    pub payload: Vec<u8>,
}

/// Bloc temporaire pour la couche réseau.
/// Il sera remplacé plus tard par la vraie structure de bloc.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Block {
    pub height: u64,
    pub hash: String,
    pub transactions: Vec<Transaction>,
}

/// Vote réseau signé.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Vote {
    pub height: u64,
    pub block_hash: Vec<u8>,
    pub voter: Vec<u8>,
    pub approve: bool,
    pub signature: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum Message {
    Ping,
    Pong,
    NewTransaction(Transaction),
    NewBlock(Block),
    GetBlocks(u64),
    Blocks(Vec<Block>),
    Vote(Vote),
}

impl Message {
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        Ok(bincode::serialize(self)?)
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        Ok(bincode::deserialize(bytes)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn message_roundtrip_with_bincode() {
        let message = Message::GetBlocks(42);

        let bytes = message.to_bytes().unwrap();
        let decoded = Message::from_bytes(&bytes).unwrap();

        assert_eq!(message, decoded);
    }
}
