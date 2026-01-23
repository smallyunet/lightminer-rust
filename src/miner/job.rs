//! Block header construction for mining

use crate::protocol::Job;
use anyhow::{Context, Result};

/// Partially built block header template
#[derive(Debug, Clone)]
pub struct BlockHeader {
    pub version: [u8; 4],
    pub prev_hash: [u8; 32],
    pub merkle_root_partial: Vec<u8>,
    pub ntime: [u8; 4],
    pub nbits: [u8; 4],
}

/// Decode hex string to bytes
pub fn hex_decode(hex: &str) -> Result<Vec<u8>> {
    hex::decode(hex).context("Failed to decode hex string")
}

/// Build the coinbase transaction
fn build_coinbase(job: &Job, extranonce1: &str, extranonce2: &str) -> Result<Vec<u8>> {
    let coinbase1 = hex_decode(&job.coinbase1)?;
    let extranonce1_bytes = hex_decode(extranonce1)?;
    let extranonce2_bytes = hex_decode(extranonce2)?;
    let coinbase2 = hex_decode(&job.coinbase2)?;

    let mut coinbase = Vec::new();
    coinbase.extend_from_slice(&coinbase1);
    coinbase.extend_from_slice(&extranonce1_bytes);
    coinbase.extend_from_slice(&extranonce2_bytes);
    coinbase.extend_from_slice(&coinbase2);

    Ok(coinbase)
}

/// Compute merkle root from coinbase and merkle branches
fn compute_merkle_root(coinbase: &[u8], merkle_branches: &[String]) -> Result<[u8; 32]> {
    use sha2::{Digest, Sha256};

    // Double SHA256 of coinbase
    let mut current = {
        let first = Sha256::digest(coinbase);
        let second = Sha256::digest(&first);
        let mut arr = [0u8; 32];
        arr.copy_from_slice(&second);
        arr
    };

    // Combine with each merkle branch
    for branch in merkle_branches {
        let branch_bytes = hex_decode(branch)?;
        let mut combined = Vec::with_capacity(64);
        combined.extend_from_slice(&current);
        combined.extend_from_slice(&branch_bytes);

        let first = Sha256::digest(&combined);
        let second = Sha256::digest(&first);
        current.copy_from_slice(&second);
    }

    Ok(current)
}

/// Build the block header template (without nonce and final merkle root)
pub fn build_header_template(job: &Job) -> Result<BlockHeader> {
    // Parse version (little-endian)
    let version_bytes = hex_decode(&job.version)?;
    let mut version = [0u8; 4];
    if version_bytes.len() >= 4 {
        version.copy_from_slice(&version_bytes[..4]);
    }

    // Parse prev_hash (needs byte reversal for proper format)
    let prev_hash_bytes = hex_decode(&job.prev_hash)?;
    let mut prev_hash = [0u8; 32];
    if prev_hash_bytes.len() >= 32 {
        prev_hash.copy_from_slice(&prev_hash_bytes[..32]);
    }

    // Parse ntime
    let ntime_bytes = hex_decode(&job.ntime)?;
    let mut ntime = [0u8; 4];
    if ntime_bytes.len() >= 4 {
        ntime.copy_from_slice(&ntime_bytes[..4]);
    }

    // Parse nbits
    let nbits_bytes = hex_decode(&job.nbits)?;
    let mut nbits = [0u8; 4];
    if nbits_bytes.len() >= 4 {
        nbits.copy_from_slice(&nbits_bytes[..4]);
    }

    Ok(BlockHeader {
        version,
        prev_hash,
        merkle_root_partial: Vec::new(), // Will be computed with extranonce2
        ntime,
        nbits,
    })
}

/// Build the full 80-byte block header
pub fn build_full_header(
    template: &BlockHeader,
    extranonce2: &str,
    job: &Job,
    nonce: u32,
    extranonce1: &str,
) -> Result<[u8; 80]> {
    // Build coinbase with the extranonce2
    let coinbase = build_coinbase(job, extranonce1, extranonce2)?;

    // Compute merkle root
    let merkle_root = compute_merkle_root(&coinbase, &job.merkle_branches)?;

    // Assemble 80-byte header
    let mut header = [0u8; 80];

    // Version (4 bytes)
    header[0..4].copy_from_slice(&template.version);

    // Previous block hash (32 bytes)
    header[4..36].copy_from_slice(&template.prev_hash);

    // Merkle root (32 bytes)
    header[36..68].copy_from_slice(&merkle_root);

    // Timestamp/ntime (4 bytes)
    header[68..72].copy_from_slice(&template.ntime);

    // Bits/difficulty (4 bytes)
    header[72..76].copy_from_slice(&template.nbits);

    // Nonce (4 bytes, little-endian)
    header[76..80].copy_from_slice(&nonce.to_le_bytes());

    Ok(header)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hex_decode() {
        let result = hex_decode("deadbeef").unwrap();
        assert_eq!(result, vec![0xde, 0xad, 0xbe, 0xef]);
    }

    #[test]
    fn test_hex_decode_empty() {
        let result = hex_decode("").unwrap();
        assert!(result.is_empty());
    }
}
