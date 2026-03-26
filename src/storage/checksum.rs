use crate::blockchain::hash::sha256;
use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CheckedPayload {
    pub checksum: Vec<u8>,
    pub payload: Vec<u8>,
}

pub fn wrap_payload(payload: &[u8]) -> Result<Vec<u8>> {
    let checked = CheckedPayload {
        checksum: sha256(payload),
        payload: payload.to_vec(),
    };

    Ok(bincode::serialize(&checked)?)
}

pub fn unwrap_payload(encoded: &[u8]) -> Result<Vec<u8>> {
    let checked: CheckedPayload = bincode::deserialize(encoded)?;
    let actual = sha256(&checked.payload);

    if actual != checked.checksum {
        bail!("checksum mismatch: critical data may be corrupted");
    }

    Ok(checked.payload)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checksum_roundtrip_preserves_payload() {
        let payload = b"prims-critical-data";
        let encoded = wrap_payload(payload).expect("wrap payload");
        let decoded = unwrap_payload(&encoded).expect("unwrap payload");

        assert_eq!(decoded, payload);
    }

    #[test]
    fn checksum_detects_corruption() {
        let payload = b"prims-critical-data";
        let mut encoded = wrap_payload(payload).expect("wrap payload");

        let last = encoded.len() - 1;
        encoded[last] ^= 0x01;

        let error = unwrap_payload(&encoded).expect_err("corruption must be detected");
        assert!(error.to_string().contains("checksum"));
    }
}
