//! Godot-style `pck_embed` self-contained binary trailer (`FPCKEXEC`).
//!
//! Layout of the appended trailer (32 bytes total at the end of the executable):
//! - Magic prefix: `b"FPCK"` (4 bytes)
//! - Pack offset: `u64 LE` (8 bytes) — byte index in the binary where package data begins
//! - Pack length: `u64 LE` (8 bytes) — length of package data in bytes
//! - FNV-1a hash: `u64 LE` (8 bytes) — 64-bit FNV-1a checksum of the package data
//! - Magic suffix: `b"EXEC"` (4 bytes)

use std::path::Path;

pub const EMBEDDED_TRAILER_MAGIC_PREFIX: &[u8; 4] = b"FPCK";
pub const EMBEDDED_TRAILER_MAGIC_SUFFIX: &[u8; 4] = b"EXEC";
pub const EMBEDDED_TRAILER_SIZE: usize = 32;

/// Errors arising during embedded package extraction or validation.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum EmbeddedPackError {
    #[error("I/O error: {0}")]
    Io(String),

    #[error("invalid embedded pack bounds: offset={offset}, len={len}, file_size={file_size}")]
    InvalidBounds {
        offset: u64,
        len: u64,
        file_size: usize,
    },

    #[error("checksum mismatch: expected {expected:#018x}, computed {computed:#018x}")]
    ChecksumMismatch {
        expected: u64,
        computed: u64,
    },

    #[error("corrupt trailer: {0}")]
    CorruptTrailer(String),
}

/// Computes the 64-bit FNV-1a hash of a byte slice.
#[must_use]
pub fn fnv1a_64(bytes: &[u8]) -> u64 {
    const FNV_OFFSET_BASIS: u64 = 0xcbf29ce484222325;
    const FNV_PRIME: u64 = 0x100000001b3;

    let mut hash = FNV_OFFSET_BASIS;
    for &b in bytes {
        hash ^= b as u64;
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

/// Appends package bytes and the 32-byte `FPCKEXEC` trailer to a base executable.
///
/// Returns a new vector containing `base_executable_bytes || pack_bytes || trailer`.
#[must_use]
pub fn embed_pack_trailer(base_executable_bytes: &[u8], pack_bytes: &[u8]) -> Vec<u8> {
    let pack_offset = base_executable_bytes.len() as u64;
    let pack_len = pack_bytes.len() as u64;
    let hash = fnv1a_64(pack_bytes);

    let total_size = base_executable_bytes.len() + pack_bytes.len() + EMBEDDED_TRAILER_SIZE;
    let mut out = Vec::with_capacity(total_size);

    out.extend_from_slice(base_executable_bytes);
    out.extend_from_slice(pack_bytes);

    // 32-byte trailer:
    // [0..4]   "FPCK"
    // [4..12]  pack_offset (u64 LE)
    // [12..20] pack_len (u64 LE)
    // [20..28] fnv1a_hash (u64 LE)
    // [28..32] "EXEC"
    out.extend_from_slice(EMBEDDED_TRAILER_MAGIC_PREFIX);
    out.extend_from_slice(&pack_offset.to_le_bytes());
    out.extend_from_slice(&pack_len.to_le_bytes());
    out.extend_from_slice(&hash.to_le_bytes());
    out.extend_from_slice(EMBEDDED_TRAILER_MAGIC_SUFFIX);

    out
}

/// Extracts embedded `.fpck` package bytes from raw binary bytes.
///
/// Returns:
/// - `Ok(None)` if the binary does not contain the 32-byte `FPCK...EXEC` trailer.
/// - `Ok(Some(pack_bytes))` if a valid, checksum-verified embedded pack is present.
/// - `Err(EmbeddedPackError)` if trailer is present but bounds or checksum are corrupt or adversarial.
pub fn extract_embedded_pack_from_bytes(exe_bytes: &[u8]) -> Result<Option<Vec<u8>>, EmbeddedPackError> {
    if exe_bytes.len() < EMBEDDED_TRAILER_SIZE {
        return Ok(None);
    }

    let trailer_start = exe_bytes.len() - EMBEDDED_TRAILER_SIZE;
    let trailer = &exe_bytes[trailer_start..];

    // Check magic prefix and suffix
    if &trailer[0..4] != EMBEDDED_TRAILER_MAGIC_PREFIX || &trailer[28..32] != EMBEDDED_TRAILER_MAGIC_SUFFIX {
        return Ok(None);
    }

    let pack_offset = u64::from_le_bytes(
        trailer[4..12]
            .try_into()
            .map_err(|_| EmbeddedPackError::CorruptTrailer("invalid pack_offset bytes".into()))?,
    );
    let pack_len = u64::from_le_bytes(
        trailer[12..20]
            .try_into()
            .map_err(|_| EmbeddedPackError::CorruptTrailer("invalid pack_len bytes".into()))?,
    );
    let expected_hash = u64::from_le_bytes(
        trailer[20..28]
            .try_into()
            .map_err(|_| EmbeddedPackError::CorruptTrailer("invalid hash bytes".into()))?,
    );

    // Bounds checking with checked_add to protect against integer overflows
    let pack_end = pack_offset
        .checked_add(pack_len)
        .ok_or(EmbeddedPackError::InvalidBounds {
            offset: pack_offset,
            len: pack_len,
            file_size: exe_bytes.len(),
        })?;

    if pack_end > trailer_start as u64 {
        return Err(EmbeddedPackError::InvalidBounds {
            offset: pack_offset,
            len: pack_len,
            file_size: exe_bytes.len(),
        });
    }

    let pack_slice = &exe_bytes[pack_offset as usize..pack_end as usize];
    let computed_hash = fnv1a_64(pack_slice);

    if computed_hash != expected_hash {
        return Err(EmbeddedPackError::ChecksumMismatch {
            expected: expected_hash,
            computed: computed_hash,
        });
    }

    Ok(Some(pack_slice.to_vec()))
}

/// Detects if the current running executable has an embedded `.fpck` package.
pub fn detect_current_exe_embedded_pack() -> Result<Option<Vec<u8>>, EmbeddedPackError> {
    let current_exe = std::env::current_exe()
        .map_err(|e| EmbeddedPackError::Io(format!("failed to locate current_exe: {e}")))?;
    extract_embedded_pack_from_path(&current_exe)
}

/// Reads a file from disk and attempts to extract an embedded `.fpck` package.
pub fn extract_embedded_pack_from_path(path: &Path) -> Result<Option<Vec<u8>>, EmbeddedPackError> {
    let bytes = std::fs::read(path)
        .map_err(|e| EmbeddedPackError::Io(format!("failed to read file at {}: {e}", path.display())))?;
    extract_embedded_pack_from_bytes(&bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embed_and_extract_roundtrip() {
        let base_exe = b"MOCK_ELF_BINARY_HEADER_AND_CODE";
        let pack_data = b"FPCK_TEST_PACKAGE_DATA_0123456789ABCDEF";

        let embedded = embed_pack_trailer(base_exe, pack_data);
        assert_eq!(embedded.len(), base_exe.len() + pack_data.len() + 32);

        let extracted = extract_embedded_pack_from_bytes(&embedded)
            .expect("extraction succeeds")
            .expect("embedded pack found");

        assert_eq!(extracted, pack_data);
    }

    #[test]
    fn plain_binary_without_trailer_returns_none() {
        let plain_exe = b"MZ_WINDOWS_EXE_WITHOUT_TRAILER_1234567890";
        let result = extract_embedded_pack_from_bytes(plain_exe).expect("scan succeeds");
        assert!(result.is_none());
    }

    #[test]
    fn corrupted_checksum_is_detected() {
        let base_exe = b"HEADER";
        let pack_data = b"REAL_PACK_DATA";
        let mut embedded = embed_pack_trailer(base_exe, pack_data);

        // Tamper with one byte inside the pack data
        embedded[base_exe.len() + 2] ^= 0xFF;

        let err = extract_embedded_pack_from_bytes(&embedded).expect_err("tampering detected");
        match err {
            EmbeddedPackError::ChecksumMismatch { expected, computed } => {
                assert_ne!(expected, computed);
            }
            other => panic!("expected ChecksumMismatch, got {other:?}"),
        }
    }

    #[test]
    fn adversarial_overflow_bounds_rejected() {
        let mut bad_data = vec![0u8; 100];
        let trailer_start = 100 - 32;

        bad_data[trailer_start..trailer_start + 4].copy_from_slice(b"FPCK");
        // offset = u64::MAX - 10
        bad_data[trailer_start + 4..trailer_start + 12].copy_from_slice(&(u64::MAX - 10).to_le_bytes());
        // len = 100
        bad_data[trailer_start + 12..trailer_start + 20].copy_from_slice(&100u64.to_le_bytes());
        // hash
        bad_data[trailer_start + 20..trailer_start + 28].copy_from_slice(&0u64.to_le_bytes());
        bad_data[trailer_start + 28..trailer_start + 32].copy_from_slice(b"EXEC");

        let err = extract_embedded_pack_from_bytes(&bad_data).expect_err("overflow caught");
        assert!(matches!(err, EmbeddedPackError::InvalidBounds { .. }));
    }

    #[test]
    fn adversarial_out_of_bounds_offset_rejected() {
        let mut bad_data = vec![0u8; 100];
        let trailer_start = 100 - 32;

        bad_data[trailer_start..trailer_start + 4].copy_from_slice(b"FPCK");
        // offset = 80 (inside trailer!)
        bad_data[trailer_start + 4..trailer_start + 12].copy_from_slice(&80u64.to_le_bytes());
        // len = 10
        bad_data[trailer_start + 12..trailer_start + 20].copy_from_slice(&10u64.to_le_bytes());
        // hash
        bad_data[trailer_start + 20..trailer_start + 28].copy_from_slice(&0u64.to_le_bytes());
        bad_data[trailer_start + 28..trailer_start + 32].copy_from_slice(b"EXEC");

        let err = extract_embedded_pack_from_bytes(&bad_data).expect_err("out of bounds caught");
        assert!(matches!(err, EmbeddedPackError::InvalidBounds { .. }));
    }
}
