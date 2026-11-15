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
}

impl ExportTarget {
    pub fn all() -> &'static [ExportTarget] {
        &[
            Self::LinuxSteamDeckX64,
            Self::WindowsX64,
            Self::MacOsAppBundle,
            Self::WebGpuBrowserUrl,
            Self::SexOsPackage,
        ]
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            Self::LinuxSteamDeckX64 => "Linux / Steam Deck (x86_64)",
            Self::WindowsX64 => "Windows (x86_64)",
            Self::MacOsAppBundle => "macOS App Bundle (.app)",
            Self::WebGpuBrowserUrl => "WebGPU HTML5 / WebApp",
            Self::SexOsPackage => "SexOS Native Package (.sexpkg)",
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
                // Minimal portable PE launcher stub
                bundle_files.insert("game.exe".into(), b"MZ_FOREPLAY_STUB_EXE".to_vec());
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
    fn export_all_five_targets() {
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
            }
        }
    }
}
