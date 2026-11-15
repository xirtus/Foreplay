# Clean-Room Porting & Differential Oracle Workflow

This template demonstrates Foreplay's **RustPorts clean-room porting methodology**, modeled after projects like `skate2-rust-port` and `rustports.org`.

---

## 1. BYO-ROM Architecture (`port://`)

Foreplay engine binaries contain zero copyrighted assets or proprietary engine bytecode. Instead, retail game discs or digital installations are mounted read-only at runtime using the `port://` VFS scheme.

```rust
use foreplay_core::VirtualFileSystem;
use foreplay_porthost::{DirectoryPortMount, SimplePakMount};

let mut vfs = VirtualFileSystem::new();

// Mount retail Xbox 360 game extract
let disc_mount = Box::new(DirectoryPortMount::new("/Volumes/Skate2_Retail/data"));
vfs.mount_port_archive("disc", disc_mount);

// Files are accessed capability-safely:
// port://disc/levels/san_van_plaza.pak
```

---

## 2. Safe Zero-Panic Endian Cursor

Xbox 360 PowerPC assets are Big-Endian, while modern x86/ARM platforms are Little-Endian. `foreplay_porthost::BinaryReader` safely reads multi-byte primitives with zero runtime panics on corrupted or truncated retail files:

```rust
use foreplay_porthost::{BinaryReader, Endian};

let bytes = std::fs::read("port://disc/sk82_na_m.xex")?;
let mut reader = BinaryReader::new(&bytes, Endian::Big);

let magic = reader.read_fourcc()?; // b"XEX2"
let header_size = reader.read_u32()?;
reader.align(16)?;
let position = reader.read_vec3()?; // [f32; 3]
```

---

## 3. Frame-by-Frame Differential Oracle

To guarantee 100% physics and feel fidelity against upstream game binaries (e.g. Havok/RenderWare traces or C++ Godot reference physics), the Differential Oracle performs automated cross-verification:

```rust
use foreplay_porthost::oracle::{evaluate_trace, OracleFrameSample};

// Load upstream reference trace
let trace_json = std::fs::read_to_string("templates/cleanroom-port/oracle_trace.json")?;
let upstream: Vec<OracleFrameSample> = serde_json::from_str(&trace_json)?;

// Capture candidate Foreplay simulation frames
let candidate: Vec<OracleFrameSample> = run_foreplay_sim_ticks(upstream.len());

// Evaluate with 1mm position & 1mm/s velocity tolerance
let report = evaluate_trace(&upstream, &candidate, 0.001);

println!("Verification pass rate: {:.2}%", report.pass_rate_percent);
println!("RMS Position Error: {:.6} m", report.position_rms_error);
println!("Max drift frame: {:?}", report.max_drift_frame);

assert!(report.is_perfect_match(), "Candidate simulation drifted from reference!");
```

---

## 4. MCP Agent Automation

AI agents can inspect and advance port verification through the JSON-RPC 2.0 API:

```json
{"jsonrpc":"2.0","id":1,"method":"foreplay/step","params":{"ticks":60}}
```
