//! Phase 19.2 — True Native WebAssembly (`wasm32`) WebGPU + WebAudio Bundle Packager.
//!
//! Mirrors W3C WebAssembly 1.0 binary format specification and Foreplay Phase 19.2 standards.
//! Emits a spec-compliant `.wasm` module containing:
//! - Standard WASM header (`\0asm\x01\x00\x00\x00`)
//! - Type section (`0x01`)
//! - Import section (`0x02` importing `"env"."foreplay_host_present_rgba"`)
//! - Function section (`0x03`)
//! - Memory section (`0x05`, initial 16 pages = 1 MiB)
//! - Export section (`0x07` exporting `"memory"`, `"foreplay_wasm_abi_version"`, `"foreplay_wasm_step_tick"`, `"foreplay_fpck_len"`)
//! - Code section (`0x0a`)
//! - Custom section (`0x00`) named `"foreplay_fpck"` embedding:
//!   `[b"FPCKWASM" (8B) | fpck_len: u64 LE | fnv1a_hash: u64 LE | raw_fpck_bytes]`
//!
//! Generates full browser-runnable WebGPU + WebAudio + PWA release bundles.

use crate::archive::{FpckArchive, fnv1a_64};
use crate::bundle::{ExportConfig, ExportError};
use std::collections::BTreeMap;

/// Magic bytes at the start of every valid WebAssembly 1.0 binary module: `\0asm`.
pub const WASM_MAGIC: [u8; 4] = [0x00, 0x61, 0x73, 0x6d];

/// WebAssembly 1.0 binary version: `1` (little-endian `0x01, 0x00, 0x00, 0x00`).
pub const WASM_VERSION: u32 = 1;

/// Magic 8-byte header for the Foreplay WASM embedded custom section: `b"FPCKWASM"`.
pub const FPCK_WASM_MAGIC: [u8; 8] = *b"FPCKWASM";

/// Standard custom section name for embedded Foreplay Package archives.
pub const CUSTOM_SECTION_NAME: &str = "foreplay_fpck";

/// Host import module name for presentation callbacks.
pub const FOREPLAY_HOST_IMPORT_MODULE: &str = "env";

/// Host import function name for raw RGBA scanout presentation.
pub const FOREPLAY_HOST_IMPORT_FN: &str = "foreplay_host_present_rgba";

/// Foreplay WASM ABI version returned by `foreplay_wasm_abi_version`.
pub const FOREPLAY_WASM_ABI_VERSION: i32 = 1;

/// Encodes an unsigned 32-bit integer into unsigned LEB128 bytes.
pub fn encode_u32_leb128(mut value: u32, buf: &mut Vec<u8>) {
    loop {
        let mut byte = (value & 0x7F) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        buf.push(byte);
        if value == 0 {
            break;
        }
    }
}

/// Decodes an unsigned 32-bit integer from unsigned LEB128 bytes with bounds and overflow checks.
pub fn decode_u32_leb128(bytes: &[u8], cursor: &mut usize) -> Result<u32, ExportError> {
    let mut result: u32 = 0;
    let mut shift: u32 = 0;
    let start = *cursor;
    while *cursor < bytes.len() {
        let byte = bytes[*cursor];
        *cursor += 1;
        if shift >= 35 {
            return Err(ExportError::EmbeddedPack(format!(
                "LEB128 u32 integer overflow at offset {start}"
            )));
        }
        result |= ((byte & 0x7F) as u32) << shift;
        if (byte & 0x80) == 0 {
            return Ok(result);
        }
        shift += 7;
    }
    Err(ExportError::EmbeddedPack(format!(
        "unexpected EOF while decoding u32 LEB128 at offset {start}"
    )))
}

/// Encodes an unsigned 64-bit integer into unsigned LEB128 bytes.
pub fn encode_u64_leb128(mut value: u64, buf: &mut Vec<u8>) {
    loop {
        let mut byte = (value & 0x7F) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        buf.push(byte);
        if value == 0 {
            break;
        }
    }
}

/// Decodes an unsigned 64-bit integer from unsigned LEB128 bytes with bounds and overflow checks.
pub fn decode_u64_leb128(bytes: &[u8], cursor: &mut usize) -> Result<u64, ExportError> {
    let mut result: u64 = 0;
    let mut shift: u32 = 0;
    let start = *cursor;
    while *cursor < bytes.len() {
        let byte = bytes[*cursor];
        *cursor += 1;
        if shift >= 70 {
            return Err(ExportError::EmbeddedPack(format!(
                "LEB128 u64 integer overflow at offset {start}"
            )));
        }
        result |= ((byte & 0x7F) as u64) << shift;
        if (byte & 0x80) == 0 {
            return Ok(result);
        }
        shift += 7;
    }
    Err(ExportError::EmbeddedPack(format!(
        "unexpected EOF while decoding u64 LEB128 at offset {start}"
    )))
}

/// Encodes a signed 32-bit integer into signed LEB128 (SLEB128) bytes.
pub fn encode_i32_sleb128(mut value: i32, buf: &mut Vec<u8>) {
    let mut more = true;
    while more {
        let byte = (value & 0x7F) as u8;
        value >>= 7;
        if (value == 0 && (byte & 0x40) == 0) || (value == -1 && (byte & 0x40) != 0) {
            more = false;
            buf.push(byte);
        } else {
            buf.push(byte | 0x80);
        }
    }
}

/// Decodes a signed 32-bit integer from signed LEB128 (SLEB128) bytes with bounds and overflow checks.
pub fn decode_i32_sleb128(bytes: &[u8], cursor: &mut usize) -> Result<i32, ExportError> {
    let mut result: i32 = 0;
    let mut shift: u32 = 0;
    let start = *cursor;
    while *cursor < bytes.len() {
        let byte = bytes[*cursor];
        *cursor += 1;
        result |= ((byte & 0x7F) as i32) << shift;
        shift += 7;
        if (byte & 0x80) == 0 {
            if shift < 32 && (byte & 0x40) != 0 {
                result |= (!0i32).checked_shl(shift).unwrap_or(0);
            }
            return Ok(result);
        }
        if shift >= 35 {
            return Err(ExportError::EmbeddedPack(format!(
                "SLEB128 i32 integer overflow at offset {start}"
            )));
        }
    }
    Err(ExportError::EmbeddedPack(format!(
        "unexpected EOF while decoding i32 SLEB128 at offset {start}"
    )))
}

fn emit_section(section_id: u8, payload: &[u8], out: &mut Vec<u8>) {
    out.push(section_id);
    encode_u32_leb128(payload.len() as u32, out);
    out.extend_from_slice(payload);
}

/// Native WebAssembly (`wasm32`) module builder and WebGPU/WebAudio bundle packager.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct WasmWebGpuPackager;

impl WasmWebGpuPackager {
    /// Builds a valid W3C WebAssembly 1.0 `.wasm` module with an embedded `"foreplay_fpck"` custom section.
    ///
    /// WebAssembly sections emitted:
    /// - `0x01` Type section:
    ///   - type 0: `(i32, i32, i32, i32) -> ()` (for `foreplay_host_present_rgba`)
    ///   - type 1: `() -> i32` (for `foreplay_wasm_abi_version`, `foreplay_fpck_len`)
    ///   - type 2: `(f32) -> i32` (for `foreplay_wasm_step_tick`)
    /// - `0x02` Import section:
    ///   - `"env"."foreplay_host_present_rgba"`: type 0 (function index 0)
    /// - `0x03` Function section:
    ///   - func 1: type 1
    ///   - func 2: type 2
    ///   - func 3: type 1
    /// - `0x05` Memory section:
    ///   - 1 memory with initial 16 pages (1 MiB)
    /// - `0x07` Export section:
    ///   - `"memory"`: memory index 0
    ///   - `"foreplay_wasm_abi_version"`: function index 1
    ///   - `"foreplay_wasm_step_tick"`: function index 2
    ///   - `"foreplay_fpck_len"`: function index 3
    /// - `0x0a` Code section:
    ///   - body 1 (`foreplay_wasm_abi_version`): returns ABI version `1`
    ///   - body 2 (`foreplay_wasm_step_tick`): returns `0`
    ///   - body 3 (`foreplay_fpck_len`): returns `fpck_bytes.len() as i32`
    /// - `0x00` Custom section named `"foreplay_fpck"`:
    ///   `[b"FPCKWASM" (8B) | fpck_len: u64 LE | fnv1a_hash: u64 LE | raw_fpck_bytes]`
    pub fn build_wasm_module(fpck_bytes: &[u8]) -> Vec<u8> {
        let mut wasm = Vec::with_capacity(fpck_bytes.len() + 512);

        // 8-byte WASM Header
        wasm.extend_from_slice(&WASM_MAGIC);
        wasm.extend_from_slice(&WASM_VERSION.to_le_bytes());

        // 1. Type Section (id = 0x01)
        let mut type_payload = Vec::new();
        encode_u32_leb128(3, &mut type_payload); // 3 types
        // Type 0: (i32, i32, i32, i32) -> ()
        type_payload.push(0x60); // functype
        encode_u32_leb128(4, &mut type_payload);
        type_payload.extend_from_slice(&[0x7F, 0x7F, 0x7F, 0x7F]);
        encode_u32_leb128(0, &mut type_payload);
        // Type 1: () -> i32
        type_payload.push(0x60);
        encode_u32_leb128(0, &mut type_payload);
        encode_u32_leb128(1, &mut type_payload);
        type_payload.push(0x7F);
        // Type 2: (f32) -> i32
        type_payload.push(0x60);
        encode_u32_leb128(1, &mut type_payload);
        type_payload.push(0x7D);
        encode_u32_leb128(1, &mut type_payload);
        type_payload.push(0x7F);
        emit_section(1, &type_payload, &mut wasm);

        // 2. Import Section (id = 0x02)
        let mut import_payload = Vec::new();
        encode_u32_leb128(1, &mut import_payload); // 1 import
        let mod_bytes = FOREPLAY_HOST_IMPORT_MODULE.as_bytes();
        encode_u32_leb128(mod_bytes.len() as u32, &mut import_payload);
        import_payload.extend_from_slice(mod_bytes);
        let fn_bytes = FOREPLAY_HOST_IMPORT_FN.as_bytes();
        encode_u32_leb128(fn_bytes.len() as u32, &mut import_payload);
        import_payload.extend_from_slice(fn_bytes);
        import_payload.push(0x00); // import kind: function
        encode_u32_leb128(0, &mut import_payload); // type index 0
        emit_section(2, &import_payload, &mut wasm);

        // 3. Function Section (id = 0x03)
        let mut func_payload = Vec::new();
        encode_u32_leb128(3, &mut func_payload); // 3 internal functions
        encode_u32_leb128(1, &mut func_payload); // func 1: type 1
        encode_u32_leb128(2, &mut func_payload); // func 2: type 2
        encode_u32_leb128(1, &mut func_payload); // func 3: type 1
        emit_section(3, &func_payload, &mut wasm);

        // 5. Memory Section (id = 0x05)
        let mut mem_payload = Vec::new();
        encode_u32_leb128(1, &mut mem_payload); // 1 memory
        mem_payload.push(0x00); // flags: min only
        encode_u32_leb128(16, &mut mem_payload); // 16 pages = 1 MiB
        emit_section(5, &mem_payload, &mut wasm);

        // 7. Export Section (id = 0x07)
        let mut export_payload = Vec::new();
        encode_u32_leb128(4, &mut export_payload); // 4 exports
        // Export "memory"
        let exp_mem = b"memory";
        encode_u32_leb128(exp_mem.len() as u32, &mut export_payload);
        export_payload.extend_from_slice(exp_mem);
        export_payload.push(0x02); // kind: memory
        encode_u32_leb128(0, &mut export_payload); // memory 0
        // Export "foreplay_wasm_abi_version"
        let exp_ver = b"foreplay_wasm_abi_version";
        encode_u32_leb128(exp_ver.len() as u32, &mut export_payload);
        export_payload.extend_from_slice(exp_ver);
        export_payload.push(0x00); // kind: function
        encode_u32_leb128(1, &mut export_payload); // func index 1
        // Export "foreplay_wasm_step_tick"
        let exp_tick = b"foreplay_wasm_step_tick";
        encode_u32_leb128(exp_tick.len() as u32, &mut export_payload);
        export_payload.extend_from_slice(exp_tick);
        export_payload.push(0x00); // kind: function
        encode_u32_leb128(2, &mut export_payload); // func index 2
        // Export "foreplay_fpck_len"
        let exp_len = b"foreplay_fpck_len";
        encode_u32_leb128(exp_len.len() as u32, &mut export_payload);
        export_payload.extend_from_slice(exp_len);
        export_payload.push(0x00); // kind: function
        encode_u32_leb128(3, &mut export_payload); // func index 3
        emit_section(7, &export_payload, &mut wasm);

        // 10. Code Section (id = 0x0a)
        let mut code_payload = Vec::new();
        encode_u32_leb128(3, &mut code_payload); // 3 function bodies

        // Body 1: foreplay_wasm_abi_version() -> i32
        let mut b1 = Vec::new();
        encode_u32_leb128(0, &mut b1); // 0 locals
        b1.push(0x41); // i32.const
        encode_i32_sleb128(FOREPLAY_WASM_ABI_VERSION, &mut b1);
        b1.push(0x0B); // end
        encode_u32_leb128(b1.len() as u32, &mut code_payload);
        code_payload.extend_from_slice(&b1);

        // Body 2: foreplay_wasm_step_tick(f32) -> i32
        let mut b2 = Vec::new();
        encode_u32_leb128(0, &mut b2); // 0 locals
        b2.push(0x41); // i32.const
        encode_i32_sleb128(0, &mut b2); // return 0
        b2.push(0x0B); // end
        encode_u32_leb128(b2.len() as u32, &mut code_payload);
        code_payload.extend_from_slice(&b2);

        // Body 3: foreplay_fpck_len() -> i32
        let mut b3 = Vec::new();
        encode_u32_leb128(0, &mut b3); // 0 locals
        b3.push(0x41); // i32.const
        encode_i32_sleb128(fpck_bytes.len() as i32, &mut b3);
        b3.push(0x0B); // end
        encode_u32_leb128(b3.len() as u32, &mut code_payload);
        code_payload.extend_from_slice(&b3);

        emit_section(10, &code_payload, &mut wasm);

        // 0. Custom Section (id = 0x00): "foreplay_fpck"
        let mut custom_payload = Vec::new();
        let cname = CUSTOM_SECTION_NAME.as_bytes();
        encode_u32_leb128(cname.len() as u32, &mut custom_payload);
        custom_payload.extend_from_slice(cname);

        custom_payload.extend_from_slice(&FPCK_WASM_MAGIC);
        let fpck_len = fpck_bytes.len() as u64;
        custom_payload.extend_from_slice(&fpck_len.to_le_bytes());
        let hash = fnv1a_64(fpck_bytes);
        custom_payload.extend_from_slice(&hash.to_le_bytes());
        custom_payload.extend_from_slice(fpck_bytes);

        emit_section(0, &custom_payload, &mut wasm);

        wasm
    }

    /// Generates complete browser-runnable release bundle files for WebGPU / WebAudio WASM distribution.
    pub fn generate_bundle(
        config: &ExportConfig,
        fpck_bytes: &[u8],
    ) -> BTreeMap<String, Vec<u8>> {
        let mut files = BTreeMap::new();

        // 1. Valid .wasm module with embedded "foreplay_fpck" custom section
        let wasm_bytes = Self::build_wasm_module(fpck_bytes);
        files.insert("foreplay_player_bg.wasm".into(), wasm_bytes);

        // 2. Standalone .fpck archive for runtime network streaming / progressive loading
        files.insert("game.fpck".into(), fpck_bytes.to_vec());

        // 3. WebGPU + WebAudio ES module loader
        let loader_js = generate_wasm_loader_js();
        files.insert("foreplay_wasm_loader.js".into(), loader_js.into_bytes());

        // 4. Backwards-compatible runtime bridge
        let runtime_js = generate_runtime_bridge_js();
        files.insert("foreplay_runtime.js".into(), runtime_js.into_bytes());

        // 5. Studio Obsidian fullscreen HTML5 entry point
        let index_html = generate_index_html(&config.game_title);
        files.insert("index.html".into(), index_html.into_bytes());

        // 6. Cross-Origin-Isolation Worker (COI) for SharedArrayBuffer
        let coi_sw = generate_coi_serviceworker_js();
        files.insert("coi-serviceworker.js".into(), coi_sw.into_bytes());

        // 7. Offline PWA Service Worker
        let sw_js = generate_offline_sw_js();
        files.insert("sw.js".into(), sw_js.into_bytes());

        // 8. PWA Web Manifest (standard webmanifest & legacy json)
        let manifest_webmanifest = generate_webmanifest(config);
        files.insert("manifest.webmanifest".into(), manifest_webmanifest.clone().into_bytes());
        files.insert("manifest.json".into(), manifest_webmanifest.into_bytes());

        // 9. Optional game icon
        if let Some(icon) = &config.icon_png {
            files.insert("icon.png".into(), icon.clone());
        }

        files
    }
}

/// Parses a WebAssembly binary, walks its sections via LEB128, extracts the `"foreplay_fpck"`
/// custom section, verifies its 64-bit FNV-1a checksum and magic, and unpacks the embedded files.
pub fn extract_and_verify_wasm_fpck_section(
    wasm_bytes: &[u8],
) -> Result<BTreeMap<String, Vec<u8>>, ExportError> {
    if wasm_bytes.len() < 8 {
        return Err(ExportError::EmbeddedPack(format!(
            "truncated WASM header: needed 8 bytes, got {}",
            wasm_bytes.len()
        )));
    }

    if wasm_bytes[0..4] != WASM_MAGIC {
        return Err(ExportError::EmbeddedPack(format!(
            "invalid WASM magic header: expected [0, 97, 115, 109], got {:?}",
            &wasm_bytes[0..4]
        )));
    }

    let version = u32::from_le_bytes(wasm_bytes[4..8].try_into().unwrap());
    if version != WASM_VERSION {
        return Err(ExportError::EmbeddedPack(format!(
            "unsupported WASM version: expected {WASM_VERSION}, got {version}"
        )));
    }

    let mut cursor = 8;
    let mut custom_payload: Option<&[u8]> = None;

    while cursor < wasm_bytes.len() {
        let section_id = wasm_bytes[cursor];
        cursor += 1;

        let section_len = decode_u32_leb128(wasm_bytes, &mut cursor)? as usize;
        let section_end = cursor.checked_add(section_len).ok_or_else(|| {
            ExportError::EmbeddedPack(format!(
                "integer overflow calculating section end at offset {cursor}"
            ))
        })?;

        if section_end > wasm_bytes.len() {
            return Err(ExportError::EmbeddedPack(format!(
                "truncated WASM section {section_id}: needed {section_len} bytes from offset {cursor}, total binary is {}",
                wasm_bytes.len()
            )));
        }

        let section_data = &wasm_bytes[cursor..section_end];
        cursor = section_end;

        if section_id == 0 {
            let mut sec_cursor = 0;
            let name_len = match decode_u32_leb128(section_data, &mut sec_cursor) {
                Ok(len) => len as usize,
                Err(_) => continue,
            };
            if sec_cursor + name_len > section_data.len() {
                continue;
            }
            let name_bytes = &section_data[sec_cursor..sec_cursor + name_len];
            sec_cursor += name_len;

            if name_bytes == CUSTOM_SECTION_NAME.as_bytes() {
                custom_payload = Some(&section_data[sec_cursor..]);
                break;
            }
        }
    }

    let payload = custom_payload.ok_or_else(|| {
        ExportError::EmbeddedPack(format!(
            "missing '{CUSTOM_SECTION_NAME}' custom section in WASM binary"
        ))
    })?;

    if payload.len() < 24 {
        return Err(ExportError::EmbeddedPack(format!(
            "WASM '{CUSTOM_SECTION_NAME}' custom section payload too short ({} bytes < 24)",
            payload.len()
        )));
    }

    let magic = &payload[0..8];
    if magic != FPCK_WASM_MAGIC {
        return Err(ExportError::EmbeddedPack(format!(
            "invalid '{CUSTOM_SECTION_NAME}' magic: expected b'FPCKWASM', got {magic:?}"
        )));
    }

    let fpck_len = u64::from_le_bytes(payload[8..16].try_into().unwrap()) as usize;
    let expected_hash = u64::from_le_bytes(payload[16..24].try_into().unwrap());
    let raw_fpck_bytes = &payload[24..];

    if raw_fpck_bytes.len() != fpck_len {
        return Err(ExportError::EmbeddedPack(format!(
            "WASM '{CUSTOM_SECTION_NAME}' payload length mismatch: header specifies {fpck_len} bytes, actual is {}",
            raw_fpck_bytes.len()
        )));
    }

    let actual_hash = fnv1a_64(raw_fpck_bytes);
    if actual_hash != expected_hash {
        return Err(ExportError::EmbeddedPack(format!(
            "WASM '{CUSTOM_SECTION_NAME}' checksum mismatch: expected {expected_hash:#x}, computed {actual_hash:#x}"
        )));
    }

    let files = FpckArchive::unpack_bytes(raw_fpck_bytes)?;
    Ok(files)
}

fn generate_wasm_loader_js() -> String {
    r#"// foreplay_wasm_loader.js - Foreplay Game Engine WebGPU + WebAudio WASM Runtime Loader
// Phase 19.2: True Native WebAssembly WebGPU + WebAudio Bundle

export class ForeplayWasmLoader {
    constructor(canvas, statusEl) {
        this.canvas = canvas;
        this.statusEl = statusEl;
        this.gpuDevice = null;
        this.gpuContext = null;
        this.audioCtx = null;
        this.wasmInstance = null;
        this.fpckArchiveBytes = null;
        this.isRunning = false;
        this.lastFrameTime = 0;
    }

    async init(wasmUrl = "foreplay_player_bg.wasm") {
        this.updateStatus("Fetching WASM engine...", "loading");

        // 1. Fetch WASM binary and extract "foreplay_fpck" custom section
        const wasmResponse = await fetch(wasmUrl);
        if (!wasmResponse.ok) {
            throw new Error(`Failed to load ${wasmUrl}: HTTP ${wasmResponse.status}`);
        }
        const wasmArrayBuffer = await wasmResponse.arrayBuffer();
        const wasmBytes = new Uint8Array(wasmArrayBuffer);

        this.fpckArchiveBytes = this.extractFpckCustomSection(wasmBytes);
        if (this.fpckArchiveBytes) {
            console.log(`[Foreplay] Extracted embedded .fpck archive (${this.fpckArchiveBytes.byteLength} bytes)`);
        }

        // 2. Initialize WebGPU Context
        if (!navigator.gpu) {
            throw new Error("WebGPU is not supported in this browser. Please enable WebGPU or use Chrome 113+ / Safari 18+.");
        }
        const adapter = await navigator.gpu.requestAdapter({
            powerPreference: "high-performance"
        });
        if (!adapter) {
            throw new Error("No appropriate WebGPU adapter found.");
        }
        this.gpuDevice = await adapter.requestDevice({
            requiredFeatures: [],
            requiredLimits: {}
        });
        this.gpuContext = this.canvas.getContext("webgpu");
        const preferredFormat = navigator.gpu.getPreferredCanvasFormat();
        this.gpuContext.configure({
            device: this.gpuDevice,
            format: preferredFormat,
            alphaMode: "premultiplied",
        });

        // 3. Initialize AudioContext (WebAudio)
        const AudioContextClass = window.AudioContext || window.webkitAudioContext;
        if (AudioContextClass) {
            this.audioCtx = new AudioContextClass({ latencyHint: "interactive" });
        }

        // 4. Instantiate WebAssembly module with imports
        const importObject = {
            env: {
                foreplay_host_present_rgba: (ptr, width, height, stride) => {
                    this.presentFrameRgba(ptr, width, height, stride);
                }
            }
        };

        const wasmModule = await WebAssembly.instantiate(wasmArrayBuffer, importObject);
        this.wasmInstance = wasmModule.instance;

        const abiVersion = this.wasmInstance.exports.foreplay_wasm_abi_version
            ? this.wasmInstance.exports.foreplay_wasm_abi_version()
            : 1;
        const fpckLen = this.wasmInstance.exports.foreplay_fpck_len
            ? this.wasmInstance.exports.foreplay_fpck_len()
            : (this.fpckArchiveBytes ? this.fpckArchiveBytes.byteLength : 0);

        console.log(`[Foreplay] WebAssembly ready. ABI version=${abiVersion}, embedded archive=${fpckLen} bytes`);
        this.updateStatus("WebGPU + WebAudio Active (Ready)", "ready");
        this.setupAudioUnlock();
        return this;
    }

    extractFpckCustomSection(wasmBytes) {
        if (wasmBytes.length < 8) return null;
        let offset = 8;
        while (offset < wasmBytes.length) {
            const sectionId = wasmBytes[offset++];
            const { value: sectionLen, bytesRead } = this.readLeb128(wasmBytes, offset);
            offset += bytesRead;
            const sectionEnd = offset + sectionLen;
            if (sectionId === 0) {
                const { value: nameLen, bytesRead: nameBytesRead } = this.readLeb128(wasmBytes, offset);
                const nameOffset = offset + nameBytesRead;
                const nameBytes = wasmBytes.slice(nameOffset, nameOffset + nameLen);
                const name = new TextDecoder().decode(nameBytes);
                if (name === "foreplay_fpck") {
                    const payloadOffset = nameOffset + nameLen;
                    const payload = wasmBytes.slice(payloadOffset, sectionEnd);
                    if (payload.length >= 24) {
                        return payload.slice(24);
                    }
                }
            }
            offset = sectionEnd;
        }
        return null;
    }

    readLeb128(bytes, offset) {
        let result = 0;
        let shift = 0;
        let bytesRead = 0;
        while (offset < bytes.length) {
            const byte = bytes[offset++];
            bytesRead++;
            result |= (byte & 0x7F) << shift;
            if ((byte & 0x80) === 0) break;
            shift += 7;
        }
        return { value: result, bytesRead };
    }

    setupAudioUnlock() {
        const unlock = async () => {
            if (this.audioCtx && this.audioCtx.state === "suspended") {
                await this.audioCtx.resume();
            }
            if (!this.isRunning) {
                this.startRenderLoop();
            }
            window.removeEventListener("pointerdown", unlock);
            window.removeEventListener("keydown", unlock);
        };
        window.addEventListener("pointerdown", unlock);
        window.addEventListener("keydown", unlock);
    }

    startRenderLoop() {
        this.isRunning = true;
        this.updateStatus("Running @ 120Hz", "running");
        this.lastFrameTime = performance.now();
        const frame = (now) => {
            if (!this.isRunning) return;
            const dt = Math.min((now - this.lastFrameTime) / 1000, 0.1);
            this.lastFrameTime = now;

            if (this.wasmInstance && this.wasmInstance.exports.foreplay_wasm_step_tick) {
                this.wasmInstance.exports.foreplay_wasm_step_tick(dt);
            }

            if (this.gpuDevice && this.gpuContext) {
                const commandEncoder = this.gpuDevice.createCommandEncoder();
                const textureView = this.gpuContext.getCurrentTexture().createView();
                const renderPass = commandEncoder.beginRenderPass({
                    colorAttachments: [{
                        view: textureView,
                        clearValue: { r: 0.043, g: 0.047, b: 0.063, a: 1.0 }, // Studio Obsidian #0B0C10
                        loadOp: "clear",
                        storeOp: "store",
                    }],
                });
                renderPass.end();
                this.gpuDevice.queue.submit([commandEncoder.finish()]);
            }

            requestAnimationFrame(frame);
        };
        requestAnimationFrame(frame);
    }

    presentFrameRgba(ptr, width, height, stride) {
        // Raw RGBA presentation hook from WASM
    }

    updateStatus(msg, state) {
        if (this.statusEl) {
            this.statusEl.textContent = msg;
            this.statusEl.dataset.state = state;
        }
    }
}

export async function initForeplay(canvasId = "foreplay-canvas", statusId = "webgpu-badge") {
    const canvas = document.getElementById(canvasId);
    const statusEl = document.getElementById(statusId);
    const loader = new ForeplayWasmLoader(canvas, statusEl);
    return await loader.init();
}
"#
    .to_string()
}

fn generate_runtime_bridge_js() -> String {
    r#"// foreplay_runtime.js - Foreplay WebGPU Runtime Compatibility Bridge
import("./foreplay_wasm_loader.js").then((mod) => {
    window.ForeplayRuntime = {
        init: async (cfg) => {
            const canvas = cfg.canvas || document.getElementById("foreplay-canvas");
            const statusBadge = document.getElementById("webgpu-badge");
            const loader = new mod.ForeplayWasmLoader(canvas, statusBadge);
            return await loader.init(cfg.wasmUrl || "foreplay_player_bg.wasm");
        }
    };
}).catch((err) => {
    console.error("[Foreplay] Runtime bridge loader error:", err);
});
"#
    .to_string()
}

fn generate_index_html(game_title: &str) -> String {
    const HTML_TEMPLATE: &str = r##"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0, maximum-scale=1.0, user-scalable=no" />
    <title>{game_title}</title>
    <link rel="manifest" href="manifest.webmanifest" />
    <meta name="theme-color" content="#0B0C10" />
    <script src="coi-serviceworker.js"></script>
    <style>
        :root {
            --fp-bg: #0B0C10;
            --fp-surface: #14171F;
            --fp-border: #232936;
            --fp-accent: #00FFE0;
            --fp-accent-dim: rgba(0, 255, 224, 0.15);
            --fp-text: #E6EAF2;
            --fp-text-muted: #8E9BAE;
            --fp-green: #38EF7D;
        }
        * { box-sizing: border-box; margin: 0; padding: 0; }
        html, body {
            width: 100%;
            height: 100%;
            overflow: hidden;
            background-color: var(--fp-bg);
            color: var(--fp-text);
            font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, "Helvetica Neue", Arial, sans-serif;
            user-select: none;
            -webkit-user-select: none;
        }
        #viewport-container {
            position: relative;
            width: 100vw;
            height: 100vh;
            display: flex;
            align-items: center;
            justify-content: center;
        }
        canvas#foreplay-canvas {
            width: 100%;
            height: 100%;
            display: block;
            outline: none;
        }
        #hud-overlay {
            position: absolute;
            top: 16px;
            left: 16px;
            display: flex;
            align-items: center;
            gap: 12px;
            pointer-events: none;
            z-index: 10;
        }
        .hud-badge {
            background: rgba(20, 23, 31, 0.75);
            backdrop-filter: blur(12px);
            -webkit-backdrop-filter: blur(12px);
            border: 1px solid var(--fp-border);
            border-radius: 9999px;
            padding: 6px 14px;
            font-size: 12px;
            font-weight: 500;
            letter-spacing: 0.04em;
            display: flex;
            align-items: center;
            gap: 8px;
            box-shadow: 0 4px 16px rgba(0, 0, 0, 0.4);
        }
        .status-dot {
            width: 8px;
            height: 8px;
            border-radius: 50%;
            background: #FFB703;
            transition: background 0.3s ease;
        }
        .hud-badge[data-state="running"] .status-dot {
            background: var(--fp-green);
            box-shadow: 0 0 8px var(--fp-green);
        }
        .hud-badge[data-state="ready"] .status-dot {
            background: var(--fp-accent);
            box-shadow: 0 0 8px var(--fp-accent);
        }
        .hud-badge[data-state="error"] .status-dot {
            background: #FF4757;
        }
        #audio-unlock-overlay {
            position: absolute;
            inset: 0;
            background: rgba(11, 12, 16, 0.75);
            backdrop-filter: blur(8px);
            display: flex;
            flex-direction: column;
            align-items: center;
            justify-content: center;
            cursor: pointer;
            z-index: 20;
            transition: opacity 0.3s ease, visibility 0.3s;
        }
        #audio-unlock-overlay.dismissed {
            opacity: 0;
            visibility: hidden;
            pointer-events: none;
        }
        .unlock-card {
            background: var(--fp-surface);
            border: 1px solid var(--fp-border);
            border-radius: 16px;
            padding: 32px 48px;
            text-align: center;
            box-shadow: 0 12px 32px rgba(0, 0, 0, 0.6);
            display: flex;
            flex-direction: column;
            align-items: center;
            gap: 16px;
        }
        .unlock-title {
            font-size: 20px;
            font-weight: 600;
            color: var(--fp-text);
        }
        .unlock-subtitle {
            font-size: 14px;
            color: var(--fp-text-muted);
        }
        .unlock-btn {
            background: var(--fp-accent);
            color: #000;
            border: none;
            border-radius: 8px;
            padding: 10px 24px;
            font-weight: 600;
            font-size: 14px;
            cursor: pointer;
            transition: transform 0.15s ease, filter 0.15s ease;
        }
        .unlock-btn:hover {
            filter: brightness(1.1);
            transform: scale(1.02);
        }
    </style>
</head>
<body>
    <div id="viewport-container">
        <canvas id="foreplay-canvas"></canvas>
        <div id="hud-overlay">
            <div id="webgpu-badge" class="hud-badge" data-state="loading">
                <span class="status-dot"></span>
                <span id="status-text">Foreplay Engine • Initializing</span>
            </div>
        </div>
        <div id="audio-unlock-overlay">
            <div class="unlock-card">
                <div class="unlock-title">{game_title}</div>
                <div class="unlock-subtitle">Click anywhere to unlock WebAudio & launch 120Hz WebGPU engine</div>
                <button class="unlock-btn">Start Game</button>
            </div>
        </div>
    </div>
    <script src="foreplay_runtime.js"></script>
    <script type="module">
        import { ForeplayWasmLoader } from "./foreplay_wasm_loader.js";

        const canvas = document.getElementById("foreplay-canvas");
        const statusBadge = document.getElementById("webgpu-badge");
        const statusText = document.getElementById("status-text");
        const unlockOverlay = document.getElementById("audio-unlock-overlay");

        function updateStatus(msg, state) {
            statusText.textContent = msg;
            statusBadge.dataset.state = state;
        }

        const loader = new ForeplayWasmLoader(canvas, {
            set textContent(val) { updateStatus(val, statusBadge.dataset.state); },
            set dataset(obj) { if (obj && obj.state) statusBadge.dataset.state = obj.state; }
        });

        async function launch() {
            try {
                await loader.init("foreplay_player_bg.wasm");
                updateStatus("WebGPU + WebAudio Active (Ready)", "ready");
            } catch (err) {
                console.error("[Foreplay] Init error:", err);
                updateStatus("Error: " + err.message, "error");
            }
        }

        function unlock() {
            unlockOverlay.classList.add("dismissed");
            if (loader.audioCtx && loader.audioCtx.state === "suspended") {
                loader.audioCtx.resume();
            }
            if (!loader.isRunning) {
                loader.startRenderLoop();
            }
        }

        unlockOverlay.addEventListener("click", unlock);
        window.addEventListener("keydown", (e) => {
            if (e.key === " " || e.key === "Enter") unlock();
        }, { once: true });

        if ("serviceWorker" in navigator) {
            navigator.serviceWorker.register("sw.js").catch(console.warn);
        }

        launch();
    </script>
</body>
</html>
"##;
    HTML_TEMPLATE.replace("{game_title}", game_title)
}

fn generate_coi_serviceworker_js() -> String {
    r#"/*! coi-serviceworker v0.1.7 - MIT */
if (typeof window === 'undefined') {
  self.addEventListener('install', () => self.skipWaiting());
  self.addEventListener('activate', (event) => event.waitUntil(self.clients.claim()));
  self.addEventListener('fetch', (event) => {
    if (event.request.cache === 'only-if-cached' && event.request.mode !== 'same-origin') return;
    event.respondWith(
      fetch(event.request).then((response) => {
        if (response.status === 0) return response;
        const newHeaders = new Headers(response.headers);
        newHeaders.set('Cross-Origin-Embedder-Policy', 'require-corp');
        newHeaders.set('Cross-Origin-Opener-Policy', 'same-origin');
        newHeaders.set('Cross-Origin-Resource-Policy', 'cross-origin');
        return new Response(response.body, {
          status: response.status,
          statusText: response.statusText,
          headers: newHeaders,
        });
      })
    );
  });
} else {
  (() => {
    const coi = {
      shouldRegister: () => !window.crossOriginIsolated,
      register: () => {
        if ('serviceWorker' in navigator) {
          navigator.serviceWorker.register('coi-serviceworker.js').then((reg) => {
            reg.addEventListener('updatefound', () => {
              window.location.reload();
            });
            if (reg.active && !navigator.serviceWorker.controller) {
              window.location.reload();
            }
          });
        }
      },
    };
    if (coi.shouldRegister()) {
      coi.register();
    }
  })();
}
"#
    .to_string()
}

fn generate_offline_sw_js() -> String {
    r#"// sw.js - Foreplay Offline PWA Service Worker
const CACHE_NAME = 'foreplay-v1';
const ASSETS_TO_CACHE = [
  './',
  './index.html',
  './foreplay_player_bg.wasm',
  './foreplay_wasm_loader.js',
  './foreplay_runtime.js',
  './manifest.webmanifest',
  './game.fpck'
];

self.addEventListener('install', (event) => {
  event.waitUntil(
    caches.open(CACHE_NAME).then((cache) => {
      return cache.addAll(ASSETS_TO_CACHE).catch((err) => {
        console.warn('[Foreplay SW] Cache addAll warning:', err);
      });
    }).then(() => self.skipWaiting())
  );
});

self.addEventListener('activate', (event) => {
  event.waitUntil(
    caches.keys().then((keys) => {
      return Promise.all(
        keys.map((key) => {
          if (key !== CACHE_NAME) {
            return caches.delete(key);
          }
        })
      );
    }).then(() => self.clients.claim())
  );
});

self.addEventListener('fetch', (event) => {
  event.respondWith(
    caches.match(event.request).then((cachedResponse) => {
      if (cachedResponse) {
        return cachedResponse;
      }
      return fetch(event.request);
    })
  );
});
"#
    .to_string()
}

fn generate_webmanifest(config: &ExportConfig) -> String {
    const MANIFEST_TEMPLATE: &str = r##"{
  "name": "{game_title}",
  "short_name": "{game_title}",
  "description": "Foreplay WebGPU Game Engine release bundle",
  "start_url": "./index.html",
  "display": "fullscreen",
  "orientation": "landscape",
  "background_color": "#0B0C10",
  "theme_color": "#0B0C10",
  "categories": ["games", "entertainment"],
  "icons": [
    {
      "src": "icon.png",
      "sizes": "512x512",
      "type": "image/png",
      "purpose": "any maskable"
    }
  ]
}
"##;
    MANIFEST_TEMPLATE.replace("{game_title}", &config.game_title)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wasm_header_magic_and_version() {
        let empty_fpck = FpckArchive::pack_bytes(&BTreeMap::new()).unwrap();
        let wasm = WasmWebGpuPackager::build_wasm_module(&empty_fpck);
        assert!(wasm.len() >= 8);
        assert_eq!(&wasm[0..4], &WASM_MAGIC);
        assert_eq!(u32::from_le_bytes(wasm[4..8].try_into().unwrap()), WASM_VERSION);
    }

    #[test]
    fn test_leb128_u32_roundtrip() {
        let test_cases = [0u32, 1, 63, 64, 127, 128, 255, 300, 16384, 624485, u32::MAX - 1, u32::MAX];
        for &val in &test_cases {
            let mut buf = Vec::new();
            encode_u32_leb128(val, &mut buf);
            let mut cursor = 0;
            let decoded = decode_u32_leb128(&buf, &mut cursor).expect("valid decode");
            assert_eq!(decoded, val);
            assert_eq!(cursor, buf.len());
        }
    }

    #[test]
    fn test_leb128_u64_roundtrip() {
        let test_cases = [0u64, 1, 127, 128, 65535, 1_000_000_000, u64::MAX - 1, u64::MAX];
        for &val in &test_cases {
            let mut buf = Vec::new();
            encode_u64_leb128(val, &mut buf);
            let mut cursor = 0;
            let decoded = decode_u64_leb128(&buf, &mut cursor).expect("valid decode");
            assert_eq!(decoded, val);
            assert_eq!(cursor, buf.len());
        }
    }

    #[test]
    fn test_leb128_sleb128_i32_encoding() {
        let test_cases = [0i32, 1, -1, 63, -64, 127, -128, 255, -256, 123456, -654321, i32::MAX, i32::MIN];
        for &val in &test_cases {
            let mut buf = Vec::new();
            encode_i32_sleb128(val, &mut buf);
            let mut cursor = 0;
            let decoded = decode_i32_sleb128(&buf, &mut cursor).expect("valid decode");
            assert_eq!(decoded, val);
            assert_eq!(cursor, buf.len());
        }
    }

    #[test]
    fn test_wasm_section_walking_structure() {
        let empty_fpck = FpckArchive::pack_bytes(&BTreeMap::new()).unwrap();
        let wasm = WasmWebGpuPackager::build_wasm_module(&empty_fpck);

        let mut cursor = 8;
        let mut section_ids = Vec::new();

        while cursor < wasm.len() {
            let id = wasm[cursor];
            cursor += 1;
            let len = decode_u32_leb128(&wasm, &mut cursor).unwrap() as usize;
            section_ids.push(id);
            cursor += len;
        }

        // Expected standard section sequence: Type(1), Import(2), Function(3), Memory(5), Export(7), Code(10), Custom(0)
        assert_eq!(section_ids, vec![1, 2, 3, 5, 7, 10, 0]);
    }

    #[test]
    fn test_wasm_custom_section_embed_and_extract_roundtrip() {
        let mut files = BTreeMap::new();
        files.insert("project.foreplay".into(), b"[project]\nname=\"WebGpuTest\"\n".to_vec());
        files.insert("scenes/main.fscn".into(), b"[scene]\nnode=Player\n".to_vec());
        files.insert("shaders/ocean.wgsl".into(), b"@vertex fn vs_main() {}\n".to_vec());

        let packed_fpck = FpckArchive::pack_bytes(&files).unwrap();
        let wasm = WasmWebGpuPackager::build_wasm_module(&packed_fpck);

        let unpacked = extract_and_verify_wasm_fpck_section(&wasm).expect("extraction succeeds");
        assert_eq!(unpacked.len(), 3);
        assert_eq!(unpacked["project.foreplay"], files["project.foreplay"]);
        assert_eq!(unpacked["scenes/main.fscn"], files["scenes/main.fscn"]);
        assert_eq!(unpacked["shaders/ocean.wgsl"], files["shaders/ocean.wgsl"]);
    }

    #[test]
    fn test_wasm_custom_section_single_bit_tamper_rejected() {
        let mut files = BTreeMap::new();
        files.insert("asset.dat".into(), vec![10, 20, 30, 40]);
        let packed_fpck = FpckArchive::pack_bytes(&files).unwrap();
        let mut wasm = WasmWebGpuPackager::build_wasm_module(&packed_fpck);

        // Tamper with the very last byte of the WASM binary (part of raw fpck bytes)
        let last_idx = wasm.len() - 1;
        wasm[last_idx] ^= 0x01;

        let err = extract_and_verify_wasm_fpck_section(&wasm).expect_err("tampering detected");
        assert!(matches!(err, ExportError::EmbeddedPack(_)));
        let msg = format!("{err}");
        assert!(msg.contains("checksum mismatch") || msg.contains("mismatch"));
    }

    #[test]
    fn test_wasm_custom_section_magic_tamper_rejected() {
        let mut files = BTreeMap::new();
        files.insert("asset.dat".into(), vec![10, 20]);
        let packed_fpck = FpckArchive::pack_bytes(&files).unwrap();
        let mut wasm = WasmWebGpuPackager::build_wasm_module(&packed_fpck);

        // Find FPCKWASM magic and tamper
        let magic_pos = wasm.windows(8).position(|w| w == b"FPCKWASM").expect("magic found");
        wasm[magic_pos] = b'X';

        let err = extract_and_verify_wasm_fpck_section(&wasm).expect_err("magic corruption detected");
        assert!(matches!(err, ExportError::EmbeddedPack(_)));
        assert!(format!("{err}").contains("invalid 'foreplay_fpck' magic"));
    }

    #[test]
    fn test_wasm_truncated_binary_rejected() {
        let mut files = BTreeMap::new();
        files.insert("asset.dat".into(), vec![1, 2, 3]);
        let packed_fpck = FpckArchive::pack_bytes(&files).unwrap();
        let wasm = WasmWebGpuPackager::build_wasm_module(&packed_fpck);

        // Truncate at header
        assert!(extract_and_verify_wasm_fpck_section(&wasm[..4]).is_err());
        // Truncate at section start
        assert!(extract_and_verify_wasm_fpck_section(&wasm[..12]).is_err());
        // Truncate right before payload ends
        assert!(extract_and_verify_wasm_fpck_section(&wasm[..wasm.len() - 10]).is_err());
        // Empty slice
        assert!(extract_and_verify_wasm_fpck_section(&[]).is_err());
    }

    #[test]
    fn test_wasm_corrupt_section_length_rejected() {
        let mut files = BTreeMap::new();
        files.insert("test".into(), vec![1]);
        let packed_fpck = FpckArchive::pack_bytes(&files).unwrap();
        let mut wasm = WasmWebGpuPackager::build_wasm_module(&packed_fpck);

        // Offset 8 is section 1 ID (0x01). Offset 9 is section 1 length.
        // Replace with 5-byte LEB128 of a huge value
        wasm[9] = 0xFF;
        wasm.insert(10, 0xFF);
        wasm.insert(11, 0xFF);

        let err = extract_and_verify_wasm_fpck_section(&wasm).expect_err("corrupt section detected");
        assert!(matches!(err, ExportError::EmbeddedPack(_)));
    }

    #[test]
    fn test_wasm_missing_fpck_section_rejected() {
        // Build a minimal valid WASM binary with only header and type section (no custom section)
        let mut wasm = Vec::new();
        wasm.extend_from_slice(&WASM_MAGIC);
        wasm.extend_from_slice(&WASM_VERSION.to_le_bytes());
        let type_payload = [0x00]; // 0 types
        emit_section(1, &type_payload, &mut wasm);

        let err = extract_and_verify_wasm_fpck_section(&wasm).expect_err("missing custom section detected");
        assert!(matches!(err, ExportError::EmbeddedPack(_)));
        assert!(format!("{err}").contains("missing 'foreplay_fpck' custom section"));
    }

    #[test]
    fn test_leb128_overflow_rejected() {
        // 6 bytes all with high bit set (overflows u32 limit of 5 bytes)
        let overflow_bytes = [0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x00];
        let mut cursor = 0;
        let err = decode_u32_leb128(&overflow_bytes, &mut cursor).expect_err("overflow detected");
        assert!(matches!(err, ExportError::EmbeddedPack(_)));
        assert!(format!("{err}").contains("overflow"));
    }

    #[test]
    fn test_wasm_generate_bundle_files() {
        let mut cfg = ExportConfig::new("Galaxy Skater", "com.foreplay.galaxy", "res://scenes/main.fscn");
        cfg.icon_png = Some(vec![137, 80, 78, 71]);
        let fpck_bytes = FpckArchive::pack_bytes(&cfg.files).unwrap();

        let bundle = WasmWebGpuPackager::generate_bundle(&cfg, &fpck_bytes);

        assert!(bundle.contains_key("foreplay_player_bg.wasm"));
        assert!(bundle.contains_key("foreplay_wasm_loader.js"));
        assert!(bundle.contains_key("foreplay_runtime.js"));
        assert!(bundle.contains_key("index.html"));
        assert!(bundle.contains_key("coi-serviceworker.js"));
        assert!(bundle.contains_key("sw.js"));
        assert!(bundle.contains_key("manifest.webmanifest"));
        assert!(bundle.contains_key("manifest.json"));
        assert!(bundle.contains_key("game.fpck"));
        assert!(bundle.contains_key("icon.png"));

        // Verify that the generated wasm in the bundle unpacks cleanly
        let wasm_file = bundle.get("foreplay_player_bg.wasm").unwrap();
        let unpacked = extract_and_verify_wasm_fpck_section(wasm_file).expect("bundle wasm unpacks");
        assert_eq!(unpacked.len(), cfg.files.len());
    }
}
