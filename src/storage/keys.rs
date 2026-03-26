pub const BLOCK_PREFIX: &str = "b:";
pub const HEIGHT_INDEX_PREFIX: &str = "h:";
pub const TRANSACTION_PREFIX: &str = "t:";
pub const ACCOUNT_PREFIX: &str = "a:";
pub const STAKE_PREFIX: &str = "s:";
pub const CONTRACT_CODE_PREFIX: &str = "c:";
pub const CONTRACT_STORAGE_PREFIX: &str = "m:";
pub const ANON_NOTE_PREFIX: &str = "n:";
pub const VIEWING_NOTE_INDEX_PREFIX: &str = "v:";
pub const NOTE_MERKLE_ROOT_PREFIX: &str = "r:";

pub fn block_key(height: u64) -> Vec<u8> {
    format!("{BLOCK_PREFIX}{height}").into_bytes()
}

pub fn height_index_key(hash: &[u8]) -> Vec<u8> {
    prefixed_key(HEIGHT_INDEX_PREFIX, hash)
}

pub fn transaction_key(hash: &[u8]) -> Vec<u8> {
    prefixed_key(TRANSACTION_PREFIX, hash)
}

pub fn account_key(address: &[u8]) -> Vec<u8> {
    prefixed_key(ACCOUNT_PREFIX, address)
}

pub fn stake_key(address: &[u8]) -> Vec<u8> {
    prefixed_key(STAKE_PREFIX, address)
}

pub fn contract_key(address: &[u8]) -> Vec<u8> {
    prefixed_key(CONTRACT_CODE_PREFIX, address)
}

pub fn contract_code_key(address: &[u8]) -> Vec<u8> {
    contract_key(address)
}

pub fn contract_storage_prefix(address: &[u8]) -> Vec<u8> {
    let mut result = Vec::with_capacity(CONTRACT_STORAGE_PREFIX.len() + address.len() + 1);
    result.extend_from_slice(CONTRACT_STORAGE_PREFIX.as_bytes());
    result.extend_from_slice(address);
    result.push(b':');
    result
}

pub fn contract_storage_key(address: &[u8], key: &[u8]) -> Vec<u8> {
    let mut result = contract_storage_prefix(address);
    result.extend_from_slice(key);
    result
}

pub fn anonymous_note_key(commitment: &[u8]) -> Vec<u8> {
    prefixed_key(ANON_NOTE_PREFIX, commitment)
}

pub fn viewing_hint_prefix(viewing_hint: &[u8]) -> Vec<u8> {
    let mut result = Vec::with_capacity(VIEWING_NOTE_INDEX_PREFIX.len() + viewing_hint.len() + 1);
    result.extend_from_slice(VIEWING_NOTE_INDEX_PREFIX.as_bytes());
    result.extend_from_slice(viewing_hint);
    result.push(b':');
    result
}

pub fn viewing_hint_note_key(viewing_hint: &[u8], commitment: &[u8]) -> Vec<u8> {
    let mut result = viewing_hint_prefix(viewing_hint);
    result.extend_from_slice(commitment);
    result
}

pub fn note_merkle_root_key(shard_id: Option<u16>) -> Vec<u8> {
    match shard_id {
        Some(shard_id) => format!("{NOTE_MERKLE_ROOT_PREFIX}shard:{shard_id}").into_bytes(),
        None => format!("{NOTE_MERKLE_ROOT_PREFIX}global").into_bytes(),
    }
}

fn prefixed_key(prefix: &str, suffix: &[u8]) -> Vec<u8> {
    let mut result = Vec::with_capacity(prefix.len() + suffix.len());
    result.extend_from_slice(prefix.as_bytes());
    result.extend_from_slice(suffix);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn storage_key_prefixes_match_roadmap() {
        assert_eq!(String::from_utf8(block_key(42)).unwrap(), "b:42");
        assert_eq!(height_index_key(b"hash"), b"h:hash".to_vec());
        assert_eq!(transaction_key(b"tx"), b"t:tx".to_vec());
        assert_eq!(account_key(b"addr"), b"a:addr".to_vec());
        assert_eq!(stake_key(b"validator"), b"s:validator".to_vec());
        assert_eq!(contract_key(b"contract"), b"c:contract".to_vec());
        assert_eq!(contract_code_key(b"contract"), b"c:contract".to_vec());
        assert_eq!(
            contract_storage_prefix(b"contract"),
            b"m:contract:".to_vec()
        );
        assert_eq!(
            contract_storage_key(b"contract", b"balance"),
            b"m:contract:balance".to_vec()
        );
        assert_eq!(anonymous_note_key(b"commitment"), b"n:commitment".to_vec());
        assert_eq!(viewing_hint_prefix(b"hint"), b"v:hint:".to_vec());
        assert_eq!(
            viewing_hint_note_key(b"hint", b"commitment"),
            b"v:hint:commitment".to_vec()
        );
        assert_eq!(note_merkle_root_key(None), b"r:global".to_vec());
        assert_eq!(note_merkle_root_key(Some(7)), b"r:shard:7".to_vec());
    }
}
