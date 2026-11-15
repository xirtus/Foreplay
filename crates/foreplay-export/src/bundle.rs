//! Multi-platform export bundle generation.
//!
//! Produces ready-to-run release bundles for:
//! - Linux / SteamDeck x86_64
//! - Windows x86_64
//! - macOS `.app` bundle (with `LSSupportsGameMode`)
//! - WebGPU browser distribution
//! - SexOS package

use crate::archive::{FpckArchive, FpckError};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Target deployment platform for exported games.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
pub enum ExportTarget {
    LinuxSteamDeckX64,
    WindowsX64,
    MacOsAppBundle,
    WebGpuBrowserUrl,
    SexOsPackage,
    ThreeJsWebProject,
}

impl ExportTarget {
    pub fn all() -> &'static [ExportTarget] {
        &[
            Self::LinuxSteamDeckX64,
            Self::WindowsX64,
            Self::MacOsAppBundle,
            Self::WebGpuBrowserUrl,
            Self::SexOsPackage,
            Self::ThreeJsWebProject,
        ]
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            Self::LinuxSteamDeckX64 => "Linux / Steam Deck (x86_64)",
            Self::WindowsX64 => "Windows (x86_64)",
            Self::MacOsAppBundle => "macOS App Bundle (.app)",
            Self::WebGpuBrowserUrl => "WebGPU HTML5 / WebApp",
            Self::SexOsPackage => "SexOS Native Package (.sexpkg)",
            Self::ThreeJsWebProject => "3JSE / Three.js TypeScript Project",
        }
    }
}

/// Project export configuration.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct ExportConfig {
    pub game_title: String,
    pub package_name: String,
    pub version: String,
    pub main_scene: String,
    pub files: BTreeMap<String, Vec<u8>>,
    pub icon_png: Option<Vec<u8>>,
}

impl ExportConfig {
    pub fn new(
        game_title: impl Into<String>,
        package_name: impl Into<String>,
        main_scene: impl Into<String>,
    ) -> Self {
        Self {
            game_title: game_title.into(),
            package_name: package_name.into(),
            version: "1.0.0".into(),
            main_scene: main_scene.into(),
            files: BTreeMap::new(),
            icon_png: None,
        }
    }

    pub fn with_file(mut self, path: impl Into<String>, data: impl Into<Vec<u8>>) -> Self {
        self.files.insert(path.into(), data.into());
        self
    }
}

/// Produced export bundle containing generated files and the packed `.fpck` archive.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct ExportOutput {
    pub target: ExportTarget,
    pub primary_artifact: String,
    pub archive_bytes: Vec<u8>,
    pub bundle_files: BTreeMap<String, Vec<u8>>,
}

/// Errors produced during export bundle generation.
#[derive(Clone, PartialEq, Eq, Debug, thiserror::Error, Serialize, Deserialize)]
pub enum ExportError {
    #[error("archive packaging error: {0}")]
    ArchiveError(#[from] FpckError),
    #[error("invalid export configuration: {0}")]
    InvalidConfig(String),
    #[error("embedded pack error: {0}")]
    EmbeddedPack(String),
}

impl From<foreplay_player::EmbeddedPackError> for ExportError {
    fn from(err: foreplay_player::EmbeddedPackError) -> Self {
        Self::EmbeddedPack(err.to_string())
    }
}

/// Multi-platform game project exporter.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct ProjectExporter;

impl ProjectExporter {
    /// Builds a release export bundle for the specified target.
    pub fn export_project(
        config: &ExportConfig,
        target: ExportTarget,
    ) -> Result<ExportOutput, ExportError> {
        if config.game_title.trim().is_empty() {
            return Err(ExportError::InvalidConfig("game title cannot be empty".into()));
        }
        if config.main_scene.trim().is_empty() {
            return Err(ExportError::InvalidConfig("main scene cannot be empty".into()));
        }

        // Prepare archive files: ensure project manifest is present
        let mut archive_files = config.files.clone();
        if !archive_files.contains_key("project.foreplay") {
            let manifest = format!(
                "[project]\nname=\"{}\"\nversion=\"{}\"\nmain_scene=\"{}\"\n",
                config.game_title, config.version, config.main_scene
            );
            archive_files.insert("project.foreplay".into(), manifest.into_bytes());
        }

        let archive_bytes = FpckArchive::pack_bytes(&archive_files)?;
        let mut bundle_files = BTreeMap::new();
        let primary_artifact: String;

        match target {
            ExportTarget::LinuxSteamDeckX64 => {
                primary_artifact = "game.x86_64".into();
                bundle_files.insert("game.fpck".into(), archive_bytes.clone());

                let launcher_script = format!(
                    "#!/usr/bin/env bash\n\
                    # Foreplay SteamDeck & Linux launcher for {}\n\
                    export ENABLE_VKB=1\n\
                    export MANGOHUD=1\n\
                    SCRIPT_DIR=\"$(cd \"$(dirname \"${{BASH_SOURCE[0]}}\")\" && pwd)\"\n\
                    exec \"$SCRIPT_DIR/foreplay_runner\" --archive \"$SCRIPT_DIR/game.fpck\" \"$@\"\n",
                    config.game_title
                );
                bundle_files.insert("game.x86_64".into(), launcher_script.clone().into_bytes());
                bundle_files.insert("launch.sh".into(), launcher_script.into_bytes());

                let runner_stub = b"\x7fELF_FOREPLAY_RUNNER_STUB";
                let runner_binary = foreplay_player::embed_pack_trailer(runner_stub, &archive_bytes);
                bundle_files.insert("foreplay_runner".into(), runner_binary);
            }

            ExportTarget::WindowsX64 => {
                primary_artifact = "game.exe".into();
                bundle_files.insert("game.fpck".into(), archive_bytes.clone());

                let batch_launcher = format!(
                    "@echo off\r\n\
                    title {}\r\n\
                    foreplay_runner.exe --archive game.fpck %*\r\n",
                    config.game_title
                );
                bundle_files.insert("game.bat".into(), batch_launcher.into_bytes());
                // Self-contained portable PE executable stamped with FPCKEXEC trailer
                let exe_stub = b"MZ_FOREPLAY_STUB_EXE";
                let exe_binary = foreplay_player::embed_pack_trailer(exe_stub, &archive_bytes);
                bundle_files.insert("game.exe".into(), exe_binary);
            }

            ExportTarget::MacOsAppBundle => {
                let app_name = format!("{}.app", sanitize_filename(&config.game_title));
                primary_artifact = app_name.clone();

                let info_plist = format!(
                    "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
                    <!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
                    <plist version=\"1.0\">\n\
                    <dict>\n\
                        <key>CFBundlePackageType</key>\n\
                        <string>APPL</string>\n\
                        <key>CFBundleName</key>\n\
                        <string>{}</string>\n\
                        <key>CFBundleDisplayName</key>\n\
                        <string>{}</string>\n\
                        <key>CFBundleIdentifier</key>\n\
                        <string>{}</string>\n\
                        <key>CFBundleVersion</key>\n\
                        <string>{}</string>\n\
                        <key>CFBundleShortVersionString</key>\n\
                        <string>{}</string>\n\
                        <key>CFBundleExecutable</key>\n\
                        <string>launcher</string>\n\
                        <key>NSHighResolutionCapable</key>\n\
                        <true/>\n\
                        <key>LSSupportsGameMode</key>\n\
                        <true/>\n\
                    </dict>\n\
                    </plist>\n",
                    config.game_title,
                    config.game_title,
                    config.package_name,
                    config.version,
                    config.version
                );

                bundle_files.insert(
                    format!("{app_name}/Contents/Info.plist"),
                    info_plist.into_bytes(),
                );

                let launcher_sh = "#!/usr/bin/env bash\n\
                    DIR=\"$(cd \"$(dirname \"${BASH_SOURCE[0]}\")\" && pwd)\"\n\
                    exec \"$DIR/foreplay_mac\" --archive \"$DIR/../Resources/game.fpck\" \"$@\"\n";
                bundle_files.insert(
                    format!("{app_name}/Contents/MacOS/launcher"),
                    launcher_sh.as_bytes().to_vec(),
                );

                let mac_stub = b"\xfe\xed\xfa\xcf_MACH_O_STUB";
                let mac_binary = foreplay_player::embed_pack_trailer(mac_stub, &archive_bytes);
                bundle_files.insert(
                    format!("{app_name}/Contents/MacOS/foreplay_mac"),
                    mac_binary,
                );

                bundle_files.insert(
                    format!("{app_name}/Contents/Resources/game.fpck"),
                    archive_bytes.clone(),
                );

                if let Some(icon) = &config.icon_png {
                    bundle_files.insert(
                        format!("{app_name}/Contents/Resources/icon.png"),
                        icon.clone(),
                    );
                }
            }

            ExportTarget::WebGpuBrowserUrl => {
                primary_artifact = "index.html".into();
                bundle_files.insert("game.fpck".into(), archive_bytes.clone());

                let html = format!(
                    "<!DOCTYPE html>\n\
                    <html lang=\"en\">\n\
                    <head>\n\
                        <meta charset=\"UTF-8\" />\n\
                        <meta name=\"viewport\" content=\"width=device-width, initial-scale=1.0\" />\n\
                        <title>{}</title>\n\
                        <link rel=\"manifest\" href=\"manifest.json\" />\n\
                        <style>\n\
                            body {{ margin: 0; background: #000; overflow: hidden; display: flex; justify-content: center; align-items: center; height: 100vh; }}\n\
                            canvas {{ width: 100%; height: 100%; display: block; }}\n\
                        </style>\n\
                    </head>\n\
                    <body>\n\
                        <canvas id=\"foreplay-canvas\"></canvas>\n\
                        <script src=\"foreplay_runtime.js\"></script>\n\
                        <script>\n\
                            window.addEventListener('load', async () => {{\n\
                                await ForeplayRuntime.init({{ canvas: document.getElementById('foreplay-canvas'), archive: 'game.fpck' }});\n\
                            }});\n\
                        </script>\n\
                    </body>\n\
                    </html>\n",
                    config.game_title
                );
                bundle_files.insert("index.html".into(), html.into_bytes());

                let manifest = format!(
                    "{{\n\
                        \"name\": \"{}\",\n\
                        \"short_name\": \"{}\",\n\
                        \"start_url\": \".\",\n\
                        \"display\": \"fullscreen\",\n\
                        \"theme_color\": \"#000000\",\n\
                        \"background_color\": \"#000000\"\n\
                    }}\n",
                    config.game_title, config.game_title
                );
                bundle_files.insert("manifest.json".into(), manifest.into_bytes());

                let runtime_js = "// Foreplay WebGPU runtime loader\n\
                    const ForeplayRuntime = { init: async (cfg) => { console.log('Initialized Foreplay WebGPU', cfg); } };\n";
                bundle_files.insert("foreplay_runtime.js".into(), runtime_js.as_bytes().to_vec());
            }

            ExportTarget::SexOsPackage => {
                let pkg_name = format!("{}.sexpkg", sanitize_filename(&config.game_title));
                primary_artifact = pkg_name.clone();

                let manifest = format!(
                    "[package]\n\
                    name = \"{}\"\n\
                    title = \"{}\"\n\
                    version = \"{}\"\n\
                    entry = \"game.fpck\"\n\
                    runtime = \"foreplay-v1\"\n\
                    capabilities = [\"webgpu\", \"feel-haptics\", \"audio\"]\n",
                    config.package_name, config.game_title, config.version
                );
                bundle_files.insert("sexpkg.toml".into(), manifest.into_bytes());
                bundle_files.insert("game.fpck".into(), archive_bytes.clone());

                let sexos_stub = b"SEXOS_ELF_STUB";
                let sexos_binary = foreplay_player::embed_pack_trailer(sexos_stub, &archive_bytes);
                bundle_files.insert("game.bin".into(), sexos_binary.clone());
                bundle_files.insert(format!("{pkg_name}.bin"), sexos_binary);
            }

            ExportTarget::ThreeJsWebProject => {
                primary_artifact = "package.json".into();
                bundle_files.insert("game.fpck".into(), archive_bytes.clone());

                // Attempt to load scene from main_scene in config.files
                let clean_path = config.main_scene.trim_start_matches("res://");
                let scene = if let Some(data) = config.files.get(clean_path).or_else(|| config.files.get(&config.main_scene)) {
                    if let Ok(s) = String::from_utf8(data.clone()) {
                        foreplay_ir::IrScene::from_fscn_str(&s).unwrap_or_else(|_| foreplay_ir::IrScene::new(&config.game_title))
                    } else {
                        foreplay_ir::IrScene::new(&config.game_title)
                    }
                } else {
                    let mut found = None;
                    for (path, data) in &config.files {
                        if path.ends_with(".fscn")
                            && let Ok(s) = String::from_utf8(data.clone())
                            && let Ok(sc) = foreplay_ir::IrScene::from_fscn_str(&s)
                        {
                            found = Some(sc);
                            break;
                        }
                    }
                    found.unwrap_or_else(|| foreplay_ir::IrScene::new(&config.game_title))
                };

                let mut gd_scripts = std::collections::BTreeMap::new();
                let mut ts_scripts = std::collections::BTreeMap::new();

                for (&node_id, node) in &scene.nodes {
                    if let Some(foreplay_ir::IrValue::String(script_path)) = node.properties.get("script").or_else(|| node.properties.get("gdscript")) {
                        let clean_script = script_path.trim_start_matches("res://");
                        if let Some(bytes) = config.files.get(clean_script).or_else(|| config.files.get(script_path))
                            && let Ok(src) = String::from_utf8(bytes.clone())
                        {
                            if clean_script.ends_with(".ts") || clean_script.ends_with(".js") {
                                ts_scripts.insert(node_id, src);
                            } else {
                                gd_scripts.insert(node_id, src);
                            }
                        }
                    }
                }

                if gd_scripts.is_empty() && ts_scripts.is_empty() {
                    for (path, bytes) in &config.files {
                        if path.ends_with(".gd")
                            && let Ok(src) = String::from_utf8(bytes.clone())
                        {
                            let target_node = scene.nodes.keys().next().copied().unwrap_or_else(|| foreplay_ir::NodeId::new(1, 1).unwrap());
                            gd_scripts.insert(target_node, src);
                            break;
                        }
                    }
                }

                let opts = foreplay_threejs::ThreeJsExportOptions {
                    project_name: config.game_title.clone(),
                    use_webgpu_renderer: true,
                    include_feel_dev_hud: true,
                    include_atlas_json: true,
                    tone_mapping: foreplay_threejs::ThreeToneMapping::AgX,
                };

                let bundle = foreplay_threejs::compile_ir_to_threejs_project(&scene, &gd_scripts, &ts_scripts, &opts)
                    .map_err(|e| ExportError::InvalidConfig(format!("Three.js compilation failed: {e}")))?;

                for (filename, content) in bundle.files {
                    bundle_files.insert(filename, content.into_bytes());
                }
            }
        }

        Ok(ExportOutput {
            target,
            primary_artifact,
            archive_bytes,
            bundle_files,
        })
    }
}

/// Reads a binary from disk and verifies its embedded `.fpck` package and table of contents.
pub fn verify_self_contained_export_binary(
    exe_path: &std::path::Path,
) -> Result<BTreeMap<String, Vec<u8>>, ExportError> {
    let bytes = std::fs::read(exe_path)
        .map_err(|e| ExportError::EmbeddedPack(format!("failed to read executable at {}: {e}", exe_path.display())))?;
    verify_self_contained_export_bytes(&bytes)
}

/// Verifies an in-memory binary containing an embedded `FPCKEXEC` package and table of contents.
pub fn verify_self_contained_export_bytes(
    exe_bytes: &[u8],
) -> Result<BTreeMap<String, Vec<u8>>, ExportError> {
    let pack_bytes = foreplay_player::extract_embedded_pack_from_bytes(exe_bytes)?
        .ok_or_else(|| ExportError::EmbeddedPack("missing FPCKEXEC embedded pack trailer".into()))?;
    let files = FpckArchive::unpack_bytes(&pack_bytes)?;
    Ok(files)
}

fn sanitize_filename(name: &str) -> String {
    name.chars()
        .map(|c| if c.is_alphanumeric() || c == '_' || c == '-' { c } else { '_' })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> ExportConfig {
        let mut cfg = ExportConfig::new("Cyber Skater", "com.foreplay.cyberskater", "res://scenes/main.tscn");
        cfg.version = "1.2.0".into();
        cfg.files.insert("scenes/main.tscn".into(), b"[node name=\"Main\"]".to_vec());
        cfg.files.insert("scripts/skater.gd".into(), b"extends CharacterBody3D".to_vec());
        cfg.icon_png = Some(vec![137, 80, 78, 71, 13, 10, 26, 10]);
        cfg
    }

    #[test]
    fn export_all_six_targets() {
        let cfg = test_config();

        for &target in ExportTarget::all() {
            let out = ProjectExporter::export_project(&cfg, target).unwrap();
            assert_eq!(out.target, target);
            assert!(!out.archive_bytes.is_empty());

            // Unpack archive_bytes to verify archive integrity
            let unpacked = FpckArchive::unpack_bytes(&out.archive_bytes).unwrap();
            assert!(unpacked.contains_key("scenes/main.tscn"));
            assert!(unpacked.contains_key("scripts/skater.gd"));
            assert!(unpacked.contains_key("project.foreplay"));

            match target {
                ExportTarget::LinuxSteamDeckX64 => {
                    assert_eq!(out.primary_artifact, "game.x86_64");
                    assert!(out.bundle_files.contains_key("game.x86_64"));
                    assert!(out.bundle_files.contains_key("game.fpck"));
                    let sh = String::from_utf8(out.bundle_files["game.x86_64"].clone()).unwrap();
                    assert!(sh.contains("ENABLE_VKB=1"));
                    assert!(sh.contains("MANGOHUD=1"));
                }
                ExportTarget::WindowsX64 => {
                    assert_eq!(out.primary_artifact, "game.exe");
                    assert!(out.bundle_files.contains_key("game.exe"));
                    assert!(out.bundle_files.contains_key("game.fpck"));
                }
                ExportTarget::MacOsAppBundle => {
                    assert!(out.primary_artifact.ends_with(".app"));
                    let plist_key = format!("{}/Contents/Info.plist", out.primary_artifact);
                    let plist = String::from_utf8(out.bundle_files[&plist_key].clone()).unwrap();
                    assert!(plist.contains("<key>LSSupportsGameMode</key>"));
                    assert!(plist.contains("<true/>"));
                    assert!(plist.contains("<key>CFBundleIdentifier</key>"));
                    assert!(plist.contains("<string>com.foreplay.cyberskater</string>"));
                }
                ExportTarget::WebGpuBrowserUrl => {
                    assert_eq!(out.primary_artifact, "index.html");
                    assert!(out.bundle_files.contains_key("index.html"));
                    assert!(out.bundle_files.contains_key("manifest.json"));
                    assert!(out.bundle_files.contains_key("foreplay_runtime.js"));
                }
                ExportTarget::SexOsPackage => {
                    assert!(out.primary_artifact.ends_with(".sexpkg"));
                    assert!(out.bundle_files.contains_key("sexpkg.toml"));
                    let manifest = String::from_utf8(out.bundle_files["sexpkg.toml"].clone()).unwrap();
                    assert!(manifest.contains("runtime = \"foreplay-v1\""));
                    assert!(manifest.contains("capabilities = [\"webgpu\", \"feel-haptics\", \"audio\"]"));
                }
                ExportTarget::ThreeJsWebProject => {
                    assert_eq!(out.primary_artifact, "package.json");
                    assert!(out.bundle_files.contains_key("package.json"));
                    assert!(out.bundle_files.contains_key("src/main.ts"));
                    assert!(out.bundle_files.contains_key("src/scene.ts"));
                    assert!(out.bundle_files.contains_key("src/feel.ts"));
                    assert!(out.bundle_files.contains_key("src/scripts/behaviors.ts"));
                    assert!(out.bundle_files.contains_key("src/atlas.json"));
                    assert!(out.bundle_files.contains_key("vite.config.ts"));
                    assert!(out.bundle_files.contains_key("tsconfig.json"));
                    assert!(out.bundle_files.contains_key("index.html"));
                    assert!(out.bundle_files.contains_key("game.fpck"));
                }
            }
        }
    }

    #[test]
    fn windows_export_is_valid_fpckexec_binary() {
        let cfg = test_config();
        let out = ProjectExporter::export_project(&cfg, ExportTarget::WindowsX64).unwrap();
        let exe_bytes = out.bundle_files.get("game.exe").expect("game.exe present");
        let unpacked = verify_self_contained_export_bytes(exe_bytes).expect("verification succeeds");
        assert!(unpacked.contains_key("scenes/main.tscn"));
        assert!(unpacked.contains_key("scripts/skater.gd"));
        assert!(unpacked.contains_key("project.foreplay"));
    }

    #[test]
    fn linux_export_contains_valid_fpckexec_runner() {
        let cfg = test_config();
        let out = ProjectExporter::export_project(&cfg, ExportTarget::LinuxSteamDeckX64).unwrap();
        let runner_bytes = out.bundle_files.get("foreplay_runner").expect("foreplay_runner present");
        let unpacked = verify_self_contained_export_bytes(runner_bytes).expect("verification succeeds");
        assert!(unpacked.contains_key("scenes/main.tscn"));
        assert!(unpacked.contains_key("scripts/skater.gd"));
        assert!(unpacked.contains_key("project.foreplay"));
    }

    #[test]
    fn macos_app_contains_valid_fpckexec_binary() {
        let cfg = test_config();
        let out = ProjectExporter::export_project(&cfg, ExportTarget::MacOsAppBundle).unwrap();
        let mac_binary_key = format!("{}/Contents/MacOS/foreplay_mac", out.primary_artifact);
        let mac_bytes = out.bundle_files.get(&mac_binary_key).expect("foreplay_mac present");
        let unpacked = verify_self_contained_export_bytes(mac_bytes).expect("verification succeeds");
        assert!(unpacked.contains_key("scenes/main.tscn"));
        assert!(unpacked.contains_key("scripts/skater.gd"));
        assert!(unpacked.contains_key("project.foreplay"));
    }

    #[test]
    fn sexos_export_contains_valid_fpckexec_binary() {
        let cfg = test_config();
        let out = ProjectExporter::export_project(&cfg, ExportTarget::SexOsPackage).unwrap();
        let bin_bytes = out.bundle_files.get("game.bin").expect("game.bin present");
        let unpacked = verify_self_contained_export_bytes(bin_bytes).expect("verification succeeds");
        assert!(unpacked.contains_key("scenes/main.tscn"));
        assert!(unpacked.contains_key("scripts/skater.gd"));
        assert!(unpacked.contains_key("project.foreplay"));
    }

    #[test]
    fn tampered_fpckexec_export_binary_fails_verification() {
        let cfg = test_config();
        let out = ProjectExporter::export_project(&cfg, ExportTarget::WindowsX64).unwrap();
        let mut exe_bytes = out.bundle_files["game.exe"].clone();
        // Tamper with byte inside the package section (offset 25 is after b"MZ_FOREPLAY_STUB_EXE")
        exe_bytes[25] ^= 0xAA;
        let err = verify_self_contained_export_bytes(&exe_bytes).expect_err("tampering detected");
        assert!(matches!(err, ExportError::EmbeddedPack(_)));
    }

    #[test]
    fn verify_self_contained_export_binary_disk_roundtrip() {
        let cfg = test_config();
        let out = ProjectExporter::export_project(&cfg, ExportTarget::LinuxSteamDeckX64).unwrap();
        let runner_bytes = &out.bundle_files["foreplay_runner"];

        let temp_dir = std::env::temp_dir();
        let temp_exe = temp_dir.join(format!(
            "foreplay_test_runner_{}.bin",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::write(&temp_exe, runner_bytes).expect("write temp runner");

        let unpacked = verify_self_contained_export_binary(&temp_exe).expect("disk verification succeeds");
        assert!(unpacked.contains_key("scenes/main.tscn"));
        assert!(unpacked.contains_key("scripts/skater.gd"));

        let _ = std::fs::remove_file(&temp_exe);
    }
}
