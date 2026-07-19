use std::fs::File;
use std::hash::Hasher;
use std::io::Read;
use std::path::Path;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use xxhash_rust::xxh3::Xxh3;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArchiveIntegrity {
    pub size: u64,
    pub xxh3_64: String,
}

#[derive(Debug, Error)]
pub enum ArchiveIntegrityError {
    #[error("Archive integrity I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error(
        "Archive integrity mismatch: expected {expected_size} bytes/{expected_hash}, \
         got {actual_size} bytes/{actual_hash}"
    )]
    Mismatch {
        expected_size: u64,
        expected_hash: String,
        actual_size: u64,
        actual_hash: String,
    },
}

impl ArchiveIntegrity {
    /// Stream a file once and calculate both its byte size and XXH3-64 digest.
    ///
    /// Time is O(n) and additional space is O(1) for an n-byte Archive.
    pub fn from_file(path: &Path) -> Result<Self, ArchiveIntegrityError> {
        let mut file = File::open(path)?;
        let mut buffer = [0_u8; 64 * 1024];
        let mut hasher = Xxh3::new();
        let mut size = 0_u64;
        loop {
            let read = file.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            hasher.write(&buffer[..read]);
            size = size
                .checked_add(read as u64)
                .ok_or_else(|| std::io::Error::other("Archive size overflow"))?;
        }
        Ok(Self {
            size,
            xxh3_64: format!("{:016x}", hasher.finish()),
        })
    }

    pub fn verify_file(&self, path: &Path) -> Result<(), ArchiveIntegrityError> {
        let actual = Self::from_file(path)?;
        if actual == *self {
            return Ok(());
        }
        Err(ArchiveIntegrityError::Mismatch {
            expected_size: self.size,
            expected_hash: self.xxh3_64.clone(),
            actual_size: actual.size,
            actual_hash: actual.xxh3_64,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_integrity_covers_size_and_hash() {
        let root = temp_dir::TempDir::new().unwrap();
        let path = root.path().join("snapshot.7z");
        std::fs::write(&path, b"archive-bytes").unwrap();

        let integrity = ArchiveIntegrity::from_file(&path).unwrap();

        assert_eq!(integrity.size, 13);
        assert_eq!(integrity.xxh3_64.len(), 16);
        integrity.verify_file(&path).unwrap();
    }

    #[test]
    fn verification_rejects_same_size_corruption() {
        let root = temp_dir::TempDir::new().unwrap();
        let path = root.path().join("snapshot.7z");
        std::fs::write(&path, b"before").unwrap();
        let integrity = ArchiveIntegrity::from_file(&path).unwrap();
        std::fs::write(&path, b"after!").unwrap();

        assert!(matches!(
            integrity.verify_file(&path),
            Err(ArchiveIntegrityError::Mismatch { .. })
        ));
    }
}
