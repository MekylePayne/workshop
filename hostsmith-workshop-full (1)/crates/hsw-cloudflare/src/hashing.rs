//! Asset hashing for the Pages upload flow (TL-03).
//!
//! The upload protocol identifies each asset by a hash of its contents;
//! Wrangler uses blake3 (via blake2b/blake3 wasm historically). The recipe
//! lives behind this one function so it can follow the documented API
//! without touching any caller.

/// Hashes file contents for the Pages asset-upload flow (blake3, lowercase
/// hex digest).
pub fn asset_hash(contents: &[u8]) -> String {
    blake3::hash(contents).to_hex().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_is_stable_hex_and_content_sensitive() {
        let first = asset_hash(b"HostSmith");
        let second = asset_hash(b"HostSmith");
        let other = asset_hash(b"HostSmith Workshop");
        assert_eq!(first, second);
        assert_ne!(first, other);
        assert_eq!(first.len(), 64, "blake3 hex digest is 64 characters");
        assert!(first.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
    }

    #[test]
    fn empty_content_hashes_deterministically() {
        assert_eq!(asset_hash(b""), asset_hash(b""));
    }
}
