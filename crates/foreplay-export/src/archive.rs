//! Deterministic `.fpck` (Foreplay Package) archive pack and unpack implementation.
//!
//! Binary layout:
//! - Magic: `b"FPCK"` (4 bytes)
//! - Version: `u32` (1)
//! - Entry Count: `u32`
//! - TOC Offset: `u64`
//! - TOC Length: `u64`
//! - File Data blobs...
//! - TOC Entries:
//!   - Path Length: `u16`
//!   - Path: UTF-8 bytes
//!   - Offset: `u64`
//!   - Size: `u64`
//!   - FNV-1a 64 Checksum: `u64`

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const FPCK_MAGIC: &[u8; 4] = b"FPCK";
pub const FPCK_CURRENT_VERSION: u32 = 1;

/// Errors produced during `.fpck` archive packing or unpacking.
#[derive(Clone, PartialEq, Eq, Debug, thiserror::Error, Serialize, Deserialize)]
pub enum FpckError {
    #[error("invalid magic bytes: expected 'FPCK', got {0:?}")]
    InvalidMagic(Vec<u8>),
    #[error("unsupported fpck version: {0}")]
    UnsupportedVersion(u32),
    #[error("path traversal detected in archive entry: '{0}'")]
    PathTraversal(String),
    #[error("checksum mismatch for '{path}': expected {expected:#x}, computed {actual:#x}")]
    ChecksumMismatch {
        path: String,
        expected: u64,
        actual: u64,
    },
    #[error("corrupt archive structure: {0}")]
    CorruptArchive(String),
    #[error("unexpected end of archive bytes (needed {needed} bytes at offset {offset})")]
    UnexpectedEof { offset: usize, needed: usize },
}

/// Computes the 64-bit FNV-1a checksum of a byte slice.
pub fn fnv1a_64(bytes: &[u8]) -> u64 {
    const FNV_OFFSET_BASIS: u64 = 0xcbf29ce484222325;
    const FNV_PRIME: u64 = 0x100000001b3;

    let mut hash = FNV_OFFSET_BASIS;
    for &byte in bytes {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

/// Metadata entry in the table of contents (TOC).
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct TocEntry {
    pub path: String,
    pub offset: u64,
    pub size: u64,
    pub checksum: u64,
}

/// High-level `.fpck` archive packer and unpacker.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct FpckArchive;

impl FpckArchive {
    /// Packs a collection of files into a contiguous `.fpck` binary archive.
    /// Files are stored deterministically sorted by path name.
    pub fn pack_bytes(files: &BTreeMap<String, Vec<u8>>) -> Result<Vec<u8>, FpckError> {
        // Validate all paths against path traversal
        for path in files.keys() {
            validate_archive_path(path)?;
        }

        let mut output = Vec::new();
        // Header placeholder: 4 bytes magic + 4 bytes version + 4 bytes count + 8 bytes toc_offset + 8 bytes toc_len = 28 bytes
        output.extend_from_slice(FPCK_MAGIC);
        output.extend_from_slice(&FPCK_CURRENT_VERSION.to_le_bytes());
        output.extend_from_slice(&(files.len() as u32).to_le_bytes());
        let toc_offset_idx = output.len();
        output.extend_from_slice(&0u64.to_le_bytes()); // placeholder toc_offset
        output.extend_from_slice(&0u64.to_le_bytes()); // placeholder toc_len

        let mut toc_entries = Vec::with_capacity(files.len());

        // Write data blobs
        for (path, data) in files {
            let offset = output.len() as u64;
            let size = data.len() as u64;
            let checksum = fnv1a_64(data);
            output.extend_from_slice(data);

            toc_entries.push(TocEntry {
                path: path.clone(),
                offset,
                size,
                checksum,
            });
        }

        // Write TOC
        let toc_offset = output.len() as u64;
        let mut toc_bytes = Vec::new();

        for entry in &toc_entries {
            let path_bytes = entry.path.as_bytes();
            if path_bytes.len() > u16::MAX as usize {
                return Err(FpckError::CorruptArchive(format!(
                    "path too long ({} bytes): '{}'",
                    path_bytes.len(),
                    entry.path
                )));
            }
            toc_bytes.extend_from_slice(&(path_bytes.len() as u16).to_le_bytes());
            toc_bytes.extend_from_slice(path_bytes);
            toc_bytes.extend_from_slice(&entry.offset.to_le_bytes());
            toc_bytes.extend_from_slice(&entry.size.to_le_bytes());
            toc_bytes.extend_from_slice(&entry.checksum.to_le_bytes());
        }

        let toc_len = toc_bytes.len() as u64;
        output.extend_from_slice(&toc_bytes);

        // Patch TOC offset and length in header
        output[toc_offset_idx..toc_offset_idx + 8].copy_from_slice(&toc_offset.to_le_bytes());
        output[toc_offset_idx + 8..toc_offset_idx + 16].copy_from_slice(&toc_len.to_le_bytes());

        Ok(output)
    }

    /// Unpacks an `.fpck` binary slice, verifying header, TOC, checksums, and path safety.
    pub fn unpack_bytes(archive: &[u8]) -> Result<BTreeMap<String, Vec<u8>>, FpckError> {
        if archive.len() < 28 {
            return Err(FpckError::UnexpectedEof {
                offset: 0,
                needed: 28,
            });
        }

        // Verify magic
        if &archive[0..4] != FPCK_MAGIC {
            return Err(FpckError::InvalidMagic(archive[0..4].to_vec()));
        }

        let version = u32::from_le_bytes(archive[4..8].try_into().unwrap());
        if version != FPCK_CURRENT_VERSION {
            return Err(FpckError::UnsupportedVersion(version));
        }

        let entry_count = u32::from_le_bytes(archive[8..12].try_into().unwrap()) as usize;
        let toc_offset = u64::from_le_bytes(archive[12..20].try_into().unwrap()) as usize;
        let toc_len = u64::from_le_bytes(archive[20..28].try_into().unwrap()) as usize;

        if toc_offset + toc_len > archive.len() {
            return Err(FpckError::UnexpectedEof {
                offset: toc_offset,
                needed: toc_len,
            });
        }

        let toc_slice = &archive[toc_offset..toc_offset + toc_len];
        let mut cursor = 0;
        let mut files = BTreeMap::new();

        for _ in 0..entry_count {
            if cursor + 2 > toc_slice.len() {
                return Err(FpckError::UnexpectedEof {
                    offset: toc_offset + cursor,
                    needed: 2,
                });
            }
            let path_len = u16::from_le_bytes(toc_slice[cursor..cursor + 2].try_into().unwrap()) as usize;
            cursor += 2;

            if cursor + path_len + 24 > toc_slice.len() {
                return Err(FpckError::UnexpectedEof {
                    offset: toc_offset + cursor,
                    needed: path_len + 24,
                });
            }

            let path_str = std::str::from_utf8(&toc_slice[cursor..cursor + path_len])
                .map_err(|e| FpckError::CorruptArchive(format!("invalid utf-8 path: {e}")))?;
            cursor += path_len;

            validate_archive_path(path_str)?;

            let data_offset = u64::from_le_bytes(toc_slice[cursor..cursor + 8].try_into().unwrap()) as usize;
            cursor += 8;
            let data_size = u64::from_le_bytes(toc_slice[cursor..cursor + 8].try_into().unwrap()) as usize;
            cursor += 8;
            let expected_checksum = u64::from_le_bytes(toc_slice[cursor..cursor + 8].try_into().unwrap());
            cursor += 8;

            if data_offset + data_size > archive.len() {
                return Err(FpckError::UnexpectedEof {
                    offset: data_offset,
                    needed: data_size,
                });
            }

            let file_data = &archive[data_offset..data_offset + data_size];
            let actual_checksum = fnv1a_64(file_data);

            if actual_checksum != expected_checksum {
                return Err(FpckError::ChecksumMismatch {
                    path: path_str.to_string(),
                    expected: expected_checksum,
                    actual: actual_checksum,
                });
            }

            files.insert(path_str.to_string(), file_data.to_vec());
        }

        Ok(files)
    }
}

/// Enforces path security to prevent arbitrary directory traversal.
pub fn validate_archive_path(path: &str) -> Result<(), FpckError> {
    if path.is_empty() {
        return Err(FpckError::PathTraversal("empty path is invalid".into()));
    }
    if path.starts_with('/') || path.starts_with('\\') {
        return Err(FpckError::PathTraversal(format!("absolute path rejected: '{path}'")));
    }
    for component in path.split(['/', '\\']) {
        if component == ".." {
            return Err(FpckError::PathTraversal(format!("parent traversal '..' rejected: '{path}'")));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fpck_pack_and_unpack_roundtrip() {
        let mut files = BTreeMap::new();
        files.insert(
            "project.foreplay".into(),
            b"[foreplay]\nname=\"SkaterTest\"\n".to_vec(),
        );
        files.insert(
            "scenes/main.fscn".into(),
            b"[fscn_scene]\nname = \"Main\"\n".to_vec(),
        );
        files.insert(
            "scripts/player.ts".into(),
            b"export class Player { speed = 5; }\n".to_vec(),
        );

        let packed = FpckArchive::pack_bytes(&files).unwrap();
        assert_eq!(&packed[0..4], FPCK_MAGIC);

        let unpacked = FpckArchive::unpack_bytes(&packed).unwrap();
        assert_eq!(unpacked.len(), 3);
        assert_eq!(
            unpacked.get("project.foreplay"),
            files.get("project.foreplay")
        );
        assert_eq!(unpacked.get("scenes/main.fscn"), files.get("scenes/main.fscn"));
        assert_eq!(
            unpacked.get("scripts/player.ts"),
            files.get("scripts/player.ts")
        );
    }

    #[test]
    fn fpck_rejects_path_traversal() {
        let mut files = BTreeMap::new();
        files.insert("../../etc/passwd".into(), b"malicious".to_vec());

        let err = FpckArchive::pack_bytes(&files).unwrap_err();
        assert!(matches!(err, FpckError::PathTraversal(_)));

        let mut files2 = BTreeMap::new();
        files2.insert("/root/.ssh/id_rsa".into(), b"malicious".to_vec());
        let err2 = FpckArchive::pack_bytes(&files2).unwrap_err();
        assert!(matches!(err2, FpckError::PathTraversal(_)));
    }

    #[test]
    fn fpck_checksum_verification_detects_tampering() {
        let mut files = BTreeMap::new();
        files.insert("asset.bin".into(), vec![1, 2, 3, 4, 5]);

        let mut packed = FpckArchive::pack_bytes(&files).unwrap();
        // Tamper with one byte in the data section (offset 28 is the first byte of asset.bin)
        packed[28] ^= 0xFF;

        let err = FpckArchive::unpack_bytes(&packed).unwrap_err();
        assert!(matches!(err, FpckError::ChecksumMismatch { .. }));
    }
}
