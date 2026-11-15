//! Clean-room game port binary cursor and VFS archive mounts.
//!
//! Provides zero-panic endian-aware binary parsing for Xbox 360 PowerPC (`Endian::Big`)
//! and PC x86/x64 (`Endian::Little`) assets, plus `port://` VFS mounts.

use foreplay_ir::PortArchiveMount;
use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};

/// Endianness for binary reading.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Endian {
    Little,
    Big,
}

/// Errors occurring during binary parsing.
#[derive(Clone, PartialEq, Eq, Debug, thiserror::Error)]
pub enum BinaryReadError {
    #[error("unexpected end of file: requested {requested} bytes, but only {available} available at offset {position}")]
    UnexpectedEof {
        requested: usize,
        available: usize,
        position: usize,
    },
    #[error("invalid alignment {alignment} at offset {position}")]
    InvalidAlignment { alignment: usize, position: usize },
    #[error("invalid UTF-8 data at offset {position}")]
    InvalidUtf8 { position: usize },
    #[error("null terminator not found for C string starting at offset {position}")]
    NullTerminatorNotFound { position: usize },
    #[error("corrupt archive format: {0}")]
    CorruptArchive(String),
}

/// Safe zero-panic binary reader cursor supporting both Big-Endian (Xbox 360 PowerPC)
/// and Little-Endian (PC) game archives and ROM assets.
#[derive(Clone, PartialEq, Debug)]
pub struct BinaryReader<'a> {
    data: &'a [u8],
    cursor: usize,
    endian: Endian,
}

impl<'a> BinaryReader<'a> {
    pub fn new(data: &'a [u8], endian: Endian) -> Self {
        Self {
            data,
            cursor: 0,
            endian,
        }
    }

    #[inline]
    pub fn endian(&self) -> Endian {
        self.endian
    }

    #[inline]
    pub fn set_endian(&mut self, endian: Endian) {
        self.endian = endian;
    }

    #[inline]
    pub fn position(&self) -> usize {
        self.cursor
    }

    #[inline]
    pub fn remaining(&self) -> usize {
        self.data.len().saturating_sub(self.cursor)
    }

    #[inline]
    pub fn is_eof(&self) -> bool {
        self.cursor >= self.data.len()
    }

    pub fn seek(&mut self, pos: usize) -> Result<(), BinaryReadError> {
        if pos > self.data.len() {
            return Err(BinaryReadError::UnexpectedEof {
                requested: pos.saturating_sub(self.cursor),
                available: self.remaining(),
                position: self.cursor,
            });
        }
        self.cursor = pos;
        Ok(())
    }

    pub fn skip(&mut self, count: usize) -> Result<(), BinaryReadError> {
        let new_pos = self.cursor.checked_add(count).ok_or_else(|| {
            BinaryReadError::UnexpectedEof {
                requested: count,
                available: self.remaining(),
                position: self.cursor,
            }
        })?;
        self.seek(new_pos)
    }

    /// Skips padding bytes until the cursor is aligned to `alignment` bytes.
    pub fn align(&mut self, alignment: usize) -> Result<(), BinaryReadError> {
        if alignment == 0 || (alignment & (alignment - 1)) != 0 {
            return Err(BinaryReadError::InvalidAlignment {
                alignment,
                position: self.cursor,
            });
        }
        let remainder = self.cursor % alignment;
        if remainder != 0 {
            let padding = alignment - remainder;
            self.skip(padding)?;
        }
        Ok(())
    }

    pub fn read_slice(&mut self, len: usize) -> Result<&'a [u8], BinaryReadError> {
        if self.remaining() < len {
            return Err(BinaryReadError::UnexpectedEof {
                requested: len,
                available: self.remaining(),
                position: self.cursor,
            });
        }
        let start = self.cursor;
        self.cursor += len;
        Ok(&self.data[start..start + len])
    }

    pub fn read_u8(&mut self) -> Result<u8, BinaryReadError> {
        let slice = self.read_slice(1)?;
        Ok(slice[0])
    }

    pub fn read_i8(&mut self) -> Result<i8, BinaryReadError> {
        let b = self.read_u8()?;
        Ok(b as i8)
    }

    pub fn read_u16(&mut self) -> Result<u16, BinaryReadError> {
        let bytes = self.read_slice(2)?;
        let arr = [bytes[0], bytes[1]];
        Ok(match self.endian {
            Endian::Little => u16::from_le_bytes(arr),
            Endian::Big => u16::from_be_bytes(arr),
        })
    }

    pub fn read_i16(&mut self) -> Result<i16, BinaryReadError> {
        let bytes = self.read_slice(2)?;
        let arr = [bytes[0], bytes[1]];
        Ok(match self.endian {
            Endian::Little => i16::from_le_bytes(arr),
            Endian::Big => i16::from_be_bytes(arr),
        })
    }

    pub fn read_u32(&mut self) -> Result<u32, BinaryReadError> {
        let bytes = self.read_slice(4)?;
        let arr = [bytes[0], bytes[1], bytes[2], bytes[3]];
        Ok(match self.endian {
            Endian::Little => u32::from_le_bytes(arr),
            Endian::Big => u32::from_be_bytes(arr),
        })
    }

    pub fn read_i32(&mut self) -> Result<i32, BinaryReadError> {
        let bytes = self.read_slice(4)?;
        let arr = [bytes[0], bytes[1], bytes[2], bytes[3]];
        Ok(match self.endian {
            Endian::Little => i32::from_le_bytes(arr),
            Endian::Big => i32::from_be_bytes(arr),
        })
    }

    pub fn read_u64(&mut self) -> Result<u64, BinaryReadError> {
        let bytes = self.read_slice(8)?;
        let arr: [u8; 8] = bytes
            .try_into()
            .map_err(|_| BinaryReadError::UnexpectedEof {
                requested: 8,
                available: self.remaining(),
                position: self.cursor,
            })?;
        Ok(match self.endian {
            Endian::Little => u64::from_le_bytes(arr),
            Endian::Big => u64::from_be_bytes(arr),
        })
    }

    pub fn read_i64(&mut self) -> Result<i64, BinaryReadError> {
        let bytes = self.read_slice(8)?;
        let arr: [u8; 8] = bytes
            .try_into()
            .map_err(|_| BinaryReadError::UnexpectedEof {
                requested: 8,
                available: self.remaining(),
                position: self.cursor,
            })?;
        Ok(match self.endian {
            Endian::Little => i64::from_le_bytes(arr),
            Endian::Big => i64::from_be_bytes(arr),
        })
    }

    pub fn read_f32(&mut self) -> Result<f32, BinaryReadError> {
        let bits = self.read_u32()?;
        Ok(f32::from_bits(bits))
    }

    pub fn read_f64(&mut self) -> Result<f64, BinaryReadError> {
        let bits = self.read_u64()?;
        Ok(f64::from_bits(bits))
    }

    pub fn read_vec2(&mut self) -> Result<[f32; 2], BinaryReadError> {
        Ok([self.read_f32()?, self.read_f32()?])
    }

    pub fn read_vec3(&mut self) -> Result<[f32; 3], BinaryReadError> {
        Ok([self.read_f32()?, self.read_f32()?, self.read_f32()?])
    }

    pub fn read_quat(&mut self) -> Result<[f32; 4], BinaryReadError> {
        Ok([
            self.read_f32()?,
            self.read_f32()?,
            self.read_f32()?,
            self.read_f32()?,
        ])
    }

    pub fn read_fourcc(&mut self) -> Result<[u8; 4], BinaryReadError> {
        let slice = self.read_slice(4)?;
        Ok([slice[0], slice[1], slice[2], slice[3]])
    }

    pub fn read_c_string(&mut self) -> Result<&'a str, BinaryReadError> {
        let start = self.cursor;
        let mut idx = start;
        while idx < self.data.len() {
            if self.data[idx] == 0 {
                let slice = &self.data[start..idx];
                self.cursor = idx + 1;
                return std::str::from_utf8(slice)
                    .map_err(|_| BinaryReadError::InvalidUtf8 { position: start });
            }
            idx += 1;
        }
        Err(BinaryReadError::NullTerminatorNotFound { position: start })
    }
}

/// In-memory or serialized `.pak` archive mount implementing `foreplay_ir::PortArchiveMount`.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct SimplePakMount {
    entries: BTreeMap<String, Vec<u8>>,
}

const SPAK_MAGIC: &[u8; 4] = b"SPAK";

impl SimplePakMount {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_entries(entries: impl IntoIterator<Item = (String, Vec<u8>)>) -> Self {
        let mut map = BTreeMap::new();
        for (path, bytes) in entries {
            let normalized = Self::normalize_path(&path);
            map.insert(normalized, bytes);
        }
        Self { entries: map }
    }

    pub fn insert(&mut self, path: impl Into<String>, bytes: Vec<u8>) {
        let norm = Self::normalize_path(&path.into());
        self.entries.insert(norm, bytes);
    }

    pub fn contains(&self, path: &str) -> bool {
        self.entries.contains_key(&Self::normalize_path(path))
    }

    pub fn get(&self, path: &str) -> Option<&[u8]> {
        self.entries
            .get(&Self::normalize_path(path))
            .map(|v| v.as_slice())
    }

    fn normalize_path(path: &str) -> String {
        let mut s = path.trim();
        if let Some(stripped) = s.strip_prefix("port://") {
            s = stripped;
        }
        s.trim_start_matches('/').to_string()
    }

    /// Serializes entries into binary `.pak` format with header, table of contents, and payloads.
    pub fn build_pak(&self, endian: Endian) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(SPAK_MAGIC);
        match endian {
            Endian::Little => out.push(1),
            Endian::Big => out.push(2),
        }
        out.push(0); // reserved
        out.push(0); // reserved
        out.push(0); // reserved

        let entry_count = self.entries.len() as u32;
        let count_bytes = match endian {
            Endian::Little => entry_count.to_le_bytes(),
            Endian::Big => entry_count.to_be_bytes(),
        };
        out.extend_from_slice(&count_bytes);

        // Pre-calculate header + TOC size to compute payload offsets
        // Each entry in TOC:
        // name_len: u16
        // name: name_len bytes
        // offset: u32
        // size: u32
        let mut toc_size = 0usize;
        for name in self.entries.keys() {
            toc_size += 2 + name.len() + 4 + 4;
        }

        let payload_base = out.len() + toc_size;
        let mut current_offset = payload_base as u32;

        let mut payloads = Vec::new();

        for (name, data) in &self.entries {
            let name_bytes = name.as_bytes();
            let name_len = name_bytes.len() as u16;
            let name_len_bytes = match endian {
                Endian::Little => name_len.to_le_bytes(),
                Endian::Big => name_len.to_be_bytes(),
            };
            out.extend_from_slice(&name_len_bytes);
            out.extend_from_slice(name_bytes);

            let offset_bytes = match endian {
                Endian::Little => current_offset.to_le_bytes(),
                Endian::Big => current_offset.to_be_bytes(),
            };
            out.extend_from_slice(&offset_bytes);

            let size = data.len() as u32;
            let size_bytes = match endian {
                Endian::Little => size.to_le_bytes(),
                Endian::Big => size.to_be_bytes(),
            };
            out.extend_from_slice(&size_bytes);

            payloads.extend_from_slice(data);
            current_offset += size;
        }

        out.extend_from_slice(&payloads);
        out
    }

    /// Deserializes a `.pak` binary byte stream into `SimplePakMount`.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, BinaryReadError> {
        let mut reader = BinaryReader::new(bytes, Endian::Little);
        let magic = reader.read_fourcc()?;
        if &magic != SPAK_MAGIC {
            return Err(BinaryReadError::CorruptArchive(format!(
                "invalid magic {magic:?}, expected {SPAK_MAGIC:?}"
            )));
        }

        let endian_flag = reader.read_u8()?;
        let endian = match endian_flag {
            1 => Endian::Little,
            2 => Endian::Big,
            other => {
                return Err(BinaryReadError::CorruptArchive(format!(
                    "unknown endian flag {other}"
                )))
            }
        };
        reader.set_endian(endian);

        // Skip 3 reserved bytes
        reader.skip(3)?;

        let entry_count = reader.read_u32()? as usize;
        let mut toc = Vec::with_capacity(entry_count);

        for _ in 0..entry_count {
            let name_len = reader.read_u16()? as usize;
            let name_bytes = reader.read_slice(name_len)?;
            let name = std::str::from_utf8(name_bytes)
                .map_err(|_| BinaryReadError::CorruptArchive("invalid utf-8 entry name".into()))?
                .to_string();
            let offset = reader.read_u32()? as usize;
            let size = reader.read_u32()? as usize;
            toc.push((name, offset, size));
        }

        let mut mount = SimplePakMount::new();
        for (name, offset, size) in toc {
            if offset + size > bytes.len() {
                return Err(BinaryReadError::CorruptArchive(format!(
                    "entry '{name}' payload out of bounds ({offset}+{size} > {})",
                    bytes.len()
                )));
            }
            let data = bytes[offset..offset + size].to_vec();
            mount.insert(name, data);
        }

        Ok(mount)
    }
}

impl PortArchiveMount for SimplePakMount {
    fn scheme_prefix(&self) -> &str {
        "port://"
    }

    fn list_entries(&self) -> Vec<String> {
        self.entries.keys().cloned().collect()
    }

    fn read_bytes(&self, virtual_path: &str) -> Result<Vec<u8>, String> {
        let norm = Self::normalize_path(virtual_path);
        self.entries
            .get(&norm)
            .cloned()
            .ok_or_else(|| format!("file not found in pak: '{norm}'"))
    }
}

/// Filesystem directory-backed clean-room archive mount (`port://`).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct DirectoryPortMount {
    base_dir: PathBuf,
}

impl DirectoryPortMount {
    pub fn new(base_dir: impl Into<PathBuf>) -> Self {
        Self {
            base_dir: base_dir.into(),
        }
    }

    pub fn base_dir(&self) -> &Path {
        &self.base_dir
    }

    fn sanitize_relative_path(&self, virtual_path: &str) -> Result<PathBuf, String> {
        let mut s = virtual_path.trim();
        if let Some(stripped) = s.strip_prefix("port://") {
            s = stripped;
        }
        let stripped = s.trim_start_matches('/');
        let rel_path = Path::new(stripped);

        // Security check: reject path traversal
        for component in rel_path.components() {
            match component {
                Component::Normal(_) => {}
                Component::CurDir => {}
                Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                    return Err(format!("path traversal rejected: '{virtual_path}'"));
                }
            }
        }

        Ok(self.base_dir.join(rel_path))
    }

    fn collect_files_recursive(dir: &Path, base: &Path, list: &mut Vec<String>) {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_dir() {
                    Self::collect_files_recursive(&p, base, list);
                } else if p.is_file()
                    && let Ok(rel) = p.strip_prefix(base)
                {
                    list.push(rel.to_string_lossy().to_string());
                }
            }
        }
    }
}

impl PortArchiveMount for DirectoryPortMount {
    fn scheme_prefix(&self) -> &str {
        "port://"
    }

    fn list_entries(&self) -> Vec<String> {
        let mut list = Vec::new();
        Self::collect_files_recursive(&self.base_dir, &self.base_dir, &mut list);
        list.sort();
        list
    }

    fn read_bytes(&self, virtual_path: &str) -> Result<Vec<u8>, String> {
        let full_path = self.sanitize_relative_path(virtual_path)?;
        std::fs::read(&full_path).map_err(|err| format!("failed reading {full_path:?}: {err}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn binary_reader_little_and_big_endian() {
        let data = [
            0x01, 0x02, 0x03, 0x04, // u32
            0x42, 0x2a, 0x00, 0x00, // f32: 42.5 in BE
            0x48, 0x65, 0x6c, 0x6c, 0x6f, 0x00, // "Hello\0"
            0xaa, 0xbb, // trailing
        ];

        // Little endian
        let mut r_le = BinaryReader::new(&data, Endian::Little);
        assert_eq!(r_le.read_u32().unwrap(), 0x04030201);

        // Big endian (PowerPC / Xbox 360)
        let mut r_be = BinaryReader::new(&data, Endian::Big);
        assert_eq!(r_be.read_u32().unwrap(), 0x01020304);
        let f = r_be.read_f32().unwrap();
        assert_eq!(f, 42.5);
        let s = r_be.read_c_string().unwrap();
        assert_eq!(s, "Hello");
    }

    #[test]
    fn binary_reader_alignment_and_bounds_check() {
        let data = [1, 2, 3, 4, 5, 6, 7, 8];
        let mut reader = BinaryReader::new(&data, Endian::Little);
        assert_eq!(reader.read_u8().unwrap(), 1);
        assert_eq!(reader.position(), 1);
        reader.align(4).unwrap();
        assert_eq!(reader.position(), 4);
        assert_eq!(reader.read_u32().unwrap(), 0x08070605);
        assert!(reader.is_eof());
        assert!(reader.read_u8().is_err());
    }

    #[test]
    fn simple_pak_mount_roundtrip() {
        let mut mount = SimplePakMount::new();
        mount.insert("tracks/plaza.fscn", b"scene plaza".to_vec());
        mount.insert("models/skater.glb", b"glTF binary dummy".to_vec());

        // Test PortArchiveMount interface
        assert_eq!(mount.scheme_prefix(), "port://");
        let entries = mount.list_entries();
        assert_eq!(entries.len(), 2);
        assert_eq!(
            mount.read_bytes("port://tracks/plaza.fscn").unwrap(),
            b"scene plaza"
        );

        // Build and parse PAK (test big-endian format)
        let pak_bytes = mount.build_pak(Endian::Big);
        let parsed_mount = SimplePakMount::from_bytes(&pak_bytes).unwrap();
        assert_eq!(
            parsed_mount.read_bytes("tracks/plaza.fscn").unwrap(),
            b"scene plaza"
        );
        assert_eq!(
            parsed_mount.read_bytes("models/skater.glb").unwrap(),
            b"glTF binary dummy"
        );
    }

    #[test]
    fn binary_reader_hostile_and_truncated_inputs_never_panic() {
        // Truncated buffer
        let truncated = [0xde, 0xad];
        let mut reader = BinaryReader::new(&truncated, Endian::Big);
        assert_eq!(reader.read_u16().unwrap(), 0xdead);
        // Expecting u32 when only 0 bytes remaining
        assert!(matches!(reader.read_u32(), Err(BinaryReadError::UnexpectedEof { .. })));
        assert!(matches!(reader.read_vec3(), Err(BinaryReadError::UnexpectedEof { .. })));

        // Unclosed C string
        let no_null = b"NotTerminated";
        let mut str_reader = BinaryReader::new(no_null, Endian::Big);
        assert!(matches!(str_reader.read_c_string(), Err(BinaryReadError::NullTerminatorNotFound { .. })));

        // Corrupt PAK magic
        let bad_magic = b"NOPE\x02\0\0\0\x00\x00\x00\x01";
        assert!(matches!(SimplePakMount::from_bytes(bad_magic), Err(BinaryReadError::CorruptArchive(_))));

        // Corrupt PAK with out-of-bounds payload offset
        let mut pak_bytes = SimplePakMount::from_entries(vec![("test.txt".into(), b"data".to_vec())]).build_pak(Endian::Big);
        // Tamper with data length to truncate
        pak_bytes.truncate(pak_bytes.len() - 3);
        assert!(SimplePakMount::from_bytes(&pak_bytes).is_err());
    }
}
